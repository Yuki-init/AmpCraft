//! An audio "output" that discards samples, paced in real time or as fast as possible.
//! Used for headless runs and machines without sound hardware.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use amp_audio_out::Controls;
use amp_core::{OUTPUT_CHANNELS, PcmSource};

/// Stops its thread when dropped.
#[derive(Debug)]
pub struct NullSink {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    controls: Arc<Controls>,
}

impl NullSink {
    /// Starts pulling from `source`: in 10 ms periods when `realtime`, otherwise as fast as
    /// audio is decoded.
    pub fn start(mut source: PcmSource, realtime: bool) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let rate = source.sample_rate();
        let handle = std::thread::Builder::new()
            .name("null-sink".into())
            .spawn(move || {
                let period = Duration::from_millis(10);
                let frames = (rate as usize / 100).max(1);
                let mut buf = vec![0.0f32; frames * OUTPUT_CHANNELS];
                let mut next = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    let n = source.fill(&mut buf);
                    if realtime {
                        next += period;
                        if let Some(wait) = next.checked_duration_since(Instant::now()) {
                            std::thread::sleep(wait);
                        }
                    } else if n == 0 {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
            controls: Arc::new(Controls::default()),
        })
    }

    /// Volume and balance controls (accepted and ignored, for symmetry with real outputs).
    pub fn controls(&self) -> &Arc<Controls> {
        &self.controls
    }
}

impl Drop for NullSink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
