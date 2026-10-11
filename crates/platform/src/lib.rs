//! Platform traits and the X11 backend.
//!
//! The rest of the app talks to windows only through [`Platform`]. Windows are borderless:
//! the skin draws the title bar, and dragging is done by the app (see `amp-dock`), not by the
//! window manager. Pixels are `0xAARRGGBB` rows, the same layout as `amp_skin::Bitmap`.

use std::time::Duration;

pub use amp_dock::{Point, Rect, Size};

pub mod x11;

/// Identifies a window created by a [`Platform`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u32);

/// What kind of window to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowKind {
    /// The main window: shown in the taskbar.
    #[default]
    Main,
    /// Equalizer, playlist and other panels: kept out of the taskbar.
    Panel,
}

/// Parameters for a new window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSpec {
    pub title: String,
    pub rect: Rect,
    pub kind: WindowKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Other(u8),
}

/// Input and window events. `local` is relative to the window, `root` to the whole desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ButtonDown {
        window: WindowId,
        button: MouseButton,
        local: Point,
        root: Point,
    },
    ButtonUp {
        window: WindowId,
        button: MouseButton,
        local: Point,
        root: Point,
    },
    /// Pointer motion. Consecutive motions are coalesced to the latest.
    Motion {
        window: WindowId,
        local: Point,
        root: Point,
    },
    /// Part of the window needs redrawing.
    Expose {
        window: WindowId,
    },
    /// The user asked to close the window (window manager close).
    CloseRequested {
        window: WindowId,
    },
    Focus {
        window: WindowId,
        focused: bool,
    },
    /// A key, as an X11 keysym (also used as the portable key code; see [`keys`]).
    Key {
        window: WindowId,
        keysym: u32,
        pressed: bool,
    },
    /// The window's position or size changed, for whatever reason.
    Configured {
        window: WindowId,
        rect: Rect,
    },
}

/// Common keysyms.
pub mod keys {
    pub const ESCAPE: u32 = 0xff1b;
    pub const RETURN: u32 = 0xff0d;
    pub const SPACE: u32 = 0x0020;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("cannot connect to the display: {0}")]
    Connect(String),
    #[error("display connection failed: {0}")]
    Connection(String),
    #[error("unsupported display: {0}")]
    Unsupported(String),
    #[error("unknown window {0:?}")]
    UnknownWindow(WindowId),
    #[error("pixel buffer is {got} pixels, expected {expected}")]
    BadBuffer { got: usize, expected: usize },
}

pub type Result<T, E = PlatformError> = std::result::Result<T, E>;

/// Window system access. One instance per process.
pub trait Platform {
    fn create_window(&mut self, spec: &WindowSpec) -> Result<WindowId>;
    fn destroy_window(&mut self, id: WindowId) -> Result<()>;
    fn set_visible(&mut self, id: WindowId, visible: bool) -> Result<()>;
    /// Moves the window's top-left corner to `pos` in desktop coordinates.
    fn move_window(&mut self, id: WindowId, pos: Point) -> Result<()>;
    fn resize_window(&mut self, id: WindowId, size: Size) -> Result<()>;
    /// Iconifies the window.
    fn minimize(&mut self, id: WindowId) -> Result<()>;
    /// Draws a full frame of `size.w * size.h` pixels at the window's top-left.
    fn present(&mut self, id: WindowId, size: Size, pixels: &[u32]) -> Result<()>;
    /// Restricts the window's visible and clickable area to `rects` (window coordinates).
    /// `None` restores a plain rectangle.
    fn set_shape(&mut self, id: WindowId, rects: Option<&[Rect]>) -> Result<()>;
    /// Where the pointer is now, in desktop coordinates.
    fn pointer(&self) -> Result<Point>;
    /// Usable area of each monitor, in desktop coordinates.
    fn screens(&self) -> Result<Vec<Rect>>;
    /// Next event, waiting at most `timeout` (`None` waits forever).
    fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>>;
    /// Sends buffered requests to the display.
    fn flush(&mut self) -> Result<()>;
}
