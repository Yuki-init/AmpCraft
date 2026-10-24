//! Media controls: the desktop's media keys, its now-playing panel and remote tools.
//!
//! One interface over each desktop's own service: MPRIS on the D-Bus session bus on Linux,
//! and on Windows the System Media Transport Controls, which drive the media flyout and
//! receive the keyboard's media keys. Requests arrive as [`Command`]s that the UI loop drains
//! with [`MediaControls::commands`]; the app publishes its [`State`] with
//! [`MediaControls::update`], and only what changed is passed on.

use std::ffi::c_void;

#[cfg(target_os = "linux")]
pub mod mpris;
#[cfg(windows)]
// Win32 FFI is the one place unsafe is allowed until the egui port replaces it (docs/gaps.md).
#[allow(unsafe_code)]
mod smtc;
#[cfg(any(windows, test))]
mod smtc_data;

/// How far a fast forward or rewind button jumps, in microseconds (the arrow keys' step).
pub const SEEK_STEP_US: i64 = 5_000_000;

/// What the desktop or a remote client asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    /// Relative seek in microseconds.
    Seek(i64),
    /// Absolute position in microseconds, for the given track id.
    SetPosition(u64, i64),
    /// 0.0 to 1.0.
    SetVolume(f64),
    SetShuffle(bool),
    /// True for any loop mode other than None.
    SetLoop(bool),
    Raise,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

/// The player state shown to the desktop.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    pub status: Status,
    /// Queue id of the current track, if any.
    pub track_id: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub length_us: Option<i64>,
    pub position_us: i64,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MediaControlsError {
    #[error("no media controls on this platform")]
    Unsupported,
    #[error("{0}")]
    Service(String),
}

#[cfg(target_os = "linux")]
type Backend = mpris::Mpris;
#[cfg(windows)]
type Backend = smtc::Smtc;
#[cfg(not(any(target_os = "linux", windows)))]
#[derive(Debug)]
struct Backend;
#[cfg(not(any(target_os = "linux", windows)))]
impl Backend {
    fn commands(&self) -> Vec<Command> {
        Vec::new()
    }

    fn update(&mut self, _state: State, _seeked: bool) {}
}

/// The running media controls service.
#[derive(Debug)]
pub struct MediaControls(Backend);

impl MediaControls {
    /// Starts the desktop's media controls for the player. `window` is the main window's
    /// native handle: on Windows the controls belong to a window (its HWND); elsewhere it is
    /// ignored.
    pub fn start(window: Option<*mut c_void>) -> Result<Self, MediaControlsError> {
        #[cfg(target_os = "linux")]
        {
            let _ = window;
            mpris::Mpris::start()
                .map(Self)
                .map_err(|e| MediaControlsError::Service(format!("MPRIS: {e}")))
        }
        #[cfg(windows)]
        {
            let hwnd =
                window.ok_or_else(|| MediaControlsError::Service("no main window".into()))?;
            smtc::Smtc::start(hwnd).map(Self)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = window;
            Err(MediaControlsError::Unsupported)
        }
    }

    /// Commands received since the last call.
    pub fn commands(&self) -> Vec<Command> {
        self.0.commands()
    }

    /// Publishes new state. `seeked` reports a jump in position (a seek or a track restart)
    /// so the desktop resyncs its progress bar.
    pub fn update(&mut self, state: State, seeked: bool) {
        self.0.update(state, seeked);
    }
}
