//! Minimal stderr logger. Level from `OPENAMP_LOG` (error, warn, info, debug, trace),
//! default warn.

struct StderrLogger;

/// Installs the logger once; later calls do nothing.
pub fn install() {
    let level = std::env::var("OPENAMP_LOG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(log::LevelFilter::Warn);
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(level);
    }
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
