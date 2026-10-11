//! The main window's visualizer: 19-bar spectrum with falling peaks, or an oscilloscope,
//! drawn in the skin's viscolor.txt palette (0 background, 1 grid dots, 2..17 spectrum top to
//! bottom, 18..22 oscilloscope, 23 peaks).

use amp_skin::{Bitmap, Rect};

/// Where the visualizer sits in the main window.
pub const AREA: Rect = Rect::new(24, 43, 76, 16);
pub const BARS: usize = 19;
const BAR_W: i32 = 3;
const BAR_STEP: i32 = 4;
/// Bars fall this many pixels per millisecond (a full bar in about 400 ms).
const BAR_FALL_PER_MS: f32 = 0.04;
/// Peaks hold this long, then fall slower than bars.
const PEAK_HOLD_MS: f32 = 400.0;
const PEAK_FALL_PER_MS: f32 = 0.015;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VisMode {
    #[default]
    Spectrum,
    Oscilloscope,
    Off,
}

#[derive(Debug, Clone)]
pub struct Visualizer {
    pub mode: VisMode,
    /// Bar heights in pixels (0..=16).
    bars: [f32; BARS],
    peaks: [f32; BARS],
    peak_age_ms: [f32; BARS],
    /// Oscilloscope samples, -1.0..=1.0, one per column.
    wave: [f32; AREA.w as usize],
}

impl Default for Visualizer {
    fn default() -> Self {
        Self {
            mode: VisMode::Spectrum,
            bars: [0.0; BARS],
            peaks: [0.0; BARS],
            peak_age_ms: [0.0; BARS],
            wave: [0.0; AREA.w as usize],
        }
    }
}

impl Visualizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spectrum, then oscilloscope, then off, as clicking the display does.
    pub fn cycle(&mut self) {
        self.mode = match self.mode {
            VisMode::Spectrum => VisMode::Oscilloscope,
            VisMode::Oscilloscope => VisMode::Off,
            VisMode::Off => VisMode::Spectrum,
        };
    }

    /// Whether anything is still moving (bars or peaks above zero).
    pub fn is_active(&self) -> bool {
        self.bars.iter().chain(&self.peaks).any(|v| *v > 0.0) || self.wave.iter().any(|v| *v != 0.0)
    }

    /// Feeds new levels (0.0..=1.0 per bar) and the newest waveform samples, `dt_ms` after the
    /// previous update. Pass zeros when nothing plays and the bars fall away.
    pub fn update(&mut self, levels: &[f32; BARS], samples: &[f32], dt_ms: f32) {
        let h = AREA.h as f32;
        let state = self
            .bars
            .iter_mut()
            .zip(self.peaks.iter_mut())
            .zip(self.peak_age_ms.iter_mut());
        for (((bar, peak), age), level) in state.zip(levels) {
            let target = level.clamp(0.0, 1.0) * h;
            *bar = target.max(*bar - BAR_FALL_PER_MS * dt_ms).max(0.0);
            if *bar >= *peak {
                *peak = *bar;
                *age = 0.0;
            } else {
                *age += dt_ms;
                if *age > PEAK_HOLD_MS {
                    *peak = (*peak - PEAK_FALL_PER_MS * dt_ms).max(0.0);
                }
            }
        }
        let n = self.wave.len();
        if samples.is_empty() {
            self.wave = [0.0; AREA.w as usize];
        } else {
            for (x, w) in self.wave.iter_mut().enumerate() {
                *w = samples[x * samples.len() / n].clamp(-1.0, 1.0);
            }
        }
    }

    /// Draws into the main window bitmap (1x) at [`AREA`].
    pub fn render(&self, b: &mut Bitmap, colors: &[u32; 24]) {
        if self.mode == VisMode::Off {
            return;
        }
        for y in 0..AREA.h {
            for x in 0..AREA.w {
                let c = if x % 2 == 1 && y % 2 == 1 {
                    colors[1]
                } else {
                    colors[0]
                };
                b.set(AREA.x + x, AREA.y + y, c);
            }
        }
        match self.mode {
            VisMode::Spectrum => {
                for (i, (bar, peak)) in self.bars.iter().zip(&self.peaks).enumerate() {
                    let x0 = AREA.x + i as i32 * BAR_STEP;
                    let height = bar.round() as i32;
                    for y in (AREA.h - height).max(0)..AREA.h {
                        for x in x0..x0 + BAR_W {
                            b.set(x, AREA.y + y, colors[2 + y as usize]);
                        }
                    }
                    let peak = peak.round() as i32;
                    if peak > 0 {
                        let y = AREA.y + (AREA.h - peak).max(0);
                        for x in x0..x0 + BAR_W {
                            b.set(x, y, colors[23]);
                        }
                    }
                }
            }
            VisMode::Oscilloscope => {
                let mid = AREA.h / 2;
                for (x, s) in self.wave.iter().enumerate() {
                    let y = (mid - (s * mid as f32).round() as i32).clamp(0, AREA.h - 1);
                    let distance = (y - mid).unsigned_abs() as usize;
                    b.set(
                        AREA.x + x as i32,
                        AREA.y + y,
                        colors[18 + (distance / 2).min(4)],
                    );
                }
            }
            VisMode::Off => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> [u32; 24] {
        std::array::from_fn(|i| 0xFF00_0000 | i as u32)
    }

    #[test]
    fn bars_rise_instantly_and_fall_smoothly() {
        let mut v = Visualizer::new();
        let mut levels = [0.0; BARS];
        levels[3] = 1.0;
        v.update(&levels, &[], 33.0);
        assert_eq!(v.bars[3], 16.0);
        v.update(&[0.0; BARS], &[], 100.0);
        assert!((v.bars[3] - 12.0).abs() < 1e-3, "{}", v.bars[3]);
        assert_eq!(v.peaks[3], 16.0, "peak holds");
        for _ in 0..40 {
            v.update(&[0.0; BARS], &[], 100.0);
        }
        assert_eq!(v.bars[3], 0.0);
        assert_eq!(v.peaks[3], 0.0);
        assert!(!v.is_active());
    }

    #[test]
    fn spectrum_render_colors_by_row_and_draws_peaks() {
        let mut v = Visualizer::new();
        let mut levels = [0.0; BARS];
        levels[0] = 0.5;
        v.update(&levels, &[], 33.0);
        let mut b = Bitmap::new(275, 116, 0);
        v.render(&mut b, &palette());
        // Bar 0 is 8 px tall: rows 8..16 of the area, each in its row's color.
        assert_eq!(b.get(AREA.x, AREA.y + 15), Some(0xFF00_0000 | 17));
        assert_eq!(b.get(AREA.x + 2, AREA.y + 9), Some(0xFF00_0000 | 11));
        // The peak sits on top of the bar.
        assert_eq!(b.get(AREA.x, AREA.y + 8), Some(0xFF00_0000 | 23));
        // Background and grid dots elsewhere.
        assert_eq!(b.get(AREA.x + 40, AREA.y), Some(0xFF00_0000));
        assert_eq!(b.get(AREA.x + 41, AREA.y + 1), Some(0xFF00_0000 | 1));
        // Nothing outside the area.
        assert_eq!(b.get(AREA.x + AREA.w, AREA.y), Some(0));
    }

    #[test]
    fn oscilloscope_and_off() {
        let mut v = Visualizer::new();
        v.cycle();
        assert_eq!(v.mode, VisMode::Oscilloscope);
        let wave: Vec<f32> = (0..152).map(|i| if i < 76 { 1.0 } else { -1.0 }).collect();
        v.update(&[0.0; BARS], &wave, 33.0);
        let mut b = Bitmap::new(275, 116, 0);
        v.render(&mut b, &palette());
        assert_eq!(b.get(AREA.x, AREA.y), Some(0xFF00_0000 | 22), "top for +1");
        assert_eq!(
            b.get(AREA.x + 75, AREA.y + 15),
            Some(0xFF00_0000 | 21),
            "bottom for -1"
        );
        v.cycle();
        assert_eq!(v.mode, VisMode::Off);
        let mut off = Bitmap::new(275, 116, 7);
        v.render(&mut off, &palette());
        assert!(off.pixels().iter().all(|p| *p == 7));
        v.cycle();
        assert_eq!(v.mode, VisMode::Spectrum);
    }
}
