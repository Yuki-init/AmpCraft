//! Real X11 tests. They run under Xvfb (scripts/test.sh) and skip when `$DISPLAY` is unset.
//! Input is injected with the XTEST extension from a second connection, like a real user.

// Integration tests are test code; panicking on setup failure is the point.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use amp_dock::{Layout, PanelId};
use amp_platform::x11::X11Platform;
use amp_platform::{
    Event, MouseButton, Platform, Point, Rect, Size, WindowId, WindowKind, WindowSpec,
};
use amp_ui::DockDrag;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{ConnectionExt as _, SK};
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

fn platform() -> Option<X11Platform> {
    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("no DISPLAY, skipping");
        return None;
    }
    Some(X11Platform::connect().unwrap())
}

/// A second client, for inspecting the server and faking input.
struct Probe {
    conn: RustConnection,
    root: u32,
}

impl Probe {
    fn new() -> Self {
        let (conn, n) = RustConnection::connect(None).unwrap();
        let root = conn.setup().roots[n].root;
        Self { conn, root }
    }

    fn sync(&self) {
        self.conn.get_input_focus().unwrap().reply().unwrap();
    }

    fn move_pointer(&self, p: Point) {
        self.conn
            .xtest_fake_input(
                xproto::MOTION_NOTIFY_EVENT,
                0,
                0,
                self.root,
                p.x as i16,
                p.y as i16,
                0,
            )
            .unwrap();
        self.sync();
    }

    fn button(&self, down: bool) {
        let kind = if down {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        };
        self.conn
            .xtest_fake_input(kind, 1, 0, self.root, 0, 0, 0)
            .unwrap();
        self.sync();
    }
}

fn spec(title: &str, rect: Rect) -> WindowSpec {
    WindowSpec {
        title: title.into(),
        rect,
        kind: WindowKind::Main,
        owner: None,
    }
}

/// Creates, shows and waits until a window is ready to draw.
fn shown(p: &mut X11Platform, title: &str, rect: Rect) -> WindowId {
    let id = p.create_window(&spec(title, rect)).unwrap();
    p.set_visible(id, true).unwrap();
    p.flush().unwrap();
    wait_for(
        p,
        |e| matches!(e, Event::Expose { window } if *window == id),
    );
    id
}

/// Pumps events until one matches, failing after a few seconds.
fn wait_for(p: &mut X11Platform, mut pred: impl FnMut(&Event) -> bool) -> Event {
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

#[test]
fn presented_pixels_reach_the_window() {
    let Some(mut p) = platform() else { return };
    let id = shown(&mut p, "pixels", Rect::new(50, 60, 40, 30));
    let pixels: Vec<u32> = (0..40 * 30u32)
        .map(|i| 0xFF00_0000 | (i * 997) & 0xFF_FFFF)
        .collect();
    p.present(id, Size::new(40, 30), &pixels).unwrap();
    // A round trip on the platform's connection guarantees the image was processed.
    p.pointer().unwrap();

    let probe = Probe::new();
    let img = probe
        .conn
        .get_image(xproto::ImageFormat::Z_PIXMAP, id.0, 0, 0, 40, 30, !0)
        .unwrap()
        .reply()
        .unwrap();
    for (i, chunk) in img.data.as_chunks::<4>().0.iter().enumerate() {
        let got = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]);
        assert_eq!(got, pixels[i] & 0xFF_FFFF, "pixel {i}");
    }
}

#[test]
fn large_frames_are_split_under_the_request_limit() {
    let Some(mut p) = platform() else { return };
    // 3x scale main window: 825x348x4 = 1.1 MB.
    let id = shown(&mut p, "big", Rect::new(0, 0, 825, 348));
    let pixels = vec![0xFF12_3456; 825 * 348];
    p.present(id, Size::new(825, 348), &pixels).unwrap();
    p.pointer().unwrap();
    let probe = Probe::new();
    let img = probe
        .conn
        .get_image(xproto::ImageFormat::Z_PIXMAP, id.0, 824, 347, 1, 1, !0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(&img.data[..3], &[0x56, 0x34, 0x12]);
}

#[test]
fn bad_buffer_size_is_rejected() {
    let Some(mut p) = platform() else { return };
    let id = p
        .create_window(&spec("bad", Rect::new(0, 0, 10, 10)))
        .unwrap();
    assert!(p.present(id, Size::new(10, 10), &[0; 5]).is_err());
    assert!(p.move_window(WindowId(12345), Point::new(0, 0)).is_err());
}

#[test]
fn move_and_resize_are_exact() {
    let Some(mut p) = platform() else { return };
    let id = shown(&mut p, "move", Rect::new(10, 10, 50, 40));
    p.move_window(id, Point::new(300, 200)).unwrap();
    p.resize_window(id, Size::new(80, 20)).unwrap();
    p.flush().unwrap();
    assert_eq!(p.window_rect(id).unwrap(), Rect::new(300, 200, 80, 20));
}

#[test]
fn shape_rectangles_are_applied() {
    let Some(mut p) = platform() else { return };
    assert!(p.has_shape(), "Xvfb provides the Shape extension");
    let id = shown(&mut p, "shape", Rect::new(0, 0, 100, 50));
    let rects = [Rect::new(0, 0, 100, 14), Rect::new(2, 14, 96, 36)];
    p.set_shape(id, Some(&rects)).unwrap();
    p.pointer().unwrap();
    let probe = Probe::new();
    let reply = probe
        .conn
        .shape_get_rectangles(id.0, SK::BOUNDING)
        .unwrap()
        .reply()
        .unwrap();
    let got: Vec<Rect> = reply
        .rectangles
        .iter()
        .map(|r| Rect::new(r.x.into(), r.y.into(), r.width.into(), r.height.into()))
        .collect();
    assert_eq!(got, rects);
    p.set_shape(id, None).unwrap();
    p.pointer().unwrap();
    let reply = probe
        .conn
        .shape_get_rectangles(id.0, SK::BOUNDING)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(reply.rectangles.len(), 1, "back to one full rectangle");
}

#[test]
fn screens_cover_the_display() {
    let Some(p) = platform() else { return };
    let screens = p.screens().unwrap();
    assert!(!screens.is_empty());
    assert!(screens.iter().all(|s| s.w > 0 && s.h > 0));
    eprintln!("screens: {screens:?}");
}

#[test]
fn injected_clicks_arrive_with_local_and_root_coordinates() {
    let Some(mut p) = platform() else { return };
    let id = shown(&mut p, "click", Rect::new(200, 150, 100, 80));
    let probe = Probe::new();
    probe.move_pointer(Point::new(230, 170));
    assert_eq!(p.pointer().unwrap(), Point::new(230, 170));
    probe.button(true);
    let e = wait_for(&mut p, |e| matches!(e, Event::ButtonDown { .. }));
    assert_eq!(
        e,
        Event::ButtonDown {
            window: id,
            button: MouseButton::Left,
            local: Point::new(30, 20),
            root: Point::new(230, 170),
            mods: amp_platform::Modifiers::default(),
        }
    );
    probe.button(false);
    wait_for(&mut p, |e| matches!(e, Event::ButtonUp { .. }));
}

/// Window ids of the root's children, bottom to top.
fn stacking(probe: &Probe) -> Vec<u32> {
    probe
        .conn
        .query_tree(probe.root)
        .unwrap()
        .reply()
        .unwrap()
        .children
}

#[test]
fn panels_record_their_owner_and_raise_restacks() {
    let Some(mut p) = platform() else { return };
    let main = shown(&mut p, "owner", Rect::new(0, 0, 50, 50));
    let mut panel_spec = spec("panel", Rect::new(0, 50, 50, 50));
    panel_spec.kind = WindowKind::Panel;
    panel_spec.owner = Some(main);
    let panel = p.create_window(&panel_spec).unwrap();
    p.set_visible(panel, true).unwrap();
    p.flush().unwrap();
    wait_for(
        &mut p,
        |e| matches!(e, Event::Expose { window } if *window == panel),
    );

    let probe = Probe::new();
    let reply = probe
        .conn
        .get_property(
            false,
            panel.0,
            xproto::AtomEnum::WM_TRANSIENT_FOR,
            xproto::AtomEnum::WINDOW,
            0,
            1,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(reply.value32().unwrap().collect::<Vec<_>>(), vec![main.0]);

    // Another client's window covers ours; raising puts ours back on top.
    let other = probe.conn.generate_id().unwrap();
    probe
        .conn
        .create_window(
            0,
            other,
            probe.root,
            0,
            0,
            100,
            100,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            0,
            &Default::default(),
        )
        .unwrap();
    probe.conn.map_window(other).unwrap();
    probe.sync();
    let pos = |order: &[u32], w: u32| order.iter().position(|x| *x == w).unwrap();
    let before = stacking(&probe);
    assert!(pos(&before, other) > pos(&before, panel.0));
    p.raise(panel).unwrap();
    p.raise(main).unwrap();
    p.pointer().unwrap();
    let after = stacking(&probe);
    assert!(pos(&after, panel.0) > pos(&after, other), "{after:?}");
    assert!(
        pos(&after, main.0) > pos(&after, panel.0),
        "last raised is on top"
    );
}

#[test]
fn close_request_from_window_manager() {
    let Some(mut p) = platform() else { return };
    let id = shown(&mut p, "close", Rect::new(0, 0, 20, 20));
    let probe = Probe::new();
    let atom = |name: &[u8]| {
        probe
            .conn
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let msg = xproto::ClientMessageEvent::new(
        32,
        id.0,
        atom(b"WM_PROTOCOLS"),
        [atom(b"WM_DELETE_WINDOW"), 0, 0, 0, 0],
    );
    probe
        .conn
        .send_event(false, id.0, xproto::EventMask::NO_EVENT, msg)
        .unwrap();
    probe.sync();
    let e = wait_for(&mut p, |e| matches!(e, Event::CloseRequested { .. }));
    assert_eq!(e, Event::CloseRequested { window: id });
}

/// Full path: real pointer input, the platform's events, the dock controller and real window
/// moves. Drags the EQ next to the main window, checks it snaps and docks, then drags the
/// main window and checks the EQ follows.
#[test]
fn dragging_a_window_snaps_and_docks_it() {
    let Some(mut p) = platform() else { return };
    let main_rect = Rect::new(100, 100, 275, 116);
    let eq_rect = Rect::new(600, 500, 275, 116);
    let main = shown(&mut p, "main", main_rect);
    let eq = shown(&mut p, "eq", eq_rect);
    let panels = [(main, PanelId(1)), (eq, PanelId(2))];
    let panel_of = |w: WindowId| panels.iter().find(|(id, _)| *id == w).map(|(_, pid)| *pid);
    let window_of = |pid: PanelId| panels.iter().find(|(_, p)| *p == pid).map(|(w, _)| *w);

    let mut layout = Layout::new();
    layout.insert(PanelId(1), main_rect);
    layout.insert(PanelId(2), eq_rect);
    layout.set_leader(Some(PanelId(1)));
    layout.set_screens(p.screens().unwrap());
    let mut drag = DockDrag::new();

    let probe = Probe::new();
    let run_drag = |p: &mut X11Platform,
                    layout: &mut Layout,
                    drag: &mut DockDrag,
                    from: Point,
                    path: &[Point]| {
        probe.move_pointer(from);
        probe.button(true);
        for pt in path {
            probe.move_pointer(*pt);
        }
        probe.button(false);
        // Feed the resulting events through the controller, as the app will.
        loop {
            let e = wait_for(p, |_| true);
            let (moves, done) = match e {
                Event::ButtonDown {
                    window,
                    button: MouseButton::Left,
                    root,
                    ..
                } => {
                    if let Some(pid) = panel_of(window) {
                        drag.press(layout, pid, root);
                    }
                    (Vec::new(), false)
                }
                Event::Motion { .. } => (drag.motion(layout, p.pointer().unwrap()), false),
                Event::ButtonUp { root, .. } => (drag.release(layout, root), true),
                _ => (Vec::new(), false),
            };
            for (pid, r) in moves {
                p.move_window(window_of(pid).unwrap(), Point::new(r.x, r.y))
                    .unwrap();
            }
            p.flush().unwrap();
            if done {
                break;
            }
        }
    };

    // Grab the EQ and drop it 5 px right of and 9 px below main's bottom-left corner.
    run_drag(
        &mut p,
        &mut layout,
        &mut drag,
        Point::new(610, 505),
        &[
            Point::new(400, 400),
            Point::new(200, 260),
            Point::new(115, 230),
        ],
    );
    assert_eq!(
        p.window_rect(eq).unwrap(),
        Rect::new(100, 216, 275, 116),
        "EQ snapped under main"
    );
    assert_eq!(layout.cluster(PanelId(1)).len(), 2, "docked");

    // Drag the main window: the EQ comes along.
    run_drag(
        &mut p,
        &mut layout,
        &mut drag,
        Point::new(150, 105),
        &[Point::new(300, 200), Point::new(350, 255)],
    );
    assert_eq!(p.window_rect(main).unwrap(), Rect::new(300, 250, 275, 116));
    assert_eq!(p.window_rect(eq).unwrap(), Rect::new(300, 366, 275, 116));
}
