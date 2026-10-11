//! Frame timing for finding slow spots. Enabled with `OPENAMP_PERF=1`; prints a summary of
//! each section's average and worst time per frame every few seconds.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct FrameStats {
    enabled: bool,
    sections: BTreeMap<&'static str, (Duration, Duration, u32)>,
    frames: u32,
    since: Instant,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::from_env()
    }
}

impl FrameStats {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var_os("OPENAMP_PERF").is_some_and(|v| v != "0"),
            sections: BTreeMap::new(),
            frames: 0,
            since: Instant::now(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Runs `f`, adding its time to `section`.
    pub fn time<T>(&mut self, section: &'static str, f: impl FnOnce() -> T) -> T {
        if !self.enabled {
            return f();
        }
        let start = Instant::now();
        let out = f();
        self.record(section, start.elapsed());
        out
    }

    pub fn record(&mut self, section: &'static str, d: Duration) {
        if !self.enabled {
            return;
        }
        let e = self.sections.entry(section).or_default();
        e.0 += d;
        e.1 = e.1.max(d);
        e.2 += 1;
    }

    /// Call once per loop iteration; prints and resets every 5 seconds.
    pub fn frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.frames += 1;
        let elapsed = self.since.elapsed();
        if elapsed < Duration::from_secs(5) {
            return;
        }
        let mut line = format!(
            "[perf] {:.1} fps:",
            f64::from(self.frames) / elapsed.as_secs_f64()
        );
        for (name, (total, worst, n)) in &self.sections {
            let avg = total.as_secs_f64() * 1000.0 / f64::from((*n).max(1));
            line.push_str(&format!(
                " {name} {avg:.2}/{:.2}ms",
                worst.as_secs_f64() * 1000.0
            ));
        }
        eprintln!("{line}");
        self.sections.clear();
        self.frames = 0;
        self.since = Instant::now();
    }
}
