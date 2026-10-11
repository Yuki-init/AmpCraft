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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// An item of a bottom-bar button menu (ADD, REM, SEL, MISC, LIST).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlMenuItem {
    AddUrl,
    AddDir,
    AddFile,
    RemoveAll,
    Crop,
    RemoveSelected,
    RemoveMisc,
    InvertSelection,
    SelectNone,
    SelectAll,
    Sort,
    FileInfo,
    MiscOptions,
    NewList,
    SaveList,
    LoadList,
}

/// The items of a button's menu, top to bottom, with their normal and selected sprites,
/// and the bar drawn beside them.
fn bar_menu_items(button: PlControl) -> (&'static [(PlMenuItem, Sprite, Sprite)], Sprite) {
    use PlMenuItem::*;
    match button {
        PlControl::Add => (
            &[
                (AddUrl, sp::ADD_URL, sp::ADD_URL_SELECTED),
                (AddDir, sp::ADD_DIR, sp::ADD_DIR_SELECTED),
                (AddFile, sp::ADD_FILE, sp::ADD_FILE_PRESSED),
            ],
            sp::ADD_BAR,
        ),
        PlControl::Remove => (
            &[
                (RemoveAll, sp::REMOVE_ALL, sp::REMOVE_ALL_SELECTED),
                (Crop, sp::REMOVE_CROP, sp::REMOVE_CROP_SELECTED),
                (
                    RemoveSelected,
                    sp::REMOVE_SELECTED,
                    sp::REMOVE_SELECTED_PRESSED,
                ),
                (RemoveMisc, sp::REMOVE_MISC, sp::REMOVE_MISC_SELECTED),
            ],
            sp::REMOVE_BAR,
        ),
        PlControl::Select => (
            &[
                (
                    InvertSelection,
                    sp::SELECT_INVERT,
                    sp::SELECT_INVERT_SELECTED,
                ),
                (SelectNone, sp::SELECT_NONE, sp::SELECT_NONE_SELECTED),
                (SelectAll, sp::SELECT_ALL, sp::SELECT_ALL_PRESSED),
            ],
            sp::SELECT_BAR,
        ),
        PlControl::Misc => (
            &[
                (Sort, sp::MISC_SORT, sp::MISC_SORT_SELECTED),
                (FileInfo, sp::MISC_INFO, sp::MISC_INFO_SELECTED),
                (MiscOptions, sp::MISC_OPTIONS, sp::MISC_OPTIONS_PRESSED),
            ],
            sp::MISC_BAR,
        ),
        _ => (
            &[
                (NewList, sp::LIST_NEW, sp::LIST_NEW_SELECTED),
                (SaveList, sp::LIST_SAVE, sp::LIST_SAVE_SELECTED),
                (LoadList, sp::LOAD_LIST, sp::LOAD_LIST_PRESSED),
            ],
            sp::LIST_BAR,
        ),
    }
}

/// An open bottom-bar menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BarMenu {
    button: PlControl,
    hover: Option<PlMenuItem>,
    /// The item under the press that opened the menu.
    start: Option<PlMenuItem>,
    /// A release now chooses: the pointer left the starting item, or the opening click
    /// ended and the menu stayed open.
    armed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaylistAction {
    PlayEntry(EntryId),
    Remove(Vec<EntryId>),
    AddFiles,
    /// ADD > DIR: pick a folder and add its audio files.
    AddFolder,
    /// ADD > URL: streams are not supported (no network access).
    AddUrl,
    /// REM > ALL and LIST > NEW.
    Clear,
    /// REM > MISC, MISC > SORT and MISC > OPTS open a popup menu.
    RemoveMiscMenu,
    SortMenu,
    MiscMenu,
    /// MISC > INFO: the selected track, else the current one.
    FileInfo(EntryId),
    SaveList,
    LoadList,
    Close,
    ToggleShade,
    /// The window wants this (unscaled) size.
    Resize(i32, i32),
    /// A mini transport button.
    Transport(Action),
    /// Rows were dragged: the playlist in its new order.
    Reorder(Vec<EntryId>),
    /// Q: queue these tracks to play next, or take them out of the queue.
    ToggleQueue(Vec<EntryId>),
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
    resize: Option<crate::grip::Resize>,
    last_click: Option<(EntryId, u64)>,
    /// Tracks queued to play next (shown as [1], [2], ...).
    up_next: Vec<EntryId>,
    /// Dragging selected rows: the row under the pointer now, and a single row to select
    /// on release if the drag never moved (a plain click inside a multi-selection).
    reorder: Option<usize>,
    click_select: Option<EntryId>,
    rows: HashMap<(String, u32, u32), TextStrip>,
    bar_menu: Option<BarMenu>,
    /// Bumped when entries or the current track change, for [`Self::render_hash`].
    revision: u64,
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
            bar_menu: None,
            up_next: Vec::new(),
            reorder: None,
            click_select: None,
            revision: 0,
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

    /// Forget text rendered with the previous skin's colors.
    pub fn skin_changed(&mut self) {
        self.revision += 1;
        self.rows.clear();
    }

    /// Unscaled height for the current mode ([`Self::size`] keeps the full height).
    pub fn height(&self) -> i32 {
        if self.shaded { 14 } else { self.h }
    }

    pub fn entries(&self) -> &[PlaylistEntry] {
        &self.entries
    }

    /// Changes whenever the rendered image would change at `now_ms`.
    pub fn render_hash(&self, scale: u32, now_ms: u64) -> u64 {
        let blink_off = self.now.state == PlayState::Paused && (now_ms / 500) % 2 == 1;
        crate::render::hash_of(&(
            (scale, self.w, self.h, self.shaded, self.focused),
            (self.revision, self.scroll, &self.selected, self.current),
            self.bar_menu,
            (
                format!("{:?}", self.now.state),
                self.now.position.as_secs(),
                blink_off,
            ),
            (
                self.pressed.map(|(c, i)| (format!("{c:?}"), i)),
                self.scroll_drag.is_some(),
            ),
        ))
    }

    /// Replaces the list, keeping selection and scroll for entries that remain.
    pub fn set_entries(&mut self, entries: Vec<PlaylistEntry>) {
        self.revision += 1;
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
            self.revision += 1;
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
        self.pressed.is_some()
            || self.scroll_drag.is_some()
            || self.resize.is_some()
            || self.bar_menu.is_some()
            || self.reorder.is_some()
    }

    /// The play-next queue, for the [n] markers.
    pub fn set_up_next(&mut self, ids: Vec<EntryId>) {
        if ids != self.up_next {
            self.up_next = ids;
            self.revision += 1;
        }
    }

    pub fn select_all(&mut self) {
        self.selected = self.entries.iter().map(|e| e.id).collect();
    }

    pub fn select_none(&mut self) {
        self.selected.clear();
    }

    pub fn invert_selection(&mut self) {
        self.selected = self
            .entries
            .iter()
            .filter(|e| !self.selected.contains(&e.id))
            .map(|e| e.id)
            .collect();
    }

    /// The row index under `p` in the list, if `p` is over the track list.
    fn row_at(&self, p: Point) -> Option<usize> {
        let list = self.list_rect();
        if self.shaded || !Self::inside(list, p) {
            return None;
        }
        Some(((p.y - list.y) / ROW_H).max(0) as usize + self.scroll)
    }

    /// A right-click: over a track that is not selected, selects just that track. Returns
    /// whether `p` is over the track list (where the track menu opens).
    pub fn right_click(&mut self, p: Point) -> bool {
        let Some(row) = self.row_at(p) else {
            return false;
        };
        if let Some(id) = self.entries.get(row).map(|e| e.id)
            && !self.selected.contains(&id)
        {
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(row);
        }
        true
    }

    /// Moves the selected rows `delta` places (as far as the ends allow), keeping their
    /// spacing. Returns the new order when anything moved.
    fn move_selection(&mut self, delta: i64) -> Option<Vec<EntryId>> {
        let picked: Vec<usize> = (0..self.entries.len())
            .filter(|i| self.selected.contains(&self.entries[*i].id))
            .collect();
        let (first, last) = (*picked.first()?, *picked.last()?);
        let delta = delta.clamp(-(first as i64), (self.entries.len() - 1 - last) as i64);
        if delta == 0 {
            return None;
        }
        for _ in 0..delta.unsigned_abs() {
            let idx: Vec<usize> = (0..self.entries.len())
                .filter(|i| self.selected.contains(&self.entries[*i].id))
                .collect();
            if delta > 0 {
                for &i in idx.iter().rev() {
                    self.entries.swap(i, i + 1);
                }
            } else {
                for &i in &idx {
                    self.entries.swap(i, i - 1);
                }
            }
        }
        self.revision += 1;
        Some(self.entries.iter().map(|e| e.id).collect())
    }

    /// The resize grip under `p` (none while shaded).
    pub fn grip(&self, p: Point) -> Option<crate::grip::Grip> {
        if self.shaded {
            return None;
        }
        crate::grip::grip_at(self.w, self.h, p, self.control_rect(PlControl::Resize), TOP)
    }

    /// Closes an open bottom-bar menu (the window lost focus).
    pub fn close_menu(&mut self) {
        self.bar_menu = None;
    }

    /// Whether a bottom-bar button menu is open.
    pub fn menu_open(&self) -> bool {
        self.bar_menu.is_some()
    }

    /// Where each item of `button`'s menu is drawn: stacked upward so the last item covers
    /// the button.
    fn bar_menu_layout(&self, button: PlControl) -> Vec<(PlMenuItem, Rect, Sprite, Sprite)> {
        let r = self.control_rect(button);
        let (items, _) = bar_menu_items(button);
        let n = items.len() as i32;
        items
            .iter()
            .enumerate()
            .map(|(i, (item, normal, selected))| {
                let y = r.y - (n - 1 - i as i32) * 18;
                (*item, Rect::new(r.x, y, 22, 18), *normal, *selected)
            })
            .collect()
    }

    fn bar_menu_item_at(&self, button: PlControl, p: Point) -> Option<PlMenuItem> {
        self.bar_menu_layout(button)
            .into_iter()
            .find(|(_, r, _, _)| Self::inside(*r, p))
            .map(|(item, ..)| item)
    }

    /// Does a menu item: selection changes here, the rest as actions.
    fn bar_menu_choice(&mut self, item: PlMenuItem) -> Vec<PlaylistAction> {
        use PlMenuItem::*;
        let unselected = || {
            self.entries
                .iter()
                .filter(|e| !self.selected.contains(&e.id))
                .map(|e| e.id)
                .collect::<Vec<_>>()
        };
        let action = match item {
            AddUrl => PlaylistAction::AddUrl,
            AddDir => PlaylistAction::AddFolder,
            AddFile => PlaylistAction::AddFiles,
            RemoveAll | NewList => PlaylistAction::Clear,
            Crop => PlaylistAction::Remove(unselected()),
            RemoveSelected => PlaylistAction::Remove(self.selected()),
            RemoveMisc => PlaylistAction::RemoveMiscMenu,
            InvertSelection => {
                self.selected = unselected().into_iter().collect();
                return Vec::new();
            }
            SelectNone => {
                self.selected.clear();
                return Vec::new();
            }
            SelectAll => {
                self.selected = self.entries.iter().map(|e| e.id).collect();
                return Vec::new();
            }
            Sort => PlaylistAction::SortMenu,
            FileInfo => {
                let target = self
                    .selected()
                    .first()
                    .copied()
                    .or(self.current)
                    .or(self.entries.first().map(|e| e.id));
                match target {
                    Some(id) => PlaylistAction::FileInfo(id),
                    None => return Vec::new(),
                }
            }
            MiscOptions => PlaylistAction::MiscMenu,
            SaveList => PlaylistAction::SaveList,
            LoadList => PlaylistAction::LoadList,
        };
        vec![action]
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
        // A new press ends any drag whose release was lost.
        self.reorder = None;
        self.click_select = None;
        if self.shaded {
            for c in [PlControl::Close, PlControl::Shade] {
                if Self::inside(self.control_rect(c), p) {
                    self.pressed = Some((c, true));
                    return (PlDown::Control(c), Vec::new());
                }
            }
            return (PlDown::DragWindow, Vec::new());
        }
        if let Some(menu) = self.bar_menu {
            // A press on an item arms it; anywhere else closes the menu.
            return match self.bar_menu_item_at(menu.button, p) {
                Some(item) => {
                    self.bar_menu = Some(BarMenu {
                        hover: Some(item),
                        armed: true,
                        ..menu
                    });
                    (PlDown::Control(menu.button), Vec::new())
                }
                None => {
                    self.bar_menu = None;
                    (PlDown::Control(PlControl::List), Vec::new())
                }
            };
        }
        if let Some(grip) = self.grip(p) {
            self.resize = Some(crate::grip::Resize::new(grip, p, self.w, self.h));
            return (PlDown::Control(PlControl::Resize), Vec::new());
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
            PlControl::Resize => {
                self.resize = Some(crate::grip::Resize::new(
                    crate::grip::Grip::Corner,
                    p,
                    self.w,
                    self.h,
                ));
            }
            PlControl::Add
            | PlControl::Remove
            | PlControl::Select
            | PlControl::Misc
            | PlControl::ListMenu => {
                let item = self.bar_menu_item_at(control, p);
                self.bar_menu = Some(BarMenu {
                    button: control,
                    hover: item,
                    start: item,
                    armed: false,
                });
            }
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
            if self.selected.contains(&id) && self.selected.len() > 1 {
                // Keep the selection so it can be dragged; a click without a drag
                // selects just this row on release.
                self.click_select = Some(id);
            } else {
                self.selected.clear();
                self.selected.insert(id);
            }
            self.anchor = Some(row);
            self.reorder = Some(row);
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
        if let Some(from) = self.reorder {
            let list = self.list_rect();
            let row = ((p.y - list.y) / ROW_H) as i64 + self.scroll as i64;
            let row = row.clamp(0, self.entries.len().saturating_sub(1) as i64);
            let delta = row - from as i64;
            if delta != 0
                && let Some(order) = self.move_selection(delta)
            {
                self.click_select = None;
                self.reorder = Some(row as usize);
                self.anchor = Some(row as usize);
                return vec![PlaylistAction::Reorder(order)];
            }
            return Vec::new();
        }
        if let Some(r) = self.resize {
            let (w, h) = r.size(p, (MIN_W, STEP_W), (MIN_H, STEP_H));
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
        if let Some(mut menu) = self.bar_menu {
            menu.hover = self.bar_menu_item_at(menu.button, p);
            menu.armed |= menu.hover != menu.start;
            self.bar_menu = Some(menu);
        }
        Vec::new()
    }

    pub fn pointer_up(&mut self, p: Point) -> Vec<PlaylistAction> {
        self.scroll_drag = None;
        if self.reorder.take().is_some() {
            if let Some(id) = self.click_select.take() {
                self.selected.clear();
                self.selected.insert(id);
            }
            return Vec::new();
        }
        if let Some(menu) = self.bar_menu {
            if !menu.armed {
                // The opening click ended where it began: keep the menu open.
                self.bar_menu = Some(BarMenu {
                    armed: true,
                    ..menu
                });
                return Vec::new();
            }
            self.bar_menu = None;
            return match self.bar_menu_item_at(menu.button, p) {
                Some(item) => self.bar_menu_choice(item),
                None => Vec::new(),
            };
        }
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
            PlControl::Previous => PlaylistAction::Transport(Action::Previous),
            PlControl::Play => PlaylistAction::Transport(Action::Play),
            PlControl::Pause => PlaylistAction::Transport(Action::Pause),
            PlControl::Stop => PlaylistAction::Transport(Action::Stop),
            PlControl::Next => PlaylistAction::Transport(Action::Next),
            PlControl::Eject => PlaylistAction::Transport(Action::Eject),
            PlControl::Add
            | PlControl::Remove
            | PlControl::Select
            | PlControl::Misc
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
        const ESCAPE: u32 = 0xff1b;
        if matches!(keysym, 0x71 | 0x51) && !mods.ctrl && self.bar_menu.is_none() {
            let ids = self.selected();
            return if ids.is_empty() {
                Vec::new()
            } else {
                vec![PlaylistAction::ToggleQueue(ids)]
            };
        }
        if self.bar_menu.is_some() {
            if keysym == ESCAPE {
                self.bar_menu = None;
            }
            return Vec::new();
        }
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
            // Queued tracks show their place in the play-next queue before the length.
            let time = match self.up_next.iter().position(|q| *q == entry.id) {
                Some(n) => format!("[{}] {}", n + 1, duration_text(entry.duration))
                    .trim_end()
                    .to_owned(),
                None => duration_text(entry.duration),
            };
            let duration = self.row_strip(skin, fallback, &time, color, height);
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
        self.render_bar_menu(skin, &mut out, s);
        out
    }

    /// Draws the open bottom-bar menu over everything (the list text included), at `scale`.
    fn render_bar_menu(&self, skin: &Skin, out: &mut Bitmap, scale: i32) {
        let Some(menu) = self.bar_menu else { return };
        let layout = self.bar_menu_layout(menu.button);
        let Some((_, top, _, _)) = layout.first().copied() else {
            return;
        };
        let (_, bar) = bar_menu_items(menu.button);
        // Drawn unscaled into its own bitmap (bar on the left), then scaled into place.
        let height = layout.len() as i32 * 18;
        let mut m = Bitmap::new(25, height as u32, 0xFF00_0000);
        skin.draw(&mut m, bar, 0, 0);
        for (item, r, normal, selected) in layout {
            let sprite = if menu.hover == Some(item) {
                selected
            } else {
                normal
            };
            skin.draw(&mut m, sprite, 3, r.y - top.y);
        }
        let big = crate::render::scale_nearest(&m, scale as u32);
        let (x0, y0) = ((top.x - 3) * scale, top.y * scale);
        for y in 0..big.height() as i32 {
            for x in 0..big.width() as i32 {
                if let Some(p) = big.get(x, y) {
                    out.set(x0 + x, y0 + y, p);
                }
            }
        }
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
        // The current track, else the first one (as the main window shows); an empty list
        // shows its name.
        let shown = self
            .current
            .and_then(|id| self.entries.iter().position(|e| e.id == id))
            .or((!self.entries.is_empty()).then_some(0));
        let (label, duration) = match shown {
            Some(i) => (
                format!("{}. {}", i + 1, self.entries[i].title),
                duration_text(self.entries[i].duration),
            ),
            None => ("Playlist".to_owned(), String::new()),
        };
        let color = skin.playlist_style().current;
        let height = ROW_TEXT_H * scale;
        let title = self.row_strip(skin, fallback, &label, color, height);
        let time = self.row_strip(skin, fallback, &duration, color, height);
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
            (PlControl::Close, PlaylistAction::Close),
            (PlControl::Play, PlaylistAction::Transport(Action::Play)),
            (PlControl::Eject, PlaylistAction::Transport(Action::Eject)),
        ] {
            let p = center(&w, c);
            assert_eq!(w.pointer_down(p, NONE, 0).0, PlDown::Control(c), "{c:?}");
            assert_eq!(w.pointer_up(p), vec![expected], "{c:?}");
        }
    }

    fn item_point(w: &PlaylistWindow, button: PlControl, item: PlMenuItem) -> Point {
        let (_, r, _, _) = w
            .bar_menu_layout(button)
            .into_iter()
            .find(|(i, ..)| *i == item)
            .unwrap();
        Point::new(r.x + r.w / 2, r.y + r.h / 2)
    }

    /// Click a bottom-bar button (menu stays open), then click an item.
    fn choose(w: &mut PlaylistWindow, button: PlControl, item: PlMenuItem) -> Vec<PlaylistAction> {
        let r = w.control_rect(button);
        let b = Point::new(r.x + 5, r.y + 5);
        w.pointer_down(b, NONE, 0);
        assert!(w.pointer_up(b).is_empty(), "the opening click only opens");
        assert!(w.menu_open());
        let p = item_point(w, button, item);
        w.pointer_down(p, NONE, 0);
        let actions = w.pointer_up(p);
        assert!(!w.menu_open());
        actions
    }

    #[test]
    fn bar_menus_stack_items_over_their_button() {
        let w = window(1);
        let add = w.control_rect(PlControl::Add);
        let layout = w.bar_menu_layout(PlControl::Add);
        let rects: Vec<Rect> = layout.iter().map(|(_, r, ..)| *r).collect();
        assert_eq!(
            rects,
            vec![
                Rect::new(add.x, add.y - 36, 22, 18),
                Rect::new(add.x, add.y - 18, 22, 18),
                Rect::new(add.x, add.y, 22, 18),
            ]
        );
        assert_eq!(w.bar_menu_layout(PlControl::Remove).len(), 4);
        let top = w.bar_menu_layout(PlControl::Remove)[0].1;
        assert!(top.y >= TOP, "fits at the smallest height");
    }

    #[test]
    fn bar_menu_items_become_actions() {
        use PlMenuItem::*;
        let mut w = window(4);
        assert_eq!(
            choose(&mut w, PlControl::Add, AddFile),
            vec![PlaylistAction::AddFiles]
        );
        assert_eq!(
            choose(&mut w, PlControl::Add, AddDir),
            vec![PlaylistAction::AddFolder]
        );
        assert_eq!(
            choose(&mut w, PlControl::Add, AddUrl),
            vec![PlaylistAction::AddUrl]
        );
        assert_eq!(
            choose(&mut w, PlControl::Remove, RemoveAll),
            vec![PlaylistAction::Clear]
        );
        assert_eq!(
            choose(&mut w, PlControl::ListMenu, NewList),
            vec![PlaylistAction::Clear]
        );
        assert_eq!(
            choose(&mut w, PlControl::ListMenu, SaveList),
            vec![PlaylistAction::SaveList]
        );
        assert_eq!(
            choose(&mut w, PlControl::ListMenu, LoadList),
            vec![PlaylistAction::LoadList]
        );
        assert_eq!(
            choose(&mut w, PlControl::Remove, RemoveMisc),
            vec![PlaylistAction::RemoveMiscMenu]
        );
        assert_eq!(
            choose(&mut w, PlControl::Misc, Sort),
            vec![PlaylistAction::SortMenu]
        );
        assert_eq!(
            choose(&mut w, PlControl::Misc, MiscOptions),
            vec![PlaylistAction::MiscMenu]
        );
        // File info: the first entry when nothing is selected or current.
        assert_eq!(
            choose(&mut w, PlControl::Misc, FileInfo),
            vec![PlaylistAction::FileInfo(1)]
        );
        w.set_current(Some(3));
        assert_eq!(
            choose(&mut w, PlControl::Misc, FileInfo),
            vec![PlaylistAction::FileInfo(3)]
        );
    }

    #[test]
    fn selection_and_removal_items() {
        use PlMenuItem::*;
        let mut w = window(4);
        w.pointer_down(row_point(&w, 1), NONE, 0);
        w.pointer_up(row_point(&w, 1));
        assert_eq!(w.selected(), vec![2]);
        assert!(choose(&mut w, PlControl::Select, InvertSelection).is_empty());
        assert_eq!(w.selected(), vec![1, 3, 4]);
        assert_eq!(
            choose(&mut w, PlControl::Misc, FileInfo),
            vec![PlaylistAction::FileInfo(1)]
        );
        assert_eq!(
            choose(&mut w, PlControl::Remove, RemoveSelected),
            vec![PlaylistAction::Remove(vec![1, 3, 4])]
        );
        assert_eq!(
            choose(&mut w, PlControl::Remove, Crop),
            vec![PlaylistAction::Remove(vec![2])]
        );
        choose(&mut w, PlControl::Select, SelectNone);
        assert!(w.selected().is_empty());
        choose(&mut w, PlControl::Select, SelectAll);
        assert_eq!(w.selected(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn bar_menu_drag_release_and_dismissal() {
        let mut w = window(2);
        // Press on ADD, drag up to DIR, release: chosen in one gesture.
        let r = w.control_rect(PlControl::Add);
        w.pointer_down(Point::new(r.x + 5, r.y + 5), NONE, 0);
        assert!(w.is_interacting());
        let dir = item_point(&w, PlControl::Add, PlMenuItem::AddDir);
        w.pointer_move(dir);
        assert_eq!(w.pointer_up(dir), vec![PlaylistAction::AddFolder]);
        // Released outside after dragging: closes without acting.
        w.pointer_down(Point::new(r.x + 5, r.y + 5), NONE, 0);
        w.pointer_move(Point::new(200, 60));
        assert!(w.pointer_up(Point::new(200, 60)).is_empty());
        assert!(!w.menu_open());
        // Open by click, then a press elsewhere closes it and does nothing else.
        w.pointer_down(Point::new(r.x + 5, r.y + 5), NONE, 0);
        w.pointer_up(Point::new(r.x + 5, r.y + 5));
        let row = row_point(&w, 0);
        w.pointer_down(row, NONE, 0);
        assert!(!w.menu_open());
        assert!(w.selected().is_empty(), "the closing click does not select");
        // Escape closes too.
        w.pointer_down(Point::new(r.x + 5, r.y + 5), NONE, 0);
        w.pointer_up(Point::new(r.x + 5, r.y + 5));
        w.key(0xff1b, NONE);
        assert!(!w.menu_open());
    }

    #[test]
    fn bar_menu_renders_items_and_highlight() {
        let skin = coded_skin();
        let mut w = window(2);
        let r = w.control_rect(PlControl::Select);
        let hash = w.render_hash(1, 0);
        w.pointer_down(Point::new(r.x + 5, r.y + 5), NONE, 0);
        assert_ne!(w.render_hash(1, 0), hash);
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        // Bottom item (ALL) highlighted under the pointer, INV above it normal, bar beside.
        assert_eq!(b.get(r.x, r.y), Some(code(Sheet::Pledit, 127, 149)));
        assert_eq!(b.get(r.x, r.y - 36), Some(code(Sheet::Pledit, 104, 111)));
        assert_eq!(
            b.get(r.x - 3, r.y - 36),
            Some(code(Sheet::Pledit, 150, 111))
        );
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

    fn ids(w: &PlaylistWindow) -> Vec<EntryId> {
        w.entries().iter().map(|e| e.id).collect()
    }

    #[test]
    fn dragging_selected_rows_reorders() {
        let mut w = window(5);
        // Select rows 1 and 3 (ids 2 and 4), then drag row 1 down two places.
        w.pointer_down(row_point(&w, 1), NONE, 0);
        w.pointer_up(row_point(&w, 1));
        let ctrl = SelectMods {
            shift: false,
            ctrl: true,
        };
        w.pointer_down(row_point(&w, 3), ctrl, 1000);
        w.pointer_up(row_point(&w, 3));
        w.pointer_down(row_point(&w, 1), NONE, 2000);
        let a = w.pointer_move(row_point(&w, 2));
        assert_eq!(a, vec![PlaylistAction::Reorder(vec![1, 3, 2, 5, 4])]);
        // The last selected row is at the end now: further down does nothing.
        assert!(w.pointer_move(row_point(&w, 4)).is_empty());
        // Back up two places, keeping the gap between them.
        let a = w.pointer_move(row_point(&w, 0));
        assert_eq!(a, vec![PlaylistAction::Reorder(vec![2, 1, 4, 3, 5])]);
        w.pointer_up(row_point(&w, 0));
        assert_eq!(w.selected(), vec![2, 4], "the selection stays");
        assert_eq!(ids(&w), vec![2, 1, 4, 3, 5]);
    }

    #[test]
    fn click_in_a_selection_without_dragging_selects_one() {
        let mut w = window(4);
        w.select_all();
        w.pointer_down(row_point(&w, 2), NONE, 0);
        assert_eq!(w.selected().len(), 4, "kept while pressed, for dragging");
        w.pointer_up(row_point(&w, 2));
        assert_eq!(w.selected(), vec![3]);
        w.invert_selection();
        assert_eq!(w.selected(), vec![1, 2, 4]);
        w.select_none();
        assert!(w.selected().is_empty());
    }

    #[test]
    fn queue_key_markers_and_right_click() {
        let skin = coded_skin();
        let mut w = window(3);
        assert!(w.key(0x71, NONE).is_empty(), "nothing selected");
        assert!(w.right_click(row_point(&w, 2)));
        assert_eq!(w.selected(), vec![3]);
        assert_eq!(
            w.key(0x71, NONE),
            vec![PlaylistAction::ToggleQueue(vec![3])]
        );
        // Right-click inside the selection keeps it; outside the list it does nothing.
        w.select_all();
        assert!(w.right_click(row_point(&w, 0)));
        assert_eq!(w.selected().len(), 3);
        assert!(!w.right_click(Point::new(5, 5)));
        let before = w.render_hash(1, 0);
        w.set_up_next(vec![3, 1]);
        assert_ne!(w.render_hash(1, 0), before, "markers redraw");
        // Rows still render with the markers.
        let b = w.render(&skin, &mut NoFallback, 0, 1);
        assert_eq!(b.width() as i32, w.size().0);
    }

    #[test]
    fn edges_resize_one_way() {
        let mut w = window(2);
        let (w0, h0) = w.size();
        // Right edge, below the title bar: width only.
        let p = Point::new(w0 - 1, 60);
        assert_eq!(w.grip(p), Some(crate::grip::Grip::Right));
        w.pointer_down(p, NONE, 0);
        assert_eq!(
            w.pointer_move(Point::new(p.x + 30, p.y + 50)),
            vec![PlaylistAction::Resize(w0 + STEP_W, h0)]
        );
        w.pointer_up(Point::new(p.x + 30, p.y + 50));
        // Bottom edge: height only.
        let (w1, _) = w.size();
        let p = Point::new(60, h0 - 1);
        w.pointer_down(p, NONE, 0);
        assert_eq!(
            w.pointer_move(Point::new(p.x + 80, p.y + 30)),
            vec![PlaylistAction::Resize(w1, h0 + STEP_H)]
        );
        w.pointer_up(p);
        // The close button is not an edge, and shaded windows do not resize.
        assert_eq!(w.grip(Point::new(w1 - 2, 5)), None);
        w.shaded = true;
        assert_eq!(w.grip(Point::new(w1 - 1, 60)), None);
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
    fn shade_bar_titles_without_a_current_track() {
        let skin = coded_skin();
        let shaded = |n: u64, current: Option<EntryId>| {
            let mut w = window(n);
            w.shaded = true;
            w.set_current(current);
            w.render(&skin, &mut NoFallback, 0, 1)
        };
        // Nothing current yet: the first track shows, as on the main window.
        let first = entries(3)[0].id;
        assert_eq!(shaded(3, None), shaded(3, Some(first)));
        // An empty list still has a label on the bar.
        let mut bare = PlaylistWindow::new();
        bare.shaded = true;
        let mut blank = Bitmap::new(275, 14, 0xFF00_0000);
        let mut x = 25;
        while x < 275 - 50 {
            skin.draw(&mut blank, sp::SHADE_TILE, x, 0);
            x += 25;
        }
        skin.draw(&mut blank, sp::SHADE_LEFT, 0, 0);
        skin.draw(&mut blank, sp::SHADE_RIGHT, 275 - 50, 0);
        assert_ne!(shaded(0, None), blank, "empty list shows a label");
        assert_eq!(bare.render(&skin, &mut NoFallback, 0, 1), shaded(0, None));
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
