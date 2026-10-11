//! Visualization taps: a real-time safe tap in the audio callback, and spectrum and
//! waveform analysis on the UI side.
//!
//! The audio thread pushes a mono mix into a lock-free ring; when the ring is full new
//! samples are dropped (the UI is behind, and the visualizer only needs recent audio). The
//! UI drains it into a short history and analyzes the newest samples each frame.

use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Number of spectrum bars in the classic main-window analyzer.
pub const BARS: usize = 19;
/// Samples analyzed per spectrum frame.
pub const FFT_SIZE: usize = 512;
/// Mono samples of history kept on the UI side.
const HISTORY: usize = 2048;

/// The audio-thread side.
#[derive(Debug)]
pub struct AnalyzerTap {
    producer: Producer<f32>,
}

impl AnalyzerTap {
    /// Pushes interleaved stereo as mono. Never blocks or allocates.
    pub fn push(&mut self, stereo: &[f32]) {
        let frames = stereo.len() / 2;
        let n = frames.min(self.producer.slots());
        if n == 0 {
            return;
        }
        if let Ok(chunk) = self.producer.write_chunk_uninit(n) {
            chunk.fill_from_iter(
                stereo
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .take(n)
                    .map(|f| (f[0] + f[1]) * 0.5),
            );
        }
    }
}

/// The UI side: recent mono samples.
#[derive(Debug)]
pub struct AnalyzerReader {
    consumer: Consumer<f32>,
    history: Vec<f32>,
    /// Next write position in `history` (a ring).
    pos: usize,
}

/// Creates a connected tap and reader.
pub fn analyzer() -> (AnalyzerTap, AnalyzerReader) {
    let (producer, consumer) = RingBuffer::new(HISTORY * 2);
    (
        AnalyzerTap { producer },
        AnalyzerReader {
            consumer,
            history: vec![0.0; HISTORY],
            pos: 0,
        },
    )
}

impl AnalyzerReader {
    /// Moves everything the audio thread produced into the history.
    pub fn update(&mut self) {
        while let Ok(s) = self.consumer.pop() {
            self.history[self.pos] = s;
            self.pos = (self.pos + 1) % HISTORY;
        }
    }

    /// Fills `out` with the newest `out.len()` samples, oldest first.
    pub fn latest(&self, out: &mut [f32]) {
        let n = out.len().min(HISTORY);
        let start = (self.pos + HISTORY - n) % HISTORY;
        for (i, o) in out.iter_mut().take(n).enumerate() {
            *o = self.history[(start + i) % HISTORY];
        }
    }

    /// Forgets the history (after stop, so the display does not replay old audio).
    pub fn clear(&mut self) {
        self.update();
        self.history.fill(0.0);
    }
}

/// Turns samples into the classic spectrum bars.
pub struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buffer: Vec<Complex32>,
    /// FFT bin ranges for each bar, log-spaced.
    ranges: [(usize, usize); BARS],
}

impl std::fmt::Debug for Spectrum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spectrum")
            .field("ranges", &self.ranges)
            .finish_non_exhaustive()
    }
}

impl Spectrum {
    pub fn new(sample_rate: u32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        // Hann window.
        let window = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos()
            })
            .collect();
        let rate = sample_rate.max(1) as f32;
        let bin_hz = rate / FFT_SIZE as f32;
        let (lo, hi) = (40.0f32, 16_000.0f32.min(rate * 0.48));
        let mut ranges = [(1, 1); BARS];
        for (i, r) in ranges.iter_mut().enumerate() {
            let f0 = lo * (hi / lo).powf(i as f32 / BARS as f32);
            let f1 = lo * (hi / lo).powf((i + 1) as f32 / BARS as f32);
            let b0 = ((f0 / bin_hz).floor() as usize).clamp(1, FFT_SIZE / 2 - 1);
            let b1 = ((f1 / bin_hz).ceil() as usize).clamp(b0 + 1, FFT_SIZE / 2);
            *r = (b0, b1);
        }
        Self {
            fft,
            window,
            buffer: vec![Complex32::default(); FFT_SIZE],
            ranges,
        }
    }

    /// Bar levels in 0.0..=1.0 for the newest [`FFT_SIZE`] samples (more are ignored, fewer
    /// are zero-padded).
    pub fn bars(&mut self, samples: &[f32]) -> [f32; BARS] {
        for (i, b) in self.buffer.iter_mut().enumerate() {
            let s = samples.get(i).copied().unwrap_or(0.0);
            *b = Complex32::new(s * self.window[i], 0.0);
        }
        self.fft.process(&mut self.buffer);
        // Hann window halves the amplitude; scale so a full-scale sine reads 0 dB.
        let norm = 4.0 / FFT_SIZE as f32;
        let mut out = [0.0; BARS];
        for (bar, (b0, b1)) in out.iter_mut().zip(self.ranges) {
            let peak = self.buffer[b0..b1]
                .iter()
                .map(|c| c.norm() * norm)
                .fold(0.0f32, f32::max);
            let db = 20.0 * peak.max(1e-9).log10();
            // Show the top 60 dB.
            *bar = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .flat_map(|i| {
                let v = 0.8 * (2.0 * std::f32::consts::PI * freq * i as f32 / 44_100.0).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn tap_to_reader_keeps_newest_samples() {
        let (mut tap, mut reader) = analyzer();
        tap.push(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0]);
        reader.update();
        let mut out = [9.0; 4];
        reader.latest(&mut out);
        assert_eq!(out, [0.0, 0.5, 0.5, 0.0]);
        reader.clear();
        reader.latest(&mut out);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn full_ring_drops_instead_of_blocking() {
        let (mut tap, mut reader) = analyzer();
        let lots = vec![0.25f32; HISTORY * 10];
        tap.push(&lots);
        tap.push(&lots);
        reader.update();
        let mut out = [0.0; 8];
        reader.latest(&mut out);
        assert!(out.iter().all(|s| *s == 0.25));
    }

    #[test]
    fn sine_lights_the_matching_bar() {
        let mut spec = Spectrum::new(44_100);
        let (mut tap, mut reader) = analyzer();
        tap.push(&sine(1000.0, FFT_SIZE));
        reader.update();
        let mut samples = vec![0.0; FFT_SIZE];
        reader.latest(&mut samples);
        let bars = spec.bars(&samples);
        let loudest = bars
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        let (b0, b1) = spec.ranges[loudest];
        let bin_hz = 44_100.0 / FFT_SIZE as f32;
        assert!(
            (b0 as f32 * bin_hz..=b1 as f32 * bin_hz).contains(&1000.0),
            "bar {loudest}"
        );
        assert!(bars[loudest] > 0.9, "{}", bars[loudest]);
        assert!(bars[0] < 0.5 && bars[BARS - 1] < 0.5, "{bars:?}");
    }

    #[test]
    fn silence_is_all_zero_and_ranges_increase() {
        let mut spec = Spectrum::new(44_100);
        assert_eq!(spec.bars(&[0.0; FFT_SIZE]), [0.0; BARS]);
        for w in spec.ranges.windows(2) {
            assert!(w[0].0 <= w[1].0 && w[0].1 <= w[1].1);
        }
        // A low sample rate does not put bars past Nyquist.
        let low = Spectrum::new(8_000);
        assert!(low.ranges.iter().all(|(_, b1)| *b1 <= FFT_SIZE / 2));
    }
}
