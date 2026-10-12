//! Platform traits and the window backends: X11 everywhere except Windows, Win32 on Windows.
//!
//! The rest of the app talks to windows only through [`Platform`], and gets the backend for
//! the OS it was built for as [`NativePlatform`]. Windows are borderless: the skin draws the
//! title bar, and dragging is done by the app (see `amp-dock`), not by the window manager.
//! Pixels are `0xAARRGGBB` rows, the same layout as `amp_skin::Bitmap`.

use std::time::Duration;

pub use amp_dock::{Point, Rect, Size};

pub mod dirs;
#[cfg(windows)]
pub mod win32;
pub mod win32_keys;
#[cfg(not(windows))]
pub mod x11;

pub use dirs::AppDirs;

/// The window backend for the OS this was built for. Each has a `connect()` constructor.
#[cfg(not(windows))]
pub type NativePlatform = x11::X11Platform;
/// The window backend for the OS this was built for. Each has a `connect()` constructor.
#[cfg(windows)]
pub type NativePlatform = win32::Win32Platform;

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
    /// A menu: shown without the window manager, above everything, until hidden.
    Popup,
    /// A dialog (preferences): framed by us, kept with its owner by the window manager.
    Dialog,
}

/// Mouse pointer shapes the app asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CursorShape {
    #[default]
    Arrow,
    /// Over a window's right edge: drag to change the width.
    ResizeRight,
    /// Over the bottom edge: drag to change the height.
    ResizeBottom,
    /// Over the bottom-right corner: drag to change both.
    ResizeCorner,
}

/// Parameters for a new window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSpec {
    pub title: String,
    pub rect: Rect,
    pub kind: WindowKind,
    /// The window this one belongs to (panels belong to the main window). The window manager
    /// keeps an owned window stacked with its owner and raises them together.
    pub owner: Option<WindowId>,
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

/// Keyboard modifiers held during an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Input and window events. `local` is relative to the window, `root` to the whole desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ButtonDown {
        window: WindowId,
        button: MouseButton,
        local: Point,
        root: Point,
        mods: Modifiers,
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
        mods: Modifiers,
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
    /// Asks for the window to be stacked above other windows. Does not change focus.
    fn raise(&mut self, id: WindowId) -> Result<()>;
    /// Draws a full frame of `size.w * size.h` pixels at the window's top-left.
    fn present(&mut self, id: WindowId, size: Size, pixels: &[u32]) -> Result<()>;
    /// Draws only rows `first..last` of a full frame (the rest of the window keeps what it
    /// shows). The default sends the whole frame.
    fn present_rows(
        &mut self,
        id: WindowId,
        size: Size,
        pixels: &[u32],
        first: usize,
        last: usize,
    ) -> Result<()> {
        let _ = (first, last);
        self.present(id, size, pixels)
    }

    /// Restricts the window's visible and clickable area to `rects` (window coordinates).
    /// `None` restores a plain rectangle.
    fn set_shape(&mut self, id: WindowId, rects: Option<&[Rect]>) -> Result<()>;
    /// Where the pointer is now, in desktop coordinates.
    fn pointer(&self) -> Result<Point>;
    /// Usable area of each monitor, in desktop coordinates.
    fn screens(&self) -> Result<Vec<Rect>>;
    /// The desktop's font DPI setting (96 is unscaled), when it has one.
    fn dpi(&self) -> Option<u32>;
    /// Keeps the window above normal windows (also applies when it is shown later).
    fn set_always_on_top(&mut self, id: WindowId, on: bool) -> Result<()>;
    /// Sends all pointer and keyboard input to `id` (a shown popup) until
    /// [`Platform::release_input`]. Returns false if another program holds the input.
    fn grab_input(&mut self, id: WindowId) -> Result<bool>;
    /// Ends [`Platform::grab_input`].
    fn release_input(&mut self) -> Result<()>;
    /// The pointer shape while over `id`, from the desktop's cursor theme when there is one.
    fn set_cursor(&mut self, id: WindowId, cursor: CursorShape) -> Result<()>;
    /// Next event, waiting at most `timeout` (`None` waits forever).
    fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>>;
    /// Sends buffered requests to the display.
    fn flush(&mut self) -> Result<()>;
}

/// Starting UI scale (1 to 3) for a monitor whose usable height is `height` pixels, given the
/// desktop's DPI setting. Whichever of the two asks for more wins: 1440p-class screens and
/// up get 2x, 5K-class get 3x, and so do desktops scaled to 175% / 275% and more.
pub fn auto_scale(height: i32, dpi: Option<u32>) -> u8 {
    let by_height = match height {
        ..1300 => 1,
        1300..2600 => 2,
        _ => 3,
    };
    let by_dpi = match dpi.unwrap_or(96) {
        ..168 => 1,
        168..264 => 2,
        _ => 3,
    };
    by_height.max(by_dpi)
}

#[cfg(test)]
mod auto_scale_tests {
    use super::auto_scale;

    #[test]
    fn picks_scale_from_height_and_dpi() {
        assert_eq!(auto_scale(1080, None), 1);
        assert_eq!(auto_scale(1024, Some(96)), 1);
        // 1440p and 2560x1600 with a panel taking some height.
        assert_eq!(auto_scale(1400, None), 2);
        assert_eq!(auto_scale(1560, Some(120)), 2);
        // 4K and 5K.
        assert_eq!(auto_scale(2120, None), 2);
        assert_eq!(auto_scale(2840, None), 3);
        // A logical 1080p desktop scaled to 200% / 300%.
        assert_eq!(auto_scale(1080, Some(192)), 2);
        assert_eq!(auto_scale(1080, Some(288)), 3);
        assert_eq!(auto_scale(1080, Some(144)), 1);
    }
}
