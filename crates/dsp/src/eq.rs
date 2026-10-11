//! Classic 10-band graphic equalizer with preamp.
//!
//! Each band is a peaking biquad (RBJ audio EQ cookbook) at the classic center frequencies,
//! ±12 dB. The UI writes settings into [`EqControls`] (atomics); the audio thread's
//! [`Equalizer`] notices the version change and recomputes its coefficients in place, so the
//! audio path never locks or allocates.

use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Band centers in Hz, as labeled on the classic equalizer.
pub const BAND_FREQUENCIES: [f32; 10] = [
    60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0,
];

/// Slider range in dB, both directions.
pub const MAX_DB: f32 = 12.0;

/// Band width: roughly an octave, matching the spacing of the lower bands.
const Q: f32 = 1.2;

/// A snapshot of equalizer settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqSettings {
    pub enabled: bool,
    pub preamp_db: f32,
    pub bands_db: [f32; 10],
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preamp_db: 0.0,
            bands_db: [0.0; 10],
        }
    }
}

fn clamp_db(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(-MAX_DB, MAX_DB)
    } else {
        0.0
    }
}

/// Settings shared between the UI and the audio thread.
#[derive(Debug, Default)]
pub struct EqControls {
    enabled: AtomicBool,
    preamp: AtomicU32,
    bands: [AtomicU32; 10],
    version: AtomicU64,
}

impl EqControls {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        self.bump();
    }

    pub fn set_preamp_db(&self, db: f32) {
        self.preamp.store(clamp_db(db).to_bits(), Ordering::Relaxed);
        self.bump();
    }

    /// Sets band `i` (0..10); out-of-range indices are ignored.
    pub fn set_band_db(&self, i: usize, db: f32) {
        if let Some(b) = self.bands.get(i) {
            b.store(clamp_db(db).to_bits(), Ordering::Relaxed);
            self.bump();
        }
    }

    pub fn set(&self, s: &EqSettings) {
        self.enabled.store(s.enabled, Ordering::Relaxed);
        self.preamp
            .store(clamp_db(s.preamp_db).to_bits(), Ordering::Relaxed);
        for (b, db) in self.bands.iter().zip(s.bands_db) {
            b.store(clamp_db(db).to_bits(), Ordering::Relaxed);
        }
        self.bump();
    }

    pub fn settings(&self) -> EqSettings {
        EqSettings {
            enabled: self.enabled.load(Ordering::Relaxed),
            preamp_db: f32::from_bits(self.preamp.load(Ordering::Relaxed)),
            bands_db: std::array::from_fn(|i| {
                f32::from_bits(self.bands[i].load(Ordering::Relaxed))
            }),
        }
    }

    fn bump(&self) {
        self.version.fetch_add(1, Ordering::Release);
    }

    fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }
}

/// One second-order section, transposed direct form II.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    active: bool,
}

impl Biquad {
    const IDENTITY: Biquad = Biquad {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
        z1: 0.0,
        z2: 0.0,
        active: false,
    };

    /// Updates coefficients for a peaking filter, keeping the filter state.
    fn set_peaking(&mut self, rate: f32, freq: f32, gain_db: f32) {
        // Flat bands and bands at or above Nyquist are skipped entirely.
        if gain_db.abs() < 0.01 || freq >= rate * 0.45 {
            self.active = false;
            return;
        }
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * Q);
        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = -2.0 * cos / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = -2.0 * cos / a0;
        self.a2 = (1.0 - alpha / a) / a0;
        if !self.active {
            self.z1 = 0.0;
            self.z2 = 0.0;
        }
        self.active = true;
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        // Flush denormals so silence does not cost CPU.
        if self.z1.abs() < 1e-20 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-20 {
            self.z2 = 0.0;
        }
        y
    }
}

/// The audio-thread side. Create once per output; `process` is real-time safe.
#[derive(Debug)]
pub struct Equalizer {
    rate: f32,
    controls: Arc<EqControls>,
    seen: u64,
    enabled: bool,
    preamp: f32,
    /// [band][channel]
    filters: [[Biquad; 2]; 10],
}

impl Equalizer {
    pub fn new(sample_rate: u32, controls: Arc<EqControls>) -> Self {
        let mut eq = Self {
            rate: sample_rate.max(1) as f32,
            controls,
            seen: u64::MAX,
            enabled: false,
            preamp: 1.0,
            filters: [[Biquad::IDENTITY; 2]; 10],
        };
        eq.refresh();
        eq
    }

    fn refresh(&mut self) {
        let version = self.controls.version();
        if version == self.seen {
            return;
        }
        self.seen = version;
        let s = self.controls.settings();
        self.enabled = s.enabled;
        self.preamp = 10f32.powf(s.preamp_db / 20.0);
        for (band, db) in s.bands_db.iter().enumerate() {
            for f in &mut self.filters[band] {
                f.set_peaking(self.rate, BAND_FREQUENCIES[band], *db);
            }
        }
    }

    /// Applies the equalizer in place to interleaved stereo.
    pub fn process(&mut self, stereo: &mut [f32]) {
        self.refresh();
        if !self.enabled {
            return;
        }
        for frame in stereo.as_chunks_mut::<2>().0 {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let mut x = *sample * self.preamp;
                for band in &mut self.filters {
                    if band[ch].active {
                        x = band[ch].run(x);
                    }
                }
                *sample = x;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn sine(freq: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = 0.25 * (2.0 * PI * freq * i as f32 / RATE as f32).sin();
                [v, v]
            })
            .collect()
    }

    /// Gain in dB of the equalizer at `freq`, measured on a steady sine.
    fn measure(controls: &Arc<EqControls>, freq: f32) -> f32 {
        let mut eq = Equalizer::new(RATE, Arc::clone(controls));
        let input = sine(freq, RATE as usize);
        let mut out = input.clone();
        eq.process(&mut out);
        // Skip the first half second while the filters settle.
        let tail = RATE as usize;
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        20.0 * (rms(&out[tail..]) / rms(&input[tail..])).log10()
    }

    #[test]
    fn disabled_is_bit_exact_bypass() {
        let c = EqControls::new();
        c.set_band_db(4, 12.0);
        let mut eq = Equalizer::new(RATE, Arc::clone(&c));
        let input = sine(1000.0, 1000);
        let mut out = input.clone();
        eq.process(&mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn flat_and_enabled_is_transparent() {
        let c = EqControls::new();
        c.set_enabled(true);
        assert!(measure(&c, 1000.0).abs() < 0.01);
    }

    #[test]
    fn boosted_band_gains_twelve_db_at_center() {
        let c = EqControls::new();
        c.set_enabled(true);
        c.set_band_db(4, 12.0);
        let g = measure(&c, 1000.0);
        assert!((g - 12.0).abs() < 0.3, "gain at 1 kHz: {g}");
        let far = measure(&c, 60.0);
        assert!(far.abs() < 0.5, "gain at 60 Hz: {far}");
    }

    #[test]
    fn cut_band_and_preamp() {
        let c = EqControls::new();
        c.set_enabled(true);
        c.set_band_db(0, -12.0);
        let g = measure(&c, 60.0);
        assert!((g + 12.0).abs() < 0.5, "cut at 60 Hz: {g}");
        c.set_band_db(0, 0.0);
        c.set_preamp_db(-6.0);
        let g = measure(&c, 1000.0);
        assert!((g + 6.0).abs() < 0.05, "preamp: {g}");
    }

    #[test]
    fn settings_round_trip_and_clamp() {
        let c = EqControls::new();
        let s = EqSettings {
            enabled: true,
            preamp_db: 30.0,
            bands_db: [f32::NAN, -20.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        };
        c.set(&s);
        let got = c.settings();
        assert!(got.enabled);
        assert_eq!(got.preamp_db, 12.0);
        assert_eq!(got.bands_db[0], 0.0);
        assert_eq!(got.bands_db[1], -12.0);
        assert_eq!(got.bands_db[9], 8.0);
        c.set_band_db(10, 5.0); // ignored
    }

    #[test]
    fn changes_apply_on_the_next_block() {
        let c = EqControls::new();
        let mut eq = Equalizer::new(RATE, Arc::clone(&c));
        let mut block = vec![0.5f32; 64];
        eq.process(&mut block);
        assert_eq!(block[0], 0.5);
        c.set_enabled(true);
        c.set_preamp_db(-6.0);
        eq.process(&mut block);
        assert!((block[0] - 0.5 * 10f32.powf(-6.0 / 20.0)).abs() < 1e-6);
    }

    #[test]
    fn extreme_settings_stay_finite() {
        let c = EqControls::new();
        c.set(&EqSettings {
            enabled: true,
            preamp_db: 12.0,
            bands_db: [12.0; 10],
        });
        let mut eq = Equalizer::new(22_050, Arc::clone(&c));
        let mut noise: Vec<f32> = (0..20_000)
            .map(|i| ((i * 7919 % 2003) as f32 / 1001.0) - 1.0)
            .collect();
        eq.process(&mut noise);
        assert!(noise.iter().all(|s| s.is_finite()));
    }
}
