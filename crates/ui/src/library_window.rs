//! The media library window, in the classic generic-window frame (GEN.BMP) with GENEX colors:
//! a category tree on the left (all tracks, artists, albums, genres), and on the right a
//! search field, a sortable track list and buttons (Play, Enqueue, Add folder, Rescan).
//!
//! The window holds no database: it asks for data with [`LibAction::Query`] and the app
//! answers through [`LibraryWindow::set_tracks`]. Coordinates are unscaled; sizes grow in the
//! classic 25 x 29 steps.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use amp_dock::Point;
use amp_skin::generic::{self, GenExColors, GenLetters, button};
use amp_skin::{Bitmap, Rect, Sheet, Skin, Sprite};

use crate::text::{TextRenderer, TextStrip};

pub const MIN_W: i32 = 275;
pub const MIN_H: i32 = 116;
pub const STEP_W: i32 = 25;
pub const STEP_H: i32 = 29;
const ROW_H: i32 = 13;
const TEXT_H: u32 = 10;
const TREE_W: i32 = 108;
const SCROLL_W: i32 = 8;
const DOUBLE_CLICK_MS: u64 = 400;
const CACHE_LIMIT: usize = 1024;

/// What the left tree selects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    All,
    Artist(String),
    Album(String),
    Genre(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Artists,
    Albums,
    Genres,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TreeItem {
    All(u64),
    Header(Section, bool),
    Value(Section, String, u64),
}

/// One track as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRow {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Artist,
    Title,
    Album,
    Time,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibAction {
    /// Show tracks for this selection and search text.
    Query {
        selection: Selection,
        text: String,
    },
    /// Replace the playlist with `ids` and start at `start`.
    Play {
        ids: Vec<i64>,
        start: usize,
    },
    Enqueue(Vec<i64>),
    AddFolder,
    Rescan,
    Close,
    /// The window wants this unscaled size.
    Resize(i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibDown {
    Handled,
    DragWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Play,
    Enqueue,
    AddFolder,
    Rescan,
}

const BUTTONS: [(Button, &str); 4] = [
    (Button::Play, "Play"),
    (Button::Enqueue, "Enqueue"),
    (Button::AddFolder, "Add folder"),
    (Button::Rescan, "Rescan"),
];
const BUTTON_W: i32 = 56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Close,
    Button(Button),
}

#[derive(Debug)]
pub struct LibraryWindow {
    w: i32,
    h: i32,
    pub focused: bool,
    total: u64,
    artists: Vec<(String, u64)>,
    albums: Vec<(String, u64)>,
    genres: Vec<(String, u64)>,
    expanded: BTreeSet<u8>,
    selection: Selection,
    tree_scroll: usize,
    search: String,
    rows: Vec<LibraryRow>,
    sort: Option<(Column, bool)>,
    selected: BTreeSet<i64>,
    anchor: Option<usize>,
    scroll: usize,
    status: String,
    pressed: Option<(Pressed, bool)>,
    scroll_drag: Option<i32>,
    resize: Option<(Point, i32, i32)>,
    last_click: Option<(i64, u64)>,
    text_cache: HashMap<(String, u32, u32), TextStrip>,
    /// Bumped when data (tree, rows, status, skin) changes, for [`Self::render_hash`].
    revision: u64,
}

impl Default for LibraryWindow {
    fn default() -> Self {
        Self::new()
    }
}

fn snap(v: i32, min: i32, step: i32) -> i32 {
    min + ((v - min).max(0) / step) * step
}

fn time_text(d: Option<Duration>) -> String {
    d.map(|d| {
        let s = d.as_secs();
        if s >= 3600 {
            format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        } else {
            format!("{}:{:02}", s / 60, s % 60)
        }
    })
    .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Draws a text strip inside `clip`, vertically centered, aligned horizontally.
fn draw_aligned(dst: &mut Bitmap, strip: &TextStrip, clip: Rect, align: Align) {
    let spare = (clip.w - strip.width()).max(0);
    let x = match align {
        Align::Left => clip.x,
        Align::Center => clip.x + spare / 2,
        Align::Right => clip.x + spare,
    };
    let y = clip.y + (clip.h - strip.height()) / 2;
    let draw_clip = Rect::new(x, clip.y, (clip.x + clip.w - x).max(0), clip.h);
    strip.draw(dst, draw_clip, y, 0, false, 0);
}

fn inside(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
}

fn section_key(s: Section) -> u8 {
    match s {
        Section::Artists => 0,
        Section::Albums => 1,
        Section::Genres => 2,
    }
}

impl LibraryWindow {
    pub fn new() -> Self {
        Self {
            w: MIN_W + 6 * STEP_W,
            h: MIN_H + 6 * STEP_H,
            focused: false,
            total: 0,
            artists: Vec::new(),
            albums: Vec::new(),
            genres: Vec::new(),
            expanded: BTreeSet::new(),
            selection: Selection::All,
            tree_scroll: 0,
            search: String::new(),
            rows: Vec::new(),
            sort: None,
            selected: BTreeSet::new(),
            anchor: None,
            scroll: 0,
            status: String::new(),
            pressed: None,
            scroll_drag: None,
            resize: None,
            last_click: None,
            text_cache: HashMap::new(),
            revision: 0,
        }
    }

    /// Forget text rendered with the previous skin's colors.
    pub fn skin_changed(&mut self) {
        self.revision += 1;
        self.text_cache.clear();
    }

    /// Changes whenever the rendered image would change.
    pub fn render_hash(&self, scale: u32) -> u64 {
        crate::render::hash_of(&(
            (scale, self.w, self.h, self.focused, self.revision),
            (
                format!("{:?}", self.selection),
                &self.search,
                &self.expanded,
            ),
            (
                self.tree_scroll,
                self.scroll,
                &self.selected,
                format!("{:?}", self.sort),
            ),
            (format!("{:?}", self.pressed), self.scroll_drag.is_some()),
        ))
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub fn set_size(&mut self, w: i32, h: i32) {
        self.w = snap(w, MIN_W, STEP_W);
        self.h = snap(h, MIN_H, STEP_H);
        self.clamp_scroll();
    }

    /// The query matching the current selection and search, to run on open.
    pub fn query(&self) -> LibAction {
        LibAction::Query {
            selection: self.selection.clone(),
            text: self.search.clone(),
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        let status = status.into();
        if status != self.status {
            self.revision += 1;
        }
        self.status = status;
    }

    /// Category contents and the total track count.
    pub fn set_tree(
        &mut self,
        total: u64,
        artists: Vec<(String, u64)>,
        albums: Vec<(String, u64)>,
        genres: Vec<(String, u64)>,
    ) {
        self.revision += 1;
        self.total = total;
        self.artists = artists;
        self.albums = albums;
        self.genres = genres;
    }

    /// Rows answering the last query.
    pub fn set_tracks(&mut self, rows: Vec<LibraryRow>) {
        self.revision += 1;
        let ids: BTreeSet<i64> = rows.iter().map(|r| r.id).collect();
        self.selected.retain(|id| ids.contains(id));
        self.rows = rows;
        self.apply_sort();
        self.anchor = None;
        self.clamp_scroll();
    }

    pub fn rows(&self) -> &[LibraryRow] {
        &self.rows
    }

    pub fn search_text(&self) -> &str {
        &self.search
    }

    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some() || self.scroll_drag.is_some() || self.resize.is_some()
    }

    // ---- geometry ----

    fn client(&self) -> Rect {
        Rect::new(
            generic::LEFT_W,
            generic::TOP_H,
            self.w - generic::LEFT_W - generic::RIGHT_W,
            self.h - generic::TOP_H - generic::BOTTOM_H,
        )
    }

    fn button_y(&self) -> i32 {
        self.h - generic::BOTTOM_H - button::HEIGHT - 3
    }

    fn tree_rect(&self) -> Rect {
        let c = self.client();
        Rect::new(c.x + 2, c.y + 2, TREE_W, self.button_y() - 3 - (c.y + 2))
    }

    fn search_rect(&self) -> Rect {
        let c = self.client();
        let x = c.x + TREE_W + 6;
        Rect::new(x + 40, c.y + 2, c.x + c.w - 2 - (x + 40), ROW_H)
    }

    fn header_rect(&self) -> Rect {
        let c = self.client();
        let x = c.x + TREE_W + 6;
        Rect::new(x, c.y + 2 + ROW_H + 3, c.x + c.w - 2 - x - SCROLL_W, ROW_H)
    }

    fn list_rect(&self) -> Rect {
        let hdr = self.header_rect();
        let y = hdr.y + ROW_H;
        Rect::new(hdr.x, y, hdr.w, self.button_y() - 3 - y)
    }

    fn scrollbar_rect(&self) -> Rect {
        let l = self.list_rect();
        Rect::new(l.x + l.w, l.y, SCROLL_W, l.h)
    }

    fn close_rect(&self) -> Rect {
        Rect::new(self.w - 11, 3, 9, 9)
    }

    fn resize_rect(&self) -> Rect {
        Rect::new(self.w - 15, self.h - 15, 15, 15)
    }

    fn button_rect(&self, i: usize) -> Rect {
        Rect::new(
            self.client().x + 2 + i as i32 * (BUTTON_W + 3),
            self.button_y(),
            BUTTON_W,
            button::HEIGHT,
        )
    }

    /// Column x offsets and widths inside the list.
    fn columns(&self) -> [(Column, i32, i32); 4] {
        let l = self.list_rect();
        let time_w = 36;
        let rest = (l.w - time_w).max(30);
        let artist_w = rest * 30 / 100;
        let album_w = rest * 30 / 100;
        let title_w = rest - artist_w - album_w;
        [
            (Column::Artist, l.x, artist_w),
            (Column::Title, l.x + artist_w, title_w),
            (Column::Album, l.x + artist_w + title_w, album_w),
            (Column::Time, l.x + rest, time_w),
        ]
    }

    fn visible_rows(&self) -> usize {
        (self.list_rect().h / ROW_H).max(1) as usize
    }

    fn tree_rows(&self) -> usize {
        (self.tree_rect().h / ROW_H).max(1) as usize
    }

    fn max_scroll(&self) -> usize {
        self.rows.len().saturating_sub(self.visible_rows())
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
        let tree_max = self.tree_items().len().saturating_sub(self.tree_rows());
        self.tree_scroll = self.tree_scroll.min(tree_max);
    }

    fn tree_items(&self) -> Vec<TreeItem> {
        let mut items = vec![TreeItem::All(self.total)];
        for (section, values) in [
            (Section::Artists, &self.artists),
            (Section::Albums, &self.albums),
            (Section::Genres, &self.genres),
        ] {
            let open = self.expanded.contains(&section_key(section));
            items.push(TreeItem::Header(section, open));
            if open {
                items.extend(
                    values
                        .iter()
                        .map(|(v, n)| TreeItem::Value(section, v.clone(), *n)),
                );
            }
        }
        items
    }

    fn handle_y(&self) -> i32 {
        let bar = self.scrollbar_rect();
        let handle_h = 18;
        let max = self.max_scroll();
        if max == 0 {
            bar.y
        } else {
            bar.y + (self.scroll as i64 * i64::from(bar.h - handle_h) / max as i64) as i32
        }
    }

    fn tree_selected(&self, item: &TreeItem) -> bool {
        match (item, &self.selection) {
            (TreeItem::All(_), Selection::All) => true,
            (TreeItem::Value(Section::Artists, v, _), Selection::Artist(s))
            | (TreeItem::Value(Section::Albums, v, _), Selection::Album(s))
            | (TreeItem::Value(Section::Genres, v, _), Selection::Genre(s)) => v == s,
            _ => false,
        }
    }

    // ---- behavior ----

    fn apply_sort(&mut self) {
        let Some((col, ascending)) = self.sort else {
            return;
        };
        self.rows.sort_by(|a, b| {
            let ord = match col {
                Column::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                Column::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                Column::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
                Column::Time => a.duration.cmp(&b.duration),
            };
            if ascending { ord } else { ord.reverse() }
        });
    }

    fn selected_ids(&self) -> Vec<i64> {
        self.rows
            .iter()
            .filter(|r| self.selected.contains(&r.id))
            .map(|r| r.id)
            .collect()
    }

    fn all_ids(&self) -> Vec<i64> {
        self.rows.iter().map(|r| r.id).collect()
    }

    /// Play from the selection (or the top), with the whole list queued.
    fn play_action(&self) -> LibAction {
        let start = self
            .rows
            .iter()
            .position(|r| self.selected.contains(&r.id))
            .unwrap_or(0);
        LibAction::Play {
            ids: self.all_ids(),
            start,
        }
    }

    fn enqueue_action(&self) -> LibAction {
        let sel = self.selected_ids();
        LibAction::Enqueue(if sel.is_empty() { self.all_ids() } else { sel })
    }

    fn select_tree(&mut self, item: &TreeItem) -> Vec<LibAction> {
        let selection = match item {
            TreeItem::All(_) => Selection::All,
            TreeItem::Header(section, _) => {
                let key = section_key(*section);
                if !self.expanded.remove(&key) {
                    self.expanded.insert(key);
                }
                self.clamp_scroll();
                return Vec::new();
            }
            TreeItem::Value(Section::Artists, v, _) => Selection::Artist(v.clone()),
            TreeItem::Value(Section::Albums, v, _) => Selection::Album(v.clone()),
            TreeItem::Value(Section::Genres, v, _) => Selection::Genre(v.clone()),
        };
        if selection == self.selection {
            return Vec::new();
        }
        self.selection = selection;
        self.scroll = 0;
        vec![self.query()]
    }

    pub fn pointer_down(
        &mut self,
        p: Point,
        ctrl: bool,
        shift: bool,
        now_ms: u64,
    ) -> (LibDown, Vec<LibAction>) {
        if inside(self.close_rect(), p) {
            self.pressed = Some((Pressed::Close, true));
            return (LibDown::Handled, Vec::new());
        }
        if inside(self.resize_rect(), p) {
            self.resize = Some((p, self.w, self.h));
            return (LibDown::Handled, Vec::new());
        }
        if p.y < generic::TOP_H {
            return (LibDown::DragWindow, Vec::new());
        }
        for (i, (b, _)) in BUTTONS.iter().enumerate() {
            if inside(self.button_rect(i), p) {
                self.pressed = Some((Pressed::Button(*b), true));
                return (LibDown::Handled, Vec::new());
            }
        }
        let tree = self.tree_rect();
        if inside(tree, p) {
            let index = ((p.y - tree.y) / ROW_H) as usize + self.tree_scroll;
            let actions = match self.tree_items().get(index).cloned() {
                Some(item) => self.select_tree(&item),
                None => Vec::new(),
            };
            return (LibDown::Handled, actions);
        }
        if inside(self.header_rect(), p) {
            if let Some((col, _, _)) = self
                .columns()
                .into_iter()
                .find(|(_, x, w)| p.x >= *x && p.x < x + w)
            {
                self.sort = Some(match self.sort {
                    Some((c, asc)) if c == col => (col, !asc),
                    _ => (col, true),
                });
                self.apply_sort();
            }
            return (LibDown::Handled, Vec::new());
        }
        if inside(self.scrollbar_rect(), p) {
            let top = self.handle_y();
            if !(top..top + 18).contains(&p.y) {
                self.scroll_to(p.y - 9);
            }
            self.scroll_drag = Some(p.y - self.handle_y());
            return (LibDown::Handled, Vec::new());
        }
        let list = self.list_rect();
        if inside(list, p) {
            return (LibDown::Handled, self.click_row(p, ctrl, shift, now_ms));
        }
        (LibDown::Handled, Vec::new())
    }

    fn scroll_to(&mut self, handle_top: i32) {
        let bar = self.scrollbar_rect();
        let travel = (bar.h - 18).max(1);
        let f = f64::from((handle_top - bar.y).clamp(0, travel)) / f64::from(travel);
        self.scroll = (f * self.max_scroll() as f64).round() as usize;
    }

    fn click_row(&mut self, p: Point, ctrl: bool, shift: bool, now_ms: u64) -> Vec<LibAction> {
        let list = self.list_rect();
        let row = ((p.y - list.y) / ROW_H) as usize + self.scroll;
        let Some(id) = self.rows.get(row).map(|r| r.id) else {
            if !ctrl {
                self.selected.clear();
            }
            return Vec::new();
        };
        if shift {
            let anchor = self.anchor.unwrap_or(row);
            let (a, b) = (anchor.min(row), anchor.max(row));
            if !ctrl {
                self.selected.clear();
            }
            self.selected.extend(self.rows[a..=b].iter().map(|r| r.id));
        } else if ctrl {
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
        if double && !ctrl && !shift {
            self.last_click = None;
            vec![LibAction::Play {
                ids: self.all_ids(),
                start: row,
            }]
        } else {
            Vec::new()
        }
    }

    pub fn pointer_move(&mut self, p: Point) -> Vec<LibAction> {
        if let Some(offset) = self.scroll_drag {
            self.scroll_to(p.y - offset);
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
                return vec![LibAction::Resize(w, h)];
            }
            return Vec::new();
        }
        if let Some((what, _)) = self.pressed {
            let r = match what {
                Pressed::Close => self.close_rect(),
                Pressed::Button(b) => {
                    self.button_rect(BUTTONS.iter().position(|(x, _)| *x == b).unwrap_or(0))
                }
            };
            self.pressed = Some((what, inside(r, p)));
        }
        Vec::new()
    }

    pub fn pointer_up(&mut self, p: Point) -> Vec<LibAction> {
        self.scroll_drag = None;
        if self.resize.take().is_some() {
            return Vec::new();
        }
        let Some((what, _)) = self.pressed.take() else {
            return Vec::new();
        };
        let (rect, action) = match what {
            Pressed::Close => (self.close_rect(), LibAction::Close),
            Pressed::Button(b) => {
                let i = BUTTONS.iter().position(|(x, _)| *x == b).unwrap_or(0);
                let action = match b {
                    Button::Play => self.play_action(),
                    Button::Enqueue => self.enqueue_action(),
                    Button::AddFolder => LibAction::AddFolder,
                    Button::Rescan => LibAction::Rescan,
                };
                (self.button_rect(i), action)
            }
        };
        if inside(rect, p) {
            vec![action]
        } else {
            Vec::new()
        }
    }

    /// Wheel over the tree or the list.
    pub fn wheel(&mut self, p: Point, up: bool) {
        let target = if inside(self.tree_rect(), p) {
            &mut self.tree_scroll
        } else {
            &mut self.scroll
        };
        *target = if up {
            target.saturating_sub(3)
        } else {
            *target + 3
        };
        self.clamp_scroll();
    }

    /// Typing goes to the search field; arrows, Enter, Escape and Ctrl+A act on the list.
    pub fn key(&mut self, keysym: u32, ctrl: bool) -> Vec<LibAction> {
        const BACKSPACE: u32 = 0xff08;
        const ESCAPE: u32 = 0xff1b;
        const RETURN: u32 = 0xff0d;
        const UP: u32 = 0xff52;
        const DOWN: u32 = 0xff54;
        match keysym {
            0x61 | 0x41 if ctrl => {
                self.selected = self.rows.iter().map(|r| r.id).collect();
                Vec::new()
            }
            RETURN => {
                if self.rows.is_empty() {
                    Vec::new()
                } else {
                    vec![self.play_action()]
                }
            }
            UP | DOWN => {
                if self.rows.is_empty() {
                    return Vec::new();
                }
                let cur = self.anchor.unwrap_or(0).min(self.rows.len() - 1);
                let i = if keysym == UP {
                    cur.saturating_sub(1)
                } else {
                    (cur + 1).min(self.rows.len() - 1)
                };
                self.selected.clear();
                self.selected.insert(self.rows[i].id);
                self.anchor = Some(i);
                if i < self.scroll {
                    self.scroll = i;
                } else if i >= self.scroll + self.visible_rows() {
                    self.scroll = i + 1 - self.visible_rows();
                }
                Vec::new()
            }
            BACKSPACE => {
                if self.search.pop().is_some() {
                    self.scroll = 0;
                    vec![self.query()]
                } else {
                    Vec::new()
                }
            }
            ESCAPE => {
                if self.search.is_empty() {
                    Vec::new()
                } else {
                    self.search.clear();
                    vec![self.query()]
                }
            }
            0x20..=0x7e if !ctrl => {
                if let Some(c) = char::from_u32(keysym) {
                    self.search.push(c);
                    self.scroll = 0;
                    return vec![self.query()];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    // ---- rendering ----

    fn strip(
        &mut self,
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        text: &str,
        color: u32,
        height: u32,
    ) -> TextStrip {
        let key = (text.to_owned(), color, height);
        if let Some(s) = self.text_cache.get(&key) {
            return s.clone();
        }
        if self.text_cache.len() >= CACHE_LIMIT {
            self.text_cache.clear();
        }
        let s = TextStrip::colored(skin, fallback, text, height, color);
        self.text_cache.insert(key, s.clone());
        s
    }

    pub fn render(&mut self, skin: &Skin, fallback: &mut dyn TextRenderer, scale: u32) -> Bitmap {
        let scale = scale.max(1);
        let (w, h) = (self.w, self.h);
        let colors = GenExColors::from_skin(skin);
        let letters = skin
            .sheet(Sheet::Gen)
            .map(GenLetters::scan)
            .unwrap_or_default();
        let mut b = Bitmap::new(w as u32, h as u32, colors.window_background);
        let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| {
            skin.draw(b, s, x, y);
        };
        let a = self.focused;
        let pick = |on: bool, x: Sprite, y: Sprite| if on { x } else { y };

        // Title bar: corners, fills, and the title centered between two end caps.
        const TITLE: &str = "MEDIA LIBRARY";
        let title_w = letters.width(TITLE) + 8;
        let center_x = (w - title_w) / 2;
        let mut x = 25;
        while x < w - 25 {
            draw(
                &mut b,
                pick(a, generic::TOP_FILL_ACTIVE, generic::TOP_FILL),
                x,
                0,
            );
            x += 25;
        }
        let mut x = center_x;
        while x < center_x + title_w {
            draw(
                &mut b,
                pick(a, generic::TOP_CENTER_FILL_ACTIVE, generic::TOP_CENTER_FILL),
                x,
                0,
            );
            x += 25;
        }
        draw(
            &mut b,
            pick(a, generic::TOP_LEFT_END_ACTIVE, generic::TOP_LEFT_END),
            center_x - 25,
            0,
        );
        draw(
            &mut b,
            pick(a, generic::TOP_RIGHT_END_ACTIVE, generic::TOP_RIGHT_END),
            center_x + title_w,
            0,
        );
        draw(
            &mut b,
            pick(a, generic::TOP_LEFT_ACTIVE, generic::TOP_LEFT),
            0,
            0,
        );
        draw(
            &mut b,
            pick(a, generic::TOP_RIGHT_ACTIVE, generic::TOP_RIGHT),
            w - 25,
            0,
        );
        letters.draw(skin, &mut b, TITLE, center_x + 4, 4, a);
        if self.pressed == Some((Pressed::Close, true)) {
            let r = self.close_rect();
            draw(&mut b, generic::CLOSE_PRESSED, r.x, r.y);
        }

        // Sides, with the bottom-end pieces just above the bottom bar.
        let side_bottom = h - generic::BOTTOM_H - 24;
        let mut y = generic::TOP_H;
        while y < side_bottom {
            draw(&mut b, generic::MIDDLE_LEFT, 0, y);
            draw(&mut b, generic::MIDDLE_RIGHT, w - generic::RIGHT_W, y);
            y += 29;
        }
        draw(&mut b, generic::MIDDLE_LEFT_BOTTOM, 0, side_bottom);
        draw(
            &mut b,
            generic::MIDDLE_RIGHT_BOTTOM,
            w - generic::RIGHT_W,
            side_bottom,
        );
        let mut x = 125;
        while x < w - 125 {
            draw(&mut b, generic::BOTTOM_FILL, x, h - generic::BOTTOM_H);
            x += 25;
        }
        draw(&mut b, generic::BOTTOM_LEFT, 0, h - generic::BOTTOM_H);
        draw(
            &mut b,
            generic::BOTTOM_RIGHT,
            w - 125,
            h - generic::BOTTOM_H,
        );

        b.fill_rect(self.client(), colors.window_background);

        // Panels: tree, search box, header, list, scrollbar.
        let tree = self.tree_rect();
        let search = self.search_rect();
        let header = self.header_rect();
        let list = self.list_rect();
        let bar = self.scrollbar_rect();
        for r in [tree, search, list] {
            b.fill_rect(r, colors.item_background);
        }
        b.fill_rect(header, colors.list_header_background);
        for (_, x, cw) in self.columns() {
            b.fill_rect(
                Rect::new(x, header.y, 1, header.h),
                colors.list_header_frame_top_left,
            );
            b.fill_rect(
                Rect::new(x + cw - 1, header.y, 1, header.h),
                colors.list_header_frame_bottom_right,
            );
        }
        b.fill_rect(bar, colors.scrollbar_dead_area);
        b.fill_rect(
            Rect::new(bar.x, self.handle_y(), bar.w, 18.min(bar.h)),
            colors.scrollbar_two,
        );

        // Selection backgrounds (text is drawn after scaling).
        let tree_items = self.tree_items();
        for (i, item) in tree_items
            .iter()
            .enumerate()
            .skip(self.tree_scroll)
            .take(self.tree_rows())
        {
            let selected = self.tree_selected(item);
            if selected {
                let y = tree.y + (i - self.tree_scroll) as i32 * ROW_H;
                b.fill_rect(
                    Rect::new(tree.x, y, tree.w, ROW_H),
                    colors.list_text_selected_background,
                );
            }
        }
        let visible: Vec<(usize, LibraryRow)> = self
            .rows
            .iter()
            .cloned()
            .enumerate()
            .skip(self.scroll)
            .take(self.visible_rows())
            .collect();
        for (i, row) in &visible {
            if self.selected.contains(&row.id) {
                let y = list.y + (*i - self.scroll) as i32 * ROW_H;
                b.fill_rect(
                    Rect::new(list.x, y, list.w, ROW_H),
                    colors.list_text_selected_background,
                );
            }
        }

        // Buttons.
        for (i, (btn, _)) in BUTTONS.iter().enumerate() {
            let r = self.button_rect(i);
            let down = self.pressed == Some((Pressed::Button(*btn), true));
            let (l, c, rt) = if down {
                (
                    button::LEFT_PRESSED,
                    button::CENTER_PRESSED,
                    button::RIGHT_PRESSED,
                )
            } else {
                (button::LEFT, button::CENTER, button::RIGHT)
            };
            // Tile the center piece, clipping the last tile to the button.
            if let Some(sheet) = skin.sheet(c.sheet) {
                let mut x = r.x + 4;
                while x < r.x + r.w - 4 {
                    let w = c.rect.w.min(r.x + r.w - 4 - x);
                    b.blit(sheet, Rect::new(c.rect.x, c.rect.y, w, c.rect.h), x, r.y);
                    x += c.rect.w;
                }
            }
            draw(&mut b, l, r.x, r.y);
            draw(&mut b, rt, r.x + r.w - 4, r.y);
        }

        // Text: skin-font strips are drawn now and scaled with the window; system-font
        // strips are drawn after scaling at full resolution.
        let s = scale as i32;
        let th = TEXT_H * scale;
        let mut texts: Vec<(String, u32, Rect, Align)> = Vec::new();
        let label = Rect::new(search.x - 40, search.y, 38, ROW_H);
        texts.push(("Search:".into(), colors.window_text, label, Align::Left));
        let cursor = if self.focused { "_" } else { "" };
        texts.push((
            format!("{}{cursor}", self.search),
            colors.item_foreground,
            search,
            Align::Left,
        ));
        for (i, item) in tree_items
            .iter()
            .enumerate()
            .skip(self.tree_scroll)
            .take(self.tree_rows())
        {
            let y = tree.y + (i - self.tree_scroll) as i32 * ROW_H;
            let (text, indent) = match item {
                TreeItem::All(n) => (format!("All tracks ({n})"), 2),
                TreeItem::Header(sec, open) => {
                    let name = match sec {
                        Section::Artists => "Artists",
                        Section::Albums => "Albums",
                        Section::Genres => "Genres",
                    };
                    (format!("{} {name}", if *open { "-" } else { "+" }), 2)
                }
                TreeItem::Value(_, v, n) => (format!("{v} ({n})"), 12),
            };
            let color = if self.tree_selected(item) {
                colors.list_text_selected
            } else {
                colors.item_foreground
            };
            texts.push((
                text,
                color,
                Rect::new(tree.x + indent, y, tree.w - indent, ROW_H),
                Align::Left,
            ));
        }
        for (col, x, cw) in self.columns() {
            let mut name = match col {
                Column::Artist => "Artist",
                Column::Title => "Title",
                Column::Album => "Album",
                Column::Time => "Time",
            }
            .to_owned();
            if let Some((c, asc)) = self.sort
                && c == col
            {
                name.push_str(if asc { " ^" } else { " v" });
            }
            texts.push((
                name,
                colors.list_header_text,
                Rect::new(x + 3, header.y, cw - 5, ROW_H),
                Align::Left,
            ));
        }
        for (i, row) in &visible {
            let y = list.y + (*i - self.scroll) as i32 * ROW_H;
            let color = if self.selected.contains(&row.id) {
                colors.list_text_selected
            } else {
                colors.item_foreground
            };
            for (col, x, cw) in self.columns() {
                let (text, align) = match col {
                    Column::Artist => (row.artist.clone(), Align::Left),
                    Column::Title => (row.title.clone(), Align::Left),
                    Column::Album => (row.album.clone(), Align::Left),
                    Column::Time => (time_text(row.duration), Align::Right),
                };
                texts.push((text, color, Rect::new(x + 3, y, cw - 6, ROW_H), align));
            }
        }
        for (i, (_, name)) in BUTTONS.iter().enumerate() {
            texts.push((
                (*name).to_owned(),
                colors.button_text,
                self.button_rect(i),
                Align::Center,
            ));
        }
        let status_x = self.button_rect(BUTTONS.len() - 1).x + BUTTON_W + 6;
        let status = Rect::new(
            status_x,
            self.button_y(),
            (self.client().x + self.client().w - 2 - status_x).max(0),
            button::HEIGHT,
        );
        texts.push((self.status.clone(), colors.window_text, status, Align::Left));

        let mut late = Vec::new();
        for (text, color, clip, align) in texts {
            if text.is_empty() {
                continue;
            }
            let strip = self.strip(skin, fallback, &text, color, th);
            if strip.is_bitmap() {
                draw_aligned(&mut b, &strip, clip, align);
            } else {
                late.push((
                    strip,
                    Rect::new(clip.x * s, clip.y * s, clip.w * s, clip.h * s),
                    align,
                ));
            }
        }
        let mut out = crate::render::scale_nearest(&b, scale);
        for (strip, clip, align) in late {
            draw_aligned(&mut out, &strip, clip, align);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::coded_skin;
    use crate::text::NoFallback;

    fn rows(n: i64) -> Vec<LibraryRow> {
        (1..=n)
            .map(|id| LibraryRow {
                id,
                artist: format!("Artist {}", (id % 3) + 1),
                title: format!("Title {id:02}"),
                album: format!("Album {}", (id % 2) + 1),
                duration: Some(Duration::from_secs(100 + id as u64)),
            })
            .collect()
    }

    fn window() -> LibraryWindow {
        let mut w = LibraryWindow::new();
        w.set_tree(
            10,
            vec![("Ma15".into(), 4), ("KEIKO".into(), 6)],
            vec![("Best 1".into(), 10)],
            vec![],
        );
        w.set_tracks(rows(10));
        w
    }

    fn row_point(w: &LibraryWindow, row: i32) -> Point {
        let l = w.list_rect();
        Point::new(l.x + 20, l.y + row * ROW_H + 5)
    }

    fn tree_point(w: &LibraryWindow, row: i32) -> Point {
        let t = w.tree_rect();
        Point::new(t.x + 20, t.y + row * ROW_H + 5)
    }

    #[test]
    fn layout_fits_inside_the_frame() {
        let w = window();
        let c = w.client();
        for r in [
            w.tree_rect(),
            w.search_rect(),
            w.header_rect(),
            w.list_rect(),
            w.scrollbar_rect(),
        ] {
            assert!(
                r.x >= c.x && r.y >= c.y && r.x + r.w <= c.x + c.w && r.y + r.h <= c.y + c.h,
                "{r:?} in {c:?}"
            );
            assert!(r.w > 0 && r.h > 0, "{r:?}");
        }
        let last = w.button_rect(BUTTONS.len() - 1);
        assert!(last.x + last.w <= c.x + c.w);
    }

    #[test]
    fn tree_expands_and_selects() {
        let mut w = window();
        // Rows: All, + Artists, + Albums, + Genres.
        let (_, a) = w.pointer_down(tree_point(&w, 1), false, false, 0);
        assert!(a.is_empty(), "header click only expands");
        // Now: All, - Artists, Ma15, KEIKO, + Albums, + Genres.
        let (_, a) = w.pointer_down(tree_point(&w, 3), false, false, 0);
        assert_eq!(
            a,
            vec![LibAction::Query {
                selection: Selection::Artist("KEIKO".into()),
                text: String::new()
            }]
        );
        let (_, a) = w.pointer_down(tree_point(&w, 3), false, false, 1000);
        assert!(a.is_empty(), "same selection does not re-query");
        let (_, a) = w.pointer_down(tree_point(&w, 0), false, false, 2000);
        assert_eq!(
            a,
            vec![LibAction::Query {
                selection: Selection::All,
                text: String::new()
            }]
        );
    }

    #[test]
    fn typing_searches() {
        let mut w = window();
        assert_eq!(
            w.key(0x4b, false),
            vec![LibAction::Query {
                selection: Selection::All,
                text: "K".into()
            }]
        );
        w.key(0x65, false);
        assert_eq!(w.search_text(), "Ke");
        assert_eq!(
            w.key(0xff08, false),
            vec![LibAction::Query {
                selection: Selection::All,
                text: "K".into()
            }]
        );
        assert_eq!(
            w.key(0xff1b, false),
            vec![LibAction::Query {
                selection: Selection::All,
                text: String::new()
            }]
        );
        assert!(w.key(0xff1b, false).is_empty());
    }

    #[test]
    fn double_click_plays_list_from_that_row() {
        let mut w = window();
        w.pointer_down(row_point(&w, 2), false, false, 100);
        let (_, a) = w.pointer_down(row_point(&w, 2), false, false, 300);
        assert_eq!(
            a,
            vec![LibAction::Play {
                ids: (1..=10).collect(),
                start: 2
            }]
        );
    }

    #[test]
    fn buttons_play_enqueue_and_more() {
        let mut w = window();
        let press = |w: &mut LibraryWindow, i: usize| {
            let r = w.button_rect(i);
            let p = Point::new(r.x + 5, r.y + 5);
            w.pointer_down(p, false, false, 0);
            w.pointer_up(p)
        };
        assert_eq!(
            press(&mut w, 1),
            vec![LibAction::Enqueue((1..=10).collect())],
            "nothing selected: all"
        );
        w.pointer_down(row_point(&w, 4), false, false, 10_000);
        w.pointer_down(row_point(&w, 6), true, false, 20_000);
        assert_eq!(press(&mut w, 1), vec![LibAction::Enqueue(vec![5, 7])]);
        assert_eq!(
            press(&mut w, 0),
            vec![LibAction::Play {
                ids: (1..=10).collect(),
                start: 4
            }]
        );
        assert_eq!(press(&mut w, 2), vec![LibAction::AddFolder]);
        assert_eq!(press(&mut w, 3), vec![LibAction::Rescan]);
    }

    #[test]
    fn header_click_sorts_and_toggles() {
        let mut w = window();
        let (_, title_x, _) = w.columns()[1];
        let p = Point::new(title_x + 5, w.header_rect().y + 3);
        w.pointer_down(p, false, false, 0);
        assert_eq!(w.rows()[0].title, "Title 01");
        w.pointer_down(p, false, false, 0);
        assert_eq!(w.rows()[0].title, "Title 10");
        let (_, time_x, _) = w.columns()[3];
        w.pointer_down(
            Point::new(time_x + 5, w.header_rect().y + 3),
            false,
            false,
            0,
        );
        assert_eq!(w.rows()[0].id, 1, "shortest first");
    }

    #[test]
    fn title_drags_close_closes_and_resize_snaps() {
        let mut w = window();
        assert_eq!(
            w.pointer_down(Point::new(150, 8), false, false, 0).0,
            LibDown::DragWindow
        );
        let c = w.close_rect();
        w.pointer_down(Point::new(c.x + 2, c.y + 2), false, false, 0);
        assert_eq!(
            w.pointer_up(Point::new(c.x + 2, c.y + 2)),
            vec![LibAction::Close]
        );
        let r = w.resize_rect();
        let start = Point::new(r.x + 3, r.y + 3);
        w.pointer_down(start, false, false, 0);
        assert_eq!(
            w.pointer_move(Point::new(start.x + 30, start.y + 30)),
            vec![LibAction::Resize(450, 319)]
        );
        w.pointer_up(start);
    }

    #[test]
    fn renders_frame_title_and_selection() {
        let skin = coded_skin();
        let mut w = window();
        w.focused = true;
        w.pointer_down(row_point(&w, 0), false, false, 0);
        let b = w.render(&skin, &mut NoFallback, 1);
        assert_eq!((b.width(), b.height()), (425, 290));
        assert_eq!(
            b.get(0, 0),
            Some(crate::test_skin::code(Sheet::Gen, 0, 0)),
            "active top-left"
        );
        let colors = GenExColors::from_skin(&skin);
        let l = w.list_rect();
        assert_eq!(
            b.get(l.x + l.w - 2, l.y + 1),
            Some(colors.list_text_selected_background)
        );
        let b2 = w.render(&skin, &mut NoFallback, 2);
        assert_eq!((b2.width(), b2.height()), (850, 580));
    }
}
