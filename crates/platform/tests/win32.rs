//! Real Win32 tests. They run on Windows, or under Wine with a virtual display
//! (scripts/test-windows.sh). Input is injected with SendInput, like a real user.

#![cfg(windows)]
#![allow(unsafe_code)]
// Integration tests are test code; panicking on setup failure is the point.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::undocumented_unsafe_blocks
)]

use std::time::{Duration, Instant};

use ampcraft_platform::win32::Win32Platform;
use ampcraft_platform::{
    Event, MouseButton, Platform, Point, Rect, Size, WindowId, WindowKind, WindowSpec, keys,
};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, GetDC, GetPixel, GetWindowRgn, PtInRegion, ReleaseDC,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, PostMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetForegroundWindow, WM_CLOSE,
};

fn spec(title: &str, rect: Rect, kind: WindowKind) -> WindowSpec {
    WindowSpec {
        title: title.into(),
        rect,
        kind,
        owner: None,
    }
}

/// Pumps events until one matches, failing after a few seconds.
fn wait_for(p: &mut Win32Platform, mut pred: impl FnMut(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out waiting for event");
        if let Some(e) = p.next_event(Some(left)).unwrap()
            && pred(&e)
        {
            return e;
        }
    }
}

/// Handles events for a short while, keeping the window system responsive.
fn settle(p: &mut Win32Platform) -> Vec<Event> {
    let mut seen = Vec::new();
    while let Some(e) = p.next_event(Some(Duration::from_millis(100))).unwrap() {
        seen.push(e);
    }
    seen
}

/// Creates, shows and waits until a window is ready to draw.
fn shown(p: &mut Win32Platform, title: &str, rect: Rect, kind: WindowKind) -> WindowId {
    let id = p.create_window(&spec(title, rect, kind)).unwrap();
    p.set_visible(id, true).unwrap();
    wait_for(
        p,
        |e| matches!(e, Event::Expose { window } if *window == id),
    );
    id
}

fn send(inputs: &[INPUT]) {
    let n = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    assert_eq!(n as usize, inputs.len(), "SendInput");
}

fn mouse(flags: u32, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Moves the pointer to a desktop point (absolute input is normalized to 0..65535).
fn move_to(p: Point) {
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let nx = ((p.x - vx) * 65535 + (vw - 1) / 2) / (vw - 1).max(1);
    let ny = ((p.y - vy) * 65535 + (vh - 1) / 2) / (vh - 1).max(1);
    send(&[mouse(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        nx,
        ny,
        0,
    )]);
}

fn left(down: bool) {
    let flag = if down {
        MOUSEEVENTF_LEFTDOWN
    } else {
        MOUSEEVENTF_LEFTUP
    };
    send(&[mouse(flag, 0, 0, 0)]);
}

fn key(vk: u16, down: bool) {
    send(&[INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if down { 0 } else { KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }]);
}

fn window_rect(hwnd: HWND) -> Rect {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    assert_ne!(unsafe { GetWindowRect(hwnd, &mut r) }, 0);
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// One platform per thread, and the window system is shared: run the scenarios in order on
/// one thread instead of as parallel tests.
#[test]
fn win32_backend() {
    let mut p = Win32Platform::connect().unwrap();
    assert!(
        Win32Platform::connect().is_err(),
        "a second platform on the same thread is refused"
    );
    presented_pixels_reach_the_window(&mut p);
    moves_and_resizes_are_reported(&mut p);
    screens_and_pointer(&mut p);
    drag_keeps_the_pointer_outside_the_window(&mut p);
    wheel_gives_button_pairs(&mut p);
    keys_become_keysyms(&mut p);
    shapes_limit_the_window(&mut p);
    menu_grab_receives_clicks_outside(&mut p);
    close_is_a_request(&mut p);
}

fn presented_pixels_reach_the_window(p: &mut Win32Platform) {
    let id = shown(p, "pixels", Rect::new(50, 60, 40, 30), WindowKind::Main);
    let pixels: Vec<u32> = (0..40 * 30u32)
        .map(|i| 0xFF00_0000 | (i * 997) & 0xFF_FFFF)
        .collect();
    p.present(id, Size::new(40, 30), &pixels).unwrap();
    // Only some rows: the rest must keep what is shown.
    let mut changed = pixels.clone();
    for px in &mut changed[10 * 40..12 * 40] {
        *px = 0xFF12_3456;
    }
    p.present_rows(id, Size::new(40, 30), &changed, 10, 12)
        .unwrap();
    let hwnd = p.hwnd(id).unwrap();
    let hdc = unsafe { GetDC(hwnd) };
    for (i, &want) in changed.iter().enumerate() {
        let (x, y) = ((i % 40) as i32, (i / 40) as i32);
        // COLORREF is 0x00BBGGRR.
        let c = unsafe { GetPixel(hdc, x, y) };
        let got = ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff);
        assert_eq!(got, want & 0xFF_FFFF, "pixel {x},{y}");
    }
    unsafe { ReleaseDC(hwnd, hdc) };
    assert!(matches!(
        p.present(id, Size::new(40, 30), &pixels[1..]),
        Err(ampcraft_platform::PlatformError::BadBuffer { .. })
    ));
    p.destroy_window(id).unwrap();
    assert!(
        p.set_visible(id, true).is_err(),
        "destroyed windows are unknown"
    );
}

fn moves_and_resizes_are_reported(p: &mut Win32Platform) {
    let id = shown(
        p,
        "configure",
        Rect::new(100, 100, 60, 40),
        WindowKind::Main,
    );
    p.move_window(id, Point::new(130, 120)).unwrap();
    let e = wait_for(
        p,
        |e| matches!(e, Event::Configured { window, .. } if *window == id),
    );
    assert_eq!(
        e,
        Event::Configured {
            window: id,
            rect: Rect::new(130, 120, 60, 40)
        }
    );
    p.resize_window(id, Size::new(90, 50)).unwrap();
    let e = wait_for(
        p,
        |e| matches!(e, Event::Configured { window, .. } if *window == id),
    );
    assert_eq!(
        e,
        Event::Configured {
            window: id,
            rect: Rect::new(130, 120, 90, 50)
        }
    );
    assert_eq!(
        window_rect(p.hwnd(id).unwrap()),
        Rect::new(130, 120, 90, 50)
    );
    // Raising and always-on-top change stacking only: no Configured.
    p.raise(id).unwrap();
    p.set_always_on_top(id, true).unwrap();
    p.set_always_on_top(id, false).unwrap();
    assert!(
        !settle(p)
            .iter()
            .any(|e| matches!(e, Event::Configured { .. })),
        "restacking is not a move"
    );
    p.destroy_window(id).unwrap();
}

fn screens_and_pointer(p: &mut Win32Platform) {
    let screens = p.screens().unwrap();
    assert!(!screens.is_empty());
    assert!(screens.iter().all(|s| s.w > 0 && s.h > 0));
    move_to(Point::new(37, 41));
    settle(p);
    assert_eq!(p.pointer().unwrap(), Point::new(37, 41));
    assert!(p.dpi().is_some_and(|d| d >= 96));
}

fn drag_keeps_the_pointer_outside_the_window(p: &mut Win32Platform) {
    let id = shown(p, "drag", Rect::new(200, 200, 80, 40), WindowKind::Main);
    settle(p);
    move_to(Point::new(210, 210));
    left(true);
    let down = wait_for(p, |e| matches!(e, Event::ButtonDown { .. }));
    assert_eq!(
        down,
        Event::ButtonDown {
            window: id,
            button: MouseButton::Left,
            local: Point::new(10, 10),
            root: Point::new(210, 210),
            mods: Default::default(),
        }
    );
    // Far outside the window, motion still comes to it, relative to it.
    move_to(Point::new(400, 330));
    let motion = wait_for(p, |e| matches!(e, Event::Motion { .. }));
    assert_eq!(
        motion,
        Event::Motion {
            window: id,
            local: Point::new(200, 130),
            root: Point::new(400, 330),
        }
    );
    left(false);
    let up = wait_for(p, |e| matches!(e, Event::ButtonUp { .. }));
    assert!(matches!(up, Event::ButtonUp { window, local, .. }
        if window == id && local == Point::new(200, 130)));
    p.destroy_window(id).unwrap();
}

fn wheel_gives_button_pairs(p: &mut Win32Platform) {
    let id = shown(p, "wheel", Rect::new(300, 100, 80, 40), WindowKind::Main);
    unsafe { SetForegroundWindow(p.hwnd(id).unwrap()) };
    move_to(Point::new(320, 110));
    settle(p);
    send(&[mouse(MOUSEEVENTF_WHEEL, 0, 0, 120)]);
    let e = wait_for(p, |e| matches!(e, Event::ButtonDown { .. }));
    assert!(
        matches!(e, Event::ButtonDown { button: MouseButton::WheelUp, local, .. }
        if local == Point::new(20, 10))
    );
    wait_for(p, |e| {
        matches!(
            e,
            Event::ButtonUp {
                button: MouseButton::WheelUp,
                ..
            }
        )
    });
    send(&[mouse(MOUSEEVENTF_WHEEL, 0, 0, -120)]);
    wait_for(p, |e| {
        matches!(
            e,
            Event::ButtonDown {
                button: MouseButton::WheelDown,
                ..
            }
        )
    });
    p.destroy_window(id).unwrap();
}

fn keys_become_keysyms(p: &mut Win32Platform) {
    let id = shown(p, "keys", Rect::new(300, 200, 80, 40), WindowKind::Main);
    let hwnd = p.hwnd(id).unwrap();
    unsafe { SetForegroundWindow(hwnd) };
    // Showing a main window focuses it (that event went by in `shown`).
    settle(p);
    assert_eq!(
        unsafe { GetFocus() },
        hwnd,
        "the shown main window has the focus"
    );
    key(u16::from(b'Z'), true);
    key(u16::from(b'Z'), false);
    let e = wait_for(p, |e| matches!(e, Event::Key { pressed: true, .. }));
    assert!(
        matches!(e, Event::Key { window, keysym, .. } if window == id && keysym == u32::from(b'z'))
    );
    wait_for(p, |e| matches!(e, Event::Key { pressed: false, .. }));
    // Shift is reported as a modifier, not a key.
    key(0x10, true);
    key(0x1b, true);
    key(0x1b, false);
    key(0x10, false);
    let e = wait_for(p, |e| matches!(e, Event::Key { pressed: true, .. }));
    assert!(matches!(e, Event::Key { keysym, mods, .. }
        if keysym == keys::ESCAPE && mods.shift && !mods.ctrl));
    p.destroy_window(id).unwrap();
}

fn shapes_limit_the_window(p: &mut Win32Platform) {
    let id = shown(p, "shape", Rect::new(100, 300, 50, 50), WindowKind::Main);
    let hwnd = p.hwnd(id).unwrap();
    p.set_shape(
        id,
        Some(&[Rect::new(0, 0, 50, 10), Rect::new(0, 10, 10, 40)]),
    )
    .unwrap();
    let rgn = unsafe { CreateRectRgn(0, 0, 0, 0) };
    assert_ne!(unsafe { GetWindowRgn(hwnd, rgn) }, 0, "a region is set");
    let inside = |x, y| unsafe { PtInRegion(rgn, x, y) } != 0;
    assert!(inside(45, 5));
    assert!(inside(5, 45));
    assert!(!inside(30, 30), "the cut-out corner is outside");
    unsafe { DeleteObject(rgn) };
    p.set_shape(id, None).unwrap();
    let rgn = unsafe { CreateRectRgn(0, 0, 0, 0) };
    assert_eq!(
        unsafe { GetWindowRgn(hwnd, rgn) },
        0,
        "no region: plain rectangle"
    );
    unsafe { DeleteObject(rgn) };
    p.destroy_window(id).unwrap();
}

fn menu_grab_receives_clicks_outside(p: &mut Win32Platform) {
    let main = shown(p, "owner", Rect::new(400, 400, 80, 40), WindowKind::Main);
    let popup = p
        .create_window(&WindowSpec {
            title: "menu".into(),
            rect: Rect::new(420, 300, 60, 60),
            kind: WindowKind::Popup,
            owner: Some(main),
        })
        .unwrap();
    p.set_visible(popup, true).unwrap();
    settle(p);
    assert!(p.grab_input(popup).unwrap());
    // A click elsewhere (here on the main window) goes to the menu, in its coordinates.
    move_to(Point::new(410, 410));
    left(true);
    left(false);
    let e = wait_for(p, |e| matches!(e, Event::ButtonDown { .. }));
    assert!(matches!(e, Event::ButtonDown { window, local, root, .. }
        if window == popup && local == Point::new(-10, 110) && root == Point::new(410, 410)));
    wait_for(
        p,
        |e| matches!(e, Event::ButtonUp { window, .. } if *window == popup),
    );
    p.release_input().unwrap();
    // After the grab, clicks go where the pointer is again.
    left(true);
    left(false);
    let e = wait_for(p, |e| matches!(e, Event::ButtonDown { .. }));
    assert!(matches!(e, Event::ButtonDown { window, .. } if window == main));
    settle(p);
    p.destroy_window(popup).unwrap();
    p.destroy_window(main).unwrap();
}

fn close_is_a_request(p: &mut Win32Platform) {
    let id = shown(p, "close", Rect::new(10, 10, 30, 30), WindowKind::Main);
    unsafe { PostMessageW(p.hwnd(id).unwrap(), WM_CLOSE, 0, 0) };
    wait_for(
        p,
        |e| matches!(e, Event::CloseRequested { window } if *window == id),
    );
    // The window is still there until the app destroys it.
    assert!(p.set_visible(id, true).is_ok());
    p.destroy_window(id).unwrap();
}
