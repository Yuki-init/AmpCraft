//! DSP: 10-band equalizer and preamp, and the visualizer's audio tap and spectrum analysis.

pub mod analyzer;
pub mod eq;
pub mod presets;

pub use analyzer::{AnalyzerReader, AnalyzerTap, BARS, FFT_SIZE, Spectrum, analyzer};
pub use eq::{BAND_FREQUENCIES, EqControls, EqSettings, Equalizer, MAX_DB};
pub use presets::{EqPreset, builtin as builtin_presets, eqf};
