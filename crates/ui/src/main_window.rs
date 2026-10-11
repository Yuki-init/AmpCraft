//! The classic main window: controls, hit testing, interaction and rendering.
//!
//! Input arrives in unscaled window coordinates (0..275, 0..116). User interactions return
//! [`Action`]s for the app to carry out; the window never talks to the player directly.
//! Positions follow the classic layout (cross-checked against Webamp, MIT).

use std::time::Duration;

use amp_dock::Point;
use amp_skin::sprites::{self, Sprite, main, numbers};
use amp_skin::{Bitmap, Rect, Sheet, Skin};

use crate::text::{TextRenderer, TextStrip};

pub const WIDTH: i32 = 275;
pub const HEIGHT: i32 = 116;

/// The title display box.
const MARQUEE: Rect = Rect::new(111, 24, 154, 12);
/// Bitmap-font text sits at this row inside the box.
const MARQUEE_TEXT_Y: i32 = 27;
/// Height of fallback (system font) title text.
pub const FALLBACK_TEXT_HEIGHT: u32 = 11;
/// Scrolling: this many pixels every [`SCROLL_STEP_MS`].
const SCROLL_STEP_PX: i32 = 5;
const SCROLL_STEP_MS: u64 = 220;

const VOLUME_TRAVEL: i32 = 51;
const BALANCE_TRAVEL: i32 = 24;
const POSITION_TRAVEL: i32 = 219;
const THUMB_HALF: i32 = 7;
const POSITION_THUMB_HALF: i32 = 14;

/// Every clickable part of the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    Options,
    Minimize,
    Shade,
    Close,
    ClutterO,
    ClutterA,
    ClutterI,
    ClutterD,
    ClutterV,
    Time,
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Shuffle,
    Repeat,
    Equalizer,
    Playlist,
    Volume,
    Balance,
    Position,
    About,
}

impl Control {
    pub const ALL: [Control; 24] = [
        Control::Options,
        Control::Minimize,
        Control::Shade,
        Control::Close,
        Control::ClutterO,
        Control::ClutterA,
        Control::ClutterI,
        Control::ClutterD,
        Control::ClutterV,
        Control::Time,
        Control::Previous,
        Control::Play,
        Control::Pause,
        Control::Stop,
        Control::Next,
        Control::Eject,
        Control::Shuffle,
        Control::Repeat,
        Control::Equalizer,
        Control::Playlist,
        Control::Volume,
        Control::Balance,
        Control::Position,
        Control::About,
    ];

    /// Where the control sits in the window.
    pub const fn rect(self) -> Rect {
        let (x, y, w, h) = match self {
            Control::Options => (6, 3, 9, 9),
            Control::Minimize => (244, 3, 9, 9),
            Control::Shade => (254, 3, 9, 9),
            Control::Close => (264, 3, 9, 9),
            Control::ClutterO => (10, 25, 8, 8),
            Control::ClutterA => (10, 33, 8, 7),
            Control::ClutterI => (10, 40, 8, 7),
            Control::ClutterD => (10, 47, 8, 8),
            Control::ClutterV => (10, 55, 8, 7),
            Control::Time => (36, 26, 63, 13),
            Control::Previous => (16, 88, 23, 18),
            Control::Play => (39, 88, 23, 18),
            Control::Pause => (62, 88, 23, 18),
            Control::Stop => (85, 88, 23, 18),
            Control::Next => (108, 88, 22, 18),
            Control::Eject => (136, 89, 22, 16),
            Control::Shuffle => (164, 89, 47, 15),
            Control::Repeat => (211, 89, 28, 15),
            Control::Equalizer => (219, 58, 23, 12),
            Control::Playlist => (242, 58, 23, 12),
            Control::Volume => (107, 57, 68, 13),
            Control::Balance => (177, 57, 38, 13),
            Control::Position => (16, 72, 248, 10),
            Control::About => (253, 91, 13, 15),
        };
        Rect::new(x, y, w, h)
    }

    fn contains(self, p: Point) -> bool {
        let r = self.rect();
        p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
    }
}

/// The control under `p`, if any.
pub fn hit_test(p: Point) -> Option<Control> {
    Control::ALL.into_iter().find(|c| c.contains(p))
}

/// Something the user asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    /// Seek to this fraction (0.0..=1.0) of the track.
    Seek(f32),
    /// Seek relative to the current position, in seconds.
    SeekBy(i32),
    SetVolume(f32),
    SetBalance(f32),
    SetShuffle(bool),
    SetRepeat(bool),
    SetEqualizerVisible(bool),
    SetPlaylistVisible(bool),
    Minimize,
    ToggleShade,
    Close,
    ToggleDoubleSize,
    Options,
    About,
    AlwaysOnTop,
    FileInfo,
    Visualization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// What the window shows about the current track. Updated by the app every frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NowPlaying {
    pub state: PlayState,
    pub position: Duration,
    pub duration: Option<Duration>,
    /// "Artist - Title" or a file name.
    pub title: String,
    /// 1-based position in the playlist.
    pub track_number: Option<usize>,
    pub kbps: Option<u32>,
    pub khz: Option<u32>,
    pub channels: u16,
}

/// What a button press started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerDown {
    /// A control took the press.
    Control(Control),
    /// Not on a control: the app should start dragging the window.
    DragWindow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pressed {
    control: Control,
    inside: bool,
}

/// The main window's state.
#[derive(Debug)]
pub struct MainWindow {
    pub now: NowPlaying,
    pub focused: bool,
    volume: f32,
    balance: f32,
    shuffle: bool,
    repeat: bool,
    equalizer_visible: bool,
    playlist_visible: bool,
    double_size: bool,
    show_remaining: bool,
    pressed: Option<Pressed>,
    /// Slider being dragged and its live value.
    slider: Option<(Control, f32)>,
    /// Last title text, its rendered strip, and whether it scrolls.
    title_cache: Option<(String, u32, TextStrip, bool)>,
}

impl Default for MainWindow {
    fn default() -> Self {
        Self::new()
    }
}

fn clamp01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `m:ss`, minutes not padded, as in the title display.
fn short_time(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

/// `mm:ss`, as in the seek message.
fn padded_time(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl MainWindow {
    pub fn new() -> Self {
        Self {
            now: NowPlaying::default(),
            focused: true,
            volume: 0.5,
            balance: 0.0,
            shuffle: false,
            repeat: false,
            equalizer_visible: false,
            playlist_visible: false,
            double_size: false,
            show_remaining: false,
            pressed: None,
            slider: None,
            title_cache: None,
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = clamp01(v);
    }

    pub fn balance(&self) -> f32 {
        self.balance
    }

    pub fn set_balance(&mut self, b: f32) {
        self.balance = if b.is_finite() {
            b.clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
    }

    pub fn repeat(&self) -> bool {
        self.repeat
    }

    pub fn set_repeat(&mut self, on: bool) {
        self.repeat = on;
    }

    pub fn set_equalizer_visible(&mut self, on: bool) {
        self.equalizer_visible = on;
    }

    pub fn set_playlist_visible(&mut self, on: bool) {
        self.playlist_visible = on;
    }

    pub fn double_size(&self) -> bool {
        self.double_size
    }

    pub fn set_double_size(&mut self, on: bool) {
        self.double_size = on;
    }

    pub fn show_remaining(&self) -> bool {
        self.show_remaining
    }

    /// Whether the pointer is busy with a control (the app should not drag the window).
    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some() || self.slider.is_some()
    }

    fn seekable(&self) -> bool {
        self.now.state != PlayState::Stopped && self.now.duration.is_some_and(|d| !d.is_zero())
    }

    fn slider_value(control: Control, p: Point) -> f32 {
        match control {
            Control::Volume => {
                clamp01((p.x - Control::Volume.rect().x - THUMB_HALF) as f32 / VOLUME_TRAVEL as f32)
            }
            Control::Balance => {
                let t = clamp01(
                    (p.x - Control::Balance.rect().x - THUMB_HALF) as f32 / BALANCE_TRAVEL as f32,
                );
                let b = t * 2.0 - 1.0;
                // Snap to center, as the classic slider does.
                if b.abs() < 0.1 { 0.0 } else { b }
            }
            Control::Position => clamp01(
                (p.x - Control::Position.rect().x - POSITION_THUMB_HALF) as f32
                    / POSITION_TRAVEL as f32,
            ),
            _ => 0.0,
        }
    }

    fn live_slider_actions(&mut self) -> Vec<Action> {
        match self.slider {
            Some((Control::Volume, v)) => {
                self.volume = v;
                vec![Action::SetVolume(v)]
            }
            Some((Control::Balance, b)) => {
                self.balance = b;
                vec![Action::SetBalance(b)]
            }
            _ => Vec::new(),
        }
    }

    /// Button press at `p`. Returns what took the press and any immediate actions.
    pub fn pointer_down(&mut self, p: Point) -> (PointerDown, Vec<Action>) {
        let Some(control) = hit_test(p) else {
            return (PointerDown::DragWindow, Vec::new());
        };
        match control {
            Control::Volume | Control::Balance => {
                self.slider = Some((control, Self::slider_value(control, p)));
                (PointerDown::Control(control), self.live_slider_actions())
            }
            Control::Position => {
                if self.seekable() {
                    self.slider = Some((control, Self::slider_value(control, p)));
                }
                (PointerDown::Control(control), Vec::new())
            }
            _ => {
                self.pressed = Some(Pressed {
                    control,
                    inside: true,
                });
                (PointerDown::Control(control), Vec::new())
            }
        }
    }

    /// Pointer motion while a button is held.
    pub fn pointer_move(&mut self, p: Point) -> Vec<Action> {
        if let Some((control, _)) = self.slider {
            self.slider = Some((control, Self::slider_value(control, p)));
            return self.live_slider_actions();
        }
        if let Some(pressed) = self.pressed.as_mut() {
            pressed.inside = pressed.control.contains(p);
        }
        Vec::new()
    }

    /// Button release.
    pub fn pointer_up(&mut self, p: Point) -> Vec<Action> {
        if let Some((control, _)) = self.slider {
            self.slider = Some((control, Self::slider_value(control, p)));
            let mut actions = self.live_slider_actions();
            if let Some((Control::Position, f)) = self.slider {
                actions.push(Action::Seek(f));
            }
            self.slider = None;
            return actions;
        }
        let Some(pressed) = self.pressed.take() else {
            return Vec::new();
        };
        if !pressed.control.contains(p) {
            return Vec::new();
        }
        self.click(pressed.control).into_iter().collect()
    }

    fn click(&mut self, control: Control) -> Option<Action> {
        Some(match control {
            Control::Options | Control::ClutterO => Action::Options,
            Control::Minimize => Action::Minimize,
            Control::Shade => Action::ToggleShade,
            Control::Close => Action::Close,
            Control::ClutterA => Action::AlwaysOnTop,
            Control::ClutterI => Action::FileInfo,
            Control::ClutterD => Action::ToggleDoubleSize,
            Control::ClutterV => Action::Visualization,
            Control::Time => {
                self.show_remaining = !self.show_remaining;
                return None;
            }
            Control::Previous => Action::Previous,
            Control::Play => Action::Play,
            Control::Pause => Action::Pause,
            Control::Stop => Action::Stop,
            Control::Next => Action::Next,
            Control::Eject => Action::Eject,
            Control::Shuffle => {
                self.shuffle = !self.shuffle;
                Action::SetShuffle(self.shuffle)
            }
            Control::Repeat => {
                self.repeat = !self.repeat;
                Action::SetRepeat(self.repeat)
            }
            Control::Equalizer => {
                self.equalizer_visible = !self.equalizer_visible;
                Action::SetEqualizerVisible(self.equalizer_visible)
            }
            Control::Playlist => {
                self.playlist_visible = !self.playlist_visible;
                Action::SetPlaylistVisible(self.playlist_visible)
            }
            Control::About => Action::About,
            Control::Volume | Control::Balance | Control::Position => return None,
        })
    }

    /// Mouse wheel over the window changes the volume.
    pub fn wheel(&mut self, up: bool) -> Vec<Action> {
        self.volume = clamp01(self.volume + if up { 0.05 } else { -0.05 });
        vec![Action::SetVolume(self.volume)]
    }

    /// Classic keyboard shortcuts. `keysym` is an X11 keysym.
    pub fn key(&mut self, keysym: u32) -> Vec<Action> {
        const LEFT: u32 = 0xff51;
        const UP: u32 = 0xff52;
        const RIGHT: u32 = 0xff53;
        const DOWN: u32 = 0xff54;
        let action = match keysym {
            0x7a | 0x5a => Action::Previous, // z
            0x78 | 0x58 => Action::Play,     // x
            0x63 | 0x43 => Action::Pause,    // c
            0x76 | 0x56 => Action::Stop,     // v
            0x62 | 0x42 => Action::Next,     // b
            LEFT => Action::SeekBy(-5),
            RIGHT => Action::SeekBy(5),
            UP | DOWN => {
                let step = if keysym == UP { 0.02 } else { -0.02 };
                self.volume = clamp01(self.volume + step);
                Action::SetVolume(self.volume)
            }
            _ => return Vec::new(),
        };
        vec![action]
    }

    /// The text the title display shows right now.
    pub fn marquee_text(&self) -> String {
        match self.slider {
            Some((Control::Volume, v)) => format!("VOLUME: {}%", (v * 100.0).round() as i32),
            Some((Control::Balance, b)) => {
                let pct = (b.abs() * 100.0).round() as i32;
                match b {
                    b if b < 0.0 => format!("BALANCE: {pct}% LEFT"),
                    b if b > 0.0 => format!("BALANCE: {pct}% RIGHT"),
                    _ => "BALANCE: CENTER".into(),
                }
            }
            Some((Control::Position, f)) => {
                let total = self.now.duration.unwrap_or_default();
                format!(
                    "SEEK TO: {}/{} ({}%)",
                    padded_time(total.mul_f32(f)),
                    padded_time(total),
                    (f * 100.0).round() as i32
                )
            }
            _ => {
                if self.now.title.is_empty() {
                    return String::new();
                }
                let mut s = String::new();
                if let Some(n) = self.now.track_number {
                    s.push_str(&format!("{n}. "));
                }
                s.push_str(&self.now.title);
                if let Some(d) = self.now.duration {
                    s.push_str(&format!(" ({})", short_time(d)));
                }
                s
            }
        }
    }

    /// The rendered title strip for `text`, and whether it must scroll. Titles wider than the
    /// display scroll with a separator; slider messages never scroll.
    fn title_strip(
        &mut self,
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        text: &str,
        static_text: bool,
        scale: u32,
    ) -> (TextStrip, bool) {
        if let Some((cached, cached_scale, strip, scroll)) = &self.title_cache
            && cached == text
            && *cached_scale == scale
        {
            return (strip.clone(), *scroll);
        }
        let height = FALLBACK_TEXT_HEIGHT * scale;
        let mut strip = TextStrip::new(skin, fallback, text, height);
        let unit = if strip.is_bitmap() { 1 } else { scale as i32 };
        let scroll = !static_text && strip.width() > MARQUEE.w * unit;
        if scroll {
            strip = TextStrip::new(skin, fallback, &format!("{text}  ***  "), height);
        }
        self.title_cache = Some((text.to_owned(), scale, strip.clone(), scroll));
        (strip, scroll)
    }

    fn pressed_on(&self, c: Control) -> bool {
        self.pressed.is_some_and(|p| p.control == c && p.inside)
    }

    /// Draws the window at `scale` (1, 2 or 3). Skin sprites and the bitmap font are scaled
    /// by pixel doubling; system-font text is rendered at the final size so it stays sharp.
    /// `now_ms` drives scrolling and blinking.
    pub fn render(
        &mut self,
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        now_ms: u64,
        scale: u32,
    ) -> Bitmap {
        let scale = scale.max(1);
        let mut b = Bitmap::new(WIDTH as u32, HEIGHT as u32, 0xFF00_0000);
        let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| {
            skin.draw(b, s, x, y);
        };
        let pick =
            |pressed: bool, normal: Sprite, down: Sprite| if pressed { down } else { normal };

        draw(&mut b, main::BACKGROUND, 0, 0);
        draw(
            &mut b,
            if self.focused {
                main::TITLE_BAR_ACTIVE
            } else {
                main::TITLE_BAR
            },
            0,
            0,
        );
        for (c, normal, down) in [
            (Control::Options, main::OPTIONS, main::OPTIONS_PRESSED),
            (Control::Minimize, main::MINIMIZE, main::MINIMIZE_PRESSED),
            (Control::Shade, main::SHADE, main::SHADE_PRESSED),
            (Control::Close, main::CLOSE, main::CLOSE_PRESSED),
        ] {
            if self.pressed_on(c) {
                let r = c.rect();
                draw(&mut b, down, r.x, r.y);
            } else if c != Control::Options {
                let r = c.rect();
                draw(&mut b, normal, r.x, r.y);
            }
        }

        draw(&mut b, main::CLUTTER_BAR, 10, 22);
        for (c, on) in [
            (Control::ClutterO, main::CLUTTER_O_ON),
            (Control::ClutterA, main::CLUTTER_A_ON),
            (Control::ClutterI, main::CLUTTER_I_ON),
            (Control::ClutterD, main::CLUTTER_D_ON),
            (Control::ClutterV, main::CLUTTER_V_ON),
        ] {
            if self.pressed_on(c) || (c == Control::ClutterD && self.double_size) {
                let r = c.rect();
                draw(&mut b, on, r.x, r.y);
            }
        }

        // Play state and time.
        match self.now.state {
            PlayState::Playing => {
                draw(&mut b, main::PLAYING, 26, 28);
                draw(&mut b, main::WORKING, 24, 28);
            }
            PlayState::Paused => draw(&mut b, main::PAUSED, 26, 28),
            PlayState::Stopped => draw(&mut b, main::STOPPED, 26, 28),
        }
        let blink_off = self.now.state == PlayState::Paused && (now_ms / 500) % 2 == 1;
        if self.now.state != PlayState::Stopped
            && !blink_off
            && let Some(sheet) = skin.numbers_sheet()
        {
            let shown = match (self.show_remaining, self.now.duration) {
                (true, Some(d)) => d.saturating_sub(self.now.position),
                _ => self.now.position,
            };
            let secs = shown.as_secs();
            let minutes = (secs / 60).min(99) as u8;
            let seconds = (secs % 60) as u8;
            for (digit, x) in [
                (minutes / 10, 48),
                (minutes % 10, 60),
                (seconds / 10, 78),
                (seconds % 10, 90),
            ] {
                draw(&mut b, numbers::digit(sheet, digit), x, 26);
            }
            if self.show_remaining && self.now.duration.is_some() {
                match sheet {
                    Sheet::NumsEx => draw(&mut b, numbers::minus(sheet), 36, 26),
                    _ => draw(&mut b, numbers::minus(sheet), 38, 32),
                }
            }
        }

        // Title display. Bitmap-font titles are drawn now and scaled with the window;
        // system-font titles are drawn at the final size after scaling.
        let text = self.marquee_text();
        let mut late_title = None;
        if !text.is_empty() {
            let static_text = self.slider.is_some();
            let (strip, scroll) = self.title_strip(skin, fallback, &text, static_text, scale);
            let steps = if scroll {
                (now_ms / SCROLL_STEP_MS) as i32
            } else {
                0
            };
            if strip.is_bitmap() {
                let offset = steps.wrapping_mul(SCROLL_STEP_PX);
                strip.draw(&mut b, MARQUEE, MARQUEE_TEXT_Y, offset, scroll, 0);
            } else {
                late_title = Some((strip, scroll, steps));
            }
        }

        // Stream info.
        if self.now.state != PlayState::Stopped {
            if let Some(kbps) = self.now.kbps {
                skin.draw_text(&mut b, &format!("{:>3}", kbps.min(999)), 111, 43);
            }
            if let Some(khz) = self.now.khz {
                skin.draw_text(&mut b, &format!("{:>2}", khz.min(99)), 156, 43);
            }
        }
        let (mono_on, stereo_on) = match (self.now.state, self.now.channels) {
            (PlayState::Stopped, _) | (_, 0) => (false, false),
            (_, 1) => (true, false),
            _ => (false, true),
        };
        draw(&mut b, pick(mono_on, main::MONO, main::MONO_ON), 212, 41);
        draw(
            &mut b,
            pick(stereo_on, main::STEREO, main::STEREO_ON),
            239,
            41,
        );

        // Volume and balance.
        let (volume, balance) = match self.slider {
            Some((Control::Volume, v)) => (v, self.balance),
            Some((Control::Balance, bal)) => (self.volume, bal),
            _ => (self.volume, self.balance),
        };
        let dragging = |c: Control| self.slider.is_some_and(|(s, _)| s == c);
        draw(&mut b, sprites::volume_frame(volume), 107, 57);
        draw(
            &mut b,
            pick(
                dragging(Control::Volume),
                main::VOLUME_THUMB,
                main::VOLUME_THUMB_PRESSED,
            ),
            107 + (volume * VOLUME_TRAVEL as f32).round() as i32,
            58,
        );
        draw(&mut b, sprites::balance_frame(balance), 177, 57);
        draw(
            &mut b,
            pick(
                dragging(Control::Balance),
                main::BALANCE_THUMB,
                main::BALANCE_THUMB_PRESSED,
            ),
            177 + ((balance + 1.0) / 2.0 * BALANCE_TRAVEL as f32).round() as i32,
            58,
        );

        // Window toggles.
        let eq = match (self.equalizer_visible, self.pressed_on(Control::Equalizer)) {
            (false, false) => main::EQ_BUTTON,
            (false, true) => main::EQ_BUTTON_PRESSED,
            (true, false) => main::EQ_BUTTON_ON,
            (true, true) => main::EQ_BUTTON_ON_PRESSED,
        };
        draw(&mut b, eq, 219, 58);
        let pl = match (self.playlist_visible, self.pressed_on(Control::Playlist)) {
            (false, false) => main::PL_BUTTON,
            (false, true) => main::PL_BUTTON_PRESSED,
            (true, false) => main::PL_BUTTON_ON,
            (true, true) => main::PL_BUTTON_ON_PRESSED,
        };
        draw(&mut b, pl, 242, 58);

        // Position bar.
        draw(&mut b, main::POSITION_BACKGROUND, 16, 72);
        if self.seekable() {
            let f = match self.slider {
                Some((Control::Position, f)) => f,
                _ => match self.now.duration {
                    Some(d) => clamp01(self.now.position.as_secs_f32() / d.as_secs_f32()),
                    None => 0.0,
                },
            };
            draw(
                &mut b,
                pick(
                    dragging(Control::Position),
                    main::POSITION_THUMB,
                    main::POSITION_THUMB_PRESSED,
                ),
                16 + (f * POSITION_TRAVEL as f32).round() as i32,
                72,
            );
        }

        // Transport.
        for (c, normal, down) in [
            (Control::Previous, main::PREVIOUS, main::PREVIOUS_PRESSED),
            (Control::Play, main::PLAY, main::PLAY_PRESSED),
            (Control::Pause, main::PAUSE, main::PAUSE_PRESSED),
            (Control::Stop, main::STOP, main::STOP_PRESSED),
            (Control::Next, main::NEXT, main::NEXT_PRESSED),
            (Control::Eject, main::EJECT, main::EJECT_PRESSED),
        ] {
            let r = c.rect();
            draw(&mut b, pick(self.pressed_on(c), normal, down), r.x, r.y);
        }
        let shuffle = match (self.shuffle, self.pressed_on(Control::Shuffle)) {
            (false, false) => main::SHUFFLE,
            (false, true) => main::SHUFFLE_PRESSED,
            (true, false) => main::SHUFFLE_ON,
            (true, true) => main::SHUFFLE_ON_PRESSED,
        };
        draw(&mut b, shuffle, 164, 89);
        let repeat = match (self.repeat, self.pressed_on(Control::Repeat)) {
            (false, false) => main::REPEAT,
            (false, true) => main::REPEAT_PRESSED,
            (true, false) => main::REPEAT_ON,
            (true, true) => main::REPEAT_ON_PRESSED,
        };
        draw(&mut b, repeat, 210, 89);
        let mut out = crate::render::scale_nearest(&b, scale);
        if let Some((strip, scroll, steps)) = late_title {
            let s = scale as i32;
            let clip = Rect::new(MARQUEE.x * s, MARQUEE.y * s, MARQUEE.w * s, MARQUEE.h * s);
            let y = clip.y + (clip.h - strip.height()) / 2;
            let offset = steps.wrapping_mul(SCROLL_STEP_PX * s);
            strip.draw(&mut out, clip, y, offset, scroll, 0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::{code, coded_skin};
    use crate::text::NoFallback;

    fn center(c: Control) -> Point {
        let r = c.rect();
        Point::new(r.x + r.w / 2, r.y + r.h / 2)
    }

    fn click(w: &mut MainWindow, c: Control) -> Vec<Action> {
        let (down, mut actions) = w.pointer_down(center(c));
        assert_eq!(down, PointerDown::Control(c));
        actions.extend(w.pointer_up(center(c)));
        actions
    }

    fn playing(position: u64, duration: u64) -> NowPlaying {
        NowPlaying {
            state: PlayState::Playing,
            position: Duration::from_secs(position),
            duration: Some(Duration::from_secs(duration)),
            title: "Artist - Song".into(),
            track_number: Some(3),
            kbps: Some(320),
            khz: Some(44),
            channels: 2,
        }
    }

    #[test]
    fn every_control_is_hit_at_its_center_and_inside_the_window() {
        for c in Control::ALL {
            let r = c.rect();
            assert!(
                r.x >= 0 && r.y >= 0 && r.x + r.w <= WIDTH && r.y + r.h <= HEIGHT,
                "{c:?}"
            );
            assert_eq!(hit_test(center(c)), Some(c), "{c:?}");
        }
    }

    #[test]
    fn empty_areas_drag_the_window() {
        let mut w = MainWindow::new();
        for p in [Point::new(100, 7), Point::new(150, 50), Point::new(5, 110)] {
            assert_eq!(w.pointer_down(p).0, PointerDown::DragWindow, "{p:?}");
        }
        assert!(!w.is_interacting());
    }

    #[test]
    fn transport_buttons_act_on_release_inside() {
        let mut w = MainWindow::new();
        for (c, a) in [
            (Control::Previous, Action::Previous),
            (Control::Play, Action::Play),
            (Control::Pause, Action::Pause),
            (Control::Stop, Action::Stop),
            (Control::Next, Action::Next),
            (Control::Eject, Action::Eject),
            (Control::Close, Action::Close),
            (Control::Minimize, Action::Minimize),
            (Control::ClutterD, Action::ToggleDoubleSize),
        ] {
            assert_eq!(click(&mut w, c), vec![a], "{c:?}");
        }
    }

    #[test]
    fn releasing_outside_cancels_and_reentering_restores() {
        let mut w = MainWindow::new();
        w.pointer_down(center(Control::Play));
        w.pointer_move(Point::new(200, 10));
        assert!(!w.pressed_on(Control::Play));
        w.pointer_move(center(Control::Play));
        assert!(w.pressed_on(Control::Play));
        w.pointer_move(Point::new(200, 10));
        assert!(w.pointer_up(Point::new(200, 10)).is_empty());
        assert!(!w.is_interacting());
    }

    #[test]
    fn toggles_flip_state() {
        let mut w = MainWindow::new();
        assert_eq!(
            click(&mut w, Control::Shuffle),
            vec![Action::SetShuffle(true)]
        );
        assert_eq!(
            click(&mut w, Control::Shuffle),
            vec![Action::SetShuffle(false)]
        );
        assert_eq!(
            click(&mut w, Control::Repeat),
            vec![Action::SetRepeat(true)]
        );
        assert_eq!(
            click(&mut w, Control::Equalizer),
            vec![Action::SetEqualizerVisible(true)]
        );
        assert_eq!(
            click(&mut w, Control::Playlist),
            vec![Action::SetPlaylistVisible(true)]
        );
        assert!(click(&mut w, Control::Time).is_empty());
        assert!(w.show_remaining());
    }

    #[test]
    fn volume_follows_the_pointer_and_clamps() {
        let mut w = MainWindow::new();
        let r = Control::Volume.rect();
        let (_, a) = w.pointer_down(Point::new(r.x + 7, r.y + 5));
        assert_eq!(a, vec![Action::SetVolume(0.0)]);
        assert_eq!(w.marquee_text(), "VOLUME: 0%");
        assert_eq!(
            w.pointer_move(Point::new(r.x + 7 + 51, 0)),
            vec![Action::SetVolume(1.0)]
        );
        assert_eq!(
            w.pointer_move(Point::new(900, 0)),
            vec![Action::SetVolume(1.0)]
        );
        assert_eq!(w.marquee_text(), "VOLUME: 100%");
        w.pointer_up(Point::new(r.x + 7 + 51, 0));
        assert_eq!(w.volume(), 1.0);
        assert_eq!(w.marquee_text(), "", "message ends with the drag");
    }

    #[test]
    fn balance_snaps_to_center() {
        let mut w = MainWindow::new();
        let r = Control::Balance.rect();
        let (_, a) = w.pointer_down(Point::new(r.x + 7 + 13, r.y + 5));
        assert_eq!(a, vec![Action::SetBalance(0.0)]);
        assert_eq!(w.marquee_text(), "BALANCE: CENTER");
        w.pointer_move(Point::new(r.x + 7, 0));
        assert_eq!(w.marquee_text(), "BALANCE: 100% LEFT");
        w.pointer_up(Point::new(r.x + 7, 0));
        assert_eq!(w.balance(), -1.0);
    }

    #[test]
    fn seeking_reports_on_release_only() {
        let mut w = MainWindow::new();
        w.now = playing(10, 200);
        let r = Control::Position.rect();
        let (_, a) = w.pointer_down(Point::new(r.x + 14 + 109, r.y + 5));
        assert!(a.is_empty());
        assert!(w.pointer_move(Point::new(r.x + 14 + 219, 0)).is_empty());
        assert_eq!(w.marquee_text(), "SEEK TO: 03:20/03:20 (100%)");
        assert_eq!(
            w.pointer_up(Point::new(r.x + 14 + 219, 0)),
            vec![Action::Seek(1.0)]
        );
    }

    #[test]
    fn position_bar_is_inert_when_stopped() {
        let mut w = MainWindow::new();
        let (down, a) = w.pointer_down(center(Control::Position));
        assert_eq!(down, PointerDown::Control(Control::Position));
        assert!(a.is_empty());
        assert!(w.pointer_up(center(Control::Position)).is_empty());
    }

    #[test]
    fn wheel_and_keys() {
        let mut w = MainWindow::new();
        w.set_volume(0.98);
        assert_eq!(w.wheel(true), vec![Action::SetVolume(1.0)]);
        assert_eq!(w.key(0x78), vec![Action::Play]);
        assert_eq!(w.key(0x42), vec![Action::Next]);
        assert_eq!(w.key(0xff51), vec![Action::SeekBy(-5)]);
        assert_eq!(w.key(0xff54), vec![Action::SetVolume(0.98)]);
        assert!(w.key(0x71).is_empty());
    }

    #[test]
    fn title_text_format() {
        let mut w = MainWindow::new();
        assert_eq!(w.marquee_text(), "");
        w.now = playing(0, 225);
        assert_eq!(w.marquee_text(), "3. Artist - Song (3:45)");
    }

    // Rendering checks use a skin whose pixels encode their own sheet coordinates.

    fn px(b: &Bitmap, x: i32, y: i32) -> u32 {
        b.get(x, y).unwrap()
    }

    #[test]
    fn pressed_button_draws_pressed_sprite() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        let normal = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(px(&normal, 39, 88), code(Sheet::Cbuttons, 23, 0));
        w.pointer_down(center(Control::Play));
        let pressed = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(px(&pressed, 39, 88), code(Sheet::Cbuttons, 23, 18));
    }

    #[test]
    fn time_digits_elapsed_and_remaining() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.now = playing(83, 300);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        // 01:23 from NUMS_EX.
        for (digit, x) in [(0, 48), (1, 60), (2, 78), (3, 90)] {
            assert_eq!(
                px(&b, x, 26),
                code(Sheet::NumsEx, digit * 9, 0),
                "digit {digit}"
            );
        }
        click(&mut w, Control::Time);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        // 300 - 83 = 217 s = 03:37, with a minus sign.
        for (digit, x) in [(0, 48), (3, 60), (3, 78), (7, 90)] {
            assert_eq!(
                px(&b, x, 26),
                code(Sheet::NumsEx, digit * 9, 0),
                "digit {digit}"
            );
        }
        assert_eq!(px(&b, 36, 26), code(Sheet::NumsEx, 99, 0));
    }

    #[test]
    fn paused_time_blinks_and_stopped_hides_it() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.now = playing(83, 300);
        w.now.state = PlayState::Paused;
        let shown = w.render(&skin, &mut NoFallback, 100, 1);
        let hidden = w.render(&skin, &mut NoFallback, 600, 1);
        assert_eq!(px(&shown, 48, 26), code(Sheet::NumsEx, 0, 0));
        assert_eq!(px(&hidden, 48, 26), code(Sheet::Main, 48, 26));
        w.now.state = PlayState::Stopped;
        let stopped = w.render(&skin, &mut NoFallback, 100, 1);
        assert_eq!(px(&stopped, 48, 26), code(Sheet::Main, 48, 26));
        assert_eq!(
            px(&stopped, 26, 28),
            code(Sheet::Playpaus, 18, 0),
            "stopped indicator"
        );
    }

    #[test]
    fn mono_stereo_and_stream_info() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.now = playing(1, 10);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(px(&b, 212, 41), code(Sheet::Monoster, 29, 12), "mono off");
        assert_eq!(px(&b, 239, 41), code(Sheet::Monoster, 0, 0), "stereo on");
        // "320" in the bitmap font: '3' is row 1, column 3.
        assert_eq!(px(&b, 111, 43), code(Sheet::Text, 15, 6));
        w.now.channels = 1;
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(px(&b, 212, 41), code(Sheet::Monoster, 29, 0), "mono on");
    }

    #[test]
    fn sliders_and_position_thumb_positions() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.set_volume(1.0);
        w.now = playing(50, 100);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(
            px(&b, 107 + 51, 58),
            code(Sheet::Volume, 15, 422),
            "volume thumb at max"
        );
        assert_eq!(
            px(&b, 107, 57),
            code(Sheet::Volume, 0, 27 * 15),
            "loudest frame"
        );
        assert_eq!(
            px(&b, 177 + 12, 58),
            code(Sheet::Balance, 15, 422),
            "balance centered"
        );
        let x = 16 + (0.5f32 * 219.0).round() as i32;
        assert_eq!(px(&b, x, 72), code(Sheet::Posbar, 248, 0), "thumb at half");
    }

    #[test]
    fn long_titles_scroll() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.now = playing(0, 10);
        w.now.title = "A VERY LONG TITLE THAT DOES NOT FIT INTO THE DISPLAY AT ALL".into();
        let a = w.render(&skin, &mut NoFallback, 0, 1);
        let b = w.render(&skin, &mut NoFallback, 1000, 1);
        let row = |bm: &Bitmap| (111..265).map(|x| px(bm, x, 27)).collect::<Vec<_>>();
        assert_ne!(row(&a), row(&b));
        // Short titles stay put.
        w.now.title = "SHORT".into();
        let c = w.render(&skin, &mut NoFallback, 0, 1);
        let d = w.render(&skin, &mut NoFallback, 1000, 1);
        assert_eq!(row(&c), row(&d));
    }

    /// Records the heights it was asked for and draws solid blocks.
    struct Recorder(Vec<u32>);

    impl TextRenderer for Recorder {
        fn render(&mut self, text: &str, height: u32) -> Option<crate::text::Mask> {
            self.0.push(height);
            let width = text.chars().count() as u32 * height / 2;
            Some(crate::text::Mask {
                width,
                height,
                alpha: vec![255; (width * height) as usize],
            })
        }
    }

    #[test]
    fn system_font_titles_render_at_the_final_size() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.now = playing(0, 10);
        w.now.title = "日本".into();
        let mut rec = Recorder(Vec::new());
        let b = w.render(&skin, &mut rec, 0, 2);
        assert_eq!((b.width(), b.height()), (550, 232));
        assert_eq!(rec.0, vec![22], "asked for 2x height, once");
        let (_, fg) = crate::text::text_colors(&skin);
        // Text starts at the scaled title box, at full resolution (not pixel-doubled).
        assert_eq!(b.get(222, 50), Some(fg));
        // Cached: a second frame does not re-render.
        w.render(&skin, &mut rec, 0, 2);
        assert_eq!(rec.0.len(), 1);
        // Bitmap-font titles keep the classic doubled look.
        w.now.title = "ASCII".into();
        let b = w.render(&skin, &mut rec, 0, 2);
        assert_eq!(b.get(2 * 120, 2 * 27), b.get(2 * 120 + 1, 2 * 27 + 1));
    }

    #[test]
    fn double_size_lights_the_d_button() {
        let skin = coded_skin();
        let mut w = MainWindow::new();
        w.set_double_size(true);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(px(&b, 10, 47), code(Sheet::Titlebar, 328, 69));
    }
}
