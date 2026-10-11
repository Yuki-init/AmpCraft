//! DSP: 10-band equalizer and preamp. Spectrum and oscilloscope taps come with the
//! visualizer.

pub mod eq;

pub use eq::{BAND_FREQUENCIES, EqControls, EqSettings, Equalizer, MAX_DB};
