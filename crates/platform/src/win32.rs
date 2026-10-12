//! Win32 backend on `windows-sys`.
//!
//! Windows deliver their messages to the thread that created them, so there is no helper
//! thread: [`Win32Platform::next_event`] pumps the calling thread's message queue and waits
//! with `MsgWaitForMultipleObjectsEx`. The window procedure translates messages into
//! [`Event`]s in a thread-local queue; the platform is therefore not `Send`, and there is one
//! per process.
//!
//! How the X11 behavior maps:
//! - Borderless `WS_POPUP` windows, dragged by the app. Panels and dialogs are owned by their
//!   owner window (stacked above it, hidden with it, out of the taskbar); panels and popups are
//!   tool windows. Menus are topmost and never take focus.
//! - X11 keeps sending pointer events to a window while a button is held; here a button press
//!   takes the mouse capture until all buttons are up. [`Platform::grab_input`] is a capture
//!   that outlives the buttons. Keys keep going to the focused window, which is fine for menus
//!   because the app routes keys to an open menu whatever window they come from.
//! - Frames are `0xAARRGGBB` rows, the memory layout of a top-down 32-bit DIB, drawn with
//!   `StretchDIBits`. The last frame is kept so `WM_PAINT` can redraw at once.
//! - Shapes are rectangle regions (`SetWindowRgn`); monitors are work areas from
//!   `EnumDisplayMonitors`, primary first; the process is per-monitor DPI aware, so all
//!   coordinates are physical pixels like on X11.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::marker::PhantomData;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_TIMEOUT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, ClientToScreen, CombineRgn, CreateRectRgn,
    DIB_RGB_COLORS, DeleteObject, EndPaint, EnumDisplayMonitors, GetDC, GetMonitorInfoW, HDC,
    HMONITOR, MONITORINFO, PAINTSTRUCT, RGN_OR, ReleaseDC, SRCCOPY, ScreenToClient, SetWindowRgn,
    StretchDIBits,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_F4, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, HCURSOR,
    HTCLIENT, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, IDC_ARROW, IDC_SIZENS, IDC_SIZENWSE,
    IDC_SIZEWE, IsIconic, LoadCursorW, MA_NOACTIVATE, MONITORINFOF_PRIMARY, MSG,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    RegisterClassExW, SW_HIDE, SW_MINIMIZE, SW_SHOW, SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetWindowPos, ShowWindow, UnregisterClassW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WINDOWPOS, WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE, WM_DPICHANGED,
    WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SYSCHAR, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_WINDOWPOSCHANGED, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    WindowFromPoint,
};

use crate::{
    CursorShape, Event, Modifiers, MouseButton, Platform, PlatformError, Point, Rect, Result, Size,
    WindowId, WindowKind, WindowSpec, win32_keys,
};

/// One notch of a mouse wheel.
const WHEEL_DELTA: i32 = 120;

/// Button flags in a mouse message's `wParam` (`MK_*` in `WinUser.h`).
const MK_LBUTTON: WPARAM = 0x0001;
const MK_RBUTTON: WPARAM = 0x0002;
const MK_MBUTTON: WPARAM = 0x0010;
const MK_XBUTTON1: WPARAM = 0x0020;
const MK_XBUTTON2: WPARAM = 0x0040;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn last_error(what: &str) -> PlatformError {
    // SAFETY: GetLastError only reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    PlatformError::Connection(format!("{what} failed (error {code})"))
}

/// Window and extended window styles for each kind.
fn styles(kind: WindowKind) -> (WINDOW_STYLE, WINDOW_EX_STYLE) {
    match kind {
        // The system menu and minimize box give the taskbar button its usual behavior
        // (click to minimize and restore, right-click menu) without drawing a frame.
        WindowKind::Main => (WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX, WS_EX_APPWINDOW),
        WindowKind::Panel => (WS_POPUP, WS_EX_TOOLWINDOW),
        WindowKind::Popup => (
            WS_POPUP,
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
        ),
        WindowKind::Dialog => (WS_POPUP, 0),
    }
}

/// The client point packed in a mouse message's `lParam` (signed 16-bit x and y).
fn lparam_point(lp: LPARAM) -> Point {
    let x = (lp & 0xffff) as u16 as i16;
    let y = ((lp >> 16) & 0xffff) as u16 as i16;
    Point::new(i32::from(x), i32::from(y))
}

/// The signed wheel rotation in a `WM_MOUSEWHEEL` `wParam`.
fn wheel_delta(wp: WPARAM) -> i32 {
    i32::from(((wp >> 16) & 0xffff) as u16 as i16)
}

/// Adds `delta` to the accumulated wheel rotation and returns how many whole notches to
/// report (positive is away from the user, X11's button 4). Smooth-scrolling devices send
/// fractions of a notch; the remainder carries over.
fn wheel_notches(acc: &mut i32, delta: i32) -> i32 {
    *acc += delta;
    let notches = *acc / WHEEL_DELTA;
    *acc -= notches * WHEEL_DELTA;
    notches
}

/// Mouse buttons held, from the `MK_*` flags in a mouse message's `wParam`.
fn buttons_held(wp: WPARAM) -> bool {
    wp & (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2) != 0
}

/// Rows `first..last` of a `w`-wide frame as a DIB header (top-down, 32 bits per pixel).
fn dib_header(w: usize, rows: usize) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // Negative: the first row is the top one.
            biHeight: -(rows as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [Default::default()],
    }
}

/// Draws rows `first..last` of `pixels` (`w` wide) at the same rows of `hdc`.
fn blit(hdc: HDC, pixels: &[u32], w: usize, first: usize, last: usize) {
    let rows = last - first;
    if w == 0 || rows == 0 || pixels.len() < last * w {
        return;
    }
    let header = dib_header(w, rows);
    let band = &pixels[first * w..last * w];
    // SAFETY: `band` holds exactly `rows` rows of `w` 32-bit pixels, matching `header`, and
    // outlives the call; `hdc` is a device context the caller holds for this window.
    unsafe {
        StretchDIBits(
            hdc,
            0,
            first as i32,
            w as i32,
            rows as i32,
            0,
            0,
            w as i32,
            rows as i32,
            band.as_ptr().cast(),
            &header,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

struct WinState {
    hwnd: HWND,
    kind: WindowKind,
    cursor: CursorShape,
    /// The last presented frame and its size, redrawn on `WM_PAINT`.
    frame: Vec<u32>,
    frame_size: Size,
}

/// State the window procedure needs. Borrowed only briefly, never across a call that can
/// send messages (most window calls do), since the window procedure runs inside them.
struct Shared {
    windows: BTreeMap<u32, WinState>,
    ids: HashMap<HWND, WindowId>,
    events: VecDeque<Event>,
    /// The window holding an explicit input grab.
    grab: Option<HWND>,
    wheel: i32,
    cursors: [HCURSOR; 4],
}

thread_local! {
    static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

/// Runs `f` on the shared state, or returns None when there is none or it is borrowed.
fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    SHARED.with(|s| s.try_borrow_mut().ok()?.as_mut().map(f))
}

fn push(event: Event) {
    with(|s| s.events.push_back(event));
}

fn to_screen(hwnd: HWND, local: Point) -> Point {
    let mut p = POINT {
        x: local.x,
        y: local.y,
    };
    // SAFETY: `hwnd` is one of our windows and `p` is a valid POINT.
    unsafe { ClientToScreen(hwnd, &mut p) };
    Point::new(p.x, p.y)
}

fn modifiers() -> Modifiers {
    // SAFETY: GetKeyState only reads this thread's keyboard state.
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    Modifiers {
        shift: down(VK_SHIFT),
        ctrl: down(VK_CONTROL),
        alt: down(VK_MENU),
    }
}

fn default_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: forwards the message the system gave us, unchanged.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn mouse_button(msg: u32, wp: WPARAM) -> Option<(MouseButton, bool)> {
    Some(match msg {
        WM_LBUTTONDOWN => (MouseButton::Left, true),
        WM_LBUTTONUP => (MouseButton::Left, false),
        WM_MBUTTONDOWN => (MouseButton::Middle, true),
        WM_MBUTTONUP => (MouseButton::Middle, false),
        WM_RBUTTONDOWN => (MouseButton::Right, true),
        WM_RBUTTONUP => (MouseButton::Right, false),
        // X11 numbers the side buttons 8 and 9.
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let n = if (wp >> 16) & 0xffff == 1 { 8 } else { 9 };
            (MouseButton::Other(n), msg == WM_XBUTTONDOWN)
        }
        _ => return None,
    })
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(Some((window, kind))) = with(|s| {
        let id = *s.ids.get(&hwnd)?;
        Some((id, s.windows.get(&id.0)?.kind))
    }) else {
        return default_proc(hwnd, msg, wp, lp);
    };
    if let Some((button, pressed)) = mouse_button(msg, wp) {
        let local = lparam_point(lp);
        let root = to_screen(hwnd, local);
        let grabbed = with(|s| s.grab.is_some()).unwrap_or(false);
        if pressed {
            if !grabbed {
                // Like X11's implicit grab: keep the pointer until the buttons are up.
                // SAFETY: `hwnd` is one of our windows.
                unsafe { SetCapture(hwnd) };
            }
            push(Event::ButtonDown {
                window,
                button,
                local,
                root,
                mods: modifiers(),
            });
        } else {
            // SAFETY: GetCapture and ReleaseCapture act on this thread's capture only.
            if !grabbed && !buttons_held(wp) && unsafe { GetCapture() } == hwnd {
                // SAFETY: as above.
                unsafe { ReleaseCapture() };
            }
            push(Event::ButtonUp {
                window,
                button,
                local,
                root,
            });
        }
        return 0;
    }
    match msg {
        WM_MOUSEMOVE => {
            let local = lparam_point(lp);
            push(Event::Motion {
                window,
                local,
                root: to_screen(hwnd, local),
            });
            0
        }
        WM_MOUSEWHEEL => {
            // The wheel reports a desktop position.
            let root = lparam_point(lp);
            let mut p = POINT {
                x: root.x,
                y: root.y,
            };
            // SAFETY: `hwnd` is one of our windows and `p` is a valid POINT.
            unsafe { ScreenToClient(hwnd, &mut p) };
            let local = Point::new(p.x, p.y);
            let notches = with(|s| wheel_notches(&mut s.wheel, wheel_delta(wp))).unwrap_or(0);
            let button = if notches > 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            let mods = modifiers();
            for _ in 0..notches.unsigned_abs() {
                push(Event::ButtonDown {
                    window,
                    button,
                    local,
                    root,
                    mods,
                });
                push(Event::ButtonUp {
                    window,
                    button,
                    local,
                    root,
                });
            }
            0
        }
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
            let vk = (wp & 0xffff) as u16;
            let pressed = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
            // Alt+F4 closes the window as usual (it turns into WM_CLOSE).
            if msg == WM_SYSKEYDOWN && vk == VK_F4 {
                return default_proc(hwnd, msg, wp, lp);
            }
            if let Some(keysym) = win32_keys::keysym(vk) {
                push(Event::Key {
                    window,
                    keysym,
                    pressed,
                    mods: modifiers(),
                });
            }
            // Not passed on: a lone Alt would enter the (invisible) window menu.
            0
        }
        // Typed characters are not used; unhandled Alt+key characters would beep.
        WM_CHAR | WM_SYSCHAR => 0,
        WM_PAINT => {
            // SAFETY: PAINTSTRUCT is plain data that BeginPaint fills.
            let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
            // SAFETY: `hwnd` is one of our windows; BeginPaint and EndPaint are paired.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            with(|s| {
                if let Some(w) = s.windows.get(&window.0) {
                    let (fw, fh) = (w.frame_size.w as usize, w.frame_size.h as usize);
                    blit(hdc, &w.frame, fw, 0, fh);
                }
            });
            // SAFETY: as above.
            unsafe { EndPaint(hwnd, &ps) };
            push(Event::Expose { window });
            0
        }
        // We paint every pixel; erasing first would flicker.
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            push(Event::CloseRequested { window });
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            push(Event::Focus {
                window,
                focused: msg == WM_SETFOCUS,
            });
            0
        }
        WM_WINDOWPOSCHANGED => {
            // SAFETY: for this message lParam points to a WINDOWPOS valid during the call.
            let pos = unsafe { &*(lp as *const WINDOWPOS) };
            let moved_or_sized = pos.flags & (SWP_NOMOVE | SWP_NOSIZE) != (SWP_NOMOVE | SWP_NOSIZE);
            // A minimized window sits at -32000, -32000; that is not a position to report.
            // SAFETY: `hwnd` is one of our windows.
            if moved_or_sized && unsafe { IsIconic(hwnd) } == 0 {
                push(Event::Configured {
                    window,
                    rect: Rect::new(pos.x, pos.y, pos.cx, pos.cy),
                });
            }
            0
        }
        WM_SETCURSOR if (lp & 0xffff) as u32 == HTCLIENT => {
            let cursor = with(|s| {
                let shape = s
                    .windows
                    .get(&window.0)
                    .map(|w| w.cursor)
                    .unwrap_or_default();
                s.cursors[shape as usize]
            });
            match cursor {
                Some(c) => {
                    // SAFETY: `c` is a shared system cursor loaded at startup.
                    unsafe { SetCursor(c) };
                    1
                }
                None => default_proc(hwnd, msg, wp, lp),
            }
        }
        WM_MOUSEACTIVATE if kind == WindowKind::Popup => MA_NOACTIVATE as LRESULT,
        WM_CAPTURECHANGED => {
            // Something else took the pointer (for example Alt+Tab): the grab is over.
            let new_owner = lp as HWND;
            with(|s| {
                if s.grab == Some(hwnd) && new_owner != hwnd {
                    s.grab = None;
                }
            });
            0
        }
        // Sizes follow the UI scale, which the app picks; ignore the suggested rectangle.
        WM_DPICHANGED => 0,
        _ => default_proc(hwnd, msg, wp, lp),
    }
}

extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> windows_sys::core::BOOL {
    // SAFETY: `data` is the `&mut Vec` that `screens` passes to EnumDisplayMonitors, which
    // calls us synchronously while it is alive.
    let out = unsafe { &mut *(data as *mut Vec<(bool, Rect)>) };
    // SAFETY: MONITORINFO is plain data; cbSize tells the call which version we pass.
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    // SAFETY: `monitor` comes from the enumeration and `info` is a valid MONITORINFO.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        let r = info.rcWork;
        let primary = info.dwFlags & MONITORINFOF_PRIMARY != 0;
        out.push((
            primary,
            Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top),
        ));
    }
    1
}

/// The Win32 implementation of [`Platform`].
#[derive(Debug)]
pub struct Win32Platform {
    class: Vec<u16>,
    next_id: u32,
    /// Windows belong to the creating thread.
    _not_send: PhantomData<*const ()>,
}

impl Win32Platform {
    /// Sets up the window class for this process. Fails if a platform already exists on this
    /// thread.
    pub fn connect() -> Result<Self> {
        if SHARED.with(|s| s.borrow().is_some()) {
            return Err(PlatformError::Connect(
                "a Win32 platform already exists on this thread".into(),
            ));
        }
        // Physical pixels everywhere, as on X11. Fails harmlessly if a manifest set it.
        // SAFETY: a plain process-wide setting, made before any window exists.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

        let class = wide("OpenAMPWindow");
        // SAFETY: a null module name means this executable.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let load = |name| {
            // SAFETY: loads a predefined system cursor (null instance).
            unsafe { LoadCursorW(std::ptr::null_mut(), name) }
        };
        let cursors = [
            load(IDC_ARROW),
            load(IDC_SIZEWE),
            load(IDC_SIZENS),
            load(IDC_SIZENWSE),
        ];
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: cursors[0],
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        // SAFETY: `wc` is fully initialized and `class` outlives the registration (it is kept
        // in the platform and unregistered on drop).
        if unsafe { RegisterClassExW(&wc) } == 0 {
            return Err(last_error("RegisterClassExW"));
        }
        SHARED.with(|s| {
            *s.borrow_mut() = Some(Shared {
                windows: BTreeMap::new(),
                ids: HashMap::new(),
                events: VecDeque::new(),
                grab: None,
                wheel: 0,
                cursors,
            });
        });
        Ok(Self {
            class,
            next_id: 1,
            _not_send: PhantomData,
        })
    }

    /// The native handle of a window, for system integrations (and tests).
    pub fn hwnd(&self, id: WindowId) -> Option<HWND> {
        with(|s| s.windows.get(&id.0).map(|w| w.hwnd)).flatten()
    }

    fn handle(&self, id: WindowId) -> Result<HWND> {
        self.hwnd(id).ok_or(PlatformError::UnknownWindow(id))
    }

    fn set_pos(&self, id: WindowId, after: HWND, rect: Rect, flags: u32) -> Result<()> {
        let hwnd = self.handle(id)?;
        // SAFETY: `hwnd` is one of our windows; `after` is a window or a HWND_* constant.
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                after,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                flags | SWP_NOACTIVATE,
            )
        };
        if ok == 0 {
            return Err(last_error("SetWindowPos"));
        }
        Ok(())
    }

    /// Pumps every waiting message through the window procedure.
    fn pump(&self) {
        // SAFETY: MSG is plain data that PeekMessageW fills.
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: `msg` is a valid MSG; a null window takes messages for the whole thread.
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            // SAFETY: dispatches the message PeekMessageW just returned.
            unsafe { DispatchMessageW(&msg) };
        }
    }

    /// The next queued event, with queued motion for the same window folded into the latest.
    fn pop(&self) -> Option<Event> {
        with(|s| {
            let first = s.events.pop_front()?;
            let Event::Motion { window, .. } = first else {
                return Some(first);
            };
            let mut latest = first;
            while let Some(Event::Motion { window: w, .. }) = s.events.front()
                && *w == window
            {
                latest = s.events.pop_front()?;
            }
            Some(latest)
        })
        .flatten()
    }
}

impl Platform for Win32Platform {
    fn create_window(&mut self, spec: &WindowSpec) -> Result<WindowId> {
        let owner = match spec.owner {
            Some(o) => self.handle(o)?,
            None => std::ptr::null_mut(),
        };
        let (style, ex_style) = styles(spec.kind);
        let title = wide(&spec.title);
        let r = spec.rect;
        // SAFETY: GetModuleHandleW(null) returns this executable's handle.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        // SAFETY: the class is registered, the strings are NUL-terminated and live across the
        // call, and `owner` is null or one of our windows.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                self.class.as_ptr(),
                title.as_ptr(),
                style,
                r.x,
                r.y,
                r.w.max(1),
                r.h.max(1),
                owner,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(last_error("CreateWindowExW"));
        }
        let id = WindowId(self.next_id);
        self.next_id += 1;
        with(|s| {
            s.ids.insert(hwnd, id);
            s.windows.insert(
                id.0,
                WinState {
                    hwnd,
                    kind: spec.kind,
                    cursor: CursorShape::Arrow,
                    frame: Vec::new(),
                    frame_size: Size::new(0, 0),
                },
            );
        });
        Ok(id)
    }

    fn destroy_window(&mut self, id: WindowId) -> Result<()> {
        let hwnd = self.handle(id)?;
        with(|s| {
            s.windows.remove(&id.0);
            s.ids.remove(&hwnd);
            if s.grab == Some(hwnd) {
                s.grab = None;
            }
        });
        // SAFETY: `hwnd` was one of our windows; it is forgotten above, so no message for it
        // is translated any more.
        if unsafe { DestroyWindow(hwnd) } == 0 {
            return Err(last_error("DestroyWindow"));
        }
        Ok(())
    }

    fn set_visible(&mut self, id: WindowId, visible: bool) -> Result<()> {
        let hwnd = self.handle(id)?;
        let kind = with(|s| s.windows.get(&id.0).map(|w| w.kind))
            .flatten()
            .unwrap_or_default();
        let cmd = match (visible, kind) {
            (false, _) => SW_HIDE,
            (true, WindowKind::Main | WindowKind::Dialog) => SW_SHOW,
            // Showing a panel or menu does not take the focus.
            (true, WindowKind::Panel | WindowKind::Popup) => SW_SHOWNA,
        };
        // SAFETY: `hwnd` is one of our windows. The result is the previous visibility.
        unsafe { ShowWindow(hwnd, cmd) };
        Ok(())
    }

    fn move_window(&mut self, id: WindowId, pos: Point) -> Result<()> {
        self.set_pos(
            id,
            std::ptr::null_mut(),
            Rect::new(pos.x, pos.y, 0, 0),
            SWP_NOSIZE | SWP_NOZORDER,
        )
    }

    fn resize_window(&mut self, id: WindowId, size: Size) -> Result<()> {
        self.set_pos(
            id,
            std::ptr::null_mut(),
            Rect::new(0, 0, size.w.max(1), size.h.max(1)),
            SWP_NOMOVE | SWP_NOZORDER,
        )
    }

    fn minimize(&mut self, id: WindowId) -> Result<()> {
        let hwnd = self.handle(id)?;
        // SAFETY: `hwnd` is one of our windows.
        unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
        Ok(())
    }

    fn raise(&mut self, id: WindowId) -> Result<()> {
        self.set_pos(id, HWND_TOP, Rect::new(0, 0, 0, 0), SWP_NOMOVE | SWP_NOSIZE)
    }

    fn present(&mut self, id: WindowId, size: Size, pixels: &[u32]) -> Result<()> {
        let rows = size.h.max(0) as usize;
        self.present_rows(id, size, pixels, 0, rows)
    }

    fn present_rows(
        &mut self,
        id: WindowId,
        size: Size,
        pixels: &[u32],
        first: usize,
        last: usize,
    ) -> Result<()> {
        let hwnd = self.handle(id)?;
        let (w, h) = (size.w.max(0) as usize, size.h.max(0) as usize);
        if pixels.len() != w * h {
            return Err(PlatformError::BadBuffer {
                got: pixels.len(),
                expected: w * h,
            });
        }
        let last = last.min(h);
        // Keep the frame for WM_PAINT: all of it when the size changed, else the new rows.
        with(|s| {
            if let Some(win) = s.windows.get_mut(&id.0) {
                if win.frame_size != size || win.frame.len() != pixels.len() {
                    win.frame.clear();
                    win.frame.extend_from_slice(pixels);
                    win.frame_size = size;
                } else if first < last {
                    win.frame[first * w..last * w].copy_from_slice(&pixels[first * w..last * w]);
                }
            }
        });
        if w == 0 || first >= last {
            return Ok(());
        }
        // SAFETY: `hwnd` is one of our windows; the DC is released below.
        let hdc = unsafe { GetDC(hwnd) };
        if hdc.is_null() {
            return Err(last_error("GetDC"));
        }
        blit(hdc, pixels, w, first, last);
        // SAFETY: releases the DC obtained above for the same window.
        unsafe { ReleaseDC(hwnd, hdc) };
        Ok(())
    }

    fn set_shape(&mut self, id: WindowId, rects: Option<&[Rect]>) -> Result<()> {
        let hwnd = self.handle(id)?;
        let region = match rects {
            None => std::ptr::null_mut(),
            Some(rects) => {
                // SAFETY: creates an empty region we own until SetWindowRgn takes it.
                let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
                for r in rects.iter().filter(|r| r.w > 0 && r.h > 0) {
                    // SAFETY: a temporary region, merged into `region` and deleted.
                    unsafe {
                        let part = CreateRectRgn(r.x, r.y, r.x + r.w, r.y + r.h);
                        CombineRgn(region, region, part, RGN_OR);
                        DeleteObject(part);
                    }
                }
                region
            }
        };
        // SAFETY: on success the system owns `region` (we must not delete it); a null region
        // restores the plain rectangle.
        if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
            if !region.is_null() {
                // SAFETY: the call failed, so the region is still ours to delete.
                unsafe { DeleteObject(region) };
            }
            return Err(last_error("SetWindowRgn"));
        }
        Ok(())
    }

    fn pointer(&self) -> Result<Point> {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: `p` is a valid POINT.
        if unsafe { GetCursorPos(&mut p) } == 0 {
            return Err(last_error("GetCursorPos"));
        }
        Ok(Point::new(p.x, p.y))
    }

    fn screens(&self) -> Result<Vec<Rect>> {
        let mut found: Vec<(bool, Rect)> = Vec::new();
        // SAFETY: the callback only runs during this call, while `found` is alive.
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect_monitor),
                &mut found as *mut Vec<(bool, Rect)> as LPARAM,
            )
        };
        // The primary monitor first (it decides the starting scale and position).
        found.sort_by_key(|&(primary, _)| !primary);
        let screens: Vec<Rect> = found.into_iter().map(|(_, r)| r).collect();
        if screens.is_empty() {
            return Err(PlatformError::Unsupported("no monitors".into()));
        }
        Ok(screens)
    }

    fn dpi(&self) -> Option<u32> {
        // SAFETY: no arguments; reads the system DPI.
        let dpi = unsafe { GetDpiForSystem() };
        (dpi > 0).then_some(dpi)
    }

    fn set_always_on_top(&mut self, id: WindowId, on: bool) -> Result<()> {
        let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        self.set_pos(id, after, Rect::new(0, 0, 0, 0), SWP_NOMOVE | SWP_NOSIZE)
    }

    fn grab_input(&mut self, id: WindowId) -> Result<bool> {
        let hwnd = self.handle(id)?;
        with(|s| s.grab = Some(hwnd));
        // SAFETY: `hwnd` is one of our windows.
        unsafe { SetCapture(hwnd) };
        // SAFETY: reads this thread's capture.
        let held = unsafe { GetCapture() } == hwnd;
        if !held {
            with(|s| s.grab = None);
        }
        Ok(held)
    }

    fn release_input(&mut self) -> Result<()> {
        let grab = with(|s| s.grab.take()).flatten();
        // SAFETY: reads and releases this thread's capture only.
        if grab.is_some() && unsafe { GetCapture() } == grab.unwrap_or(std::ptr::null_mut()) {
            // SAFETY: as above.
            unsafe { ReleaseCapture() };
        }
        Ok(())
    }

    fn set_cursor(&mut self, id: WindowId, cursor: CursorShape) -> Result<()> {
        let changed = with(|s| {
            let w = s.windows.get_mut(&id.0)?;
            let changed = w.cursor != cursor;
            w.cursor = cursor;
            Some((changed, s.cursors[cursor as usize]))
        })
        .flatten();
        let Some((changed, handle)) = changed else {
            return Err(PlatformError::UnknownWindow(id));
        };
        // Windows only asks for a cursor when the pointer moves; switch it now if the
        // pointer is over this window.
        if changed {
            let hwnd = self.handle(id)?;
            let p = self.pointer()?;
            // SAFETY: plain hit test at a desktop point.
            let under = unsafe { WindowFromPoint(POINT { x: p.x, y: p.y }) };
            if under == hwnd {
                // SAFETY: `handle` is a shared system cursor.
                unsafe { SetCursor(handle) };
            }
        }
        Ok(())
    }

    fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            self.pump();
            if let Some(e) = self.pop() {
                return Ok(Some(e));
            }
            let wait_ms = match deadline {
                None => u32::MAX,
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(None);
                    }
                    // Round up so a short timeout still waits rather than spinning.
                    u32::try_from(left.as_micros().div_ceil(1000)).unwrap_or(u32::MAX - 1)
                }
            };
            // SAFETY: no handles; wakes on any input or message for this thread.
            let r = unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    wait_ms,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            };
            if r == WAIT_TIMEOUT && deadline.is_some_and(|d| Instant::now() >= d) {
                self.pump();
                return Ok(self.pop());
            }
        }
    }

    fn flush(&mut self) -> Result<()> {
        // Win32 calls take effect immediately; there is no request buffer.
        Ok(())
    }
}

impl Drop for Win32Platform {
    fn drop(&mut self) {
        let hwnds: Vec<HWND> = with(|s| {
            s.ids.clear();
            std::mem::take(&mut s.windows)
                .into_values()
                .map(|w| w.hwnd)
                .collect()
        })
        .unwrap_or_default();
        for hwnd in hwnds {
            // SAFETY: our windows, already forgotten by the window procedure.
            unsafe { DestroyWindow(hwnd) };
        }
        // SAFETY: every window of the class is destroyed; the name is still alive.
        unsafe {
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(std::ptr::null()));
        }
        SHARED.with(|s| s.borrow_mut().take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_points_are_signed() {
        assert_eq!(lparam_point(0x0014_000a), Point::new(10, 20));
        // A drag to the left of or above the window gives negative client coordinates.
        assert_eq!(lparam_point(0xfffe_ffff), Point::new(-1, -2));
        assert_eq!(lparam_point(0x8000_7fff), Point::new(32767, -32768));
    }

    #[test]
    fn wheel_rotation_becomes_whole_notches() {
        assert_eq!(wheel_delta(0x0078_0000), 120);
        assert_eq!(wheel_delta(0xff88_0000), -120);
        let mut acc = 0;
        assert_eq!(wheel_notches(&mut acc, 240), 2);
        // Smooth scrolling: fractions add up across messages.
        assert_eq!(wheel_notches(&mut acc, 60), 0);
        assert_eq!(wheel_notches(&mut acc, 60), 1);
        assert_eq!(acc, 0);
        assert_eq!(wheel_notches(&mut acc, -130), -1);
        assert_eq!(acc, -10);
    }

    #[test]
    fn window_kinds_get_the_right_styles() {
        let (main, main_ex) = styles(WindowKind::Main);
        assert_ne!(main & WS_POPUP, 0);
        assert_ne!(
            main_ex & WS_EX_APPWINDOW,
            0,
            "main window is in the taskbar"
        );
        let (_, panel_ex) = styles(WindowKind::Panel);
        assert_ne!(
            panel_ex & WS_EX_TOOLWINDOW,
            0,
            "panels stay out of the taskbar"
        );
        let (_, popup_ex) = styles(WindowKind::Popup);
        assert_ne!(popup_ex & WS_EX_NOACTIVATE, 0, "menus never take focus");
        assert_ne!(popup_ex & WS_EX_TOPMOST, 0);
    }

    #[test]
    fn dib_header_is_top_down_32_bit() {
        let h = dib_header(275, 116).bmiHeader;
        assert_eq!((h.biWidth, h.biHeight, h.biBitCount), (275, -116, 32));
    }

    #[test]
    fn held_buttons() {
        assert!(!buttons_held(0));
        assert!(buttons_held(MK_LBUTTON));
        // Shift (0x0004) alone is not a button.
        assert!(!buttons_held(0x0004));
        assert!(buttons_held(MK_RBUTTON | 0x0004));
    }
}
