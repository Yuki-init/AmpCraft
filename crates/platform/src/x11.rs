//! X11 backend on x11rb's pure-Rust connection. Works on Xorg and XWayland.
//!
//! A helper thread blocks on the connection and forwards events over a channel, so
//! [`X11Platform::next_event`] can wait with a timeout. Requests are sent from the caller's
//! thread; the connection is shared and thread safe.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::properties::{WmHints, WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    self, AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, ImageFormat, ImageOrder, PropMode, VisualClass, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::{
    Event, MouseButton, Platform, PlatformError, Point, Rect, Result, Size, WindowId, WindowKind,
    WindowSpec,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        WM_CHANGE_STATE,
        UTF8_STRING,
        _NET_WM_NAME,
        _MOTIF_WM_HINTS,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WM_STATE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WORKAREA,
        _OPENAMP_WAKE,
    }
}

/// Converts any x11rb error into a [`PlatformError`].
trait OrConn<T> {
    fn conn(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> OrConn<T> for std::result::Result<T, E> {
    fn conn(self) -> Result<T> {
        self.map_err(|e| PlatformError::Connection(e.to_string()))
    }
}

fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn clamp_u16(v: i32) -> u16 {
    v.clamp(1, i32::from(u16::MAX)) as u16
}

struct Keymap {
    min_keycode: u8,
    per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Keymap {
    fn keysym(&self, keycode: u8) -> u32 {
        let i = usize::from(keycode.saturating_sub(self.min_keycode)) * self.per_keycode;
        self.keysyms.get(i).copied().unwrap_or(0)
    }
}

/// The X11 implementation of [`Platform`].
pub struct X11Platform {
    conn: Arc<RustConnection>,
    root: xproto::Window,
    depth: u8,
    visual: xproto::Visualid,
    gc: xproto::Gcontext,
    atoms: Atoms,
    windows: BTreeMap<u32, Size>,
    control: xproto::Window,
    events: Receiver<std::result::Result<XEvent, String>>,
    pending: Option<Event>,
    thread: Option<JoinHandle<()>>,
    keymap: Keymap,
    has_shape: bool,
    has_monitors: bool,
}

impl std::fmt::Debug for X11Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X11Platform")
            .field("root", &self.root)
            .field("windows", &self.windows.len())
            .finish_non_exhaustive()
    }
}

impl X11Platform {
    /// Connects to `$DISPLAY`.
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) =
            RustConnection::connect(None).map_err(|e| PlatformError::Connect(e.to_string()))?;
        let conn = Arc::new(conn);
        let setup = conn.setup();
        let screen = setup
            .roots
            .get(screen_num)
            .ok_or_else(|| PlatformError::Unsupported("no such screen".into()))?;
        let (root, depth, visual) = (screen.root, screen.root_depth, screen.root_visual);

        // We upload 32-bit little-endian BGRX pixels: insist on a matching TrueColor visual.
        let format_ok = setup
            .pixmap_formats
            .iter()
            .any(|f| f.depth == depth && f.bits_per_pixel == 32);
        let visual_ok = screen
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .any(|v| {
                v.visual_id == visual
                    && v.class == VisualClass::TRUE_COLOR
                    && (v.red_mask, v.green_mask, v.blue_mask) == (0xff0000, 0xff00, 0xff)
            });
        if !(format_ok
            && visual_ok
            && depth >= 24
            && setup.image_byte_order == ImageOrder::LSB_FIRST)
        {
            return Err(PlatformError::Unsupported(format!(
                "need a 24/32-bit little-endian TrueColor visual (depth {depth})"
            )));
        }

        let atoms = Atoms::new(&*conn).conn()?.reply().conn()?;
        let gc = conn.generate_id().conn()?;
        conn.create_gc(gc, root, &CreateGCAux::new().graphics_exposures(0))
            .conn()?;
        let control = conn.generate_id().conn()?;
        conn.create_window(
            0,
            control,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )
        .conn()?;

        let has_shape = conn
            .extension_information(shape::X11_EXTENSION_NAME)
            .conn()?
            .is_some();
        let has_monitors = conn
            .randr_query_version(1, 5)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|v| (v.major_version, v.minor_version) >= (1, 5));

        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min, max - min + 1)
            .conn()?
            .reply()
            .conn()?;
        let keymap = Keymap {
            min_keycode: min,
            per_keycode: usize::from(mapping.keysyms_per_keycode).max(1),
            keysyms: mapping.keysyms,
        };

        let (tx, events) = mpsc::channel();
        let reader = Arc::clone(&conn);
        let wake = atoms._OPENAMP_WAKE;
        let thread = std::thread::Builder::new()
            .name("x11-events".into())
            .spawn(move || {
                loop {
                    match reader.wait_for_event() {
                        Ok(XEvent::ClientMessage(e)) if e.type_ == wake && e.window == control => {
                            return;
                        }
                        Ok(ev) => {
                            if tx.send(Ok(ev)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string()));
                            return;
                        }
                    }
                }
            })
            .map_err(|e| PlatformError::Connection(e.to_string()))?;
        conn.flush().conn()?;

        Ok(Self {
            conn,
            root,
            depth,
            visual,
            gc,
            atoms,
            windows: BTreeMap::new(),
            control,
            events,
            pending: None,
            thread: Some(thread),
            keymap,
            has_shape,
            has_monitors,
        })
    }

    /// Whether the Shape extension is available (needed for skinned window shapes).
    pub fn has_shape(&self) -> bool {
        self.has_shape
    }

    /// The window's current position and size on the desktop, asked from the server.
    pub fn window_rect(&self, id: WindowId) -> Result<Rect> {
        self.check(id)?;
        let geo = self.conn.get_geometry(id.0).conn()?.reply().conn()?;
        let pos = self
            .conn
            .translate_coordinates(id.0, self.root, 0, 0)
            .conn()?
            .reply()
            .conn()?;
        Ok(Rect::new(
            i32::from(pos.dst_x),
            i32::from(pos.dst_y),
            i32::from(geo.width),
            i32::from(geo.height),
        ))
    }

    fn check(&self, id: WindowId) -> Result<()> {
        if self.windows.contains_key(&id.0) {
            Ok(())
        } else {
            Err(PlatformError::UnknownWindow(id))
        }
    }

    fn translate(&self, ev: XEvent) -> Option<Event> {
        let known = |w: u32| self.windows.contains_key(&w);
        let button = |b: u8| match b {
            1 => MouseButton::Left,
            2 => MouseButton::Middle,
            3 => MouseButton::Right,
            4 => MouseButton::WheelUp,
            5 => MouseButton::WheelDown,
            n => MouseButton::Other(n),
        };
        let p = |x: i16, y: i16| Point::new(i32::from(x), i32::from(y));
        Some(match ev {
            XEvent::ButtonPress(e) if known(e.event) => Event::ButtonDown {
                window: WindowId(e.event),
                button: button(e.detail),
                local: p(e.event_x, e.event_y),
                root: p(e.root_x, e.root_y),
            },
            XEvent::ButtonRelease(e) if known(e.event) => Event::ButtonUp {
                window: WindowId(e.event),
                button: button(e.detail),
                local: p(e.event_x, e.event_y),
                root: p(e.root_x, e.root_y),
            },
            XEvent::MotionNotify(e) if known(e.event) => Event::Motion {
                window: WindowId(e.event),
                local: p(e.event_x, e.event_y),
                root: p(e.root_x, e.root_y),
            },
            XEvent::Expose(e) if known(e.window) && e.count == 0 => Event::Expose {
                window: WindowId(e.window),
            },
            XEvent::ClientMessage(e)
                if known(e.window)
                    && e.type_ == self.atoms.WM_PROTOCOLS
                    && e.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW =>
            {
                Event::CloseRequested {
                    window: WindowId(e.window),
                }
            }
            XEvent::FocusIn(e) if known(e.event) => Event::Focus {
                window: WindowId(e.event),
                focused: true,
            },
            XEvent::FocusOut(e) if known(e.event) => Event::Focus {
                window: WindowId(e.event),
                focused: false,
            },
            XEvent::KeyPress(e) if known(e.event) => Event::Key {
                window: WindowId(e.event),
                keysym: self.keymap.keysym(e.detail),
                pressed: true,
            },
            XEvent::KeyRelease(e) if known(e.event) => Event::Key {
                window: WindowId(e.event),
                keysym: self.keymap.keysym(e.detail),
                pressed: false,
            },
            XEvent::ConfigureNotify(e) if known(e.window) => {
                // Coordinates in ConfigureNotify are relative to the parent, which is a frame
                // under reparenting window managers. Ask for the desktop position instead.
                let rect = self.window_rect(WindowId(e.window)).ok()?;
                Event::Configured {
                    window: WindowId(e.window),
                    rect,
                }
            }
            XEvent::Error(e) => {
                log::warn!("X11 error: {e:?}");
                return None;
            }
            _ => return None,
        })
    }

    /// Receives one raw event, translated; None on timeout.
    fn receive(&mut self, deadline: Option<Instant>) -> Result<Option<Event>> {
        loop {
            let raw = match deadline {
                None => self
                    .events
                    .recv()
                    .map_err(|_| PlatformError::Connection("event thread ended".into()))?,
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    match self.events.recv_timeout(left) {
                        Ok(r) => r,
                        Err(RecvTimeoutError::Timeout) => return Ok(None),
                        Err(RecvTimeoutError::Disconnected) => {
                            return Err(PlatformError::Connection("event thread ended".into()));
                        }
                    }
                }
            };
            let ev = raw.map_err(PlatformError::Connection)?;
            if let Some(e) = self.translate(ev) {
                return Ok(Some(e));
            }
        }
    }

    fn set_motif_and_type(&self, id: u32, kind: WindowKind) -> Result<()> {
        let a = &self.atoms;
        // flags = MWM_HINTS_DECORATIONS, decorations = 0: no frame.
        self.conn
            .change_property32(
                PropMode::REPLACE,
                id,
                a._MOTIF_WM_HINTS,
                a._MOTIF_WM_HINTS,
                &[2, 0, 0, 0, 0],
            )
            .conn()?;
        let wtype = match kind {
            WindowKind::Main => a._NET_WM_WINDOW_TYPE_NORMAL,
            WindowKind::Panel => a._NET_WM_WINDOW_TYPE_UTILITY,
        };
        self.conn
            .change_property32(
                PropMode::REPLACE,
                id,
                a._NET_WM_WINDOW_TYPE,
                AtomEnum::ATOM,
                &[wtype],
            )
            .conn()?;
        if kind == WindowKind::Panel {
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    id,
                    a._NET_WM_STATE,
                    AtomEnum::ATOM,
                    &[a._NET_WM_STATE_SKIP_TASKBAR, a._NET_WM_STATE_SKIP_PAGER],
                )
                .conn()?;
        }
        Ok(())
    }

    fn workarea(&self) -> Option<Rect> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.atoms._NET_WORKAREA,
                AtomEnum::CARDINAL,
                0,
                4,
            )
            .ok()?
            .reply()
            .ok()?;
        let v: Vec<u32> = reply.value32()?.collect();
        match v[..] {
            [x, y, w, h] if w > 0 && h > 0 => {
                Some(Rect::new(x as i32, y as i32, w as i32, h as i32))
            }
            _ => None,
        }
    }
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let l = a.left().max(b.left());
    let t = a.top().max(b.top());
    let r = a.right().min(b.right());
    let btm = a.bottom().min(b.bottom());
    (r > l && btm > t).then(|| Rect::new(l, t, r - l, btm - t))
}

impl Platform for X11Platform {
    fn create_window(&mut self, spec: &WindowSpec) -> Result<WindowId> {
        let id = self.conn.generate_id().conn()?;
        let r = spec.rect;
        let mask = EventMask::EXPOSURE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::KEY_PRESS
            | EventMask::KEY_RELEASE
            | EventMask::FOCUS_CHANGE
            | EventMask::STRUCTURE_NOTIFY;
        self.conn
            .create_window(
                self.depth,
                id,
                self.root,
                clamp_i16(r.x),
                clamp_i16(r.y),
                clamp_u16(r.w),
                clamp_u16(r.h),
                0,
                WindowClass::INPUT_OUTPUT,
                self.visual,
                &CreateWindowAux::new()
                    .background_pixel(0)
                    .border_pixel(0)
                    .event_mask(mask),
            )
            .conn()?;

        let title = spec.title.as_bytes();
        self.conn
            .change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title,
            )
            .conn()?;
        self.conn
            .change_property8(
                PropMode::REPLACE,
                id,
                self.atoms._NET_WM_NAME,
                self.atoms.UTF8_STRING,
                title,
            )
            .conn()?;
        self.conn
            .change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"openamp\0OpenAMP\0",
            )
            .conn()?;
        self.conn
            .change_property32(
                PropMode::REPLACE,
                id,
                self.atoms.WM_PROTOCOLS,
                AtomEnum::ATOM,
                &[self.atoms.WM_DELETE_WINDOW],
            )
            .conn()?;
        // Ask the window manager to honor our position and size.
        let mut hints = WmSizeHints::new();
        hints.position = Some((WmSizeHintsSpecification::UserSpecified, r.x, r.y));
        hints.size = Some((WmSizeHintsSpecification::UserSpecified, r.w, r.h));
        hints.set_normal_hints(&*self.conn, id).conn()?;
        let mut wm_hints = WmHints::new();
        wm_hints.input = Some(true);
        wm_hints.set(&*self.conn, id).conn()?;
        self.set_motif_and_type(id, spec.kind)?;

        self.windows.insert(id, Size::new(r.w, r.h));
        Ok(WindowId(id))
    }

    fn destroy_window(&mut self, id: WindowId) -> Result<()> {
        self.check(id)?;
        self.conn.destroy_window(id.0).conn()?;
        self.windows.remove(&id.0);
        Ok(())
    }

    fn set_visible(&mut self, id: WindowId, visible: bool) -> Result<()> {
        self.check(id)?;
        if visible {
            self.conn.map_window(id.0).conn()?;
        } else {
            self.conn.unmap_window(id.0).conn()?;
        }
        Ok(())
    }

    fn minimize(&mut self, id: WindowId) -> Result<()> {
        self.check(id)?;
        // ICCCM: ask the window manager for IconicState (3).
        let msg = ClientMessageEvent::new(32, id.0, self.atoms.WM_CHANGE_STATE, [3u32, 0, 0, 0, 0]);
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                msg,
            )
            .conn()?;
        Ok(())
    }

    fn move_window(&mut self, id: WindowId, pos: Point) -> Result<()> {
        self.check(id)?;
        self.conn
            .configure_window(id.0, &ConfigureWindowAux::new().x(pos.x).y(pos.y))
            .conn()?;
        Ok(())
    }

    fn resize_window(&mut self, id: WindowId, size: Size) -> Result<()> {
        self.check(id)?;
        let (w, h) = (clamp_u16(size.w), clamp_u16(size.h));
        self.conn
            .configure_window(
                id.0,
                &ConfigureWindowAux::new()
                    .width(u32::from(w))
                    .height(u32::from(h)),
            )
            .conn()?;
        self.windows
            .insert(id.0, Size::new(i32::from(w), i32::from(h)));
        Ok(())
    }

    fn present(&mut self, id: WindowId, size: Size, pixels: &[u32]) -> Result<()> {
        self.check(id)?;
        let (w, h) = (size.w.max(0) as usize, size.h.max(0) as usize);
        if pixels.len() != w * h {
            return Err(PlatformError::BadBuffer {
                got: pixels.len(),
                expected: w * h,
            });
        }
        if w == 0 || h == 0 {
            return Ok(());
        }
        // Keep every PutImage under the server's request size limit.
        let max_bytes = self.conn.maximum_request_bytes().saturating_sub(64);
        let rows_per_band = (max_bytes / (w * 4)).clamp(1, h);
        let mut bytes = Vec::with_capacity(rows_per_band * w * 4);
        for (band_index, band) in pixels.chunks(rows_per_band * w).enumerate() {
            bytes.clear();
            bytes.extend(band.iter().flat_map(|p| p.to_le_bytes()));
            let rows = band.len() / w;
            self.conn
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    id.0,
                    self.gc,
                    w as u16,
                    rows as u16,
                    0,
                    clamp_i16((band_index * rows_per_band) as i32),
                    0,
                    self.depth,
                    &bytes,
                )
                .conn()?;
        }
        Ok(())
    }

    fn set_shape(&mut self, id: WindowId, rects: Option<&[Rect]>) -> Result<()> {
        self.check(id)?;
        if !self.has_shape {
            return Ok(());
        }
        match rects {
            Some(rects) => {
                let xr: Vec<xproto::Rectangle> = rects
                    .iter()
                    .filter(|r| r.w > 0 && r.h > 0)
                    .map(|r| xproto::Rectangle {
                        x: clamp_i16(r.x),
                        y: clamp_i16(r.y),
                        width: clamp_u16(r.w),
                        height: clamp_u16(r.h),
                    })
                    .collect();
                self.conn
                    .shape_rectangles(
                        shape::SO::SET,
                        shape::SK::BOUNDING,
                        xproto::ClipOrdering::UNSORTED,
                        id.0,
                        0,
                        0,
                        &xr,
                    )
                    .conn()?;
            }
            None => {
                self.conn
                    .shape_mask(shape::SO::SET, shape::SK::BOUNDING, id.0, 0, 0, x11rb::NONE)
                    .conn()?;
            }
        }
        Ok(())
    }

    fn pointer(&self) -> Result<Point> {
        let r = self.conn.query_pointer(self.root).conn()?.reply().conn()?;
        Ok(Point::new(i32::from(r.root_x), i32::from(r.root_y)))
    }

    fn screens(&self) -> Result<Vec<Rect>> {
        let mut monitors = Vec::new();
        if self.has_monitors
            && let Ok(reply) = self
                .conn
                .randr_get_monitors(self.root, true)
                .conn()?
                .reply()
        {
            monitors.extend(reply.monitors.iter().map(|m| {
                Rect::new(
                    i32::from(m.x),
                    i32::from(m.y),
                    i32::from(m.width),
                    i32::from(m.height),
                )
            }));
        }
        if monitors.is_empty() {
            let geo = self.conn.get_geometry(self.root).conn()?.reply().conn()?;
            monitors.push(Rect::new(0, 0, i32::from(geo.width), i32::from(geo.height)));
        }
        // Keep panels off docks and panels reserved by the desktop.
        if let Some(work) = self.workarea() {
            monitors = monitors
                .into_iter()
                .map(|m| intersect(m, work).unwrap_or(m))
                .collect();
        }
        Ok(monitors)
    }

    fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>> {
        if let Some(e) = self.pending.take() {
            return Ok(Some(e));
        }
        let deadline = timeout.map(|t| Instant::now() + t);
        let Some(first) = self.receive(deadline)? else {
            return Ok(None);
        };
        let Event::Motion { window, .. } = first else {
            return Ok(Some(first));
        };
        // Coalesce queued motion for the same window into the latest one.
        let mut latest = first;
        while let Ok(raw) = self.events.try_recv() {
            match self.translate(raw.map_err(PlatformError::Connection)?) {
                Some(e @ Event::Motion { window: w, .. }) if w == window => latest = e,
                Some(other) => {
                    self.pending = Some(other);
                    break;
                }
                None => {}
            }
        }
        Ok(Some(latest))
    }

    fn flush(&mut self) -> Result<()> {
        self.conn.flush().conn()
    }
}

impl Drop for X11Platform {
    fn drop(&mut self) {
        for id in std::mem::take(&mut self.windows).into_keys() {
            let _ = self.conn.destroy_window(id);
        }
        // Wake the event thread so it can exit.
        let wake = ClientMessageEvent::new(32, self.control, self.atoms._OPENAMP_WAKE, [0u32; 5]);
        let _ = self
            .conn
            .send_event(false, self.control, EventMask::NO_EVENT, wake);
        let _ = self.conn.destroy_window(self.control);
        let _ = self.conn.flush();
        if let Some(t) = self.thread.take()
            && t.join().is_err()
        {
            log::error!("X11 event thread panicked");
        }
    }
}
