//! Win32 backend. Not implemented yet: [`Win32Platform::connect`] fails with
//! [`PlatformError::Unsupported`], so the app builds for Windows and exits with a clear
//! message instead of failing to compile.
//!
//! Plan, mirroring the X11 backend:
//! - Borderless popup windows (`WS_POPUP`) with our own dragging; panels are owned windows
//!   (`hWndParent` = main window) with `WS_EX_TOOLWINDOW` so they stay out of the taskbar.
//! - Frames are `0xAARRGGBB` rows, which is the memory layout of a top-down 32-bit DIB, so
//!   `present` is a `StretchDIBits`/`SetDIBitsToDevice` call with no conversion.
//! - Shapes from region.txt become a region of rectangles (`CreateRectRgn` + `CombineRgn`)
//!   applied with `SetWindowRgn`.
//! - Pointer from `GetCursorPos`, monitors from `EnumDisplayMonitors` work areas, DPI from
//!   `GetDpiForSystem`, always on top with `SetWindowPos(HWND_TOPMOST)`, input grabs with
//!   `SetCapture`.
//! - `next_event` pumps messages with `MsgWaitForMultipleObjects` for the timeout, and keys
//!   go through [`crate::win32_keys::keysym`].
//!
//! Media keys and the system media controls come with this backend.

use std::time::Duration;

use crate::{
    CursorShape, Event, Platform, PlatformError, Point, Rect, Result, Size, WindowId, WindowSpec,
};

/// The Win32 implementation of [`Platform`] (not implemented yet).
#[derive(Debug)]
pub struct Win32Platform {
    _private: (),
}

fn unsupported<T>() -> Result<T> {
    Err(PlatformError::Unsupported(
        "the Windows window backend is not implemented yet".into(),
    ))
}

impl Win32Platform {
    /// Opens the desktop session. Always fails until the backend exists.
    pub fn connect() -> Result<Self> {
        unsupported()
    }
}

impl Platform for Win32Platform {
    fn create_window(&mut self, _spec: &WindowSpec) -> Result<WindowId> {
        unsupported()
    }
    fn destroy_window(&mut self, _id: WindowId) -> Result<()> {
        unsupported()
    }
    fn set_visible(&mut self, _id: WindowId, _visible: bool) -> Result<()> {
        unsupported()
    }
    fn move_window(&mut self, _id: WindowId, _pos: Point) -> Result<()> {
        unsupported()
    }
    fn resize_window(&mut self, _id: WindowId, _size: Size) -> Result<()> {
        unsupported()
    }
    fn minimize(&mut self, _id: WindowId) -> Result<()> {
        unsupported()
    }
    fn raise(&mut self, _id: WindowId) -> Result<()> {
        unsupported()
    }
    fn present(&mut self, _id: WindowId, _size: Size, _pixels: &[u32]) -> Result<()> {
        unsupported()
    }
    fn set_shape(&mut self, _id: WindowId, _rects: Option<&[Rect]>) -> Result<()> {
        unsupported()
    }
    fn pointer(&self) -> Result<Point> {
        unsupported()
    }
    fn screens(&self) -> Result<Vec<Rect>> {
        unsupported()
    }
    fn dpi(&self) -> Option<u32> {
        None
    }
    fn set_always_on_top(&mut self, _id: WindowId, _on: bool) -> Result<()> {
        unsupported()
    }
    fn grab_input(&mut self, _id: WindowId) -> Result<bool> {
        unsupported()
    }
    fn release_input(&mut self) -> Result<()> {
        unsupported()
    }
    fn set_cursor(&mut self, _id: WindowId, _cursor: CursorShape) -> Result<()> {
        unsupported()
    }
    fn next_event(&mut self, _timeout: Option<Duration>) -> Result<Option<Event>> {
        unsupported()
    }
    fn flush(&mut self) -> Result<()> {
        unsupported()
    }
}
