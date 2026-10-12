//! Audio output: device stream, volume and balance.
//!
//! The output pulls interleaved stereo `f32` from a render closure (normally
//! `amp_core::PcmSource::fill`), applies volume and balance, and converts to whatever the
//! device wants. The callback never allocates or locks: scratch space is allocated up front
//! and controls are atomics.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

/// Frames rendered per inner step of the callback. Bounds the preallocated scratch buffer.
const SCRATCH_FRAMES: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("no audio output device available")]
    NoDevice,
    #[error("device sample format {0:?} is not supported")]
    UnsupportedFormat(SampleFormat),
    #[error("device has no output channels")]
    NoChannels,
    #[error("no output device matches {0:?}")]
    NotFound(String),
    #[error("audio backend error: {0}")]
    Backend(#[from] cpal::Error),
    /// The backend panicked instead of returning an error (see [`guarded`]).
    #[error("the system audio service is unavailable: {0}")]
    Unavailable(String),
}

pub type Result<T, E = OutputError> = std::result::Result<T, E>;

/// Volume and balance, shared between the UI and the audio callback.
#[derive(Debug)]
pub struct Controls {
    volume: AtomicU32,
    balance: AtomicU32,
    /// Callbacks that ran, for diagnostics and tests.
    callbacks: AtomicU64,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            volume: AtomicU32::new(1.0f32.to_bits()),
            balance: AtomicU32::new(0.0f32.to_bits()),
            callbacks: AtomicU64::new(0),
        }
    }
}

impl Controls {
    /// Volume from 0.0 (silent) to 1.0 (full). Values outside are clamped.
    pub fn set_volume(&self, v: f32) {
        let v = if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Balance from -1.0 (left only) through 0.0 (center) to 1.0 (right only).
    pub fn set_balance(&self, b: f32) {
        let b = if b.is_finite() {
            b.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.balance.store(b.to_bits(), Ordering::Relaxed);
    }

    pub fn balance(&self) -> f32 {
        f32::from_bits(self.balance.load(Ordering::Relaxed))
    }

    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }
}

/// Left and right gain for a volume and balance. Balance attenuates the opposite side
/// linearly and leaves the favored side at full volume.
pub fn channel_gains(volume: f32, balance: f32) -> (f32, f32) {
    let left = volume * (1.0 - balance.max(0.0));
    let right = volume * (1.0 + balance.min(0.0));
    (left, right)
}

/// Applies gains to interleaved stereo in place.
pub fn apply_gains(stereo: &mut [f32], (left, right): (f32, f32)) {
    for frame in stereo.as_chunks_mut::<2>().0 {
        frame[0] *= left;
        frame[1] *= right;
    }
}

/// Writes stereo frames into a device buffer with `channels` channels.
/// Mono devices get the average; extra channels get silence. Samples are clipped to
/// [-1.0, 1.0], since decoded float audio can overshoot.
pub fn spread_to_device<T>(stereo: &[f32], out: &mut [T], channels: usize)
where
    T: FromSample<f32> + Copy,
{
    let silence = T::from_sample_(0.0);
    let conv = |s: f32| T::from_sample_(s.clamp(-1.0, 1.0));
    for (src, dst) in stereo
        .as_chunks::<2>()
        .0
        .iter()
        .zip(out.chunks_exact_mut(channels))
    {
        if channels == 1 {
            dst[0] = conv((src[0] + src[1]) * 0.5);
        } else {
            dst[0] = conv(src[0]);
            dst[1] = conv(src[1]);
            for d in &mut dst[2..] {
                *d = silence;
            }
        }
    }
}

/// Runs a call into cpal, turning a panic into [`OutputError::Unavailable`]. cpal's WASAPI host
/// unwraps the result of creating its device enumerator, so on a Windows system without a
/// working audio endpoint service every device query panics.
fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "the audio backend panicked".to_owned());
        log::warn!("audio backend panicked: {msg}");
        Err(OutputError::Unavailable(msg))
    })
}

fn device_id(device: &cpal::Device) -> Option<String> {
    device.id().ok().map(|id| id.id().to_owned())
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_owned())
}

/// Available outputs as (id, human-readable name) pairs.
pub fn list_outputs() -> Result<Vec<(String, String)>> {
    guarded(|| {
        let host = cpal::default_host();
        Ok(host
            .output_devices()?
            .filter_map(|d| {
                let id = device_id(&d)?;
                let name = device_name(&d).unwrap_or_else(|| id.clone());
                Some((id, name))
            })
            .collect())
    })
}

/// An output device and the stream configuration chosen for it.
pub struct OutputDevice {
    device: cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
}

impl std::fmt::Debug for OutputDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputDevice")
            .field("config", &self.config)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl OutputDevice {
    /// The system default output, at its default rate. Prefers an `f32` stereo configuration
    /// at that rate when the device offers one.
    pub fn default_output() -> Result<Self> {
        guarded(|| {
            let host = cpal::default_host();
            let device = host.default_output_device().ok_or(OutputError::NoDevice)?;
            Self::from_device(device)
        })
    }

    /// The output whose id equals `query`, or failing that, whose id or name contains it
    /// (case-insensitive). See [`list_outputs`] for what is available.
    pub fn named(query: &str) -> Result<Self> {
        guarded(|| Self::find(query))
    }

    fn find(query: &str) -> Result<Self> {
        let host = cpal::default_host();
        let mut devices: Vec<cpal::Device> = host.output_devices()?.collect();
        let wanted = query.to_lowercase();
        let exact = devices
            .iter()
            .position(|d| device_id(d).is_some_and(|id| id == query));
        let fuzzy = || {
            devices.iter().position(|d| {
                let (id, name) = (device_id(d), device_name(d));
                [id, name]
                    .into_iter()
                    .flatten()
                    .any(|s| s.to_lowercase().contains(&wanted))
            })
        };
        let index = exact
            .or_else(fuzzy)
            .ok_or_else(|| OutputError::NotFound(query.to_owned()))?;
        Self::from_device(devices.swap_remove(index))
    }

    /// Opens `name` if given; otherwise the default output, falling back to the sound
    /// servers' own ALSA devices (`pipewire`, then `pulse`) when the default cannot be opened,
    /// which happens on Linux when ALSA's default goes straight to hardware a server holds.
    pub fn open(name: Option<&str>) -> Result<Self> {
        if let Some(n) = name {
            return Self::named(n);
        }
        match Self::default_output() {
            Ok(d) => Ok(d),
            Err(default_err) => {
                for fallback in ["pipewire", "pulse"] {
                    if let Ok(d) = Self::named(fallback) {
                        log::info!("default device unusable ({default_err}), using {fallback}");
                        return Ok(d);
                    }
                }
                Err(default_err)
            }
        }
    }

    fn from_device(device: cpal::Device) -> Result<Self> {
        let default = device.default_output_config()?;
        let rate = default.sample_rate();
        let preferred = device
            .supported_output_configs()
            .ok()
            .and_then(|mut configs| {
                configs.find(|c| {
                    c.sample_format() == SampleFormat::F32
                        && c.channels() == 2
                        && c.min_sample_rate() <= rate
                        && rate <= c.max_sample_rate()
                })
            });
        let chosen = match preferred {
            Some(c) => c.with_sample_rate(rate),
            None => default,
        };
        if chosen.channels() == 0 {
            return Err(OutputError::NoChannels);
        }
        Ok(Self {
            device,
            format: chosen.sample_format(),
            config: chosen.config(),
        })
    }

    /// Backend id of the device, e.g. `pipewire` or `hw:CARD=PCH,DEV=0` on ALSA.
    pub fn id(&self) -> Option<String> {
        device_id(&self.device)
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    pub fn sample_format(&self) -> SampleFormat {
        self.format
    }

    /// Starts the stream. `render` fills interleaved stereo `f32` at [`Self::sample_rate`].
    pub fn start<F>(self, render: F) -> Result<AudioOutput>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let controls = Arc::new(Controls::default());
        let stream = match self.format {
            SampleFormat::F32 => self.build::<f32, F>(render, &controls)?,
            SampleFormat::F64 => self.build::<f64, F>(render, &controls)?,
            SampleFormat::I16 => self.build::<i16, F>(render, &controls)?,
            SampleFormat::I32 => self.build::<i32, F>(render, &controls)?,
            SampleFormat::U16 => self.build::<u16, F>(render, &controls)?,
            SampleFormat::U8 => self.build::<u8, F>(render, &controls)?,
            other => return Err(OutputError::UnsupportedFormat(other)),
        };
        stream.play()?;
        Ok(AudioOutput {
            _stream: stream,
            controls,
            sample_rate: self.config.sample_rate,
        })
    }

    fn build<T, F>(&self, mut render: F, controls: &Arc<Controls>) -> Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let channels = usize::from(self.config.channels);
        let controls = Arc::clone(controls);
        let mut scratch = vec![0.0f32; SCRATCH_FRAMES * 2];
        let stream = self.device.build_output_stream::<T, _, _>(
            self.config,
            move |data: &mut [T], _info| {
                controls.callbacks.fetch_add(1, Ordering::Relaxed);
                let gains = channel_gains(controls.volume(), controls.balance());
                for chunk in data.chunks_mut(SCRATCH_FRAMES * channels) {
                    let frames = chunk.len() / channels;
                    let stereo = &mut scratch[..frames * 2];
                    render(stereo);
                    apply_gains(stereo, gains);
                    spread_to_device(stereo, chunk, channels);
                }
            },
            |err| log::error!("audio stream error: {err}"),
            None,
        )?;
        Ok(stream)
    }
}

/// A running output stream. Dropping it stops playback.
pub struct AudioOutput {
    _stream: cpal::Stream,
    controls: Arc<Controls>,
    sample_rate: u32,
}

impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("sample_rate", &self.sample_rate)
            .field("controls", &self.controls)
            .finish_non_exhaustive()
    }
}

impl AudioOutput {
    pub fn controls(&self) -> &Arc<Controls> {
        &self.controls
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_panics_become_errors() {
        // cpal's WASAPI host unwraps COM errors (for example when the audio endpoint service
        // is missing); those must reach the caller as errors, not take the process down.
        let r: Result<()> = guarded(|| panic!("CoCreateInstance failed"));
        match r {
            Err(OutputError::Unavailable(msg)) => assert!(msg.contains("CoCreateInstance")),
            other => panic!("expected Unavailable, got {other:?}"),
        }
        let formatted: Result<()> = guarded(|| panic!("code {}", 7));
        assert!(matches!(formatted, Err(OutputError::Unavailable(m)) if m == "code 7"));
        assert_eq!(guarded(|| Ok(5)).ok(), Some(5));
    }

    #[test]
    fn gains_center_and_extremes() {
        assert_eq!(channel_gains(1.0, 0.0), (1.0, 1.0));
        assert_eq!(channel_gains(0.5, 0.0), (0.5, 0.5));
        assert_eq!(channel_gains(1.0, 1.0), (0.0, 1.0));
        assert_eq!(channel_gains(1.0, -1.0), (1.0, 0.0));
        assert_eq!(channel_gains(0.8, 0.5), (0.4, 0.8));
    }

    #[test]
    fn controls_clamp_and_reject_nan() {
        let c = Controls::default();
        c.set_volume(3.0);
        assert_eq!(c.volume(), 1.0);
        c.set_volume(-1.0);
        assert_eq!(c.volume(), 0.0);
        c.set_volume(f32::NAN);
        assert_eq!(c.volume(), 1.0);
        c.set_balance(-7.0);
        assert_eq!(c.balance(), -1.0);
        c.set_balance(f32::INFINITY);
        assert_eq!(c.balance(), 0.0);
    }

    #[test]
    fn apply_gains_scales_each_side() {
        let mut s = [1.0, 1.0, -0.5, 0.5];
        apply_gains(&mut s, (0.5, 0.25));
        assert_eq!(s, [0.5, 0.25, -0.25, 0.125]);
    }

    #[test]
    fn spread_to_mono_stereo_and_surround() {
        let stereo = [0.5, -0.5, 1.0, 0.0];
        let mut mono = [0.0f32; 2];
        spread_to_device(&stereo, &mut mono, 1);
        assert_eq!(mono, [0.0, 0.5]);

        let mut st = [9.0f32; 4];
        spread_to_device(&stereo, &mut st, 2);
        assert_eq!(st, stereo);

        let mut six = [9.0f32; 12];
        spread_to_device(&stereo, &mut six, 6);
        assert_eq!(&six[..6], &[0.5, -0.5, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(&six[6..], &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn spread_converts_to_integer_formats() {
        let stereo = [1.0, -1.0, 1.7, -3.0, 0.0, 0.0];
        let mut out = [0i16; 6];
        spread_to_device(&stereo, &mut out, 2);
        assert_eq!(out, [i16::MAX, i16::MIN, i16::MAX, i16::MIN, 0, 0]);
    }

    /// Runs against the container's ALSA null device. Skips where no output exists at all.
    #[test]
    fn default_device_pulls_audio() {
        let device = match OutputDevice::default_output() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("no output device, skipping: {e}");
                return;
            }
        };
        assert!(device.sample_rate() > 0);
        let rendered = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&rendered);
        let output = device
            .start(move |buf| {
                buf.fill(0.0);
                counter.fetch_add(buf.len() as u64 / 2, Ordering::Relaxed);
            })
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while rendered.load(Ordering::Relaxed) == 0 {
            assert!(std::time::Instant::now() < deadline, "callback never ran");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(output.controls().callbacks() > 0);
    }
}
