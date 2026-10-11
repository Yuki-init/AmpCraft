//! The playlist window: a resizable track list.
//!
//! Sizes are unscaled and grow in classic steps (25 px wide, 29 px tall) from 275x116.
//! Track text uses the system font in the skin's pledit.txt colors, with the bitmap font as a
//! fallback. Like the main window, interactions return actions for the app to perform.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use amp_dock::Point;
use amp_skin::sprites::playlist as sp;
use amp_skin::{Bitmap, Rect, Skin, Sprite};

use crate::main_window::{Action, NowPlaying, PlayState};
use crate::text::{TextRenderer, TextStrip};

pub const MIN_W: i32 = 275;
pub const MIN_H: i32 = 116;
pub const STEP_W: i32 = 25;
pub const STEP_H: i32 = 29;
const TOP: i32 = 20;
const BOTTOM: i32 = 38;
const LEFT: i32 = 12;
const RIGHT: i32 = 20;
const ROW_H: i32 = 13;
const ROW_TEXT_H: u32 = 10;
const LIST_PAD: i32 = 3;
const HANDLE_H: i32 = 18;
const DOUBLE_CLICK_MS: u64 = 400;
const ROW_CACHE_LIMIT: usize = 512;

/// Opaque id of a playlist entry (the app uses the queue's track ids).
pub type EntryId = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistEntry {
    pub id: EntryId,
    pub title: String,
    pub duration: Option<Duration>,
}

/// Clickable parts of the playlist window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlControl {
    Close,
    Shade,
    Resize,
    Add,
    Remove,
    Select,
    Misc,
    ListMenu,
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Scrollbar,
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaylistAction {
    PlayEntry(EntryId),
    Remove(Vec<EntryId>),
    AddFiles,
    Close,
    ToggleShade,
    /// The window wants this (unscaled) size.
    Resize(i32, i32),
    /// A mini transport button.
    Transport(Action),
}

/// What a button press started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlDown {
    Control(PlControl),
    DragWindow,
}

/// Modifier keys for list selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectMods {
    pub shift: bool,
    pub ctrl: bool,
}

#[derive(Debug)]
pub struct PlaylistWindow {
    w: i32,
    h: i32,
    entries: Vec<PlaylistEntry>,
    current: Option<EntryId>,
    selected: BTreeSet<EntryId>,
    anchor: Option<usize>,
    scroll: usize,
    pub focused: bool,
    pub shaded: bool,
    /// Now playing, for the mini time display.
    pub now: NowPlaying,
    pressed: Option<(PlControl, bool)>,
    /// Pointer offset inside the scroll handle while dragging it.
    scroll_drag: Option<i32>,
    /// Pointer position and window size when a resize began.
    resize: Option<(Point, i32, i32)>,
    last_click: Option<(EntryId, u64)>,
    rows: HashMap<(String, u32, u32), TextStrip>,
}

impl Default for PlaylistWindow {
    fn default() -> Self {
        Self::new()
    }
}

fn duration_text(d: Option<Duration>) -> String {
    match d {
        Some(d) => {
            let s = d.as_secs();
            if s >= 3600 {
                format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
            } else {
                format!("{}:{:02}", s / 60, s % 60)
            }
        }
        None => String::new(),
    }
}

fn snap(v: i32, min: i32, step: i32) -> i32 {
    min + ((v - min).max(0) / step) * step
}

impl PlaylistWindow {
    pub fn new() -> Self {
        Self {
            w: MIN_W,
            h: MIN_H + 4 * STEP_H,
            entries: Vec::new(),
            current: None,
            selected: BTreeSet::new(),
            anchor: None,
            scroll: 0,
            focused: false,
            shaded: false,
            now: NowPlaying::default(),
            pressed: None,
            scroll_drag: None,
            resize: None,
            last_click: None,
            rows: HashMap::new(),
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// Sets the size, snapped to the classic steps.
    pub fn set_size(&mut self, w: i32, h: i32) {
        self.w = snap(w, MIN_W, STEP_W);
        self.h = snap(h, MIN_H, STEP_H);
        self.clamp_scroll();
    }

    /// Unscaled height for the current mode ([`Self::size`] keeps the full height).
    pub fn height(&self) -> i32 {
        if self.shaded { 14 } else { self.h }
    }

    pub fn entries(&self) -> &[PlaylistEntry] {
        &self.entries
    }

    /// Replaces the list, keeping selection and scroll for entries that remain.
    pub fn set_entries(&mut self, entries: Vec<PlaylistEntry>) {
        let ids: BTreeSet<EntryId> = entries.iter().map(|e| e.id).collect();
        self.selected.retain(|id| ids.contains(id));
        self.entries = entries;
        if self.anchor.is_some_and(|a| a >= self.entries.len()) {
            self.anchor = None;
        }
        self.clamp_scroll();
    }

    /// Marks the playing entry, scrolling it into view when it changes.
    pub fn set_current(&mut self, id: Option<EntryId>) {
        if id != self.current {
            self.current = id;
            if let Some(i) = id.and_then(|id| self.index_of(id)) {
                self.ensure_visible(i);
            }
        }
    }

    pub fn selected(&self) -> Vec<EntryId> {
        self.entries
            .iter()
            .filter(|e| self.selected.contains(&e.id))
            .map(|e| e.id)
            .collect()
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some() || self.scroll_drag.is_some() || self.resize.is_some()
    }

    fn index_of(&self, id: EntryId) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    fn middle_h(&self) -> i32 {
        self.h - TOP - BOTTOM
    }

    fn list_rect(&self) -> Rect {
        Rect::new(
            LEFT,
            TOP + LIST_PAD,
            self.w - LEFT - RIGHT,
            self.middle_h() - 2 * LIST_PAD,
        )
    }

    pub fn visible_rows(&self) -> usize {
        (self.list_rect().h / ROW_H).max(1) as usize
    }

    fn max_scroll(&self) -> usize {
        self.entries.len().saturating_sub(self.visible_rows())
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn ensure_visible(&mut self, i: usize) {
        if i < self.scroll {
            self.scroll = i;
        } else if i >= self.scroll + self.visible_rows() {
            self.scroll = i + 1 - self.visible_rows();
        }
        self.clamp_scroll();
    }

    fn handle_y(&self) -> i32 {
        let travel = self.middle_h() - HANDLE_H;
        let max = self.max_scroll();
        if max == 0 {
            TOP
        } else {
            TOP + (self.scroll as i64 * i64::from(travel) / max as i64) as i32
        }
    }

    fn scroll_to_handle(&mut self, handle_top: i32) {
        let travel = (self.middle_h() - HANDLE_H).max(1);
        let f = f64::from((handle_top - TOP).clamp(0, travel)) / f64::from(travel);
        self.scroll = (f * self.max_scroll() as f64).round() as usize;
    }

    /// Where a control sits for the current size.
    pub fn control_rect(&self, c: PlControl) -> Rect {
        let (w, h) = (self.w, self.h);
        let corner = w - 150;
        let mini = |i: i32| Rect::new(corner + 3 + i * 10, h - BOTTOM + 22, 10, 10);
        match c {
            PlControl::Close => Rect::new(w - 11, 3, 9, 9),
            PlControl::Shade => Rect::new(w - 21, 3, 9, 9),
            PlControl::Resize => Rect::new(w - 20, h - 20, 20, 20),
            PlControl::Add => Rect::new(14, h - 30, 22, 18),
            PlControl::Remove => Rect::new(43, h - 30, 22, 18),
            PlControl::Select => Rect::new(72, h - 30, 22, 18),
            PlControl::Misc => Rect::new(101, h - 30, 22, 18),
            PlControl::ListMenu => Rect::new(w - 44, h - 30, 22, 18),
            PlControl::Previous => mini(0),
            PlControl::Play => mini(1),
            PlControl::Pause => mini(2),
            PlControl::Stop => mini(3),
            PlControl::Next => mini(4),
            PlControl::Eject => mini(5),
            PlControl::Scrollbar => Rect::new(w - 15, TOP, 8, self.middle_h()),
            PlControl::List => self.list_rect(),
        }
    }

    fn inside(r: Rect, p: Point) -> bool {
        p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
    }

    pub fn hit_test(&self, p: Point) -> Option<PlControl> {
        use PlControl::*;
        [
            Close, Shade, Resize, Add, Remove, Select, Misc, ListMenu, Previous, Play, Pause, Stop,
            Next, Eject, Scrollbar, List,
        ]
        .into_iter()
        .find(|c| Self::inside(self.control_rect(*c), p))
    }

    /// Button press at `p` (unscaled window coordinates).
    pub fn pointer_down(
        &mut self,
        p: Point,
        mods: SelectMods,
        now_ms: u64,
    ) -> (PlDown, Vec<PlaylistAction>) {
        if self.shaded {
            for c in [PlControl::Close, PlControl::Shade] {
                if Self::inside(self.control_rect(c), p) {
                    self.pressed = Some((c, true));
                    return (PlDown::Control(c), Vec::new());
                }
            }
            return (PlDown::DragWindow, Vec::new());
        }
        let Some(control) = self.hit_test(p) else {
            return if p.y < TOP {
                (PlDown::DragWindow, Vec::new())
            } else {
                // Frame edges and empty bottom bar: nothing.
                (PlDown::Control(PlControl::List), Vec::new())
            };
        };
        let mut actions = Vec::new();
        match control {
            PlControl::List => actions.extend(self.click_list(p, mods, now_ms)),
            PlControl::Scrollbar => {
                let top = self.handle_y();
                if (top..top + HANDLE_H).contains(&p.y) {
                    self.scroll_drag = Some(p.y - top);
                } else {
                    self.scroll_to_handle(p.y - HANDLE_H / 2);
                    self.scroll_drag = Some(HANDLE_H / 2);
                }
            }
            PlControl::Resize => self.resize = Some((p, self.w, self.h)),
            _ => self.pressed = Some((control, true)),
        }
        (PlDown::Control(control), actions)
    }

    fn click_list(&mut self, p: Point, mods: SelectMods, now_ms: u64) -> Vec<PlaylistAction> {
        let list = self.list_rect();
        let row = ((p.y - list.y) / ROW_H).max(0) as usize + self.scroll;
        let Some(entry) = self.entries.get(row) else {
            if !mods.ctrl {
                self.selected.clear();
            }
            return Vec::new();
        };
        let id = entry.id;
        if mods.shift {
            let anchor = self.anchor.unwrap_or(row);
            let (a, b) = (anchor.min(row), anchor.max(row));
            if !mods.ctrl {
                self.selected.clear();
            }
            self.selected
                .extend(self.entries[a..=b].iter().map(|e| e.id));
        } else if mods.ctrl {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
            self.anchor = Some(row);
        } else {
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(row);
        }
        let double = self
            .last_click
            .is_some_and(|(last, t)| last == id && now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
        self.last_click = Some((id, now_ms));
        if double && !mods.shift && !mods.ctrl {
            self.last_click = None;
            vec![PlaylistAction::PlayEntry(id)]
        } else {
            Vec::new()
        }
    }

    pub fn pointer_move(&mut self, p: Point) -> Vec<PlaylistAction> {
        if let Some(offset) = self.scroll_drag {
            self.scroll_to_handle(p.y - offset);
            return Vec::new();
        }
        if let Some((start, w0, h0)) = self.resize {
            let (w, h) = (
                snap(w0 + p.x - start.x, MIN_W, STEP_W),
                snap(h0 + p.y - start.y, MIN_H, STEP_H),
            );
            if (w, h) != (self.w, self.h) {
                self.w = w;
                self.h = h;
                self.clamp_scroll();
                return vec![PlaylistAction::Resize(w, h)];
            }
            return Vec::new();
        }
        if let Some((c, _)) = self.pressed {
            self.pressed = Some((c, Self::inside(self.control_rect(c), p)));
        }
        Vec::new()
    }

    pub fn pointer_up(&mut self, p: Point) -> Vec<PlaylistAction> {
        self.scroll_drag = None;
        if self.resize.take().is_some() {
            return Vec::new();
        }
        let Some((control, _)) = self.pressed.take() else {
            return Vec::new();
        };
        if !Self::inside(self.control_rect(control), p) {
            return Vec::new();
        }
        let action = match control {
            PlControl::Close => PlaylistAction::Close,
            PlControl::Shade => PlaylistAction::ToggleShade,
            PlControl::Add => PlaylistAction::AddFiles,
            PlControl::Remove => PlaylistAction::Remove(self.selected()),
            PlControl::Select => {
                if self.selected.len() == self.entries.len() {
                    self.selected.clear();
                } else {
                    self.selected = self.entries.iter().map(|e| e.id).collect();
                }
                return Vec::new();
            }
            PlControl::Previous => PlaylistAction::Transport(Action::Previous),
            PlControl::Play => PlaylistAction::Transport(Action::Play),
            PlControl::Pause => PlaylistAction::Transport(Action::Pause),
            PlControl::Stop => PlaylistAction::Transport(Action::Stop),
            PlControl::Next => PlaylistAction::Transport(Action::Next),
            PlControl::Eject => PlaylistAction::Transport(Action::Eject),
            PlControl::Misc
            | PlControl::ListMenu
            | PlControl::Resize
            | PlControl::Scrollbar
            | PlControl::List => return Vec::new(),
        };
        vec![action]
    }

    pub fn wheel(&mut self, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_sub(3)
        } else {
            self.scroll + 3
        };
        self.clamp_scroll();
    }

    /// Keyboard: arrows move the selection, Enter plays, Delete removes, Ctrl+A selects all.
    pub fn key(&mut self, keysym: u32, mods: SelectMods) -> Vec<PlaylistAction> {
        const UP: u32 = 0xff52;
        const DOWN: u32 = 0xff54;
        const HOME: u32 = 0xff50;
        const END: u32 = 0xff57;
        const RETURN: u32 = 0xff0d;
        const DELETE: u32 = 0xffff;
        let len = self.entries.len();
        if len == 0 {
            return Vec::new();
        }
        let cursor = self.anchor.unwrap_or(0).min(len - 1);
        let target = match keysym {
            UP => Some(cursor.saturating_sub(1)),
            DOWN => Some((cursor + 1).min(len - 1)),
            HOME => Some(0),
            END => Some(len - 1),
            RETURN => {
                return self
                    .selected()
                    .first()
                    .map(|id| vec![PlaylistAction::PlayEntry(*id)])
                    .unwrap_or_default();
            }
            DELETE => return vec![PlaylistAction::Remove(self.selected())],
            0x61 | 0x41 if mods.ctrl => {
                self.selected = self.entries.iter().map(|e| e.id).collect();
                return Vec::new();
            }
            _ => None,
        };
        if let Some(i) = target {
            self.selected.clear();
            self.selected.insert(self.entries[i].id);
            self.anchor = Some(i);
            self.ensure_visible(i);
        }
        Vec::new()
    }

    fn row_strip(
        &mut self,
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        text: &str,
        color: u32,
        height: u32,
    ) -> TextStrip {
        let key = (text.to_owned(), color, height);
        if let Some(s) = self.rows.get(&key) {
            return s.clone();
        }
        if self.rows.len() >= ROW_CACHE_LIMIT {
            self.rows.clear();
        }
        let strip = TextStrip::colored(skin, fallback, text, height, color);
        self.rows.insert(key, strip.clone());
        strip
    }

    fn pressed_on(&self, c: PlControl) -> bool {
        self.pressed == Some((c, true))
    }

    /// Draws the window at `scale`, with system-font rows rendered at the final size.
    pub fn render(
        &mut self,
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        now_ms: u64,
        scale: u32,
    ) -> Bitmap {
        let scale = scale.max(1);
        if self.shaded {
            return self.render_shade(skin, fallback, scale);
        }
        let (w, h) = (self.w, self.h);
        let style = skin.playlist_style().clone();
        let mut b = Bitmap::new(w as u32, h as u32, style.normal_bg);
        let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| {
            skin.draw(b, s, x, y);
        };
        let active = self.focused;
        let pick = |on: bool, a: Sprite, n: Sprite| if on { a } else { n };

        // Top: corners, tiles and the centered title.
        let mut x = 25;
        while x < w - 25 {
            draw(
                &mut b,
                pick(active, sp::TOP_TILE_ACTIVE, sp::TOP_TILE),
                x,
                0,
            );
            x += 25;
        }
        draw(
            &mut b,
            pick(active, sp::TOP_LEFT_ACTIVE, sp::TOP_LEFT),
            0,
            0,
        );
        draw(
            &mut b,
            pick(active, sp::TITLE_ACTIVE, sp::TITLE),
            (w - 100) / 2,
            0,
        );
        draw(
            &mut b,
            pick(active, sp::TOP_RIGHT_ACTIVE, sp::TOP_RIGHT),
            w - 25,
            0,
        );
        if self.pressed_on(PlControl::Close) {
            let r = self.control_rect(PlControl::Close);
            draw(&mut b, sp::CLOSE_PRESSED, r.x, r.y);
        }
        if self.pressed_on(PlControl::Shade) {
            let r = self.control_rect(PlControl::Shade);
            draw(&mut b, sp::SHADE_PRESSED, r.x, r.y);
        }

        // Sides.
        let mut y = TOP;
        while y < h - BOTTOM {
            draw(&mut b, sp::LEFT_TILE, 0, y);
            draw(&mut b, sp::RIGHT_TILE, w - RIGHT, y);
            y += 29;
        }

        // Bottom: corners with tiles between, and the visualizer panel when wide enough.
        let mut x = 125;
        while x < w - 150 {
            draw(&mut b, sp::BOTTOM_TILE, x, h - BOTTOM);
            x += 25;
        }
        if w >= 350 {
            draw(&mut b, sp::VISUALIZER_BACKGROUND, w - 225, h - BOTTOM);
        }
        draw(&mut b, sp::BOTTOM_LEFT, 0, h - BOTTOM);
        draw(&mut b, sp::BOTTOM_RIGHT, w - 150, h - BOTTOM);
        for (c, pressed) in [
            (PlControl::Add, sp::ADD_FILE_PRESSED),
            (PlControl::Remove, sp::REMOVE_SELECTED_PRESSED),
            (PlControl::Select, sp::SELECT_ALL_PRESSED),
            (PlControl::Misc, sp::MISC_OPTIONS_PRESSED),
            (PlControl::ListMenu, sp::LOAD_LIST_PRESSED),
        ] {
            if self.pressed_on(c) {
                let r = self.control_rect(c);
                draw(&mut b, pressed, r.x, r.y);
            }
        }

        // Scroll handle.
        draw(
            &mut b,
            pick(
                self.scroll_drag.is_some(),
                sp::SCROLL_HANDLE_PRESSED,
                sp::SCROLL_HANDLE,
            ),
            w - 15,
            self.handle_y(),
        );

        // Track rows.
        let list = self.list_rect();
        b.fill_rect(
            Rect::new(LEFT, TOP, w - LEFT - RIGHT, self.middle_h()),
            style.normal_bg,
        );
        let rows = self.visible_rows();
        let visible: Vec<(usize, PlaylistEntry)> = self
            .entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(rows)
            .map(|(i, e)| (i, e.clone()))
            .collect();
        // Rows rendered with a system font are drawn after scaling, at the final size.
        let mut late = Vec::new();
        for (row, (i, entry)) in visible.into_iter().enumerate() {
            let y = list.y + row as i32 * ROW_H;
            if self.selected.contains(&entry.id) {
                b.fill_rect(Rect::new(list.x, y, list.w, ROW_H), style.selected_bg);
            }
            let color = if Some(entry.id) == self.current {
                style.current
            } else {
                style.normal
            };
            let height = ROW_TEXT_H * scale;
            let duration = self.row_strip(
                skin,
                fallback,
                &duration_text(entry.duration),
                color,
                height,
            );
            let title = self.row_strip(
                skin,
                fallback,
                &format!("{}. {}", i + 1, entry.title),
                color,
                height,
            );
            if title.is_bitmap() {
                Self::draw_row(&mut b, &title, &duration, list, y, 1);
            } else {
                late.push((title, duration, y));
            }
        }
        // Running time (selected/total) and the mini time display, in the skin font.
        let total: Duration = self.entries.iter().filter_map(|e| e.duration).sum();
        let selected: Duration = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.id))
            .filter_map(|e| e.duration)
            .sum();
        let running = format!(
            "{}/{}",
            duration_text(Some(selected)),
            duration_text(Some(total))
        );
        skin.draw_text(&mut b, &running, w - 150 + 7, h - BOTTOM + 10);
        let blink_off = self.now.state == PlayState::Paused && (now_ms / 500) % 2 == 1;
        if self.now.state != PlayState::Stopped && !blink_off {
            let t = duration_text(Some(self.now.position));
            skin.draw_text(&mut b, &format!("{t:>5}"), w - 150 + 66, h - BOTTOM + 23);
        }
        let mut out = crate::render::scale_nearest(&b, scale);
        let s = scale as i32;
        let list = Rect::new(list.x * s, list.y * s, list.w * s, list.h * s);
        for (title, duration, y) in late {
            Self::draw_row(&mut out, &title, &duration, list, y * s, s);
        }
        out
    }

    /// The shade bar: current track title and time, with close and unshade buttons.
    fn render_shade(&mut self, skin: &Skin, fallback: &mut dyn TextRenderer, scale: u32) -> Bitmap {
        let w = self.w;
        let mut b = Bitmap::new(w as u32, 14, 0xFF00_0000);
        let mut x = 25;
        while x < w - 50 {
            skin.draw(&mut b, sp::SHADE_TILE, x, 0);
            x += 25;
        }
        skin.draw(&mut b, sp::SHADE_LEFT, 0, 0);
        skin.draw(
            &mut b,
            if self.focused {
                sp::SHADE_RIGHT_ACTIVE
            } else {
                sp::SHADE_RIGHT
            },
            w - 50,
            0,
        );
        if self.pressed_on(PlControl::Close) {
            let r = self.control_rect(PlControl::Close);
            skin.draw(&mut b, sp::CLOSE_PRESSED, r.x, r.y);
        }
        if self.pressed_on(PlControl::Shade) {
            let r = self.control_rect(PlControl::Shade);
            skin.draw(&mut b, sp::UNSHADE_PRESSED, r.x, r.y);
        }
        let current = self.current.and_then(|id| {
            let i = self.entries.iter().position(|e| e.id == id)?;
            Some((i, self.entries[i].clone()))
        });
        let Some((i, entry)) = current else {
            return crate::render::scale_nearest(&b, scale);
        };
        let color = skin.playlist_style().current;
        let height = ROW_TEXT_H * scale;
        let title = self.row_strip(
            skin,
            fallback,
            &format!("{}. {}", i + 1, entry.title),
            color,
            height,
        );
        let time = self.row_strip(
            skin,
            fallback,
            &duration_text(entry.duration),
            color,
            height,
        );
        // Skin-font text is drawn before scaling, system-font text after.
        let draw_text = |dst: &mut Bitmap, unit: i32| {
            // The time ends 30 px from the right, clear of the shade and close buttons.
            let time_x = (w - 30) * unit - time.width();
            let y = (14 * unit - title.height()) / 2;
            let title_clip = Rect::new(5 * unit, 0, (time_x - 8 * unit).max(0), 14 * unit);
            title.draw(dst, title_clip, y, 0, false, 0);
            time.draw(
                dst,
                Rect::new(time_x, 0, time.width(), 14 * unit),
                y,
                0,
                false,
                0,
            );
        };
        if title.is_bitmap() {
            draw_text(&mut b, 1);
            crate::render::scale_nearest(&b, scale)
        } else {
            let mut out = crate::render::scale_nearest(&b, scale);
            draw_text(&mut out, scale as i32);
            out
        }
    }

    /// Draws a row's title (clipped) and right-aligned duration. `list` and `y` are in the
    /// same pixel space as `dst`; `unit` is that space's scale.
    fn draw_row(
        dst: &mut Bitmap,
        title: &TextStrip,
        duration: &TextStrip,
        list: Rect,
        y: i32,
        unit: i32,
    ) {
        let row_h = ROW_H * unit;
        let dur_x = list.x + list.w - 3 * unit - duration.width();
        let title_clip = Rect::new(
            list.x + 2 * unit,
            y,
            (dur_x - 4 * unit - list.x).max(0),
            row_h,
        );
        title.draw(
            dst,
            title_clip,
            y + (row_h - title.height()) / 2,
            0,
            false,
            0,
        );
        let dur_clip = Rect::new(dur_x, y, duration.width(), row_h);
        duration.draw(
            dst,
            dur_clip,
            y + (row_h - duration.height()) / 2,
            0,
            false,
            0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::{code, coded_skin};
    use crate::text::NoFallback;
    use amp_skin::Sheet;

    fn entries(n: u64) -> Vec<PlaylistEntry> {
        (1..=n)
            .map(|id| PlaylistEntry {
                id,
                title: format!("Track {id}"),
                duration: Some(Duration::from_secs(60 + id)),
            })
            .collect()
    }

    fn window(n: u64) -> PlaylistWindow {
        let mut w = PlaylistWindow::new();
        w.set_entries(entries(n));
        w
    }

    fn row_point(w: &PlaylistWindow, row: i32) -> Point {
        let l = w.list_rect();
        Point::new(l.x + 10, l.y + row * ROW_H + 5)
    }

    const NONE: SelectMods = SelectMods {
        shift: false,
        ctrl: false,
    };

    #[test]
    fn default_size_and_snapping() {
        let mut w = PlaylistWindow::new();
        assert_eq!(w.size(), (275, 232));
        w.set_size(330, 200);
        assert_eq!(w.size(), (325, 174));
        w.set_size(10, 10);
        assert_eq!(w.size(), (275, 116));
    }

    #[test]
    fn rows_fit_the_list_area() {
        let w = window(3);
        // 232 - 20 - 38 - 6 = 168 px: 12 rows of 13.
        assert_eq!(w.visible_rows(), 12);
    }

    #[test]
    fn click_selects_and_double_click_plays() {
        let mut w = window(5);
        let (_, a) = w.pointer_down(row_point(&w, 1), NONE, 1000);
        assert!(a.is_empty());
        w.pointer_up(row_point(&w, 1));
        assert_eq!(w.selected(), vec![2]);
        let (_, a) = w.pointer_down(row_point(&w, 1), NONE, 1300);
        assert_eq!(a, vec![PlaylistAction::PlayEntry(2)]);
        // Too slow is two single clicks.
        w.pointer_down(row_point(&w, 3), NONE, 2000);
        let (_, a) = w.pointer_down(row_point(&w, 3), NONE, 3000);
        assert!(a.is_empty());
    }

    #[test]
    fn shift_and_ctrl_selection() {
        let mut w = window(6);
        w.pointer_down(row_point(&w, 1), NONE, 0);
        w.pointer_down(
            row_point(&w, 4),
            SelectMods {
                shift: true,
                ctrl: false,
            },
            10_000,
        );
        assert_eq!(w.selected(), vec![2, 3, 4, 5]);
        w.pointer_down(
            row_point(&w, 2),
            SelectMods {
                shift: false,
                ctrl: true,
            },
            20_000,
        );
        assert_eq!(w.selected(), vec![2, 4, 5]);
        // Clicking below the last entry clears.
        w.pointer_down(row_point(&w, 9), NONE, 30_000);
        assert!(w.selected().is_empty());
    }

    #[test]
    fn buttons_act_on_release() {
        let mut w = window(4);
        w.pointer_down(row_point(&w, 0), NONE, 0);
        let center = |w: &PlaylistWindow, c: PlControl| {
            let r = w.control_rect(c);
            Point::new(r.x + r.w / 2, r.y + r.h / 2)
        };
        for (c, expected) in [
            (PlControl::Remove, PlaylistAction::Remove(vec![1])),
            (PlControl::Add, PlaylistAction::AddFiles),
            (PlControl::Close, PlaylistAction::Close),
            (PlControl::Play, PlaylistAction::Transport(Action::Play)),
            (PlControl::Eject, PlaylistAction::Transport(Action::Eject)),
        ] {
            let p = center(&w, c);
            assert_eq!(w.pointer_down(p, NONE, 0).0, PlDown::Control(c), "{c:?}");
            assert_eq!(w.pointer_up(p), vec![expected], "{c:?}");
        }
        let p = center(&w, PlControl::Select);
        w.pointer_down(p, NONE, 0);
        w.pointer_up(p);
        assert_eq!(w.selected(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn title_bar_drags_and_frame_does_not() {
        let mut w = window(1);
        assert_eq!(
            w.pointer_down(Point::new(100, 10), NONE, 0).0,
            PlDown::DragWindow
        );
        assert_ne!(
            w.pointer_down(Point::new(5, 100), NONE, 0).0,
            PlDown::DragWindow
        );
    }

    #[test]
    fn resize_snaps_to_steps_and_reports() {
        let mut w = window(1);
        let r = w.control_rect(PlControl::Resize);
        let start = Point::new(r.x + 5, r.y + 5);
        w.pointer_down(start, NONE, 0);
        assert!(
            w.pointer_move(Point::new(start.x + 10, start.y + 10))
                .is_empty(),
            "under a step"
        );
        assert_eq!(
            w.pointer_move(Point::new(start.x + 60, start.y + 30)),
            vec![PlaylistAction::Resize(325, 261)]
        );
        assert_eq!(
            w.pointer_move(Point::new(start.x - 500, start.y - 500)),
            vec![PlaylistAction::Resize(275, 116)]
        );
        w.pointer_up(Point::new(0, 0));
        assert!(!w.is_interacting());
    }

    #[test]
    fn scrolling_wheel_keys_and_current() {
        let mut w = window(40);
        w.wheel(false);
        assert_eq!(w.scroll(), 3);
        w.wheel(true);
        w.wheel(true);
        assert_eq!(w.scroll(), 0);
        w.set_current(Some(30));
        assert!(
            w.scroll() > 0 && w.scroll() <= 29,
            "current scrolled into view"
        );
        w.key(0xff57, NONE); // End
        assert_eq!(w.selected(), vec![40]);
        assert_eq!(w.scroll(), 40 - 12);
        assert_eq!(w.key(0xff0d, NONE), vec![PlaylistAction::PlayEntry(40)]);
        assert_eq!(w.key(0xffff, NONE), vec![PlaylistAction::Remove(vec![40])]);
        w.key(
            0x61,
            SelectMods {
                shift: false,
                ctrl: true,
            },
        );
        assert_eq!(w.selected().len(), 40);
    }

    #[test]
    fn scrollbar_drag_reaches_both_ends() {
        let mut w = window(40);
        let bar = w.control_rect(PlControl::Scrollbar);
        w.pointer_down(Point::new(bar.x + 2, bar.y + 2), NONE, 0);
        w.pointer_move(Point::new(bar.x + 2, bar.y + bar.h + 50));
        assert_eq!(w.scroll(), 28);
        w.pointer_move(Point::new(bar.x + 2, -50));
        assert_eq!(w.scroll(), 0);
        w.pointer_up(Point::new(0, 0));
    }

    #[test]
    fn entries_update_keeps_selection_of_survivors() {
        let mut w = window(5);
        w.key(
            0x61,
            SelectMods {
                shift: false,
                ctrl: true,
            },
        );
        w.set_entries(entries(3));
        assert_eq!(w.selected(), vec![1, 2, 3]);
    }

    #[test]
    fn shade_mode_bar() {
        let skin = coded_skin();
        let mut w = window(3);
        let c = w.control_rect(PlControl::Shade);
        let p = Point::new(c.x + 4, c.y + 4);
        w.pointer_down(p, NONE, 0);
        assert_eq!(w.pointer_up(p), vec![PlaylistAction::ToggleShade]);
        w.shaded = true;
        assert_eq!(w.height(), 14);
        assert_eq!(w.size(), (275, 232), "full size is kept for unshading");
        assert_eq!(
            w.pointer_down(Point::new(100, 7), NONE, 0).0,
            PlDown::DragWindow
        );
        w.pointer_down(p, NONE, 0);
        assert_eq!(w.pointer_up(p), vec![PlaylistAction::ToggleShade]);
        w.set_current(Some(2));
        let b = w.render(&skin, &mut NoFallback, 0, 2);
        assert_eq!((b.width(), b.height()), (550, 28));
        assert_eq!(
            b.get(0, 0),
            Some(code(Sheet::Pledit, 72, 42)),
            "left end of the bar"
        );
    }

    #[test]
    fn render_tiles_frame_and_highlights_selection() {
        let skin = coded_skin();
        let mut w = window(3);
        w.set_size(350, 261);
        w.focused = true;
        w.pointer_down(row_point(&w, 1), NONE, 0);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!((b.width(), b.height()), (350, 261));
        assert_eq!(
            b.get(0, 0),
            Some(code(Sheet::Pledit, 0, 0)),
            "active top-left corner"
        );
        assert_eq!(
            b.get(349, 0),
            Some(code(Sheet::Pledit, 177, 0)),
            "top-right corner"
        );
        assert_eq!(
            b.get(0, 223),
            Some(code(Sheet::Pledit, 0, 72)),
            "bottom-left corner"
        );
        assert_eq!(
            b.get(125, 223),
            Some(code(Sheet::Pledit, 205, 0)),
            "visualizer panel"
        );
        let style = skin.playlist_style();
        let sel_y = w.list_rect().y + ROW_H;
        assert_eq!(
            b.get(200, sel_y),
            Some(style.selected_bg),
            "selected row background"
        );
        assert_eq!(b.get(200, sel_y + ROW_H), Some(style.normal_bg));
    }
}
