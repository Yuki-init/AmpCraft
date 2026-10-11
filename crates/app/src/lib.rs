//! Shared pieces of the player binaries: logging, command-line file expansion and the
//! system-font text renderer.

pub mod config;
pub mod dialog;
pub mod files;
pub mod logging;
pub mod metadata;
#[cfg(target_os = "linux")]
pub mod mpris;
pub mod null_sink;
pub mod system_font;
