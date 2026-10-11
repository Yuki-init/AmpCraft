//! The media library window, laid out like the 5.x media library in a classic skin: the
//! generic-window frame (GEN.BMP) and GENEX colors, a "Local Media" tree on the left with a
//! Library button under it, and on the right a search bar, Artist and Album filter panes, the
//! track list, Play and Enqueue buttons and an item count.
//!
//! The window holds no database: it asks for data with [`LibAction::Query`] and the app
//! answers through [`LibraryWindow::set_filters`] and [`LibraryWindow::set_tracks`].
//! Coordinates are unscaled; sizes grow in the classic 25 x 29 steps.

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
const TREE_W: i32 = 100;
const SCROLL_W: i32 = 8;
const HANDLE_H: i32 = 18;
const GAP: i32 = 3;
const DOUBLE_CLICK_MS: u64 = 400;
const CACHE_LIMIT: usize = 1024;

/// The tree's views, as in the 5.x "Local Media" node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LibView {
    #[default]
    Audio,
    MostPlayed,
    RecentlyAdded,
    RecentlyPlayed,
    NeverPlayed,
}

const VIEWS: [(LibView, &str); 5] = [
    (LibView::Audio, "Audio"),
    (LibView::MostPlayed, "Most Played"),
    (LibView::RecentlyAdded, "Recently Added"),
    (LibView::RecentlyPlayed, "Recently Played"),
    (LibView::NeverPlayed, "Never Played"),
];

/// One track as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryRow {
    pub id: i64,
    pub artist: String,
    pub album: String,
    pub track: Option<u32>,
    pub title: String,
    pub duration: Option<Duration>,
    pub genre: String,
    pub year: Option<u32>,
    pub plays: u32,
    /// File size in bytes.
    pub size: u64,
}

/// A row of the Artist pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistRow {
    pub name: String,
    pub albums: u64,
    pub tracks: u64,
}

/// A row of the Album pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumRow {
    pub name: String,
    pub year: Option<u32>,
    pub tracks: u64,
}

/// Track list columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Artist,
    Album,
    Track,
    Title,
    Length,
    Genre,
    Year,
    Plays,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibAction {
    /// Show the filter panes and tracks for this view, search text and filter choice.
    Query {
        view: LibView,
        text: String,
        artist: Option<String>,
        album: Option<String>,
    },
    /// Replace the playlist with `ids` and start at `start`.
    Play {
        ids: Vec<i64>,
        start: usize,
    },
    Enqueue(Vec<i64>),
    /// Play every track the list shows once the preceding query has been answered
    /// (double-click in a filter pane).
    PlayAll,
    /// The Library button: the app shows its menu (add folder, rescan).
    LibraryMenu,
    Close,
    /// The window wants this unscaled size.
    Resize(i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibDown {
    Handled,
    DragWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Button {
    Play,
    Enqueue,
    Library,
    ClearSearch,
}

const PLAY_W: i32 = 40;
const ENQUEUE_W: i32 = 52;
const CLEAR_W: i32 = 66;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Pressed {
    Close,
    Button(Button),
}

/// The three scrolling lists on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Pane {
    Artists,
    Albums,
    Tracks,
}

/// A tree line: the Local Media node or one of its views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeItem {
    LocalMedia,
    View(LibView),
}

#[derive(Debug)]
pub struct LibraryWindow {
    w: i32,
    h: i32,
    pub focused: bool,
    view: LibView,
    tree_open: bool,
    search: String,
    artists: Vec<ArtistRow>,
    albums: Vec<AlbumRow>,
    artist: Option<String>,
    album: Option<String>,
    rows: Vec<LibraryRow>,
    sort: Option<(Column, bool)>,
    selected: BTreeSet<i64>,
    anchor: Option<usize>,
    /// Scroll offsets (rows) of the artist, album and track lists.
    scroll: [usize; 3],
    status: String,
    pressed: Option<(Pressed, bool)>,
    /// A scroll handle being dragged, and the pointer offset inside it.
    scroll_drag: Option<(Pane, i32)>,
    resize: Option<(Point, i32, i32)>,
    last_click: Option<(Pane, i64, u64)>,
    text_cache: HashMap<(String, u32, u32), TextStrip>,
    /// Bumped when data (panes, rows, status, skin) changes, for [`Self::render_hash`].
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

fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.2} MB", b / 1e6)
    } else {
        format!("{:.0} KB", b / 1e3)
    }
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

fn pane_index(p: Pane) -> usize {
    match p {
        Pane::Artists => 0,
        Pane::Albums => 1,
        Pane::Tracks => 2,
    }
}

/// The parts of a list pane: header, rows and scrollbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaneRects {
    frame: Rect,
    header: Rect,
    rows: Rect,
    bar: Rect,
}

impl PaneRects {
    /// Splits a framed area (1 px border) into header, rows and scrollbar.
    fn new(frame: Rect) -> Self {
        let inner = Rect::new(
            frame.x + 1,
            frame.y + 1,
            (frame.w - 2).max(0),
            (frame.h - 2).max(0),
        );
        let list_w = (inner.w - SCROLL_W).max(0);
        let header = Rect::new(inner.x, inner.y, list_w, ROW_H.min(inner.h));
        let rows = Rect::new(
            inner.x,
            inner.y + header.h,
            list_w,
            (inner.h - header.h).max(0),
        );
        // The scrollbar runs beside the rows; the header's corner above it stays blank.
        let bar = Rect::new(inner.x + list_w, rows.y, inner.w - list_w, rows.h);
        Self {
            frame,
            header,
            rows,
            bar,
        }
    }

    fn visible(&self) -> usize {
        (self.rows.h / ROW_H).max(1) as usize
    }
}

impl LibraryWindow {
    pub fn new() -> Self {
        Self {
            w: MIN_W + 14 * STEP_W,
            h: MIN_H + 10 * STEP_H,
            focused: false,
            view: LibView::Audio,
            tree_open: true,
            search: String::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            artist: None,
            album: None,
            rows: Vec::new(),
            sort: None,
            selected: BTreeSet::new(),
            anchor: None,
            scroll: [0; 3],
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
                self.view,
                self.tree_open,
                &self.search,
                &self.artist,
                &self.album,
            ),
            (self.scroll, &self.selected, self.sort),
            (self.pressed, self.scroll_drag.is_some()),
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

    /// The query matching the current view, search and filter choice.
    pub fn query(&self) -> LibAction {
        LibAction::Query {
            view: self.view,
            text: self.search.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
        }
    }

    /// A message shown instead of the item count (scan progress, errors). Empty clears it.
    pub fn set_status(&mut self, status: impl Into<String>) {
        let status = status.into();
        if status != self.status {
            self.revision += 1;
        }
        self.status = status;
    }

    /// The filter panes' contents. A chosen artist or album that is no longer listed is
    /// kept: the app asked for it and the track list still honors it.
    pub fn set_filters(&mut self, artists: Vec<ArtistRow>, albums: Vec<AlbumRow>) {
        self.revision += 1;
        self.artists = artists;
        self.albums = albums;
        self.clamp_scroll();
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

    pub fn view(&self) -> LibView {
        self.view
    }

    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some() || self.scroll_drag.is_some() || self.resize.is_some()
    }

    /// Plays every listed track from the top.
    pub fn play_all(&self) -> LibAction {
        LibAction::Play {
            ids: self.all_ids(),
            start: 0,
        }
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
        self.h - generic::BOTTOM_H - button::HEIGHT - GAP
    }

    fn tree_rect(&self) -> Rect {
        let c = self.client();
        Rect::new(
            c.x + 2,
            c.y + 2,
            TREE_W,
            (self.button_y() - GAP - (c.y + 2)).max(0),
        )
    }

    /// Left and right edges of the area right of the tree.
    fn right_x(&self) -> (i32, i32) {
        let c = self.client();
        (c.x + 2 + TREE_W + 4, c.x + c.w - 2)
    }

    fn search_row_y(&self) -> i32 {
        self.client().y + 2
    }

    fn search_rect(&self) -> Rect {
        let (x0, x1) = self.right_x();
        let x = x0 + 40;
        Rect::new(
            x,
            self.search_row_y(),
            (x1 - CLEAR_W - GAP - x).max(0),
            button::HEIGHT,
        )
    }

    fn button_rect(&self, b: Button) -> Rect {
        let (x0, x1) = self.right_x();
        let y = self.button_y();
        match b {
            Button::Play => Rect::new(x0, y, PLAY_W, button::HEIGHT),
            Button::Enqueue => Rect::new(x0 + PLAY_W + GAP, y, ENQUEUE_W, button::HEIGHT),
            Button::Library => Rect::new(self.client().x + 2, y, TREE_W, button::HEIGHT),
            Button::ClearSearch => {
                Rect::new(x1 - CLEAR_W, self.search_row_y(), CLEAR_W, button::HEIGHT)
            }
        }
    }

    /// The filter panes take the top part of the right side (at least a few rows), the
    /// track list the rest.
    fn pane_rects(&self, pane: Pane) -> PaneRects {
        let (x0, x1) = self.right_x();
        let top = self.search_row_y() + button::HEIGHT + GAP;
        let bottom = self.button_y() - GAP;
        let avail = (bottom - top).max(0);
        let filter_h = (avail * 2 / 5).max(ROW_H * 4 + 2).min(avail / 2);
        let half = ((x1 - x0) - GAP) / 2;
        let frame = match pane {
            Pane::Artists => Rect::new(x0, top, half.max(0), filter_h),
            Pane::Albums => Rect::new(
                x0 + half + GAP,
                top,
                (x1 - x0 - half - GAP).max(0),
                filter_h,
            ),
            Pane::Tracks => Rect::new(
                x0,
                top + filter_h + GAP,
                (x1 - x0).max(0),
                (avail - filter_h - GAP).max(0),
            ),
        };
        PaneRects::new(frame)
    }

    fn close_rect(&self) -> Rect {
        Rect::new(self.w - 11, 3, 9, 9)
    }

    fn resize_rect(&self) -> Rect {
        Rect::new(self.w - 15, self.h - 15, 15, 15)
    }

    /// Track columns that fit, with x offsets and widths. Fixed-width columns are dropped
    /// from the right when the window is narrow; the rest share the remaining width.
    fn columns(&self) -> Vec<(Column, i32, i32)> {
        let r = self.pane_rects(Pane::Tracks).rows;
        let fixed = [
            (Column::Track, 20),
            (Column::Length, 36),
            (Column::Genre, 56),
            (Column::Year, 30),
            (Column::Plays, 30),
        ];
        // Keep fixed columns while the flexible ones keep at least 150 px.
        let mut keep = fixed.len();
        while keep > 0 && r.w - fixed[..keep].iter().map(|(_, w)| w).sum::<i32>() < 150 {
            keep -= 1;
        }
        let kept: Vec<(Column, i32)> = fixed[..keep].to_vec();
        let width = |c: Column| kept.iter().find(|(k, _)| *k == c).map(|(_, w)| *w);
        let flex = (r.w - kept.iter().map(|(_, w)| w).sum::<i32>()).max(30);
        let order = [
            (Column::Artist, Some(flex * 28 / 100)),
            (Column::Album, Some(flex * 28 / 100)),
            (Column::Track, width(Column::Track)),
            (Column::Title, Some(flex - 2 * (flex * 28 / 100))),
            (Column::Length, width(Column::Length)),
            (Column::Genre, width(Column::Genre)),
            (Column::Year, width(Column::Year)),
            (Column::Plays, width(Column::Plays)),
        ];
        let mut x = r.x;
        let mut out = Vec::new();
        for (c, w) in order {
            if let Some(w) = w {
                out.push((c, x, w));
                x += w;
            }
        }
        out
    }

    fn pane_len(&self, pane: Pane) -> usize {
        match pane {
            // Plus the "All" row.
            Pane::Artists => self.artists.len() + 1,
            Pane::Albums => self.albums.len() + 1,
            Pane::Tracks => self.rows.len(),
        }
    }

    fn max_scroll(&self, pane: Pane) -> usize {
        self.pane_len(pane)
            .saturating_sub(self.pane_rects(pane).visible())
    }

    fn clamp_scroll(&mut self) {
        for pane in [Pane::Artists, Pane::Albums, Pane::Tracks] {
            let i = pane_index(pane);
            self.scroll[i] = self.scroll[i].min(self.max_scroll(pane));
        }
    }

    fn handle_y(&self, pane: Pane) -> i32 {
        let bar = self.pane_rects(pane).bar;
        let max = self.max_scroll(pane);
        if max == 0 {
            bar.y
        } else {
            bar.y
                + (self.scroll[pane_index(pane)] as i64 * i64::from((bar.h - HANDLE_H).max(0))
                    / max as i64) as i32
        }
    }

    fn tree_items(&self) -> Vec<TreeItem> {
        let mut items = vec![TreeItem::LocalMedia];
        if self.tree_open {
            items.extend(VIEWS.iter().map(|(v, _)| TreeItem::View(*v)));
        }
        items
    }

    // ---- behavior ----

    fn apply_sort(&mut self) {
        let Some((col, ascending)) = self.sort else {
            return;
        };
        let lower = |s: &str| s.to_lowercase();
        self.rows.sort_by(|a, b| {
            let ord = match col {
                Column::Artist => lower(&a.artist).cmp(&lower(&b.artist)),
                Column::Album => lower(&a.album).cmp(&lower(&b.album)),
                Column::Track => a.track.cmp(&b.track),
                Column::Title => lower(&a.title).cmp(&lower(&b.title)),
                Column::Length => a.duration.cmp(&b.duration),
                Column::Genre => lower(&a.genre).cmp(&lower(&b.genre)),
                Column::Year => a.year.cmp(&b.year),
                Column::Plays => a.plays.cmp(&b.plays),
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

    fn click_tree(&mut self, p: Point) -> Vec<LibAction> {
        let tree = self.tree_rect();
        let index = ((p.y - tree.y) / ROW_H).max(0) as usize;
        match self.tree_items().get(index) {
            Some(TreeItem::LocalMedia) => {
                self.tree_open = !self.tree_open;
                Vec::new()
            }
            Some(TreeItem::View(v)) if *v != self.view => {
                self.view = *v;
                self.artist = None;
                self.album = None;
                self.scroll = [0; 3];
                vec![self.query()]
            }
            _ => Vec::new(),
        }
    }

    /// Picks the artist (or album) in row `index` of a filter pane; row 0 is "All".
    fn click_filter(&mut self, pane: Pane, index: usize, now_ms: u64) -> Vec<LibAction> {
        let name = match pane {
            Pane::Artists if index == 0 => None,
            Pane::Artists => match self.artists.get(index - 1) {
                Some(a) => Some(a.name.clone()),
                None => return Vec::new(),
            },
            _ if index == 0 => None,
            _ => match self.albums.get(index - 1) {
                Some(a) => Some(a.name.clone()),
                None => return Vec::new(),
            },
        };
        let key = index as i64;
        let double = self.last_click.is_some_and(|(pn, k, t)| {
            pn == pane && k == key && now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS
        });
        self.last_click = Some((pane, key, now_ms));
        let changed = match pane {
            Pane::Artists if self.artist != name => {
                self.artist = name;
                self.album = None;
                self.scroll[1] = 0;
                true
            }
            Pane::Albums if self.album != name => {
                self.album = name;
                true
            }
            _ => false,
        };
        let mut actions = Vec::new();
        if changed {
            self.scroll[2] = 0;
            actions.push(self.query());
        }
        if double {
            self.last_click = None;
            actions.push(LibAction::PlayAll);
        }
        actions
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
        for b in [
            Button::Play,
            Button::Enqueue,
            Button::Library,
            Button::ClearSearch,
        ] {
            if inside(self.button_rect(b), p) {
                self.pressed = Some((Pressed::Button(b), true));
                return (LibDown::Handled, Vec::new());
            }
        }
        if inside(self.tree_rect(), p) {
            return (LibDown::Handled, self.click_tree(p));
        }
        for pane in [Pane::Artists, Pane::Albums, Pane::Tracks] {
            let r = self.pane_rects(pane);
            if inside(r.bar, p) {
                let top = self.handle_y(pane);
                if !(top..top + HANDLE_H).contains(&p.y) {
                    self.scroll_to(pane, p.y - HANDLE_H / 2);
                }
                self.scroll_drag = Some((pane, p.y - self.handle_y(pane)));
                return (LibDown::Handled, Vec::new());
            }
            if pane == Pane::Tracks && inside(r.header, p) {
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
            if inside(r.rows, p) {
                let index = ((p.y - r.rows.y) / ROW_H) as usize + self.scroll[pane_index(pane)];
                let actions = match pane {
                    Pane::Tracks => self.click_row(index, ctrl, shift, now_ms),
                    _ => self.click_filter(pane, index, now_ms),
                };
                return (LibDown::Handled, actions);
            }
        }
        (LibDown::Handled, Vec::new())
    }

    fn scroll_to(&mut self, pane: Pane, handle_top: i32) {
        let bar = self.pane_rects(pane).bar;
        let travel = (bar.h - HANDLE_H).max(1);
        let f = f64::from((handle_top - bar.y).clamp(0, travel)) / f64::from(travel);
        self.scroll[pane_index(pane)] = (f * self.max_scroll(pane) as f64).round() as usize;
    }

    fn click_row(&mut self, row: usize, ctrl: bool, shift: bool, now_ms: u64) -> Vec<LibAction> {
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
        let double = self.last_click.is_some_and(|(pane, last, t)| {
            pane == Pane::Tracks && last == id && now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS
        });
        self.last_click = Some((Pane::Tracks, id, now_ms));
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
        if let Some((pane, offset)) = self.scroll_drag {
            self.scroll_to(pane, p.y - offset);
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
                Pressed::Button(b) => self.button_rect(b),
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
        let rect = match what {
            Pressed::Close => self.close_rect(),
            Pressed::Button(b) => self.button_rect(b),
        };
        if !inside(rect, p) {
            return Vec::new();
        }
        let action = match what {
            Pressed::Close => LibAction::Close,
            Pressed::Button(Button::Play) => self.play_action(),
            Pressed::Button(Button::Enqueue) => self.enqueue_action(),
            Pressed::Button(Button::Library) => LibAction::LibraryMenu,
            Pressed::Button(Button::ClearSearch) => {
                if self.search.is_empty() {
                    return Vec::new();
                }
                self.search.clear();
                self.scroll = [0; 3];
                self.query()
            }
        };
        vec![action]
    }

    /// Wheel over one of the lists.
    pub fn wheel(&mut self, p: Point, up: bool) {
        let pane = [Pane::Artists, Pane::Albums]
            .into_iter()
            .find(|pane| inside(self.pane_rects(*pane).frame, p))
            .unwrap_or(Pane::Tracks);
        let s = &mut self.scroll[pane_index(pane)];
        *s = if up { s.saturating_sub(3) } else { *s + 3 };
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
                let visible = self.pane_rects(Pane::Tracks).visible();
                let s = &mut self.scroll[2];
                if i < *s {
                    *s = i;
                } else if i >= *s + visible {
                    *s = i + 1 - visible;
                }
                Vec::new()
            }
            BACKSPACE => {
                if self.search.pop().is_some() {
                    self.scroll = [0; 3];
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
                    self.scroll = [0; 3];
                    vec![self.query()]
                }
            }
            0x20..=0x7e if !ctrl => {
                if let Some(c) = char::from_u32(keysym) {
                    self.search.push(c);
                    self.scroll = [0; 3];
                    return vec![self.query()];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The line under the track list: scan messages, else counts, length and size.
    fn status_text(&self) -> String {
        if !self.status.is_empty() {
            return self.status.clone();
        }
        let total: Duration = self.rows.iter().filter_map(|r| r.duration).sum();
        let size: u64 = self.rows.iter().map(|r| r.size).sum();
        let n = self.rows.len();
        format!(
            "{n} item{} [{}] [{}]",
            if n == 1 { "" } else { "s" },
            time_text(Some(total)),
            size_text(size)
        )
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

    fn draw_frame(&self, b: &mut Bitmap, skin: &Skin) {
        let (w, h) = (self.w, self.h);
        let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| skin.draw(b, s, x, y);
        let a = self.focused;
        let pick = |on: bool, x: Sprite, y: Sprite| if on { x } else { y };
        let letters = skin
            .sheet(Sheet::Gen)
            .map(GenLetters::scan)
            .unwrap_or_default();

        // Title bar: corners, fills, and the title centered between two end caps.
        const TITLE: &str = "MEDIA LIBRARY";
        let title_w = letters.width(TITLE) + 8;
        let center_x = (w - title_w) / 2;
        let mut x = 25;
        while x < w - 25 {
            draw(
                b,
                pick(a, generic::TOP_FILL_ACTIVE, generic::TOP_FILL),
                x,
                0,
            );
            x += 25;
        }
        let mut x = center_x;
        while x < center_x + title_w {
            draw(
                b,
                pick(a, generic::TOP_CENTER_FILL_ACTIVE, generic::TOP_CENTER_FILL),
                x,
                0,
            );
            x += 25;
        }
        draw(
            b,
            pick(a, generic::TOP_LEFT_END_ACTIVE, generic::TOP_LEFT_END),
            center_x - 25,
            0,
        );
        draw(
            b,
            pick(a, generic::TOP_RIGHT_END_ACTIVE, generic::TOP_RIGHT_END),
            center_x + title_w,
            0,
        );
        draw(
            b,
            pick(a, generic::TOP_LEFT_ACTIVE, generic::TOP_LEFT),
            0,
            0,
        );
        draw(
            b,
            pick(a, generic::TOP_RIGHT_ACTIVE, generic::TOP_RIGHT),
            w - 25,
            0,
        );
        letters.draw(skin, b, TITLE, center_x + 4, 4, a);
        if self.pressed == Some((Pressed::Close, true)) {
            let r = self.close_rect();
            draw(b, generic::CLOSE_PRESSED, r.x, r.y);
        }

        // Sides, with the bottom-end pieces just above the bottom bar.
        let side_bottom = h - generic::BOTTOM_H - 24;
        let mut y = generic::TOP_H;
        while y < side_bottom {
            draw(b, generic::MIDDLE_LEFT, 0, y);
            draw(b, generic::MIDDLE_RIGHT, w - generic::RIGHT_W, y);
            y += 29;
        }
        draw(b, generic::MIDDLE_LEFT_BOTTOM, 0, side_bottom);
        draw(
            b,
            generic::MIDDLE_RIGHT_BOTTOM,
            w - generic::RIGHT_W,
            side_bottom,
        );
        let mut x = 125;
        while x < w - 125 {
            draw(b, generic::BOTTOM_FILL, x, h - generic::BOTTOM_H);
            x += 25;
        }
        draw(b, generic::BOTTOM_LEFT, 0, h - generic::BOTTOM_H);
        draw(b, generic::BOTTOM_RIGHT, w - 125, h - generic::BOTTOM_H);
    }

    fn draw_button(&self, b: &mut Bitmap, skin: &Skin, which: Button) {
        let r = self.button_rect(which);
        if r.w < 8 {
            return;
        }
        let down = self.pressed == Some((Pressed::Button(which), true));
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
        skin.draw(b, l, r.x, r.y);
        skin.draw(b, rt, r.x + r.w - 4, r.y);
    }

    /// A list pane's frame, header cells (raised), background, scrollbar and selection.
    fn draw_pane(
        &self,
        b: &mut Bitmap,
        colors: &GenExColors,
        pane: Pane,
        cells: &[(i32, i32)],
        selected_rows: &[usize],
    ) {
        let r = self.pane_rects(pane);
        if r.frame.w <= 2 || r.frame.h <= 2 {
            return;
        }
        b.fill_rect(r.frame, colors.divider);
        b.fill_rect(
            Rect::new(r.header.x, r.header.y, r.header.w + r.bar.w, r.header.h),
            colors.list_header_dead_area,
        );
        b.fill_rect(r.rows, colors.item_background);
        for &(x, w) in cells {
            let cell = Rect::new(
                x,
                r.header.y,
                w.min(r.header.x + r.header.w - x),
                r.header.h,
            );
            if cell.w <= 0 {
                continue;
            }
            b.fill_rect(cell, colors.list_header_background);
            b.fill_rect(
                Rect::new(cell.x, cell.y, cell.w, 1),
                colors.list_header_frame_top_left,
            );
            b.fill_rect(
                Rect::new(cell.x, cell.y, 1, cell.h),
                colors.list_header_frame_top_left,
            );
            b.fill_rect(
                Rect::new(cell.x, cell.y + cell.h - 1, cell.w, 1),
                colors.list_header_frame_bottom_right,
            );
            b.fill_rect(
                Rect::new(cell.x + cell.w - 1, cell.y, 1, cell.h),
                colors.list_header_frame_bottom_right,
            );
        }
        // Scrollbar: track and a raised handle.
        b.fill_rect(r.bar, colors.scrollbar_dead_area);
        let hy = self.handle_y(pane);
        let handle = Rect::new(r.bar.x, hy, r.bar.w, HANDLE_H.min(r.bar.h));
        let dragging = self.scroll_drag.is_some_and(|(p, _)| p == pane);
        let (one, two) = if dragging {
            (colors.pressed_scrollbar_one, colors.pressed_scrollbar_two)
        } else {
            (colors.scrollbar_one, colors.scrollbar_two)
        };
        b.fill_rect(handle, two);
        b.fill_rect(Rect::new(handle.x, handle.y, handle.w, 1), one);
        b.fill_rect(Rect::new(handle.x, handle.y, 1, handle.h), one);
        let scroll = self.scroll[pane_index(pane)];
        for &row in selected_rows {
            if row < scroll || row >= scroll + r.visible() {
                continue;
            }
            let y = r.rows.y + (row - scroll) as i32 * ROW_H;
            b.fill_rect(
                Rect::new(r.rows.x, y, r.rows.w, ROW_H.min(r.rows.y + r.rows.h - y)),
                colors.list_text_selected_background,
            );
        }
    }

    pub fn render(&mut self, skin: &Skin, fallback: &mut dyn TextRenderer, scale: u32) -> Bitmap {
        let scale = scale.max(1);
        let colors = GenExColors::from_skin(skin);
        let mut b = Bitmap::new(self.w as u32, self.h as u32, colors.window_background);
        self.draw_frame(&mut b, skin);
        b.fill_rect(self.client(), colors.window_background);

        // Tree.
        let tree = self.tree_rect();
        b.fill_rect(tree, colors.divider);
        let tree_in = Rect::new(
            tree.x + 1,
            tree.y + 1,
            (tree.w - 2).max(0),
            (tree.h - 2).max(0),
        );
        b.fill_rect(tree_in, colors.item_background);
        let tree_items = self.tree_items();
        let tree_rows = (tree_in.h / ROW_H).max(0) as usize;
        let mut texts: Vec<(String, u32, Rect, Align)> = Vec::new();
        for (i, item) in tree_items.iter().enumerate().take(tree_rows) {
            let y = tree_in.y + i as i32 * ROW_H;
            let (label, indent, selected) = match item {
                TreeItem::LocalMedia => {
                    // Expand box: a small square with + or -.
                    let bx = Rect::new(tree_in.x + 2, y + 2, 9, 9);
                    b.fill_rect(bx, colors.item_foreground);
                    b.fill_rect(Rect::new(bx.x + 1, bx.y + 1, 7, 7), colors.item_background);
                    b.fill_rect(Rect::new(bx.x + 2, bx.y + 4, 5, 1), colors.item_foreground);
                    if !self.tree_open {
                        b.fill_rect(Rect::new(bx.x + 4, bx.y + 2, 1, 5), colors.item_foreground);
                    }
                    ("Local Media", 14, false)
                }
                TreeItem::View(v) => {
                    let name = VIEWS.iter().find(|(x, _)| x == v).map_or("", |(_, n)| *n);
                    (name, 24, *v == self.view)
                }
            };
            let clip = Rect::new(tree_in.x + indent, y, (tree_in.w - indent).max(0), ROW_H);
            let color = if selected {
                b.fill_rect(
                    Rect::new(clip.x - 2, y, clip.w + 2, ROW_H),
                    colors.list_text_selected_background,
                );
                colors.list_text_selected
            } else {
                colors.item_foreground
            };
            texts.push((label.to_owned(), color, clip, Align::Left));
        }

        // Search row.
        let (x0, _) = self.right_x();
        let search = self.search_rect();
        texts.push((
            "Search:".into(),
            colors.window_text,
            Rect::new(x0, search.y, 38, search.h),
            Align::Left,
        ));
        if search.w > 2 {
            b.fill_rect(search, colors.divider);
            let field = Rect::new(search.x + 1, search.y + 1, search.w - 2, search.h - 2);
            b.fill_rect(field, colors.item_background);
            let cursor = if self.focused { "_" } else { "" };
            texts.push((
                format!("{}{cursor}", self.search),
                colors.item_foreground,
                Rect::new(field.x + 2, field.y, field.w - 3, field.h),
                Align::Left,
            ));
        }

        // Filter panes.
        let artist_pane = self.pane_rects(Pane::Artists);
        let album_pane = self.pane_rects(Pane::Albums);
        let filter_cols = |r: Rect, a: &'static str, b: &'static str, c: &'static str| {
            let (w2, w3) = (36.min(r.w / 4), 36.min(r.w / 4));
            let w1 = (r.w - w2 - w3).max(0);
            [
                (a, r.x, w1, Align::Left),
                (b, r.x + w1, w2, Align::Right),
                (c, r.x + w1 + w2, w3, Align::Right),
            ]
        };
        let artist_cols = filter_cols(artist_pane.header, "Artist", "Albums", "Tracks");
        let album_cols = filter_cols(album_pane.header, "Album", "Year", "Tracks");
        let artist_sel = match &self.artist {
            None => vec![0],
            Some(a) => self
                .artists
                .iter()
                .position(|x| &x.name == a)
                .map(|i| vec![i + 1])
                .unwrap_or_default(),
        };
        let album_sel = match &self.album {
            None => vec![0],
            Some(a) => self
                .albums
                .iter()
                .position(|x| &x.name == a)
                .map(|i| vec![i + 1])
                .unwrap_or_default(),
        };
        let cells = |cols: &[(&str, i32, i32, Align); 3]| -> Vec<(i32, i32)> {
            cols.iter().map(|(_, x, w, _)| (*x, *w)).collect()
        };
        self.draw_pane(
            &mut b,
            &colors,
            Pane::Artists,
            &cells(&artist_cols),
            &artist_sel,
        );
        self.draw_pane(
            &mut b,
            &colors,
            Pane::Albums,
            &cells(&album_cols),
            &album_sel,
        );
        let artist_rows: Vec<[String; 3]> = std::iter::once([
            format!(
                "All ({} artist{})",
                self.artists.len(),
                if self.artists.len() == 1 { "" } else { "s" }
            ),
            self.albums.len().to_string(),
            self.artists
                .iter()
                .map(|a| a.tracks)
                .sum::<u64>()
                .to_string(),
        ])
        .chain(
            self.artists
                .iter()
                .map(|a| [a.name.clone(), a.albums.to_string(), a.tracks.to_string()]),
        )
        .collect();
        let album_rows: Vec<[String; 3]> = std::iter::once([
            format!(
                "All ({} album{})",
                self.albums.len(),
                if self.albums.len() == 1 { "" } else { "s" }
            ),
            String::new(),
            self.albums
                .iter()
                .map(|a| a.tracks)
                .sum::<u64>()
                .to_string(),
        ])
        .chain(self.albums.iter().map(|a| {
            [
                a.name.clone(),
                a.year.map(|y| y.to_string()).unwrap_or_default(),
                a.tracks.to_string(),
            ]
        }))
        .collect();
        for (pane, cols, rows, sel) in [
            (Pane::Artists, &artist_cols, &artist_rows, &artist_sel),
            (Pane::Albums, &album_cols, &album_rows, &album_sel),
        ] {
            let r = self.pane_rects(pane);
            if r.rows.h <= 0 {
                continue;
            }
            for (name, x, w, align) in cols.iter() {
                texts.push((
                    (*name).to_owned(),
                    colors.list_header_text,
                    Rect::new(x + 3, r.header.y, (w - 6).max(0), r.header.h),
                    *align,
                ));
            }
            let scroll = self.scroll[pane_index(pane)];
            for (i, row) in rows.iter().enumerate().skip(scroll).take(r.visible()) {
                let y = r.rows.y + (i - scroll) as i32 * ROW_H;
                if y + ROW_H > r.rows.y + r.rows.h {
                    break;
                }
                let color = if sel.contains(&i) {
                    colors.list_text_selected
                } else {
                    colors.item_foreground
                };
                for ((_, x, w, align), text) in cols.iter().zip(row) {
                    texts.push((
                        text.clone(),
                        color,
                        Rect::new(x + 3, y, (w - 6).max(0), ROW_H),
                        *align,
                    ));
                }
            }
        }

        // Track list.
        let tracks = self.pane_rects(Pane::Tracks);
        let columns = self.columns();
        let scroll = self.scroll[2];
        let selected_rows: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| self.selected.contains(&r.id))
            .map(|(i, _)| i)
            .collect();
        let col_cells: Vec<(i32, i32)> = columns.iter().map(|(_, x, w)| (*x, *w)).collect();
        self.draw_pane(&mut b, &colors, Pane::Tracks, &col_cells, &selected_rows);
        if tracks.rows.h > 0 {
            for (col, x, w) in &columns {
                let name = match col {
                    Column::Artist => "Artist",
                    Column::Album => "Album",
                    Column::Track => "#",
                    Column::Title => "Title",
                    Column::Length => "Length",
                    Column::Genre => "Genre",
                    Column::Year => "Year",
                    Column::Plays => "Plays",
                };
                let mut clip = Rect::new(x + 3, tracks.header.y, (w - 6).max(0), tracks.header.h);
                if let Some((c, asc)) = self.sort
                    && c == *col
                    && clip.w > 8
                {
                    // Sort arrow at the right of the header cell.
                    let ax = x + w - 9;
                    let ay = tracks.header.y + 4;
                    // A 5 px wide triangle: point up for ascending, down for descending.
                    for i in 0..3 {
                        let yy = if asc { ay + i } else { ay + 2 - i };
                        b.fill_rect(
                            Rect::new(ax + 2 - i, yy, 1 + 2 * i, 1),
                            colors.list_header_text,
                        );
                    }
                    clip.w -= 8;
                }
                texts.push((name.into(), colors.list_header_text, clip, Align::Left));
            }
            let visible = tracks.visible();
            for (i, row) in self.rows.iter().enumerate().skip(scroll).take(visible) {
                let y = tracks.rows.y + (i - scroll) as i32 * ROW_H;
                if y + ROW_H > tracks.rows.y + tracks.rows.h {
                    break;
                }
                let color = if self.selected.contains(&row.id) {
                    colors.list_text_selected
                } else {
                    colors.item_foreground
                };
                for (col, x, w) in &columns {
                    let (text, align) = match col {
                        Column::Artist => (row.artist.clone(), Align::Left),
                        Column::Album => (row.album.clone(), Align::Left),
                        Column::Track => (
                            row.track.map(|t| t.to_string()).unwrap_or_default(),
                            Align::Right,
                        ),
                        Column::Title => (row.title.clone(), Align::Left),
                        Column::Length => (time_text(row.duration), Align::Right),
                        Column::Genre => (row.genre.clone(), Align::Left),
                        Column::Year => (
                            row.year.map(|y| y.to_string()).unwrap_or_default(),
                            Align::Right,
                        ),
                        Column::Plays => (
                            if row.plays > 0 {
                                row.plays.to_string()
                            } else {
                                String::new()
                            },
                            Align::Right,
                        ),
                    };
                    texts.push((
                        text,
                        color,
                        Rect::new(x + 3, y, (w - 6).max(0), ROW_H),
                        align,
                    ));
                }
            }
        }

        // Buttons and the status line.
        for (btn, label) in [
            (Button::Library, "Library"),
            (Button::Play, "Play"),
            (Button::Enqueue, "Enqueue"),
            (Button::ClearSearch, "Clear Search"),
        ] {
            self.draw_button(&mut b, skin, btn);
            texts.push((
                label.into(),
                colors.button_text,
                self.button_rect(btn),
                Align::Center,
            ));
        }
        let enqueue = self.button_rect(Button::Enqueue);
        let status_x = enqueue.x + enqueue.w + 6;
        let (_, x1) = self.right_x();
        texts.push((
            self.status_text(),
            colors.window_text,
            Rect::new(
                status_x,
                self.button_y(),
                (x1 - status_x).max(0),
                button::HEIGHT,
            ),
            Align::Left,
        ));

        // Text: skin-font strips are drawn now and scaled with the window; system-font
        // strips are drawn after scaling at full resolution.
        let s = scale as i32;
        let th = TEXT_H * scale;
        let mut late = Vec::new();
        for (text, color, clip, align) in texts {
            if text.is_empty() || clip.w <= 0 {
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
                track: Some(id as u32),
                duration: Some(Duration::from_secs(100 + id as u64)),
                genre: "Trance".into(),
                year: Some(2000 + id as u32),
                plays: (id % 4) as u32,
                size: 1_000_000,
            })
            .collect()
    }

    fn window() -> LibraryWindow {
        let mut w = LibraryWindow::new();
        w.set_filters(
            vec![
                ArtistRow {
                    name: "KEIKO".into(),
                    albums: 1,
                    tracks: 6,
                },
                ArtistRow {
                    name: "Ma15".into(),
                    albums: 2,
                    tracks: 4,
                },
            ],
            vec![AlbumRow {
                name: "Best 1".into(),
                year: Some(2006),
                tracks: 10,
            }],
        );
        w.set_tracks(rows(10));
        w
    }

    fn row_point(w: &LibraryWindow, pane: Pane, row: i32) -> Point {
        let r = w.pane_rects(pane).rows;
        Point::new(r.x + 20, r.y + row * ROW_H + 5)
    }

    fn tree_point(w: &LibraryWindow, row: i32) -> Point {
        let t = w.tree_rect();
        Point::new(t.x + 30, t.y + 1 + row * ROW_H + 5)
    }

    fn click_button(w: &mut LibraryWindow, b: Button) -> Vec<LibAction> {
        let r = w.button_rect(b);
        let p = Point::new(r.x + 3, r.y + 3);
        w.pointer_down(p, false, false, 0);
        w.pointer_up(p)
    }

    #[test]
    fn layout_fits_inside_the_frame_at_every_size() {
        for (width, height) in [(MIN_W, MIN_H), (MIN_W + 14 * STEP_W, MIN_H + 10 * STEP_H)] {
            let mut w = window();
            w.set_size(width, height);
            let c = w.client();
            let within = |r: Rect| {
                r.x >= c.x && r.y >= c.y && r.x + r.w <= c.x + c.w && r.y + r.h <= c.y + c.h
            };
            let mut rects = vec![w.tree_rect(), w.search_rect()];
            for b in [
                Button::Play,
                Button::Enqueue,
                Button::Library,
                Button::ClearSearch,
            ] {
                rects.push(w.button_rect(b));
            }
            for p in [Pane::Artists, Pane::Albums, Pane::Tracks] {
                rects.push(w.pane_rects(p).frame);
            }
            for r in rects {
                assert!(within(r), "{r:?} in {c:?} at {width}x{height}");
            }
        }
        // At the default size, panes do not overlap and the track list is the tallest.
        let w = window();
        let (a, b, t) = (
            w.pane_rects(Pane::Artists).frame,
            w.pane_rects(Pane::Albums).frame,
            w.pane_rects(Pane::Tracks).frame,
        );
        assert!(a.x + a.w <= b.x);
        assert!(a.y + a.h <= t.y);
        assert!(t.h > a.h);
    }

    #[test]
    fn columns_drop_from_the_right_when_narrow() {
        let mut w = window();
        let wide: Vec<Column> = w.columns().into_iter().map(|(c, ..)| c).collect();
        assert_eq!(wide.len(), 8);
        w.set_size(MIN_W + 4 * STEP_W, MIN_H);
        let narrow: Vec<Column> = w.columns().into_iter().map(|(c, ..)| c).collect();
        assert!(narrow.len() < 8);
        assert!(narrow.contains(&Column::Title));
        assert!(!narrow.contains(&Column::Plays));
        // Columns tile the list exactly.
        let r = w.pane_rects(Pane::Tracks).rows;
        let cols = w.columns();
        let last = cols.last().unwrap();
        assert_eq!(cols[0].1, r.x);
        assert!(last.1 + last.2 <= r.x + r.w);
    }

    #[test]
    fn tree_views_query_and_local_media_folds() {
        let mut w = window();
        // Rows: Local Media, Audio, Most Played, Recently Added, Recently Played, Never Played.
        let (_, a) = w.pointer_down(tree_point(&w, 2), false, false, 0);
        assert_eq!(
            a,
            vec![LibAction::Query {
                view: LibView::MostPlayed,
                text: String::new(),
                artist: None,
                album: None,
            }]
        );
        assert_eq!(w.view(), LibView::MostPlayed);
        let (_, a) = w.pointer_down(tree_point(&w, 2), false, false, 0);
        assert!(a.is_empty(), "same view again");
        w.pointer_down(tree_point(&w, 0), false, false, 0);
        assert_eq!(w.tree_items(), vec![TreeItem::LocalMedia]);
    }

    #[test]
    fn filter_panes_choose_artist_then_album() {
        let mut w = window();
        // Artist pane rows: All, KEIKO, Ma15.
        let (_, a) = w.pointer_down(row_point(&w, Pane::Artists, 2), false, false, 0);
        assert_eq!(
            a,
            vec![LibAction::Query {
                view: LibView::Audio,
                text: String::new(),
                artist: Some("Ma15".into()),
                album: None,
            }]
        );
        let (_, a) = w.pointer_down(row_point(&w, Pane::Albums, 1), false, false, 1000);
        assert_eq!(
            a,
            vec![LibAction::Query {
                view: LibView::Audio,
                text: String::new(),
                artist: Some("Ma15".into()),
                album: Some("Best 1".into()),
            }]
        );
        // Choosing another artist resets the album; "All" clears the artist.
        w.pointer_down(row_point(&w, Pane::Artists, 1), false, false, 2000);
        assert_eq!(
            w.query(),
            LibAction::Query {
                view: LibView::Audio,
                text: String::new(),
                artist: Some("KEIKO".into()),
                album: None,
            }
        );
        let (_, a) = w.pointer_down(row_point(&w, Pane::Artists, 0), false, false, 3000);
        assert!(matches!(&a[0], LibAction::Query { artist: None, .. }));
    }

    #[test]
    fn double_click_in_a_filter_plays_it() {
        let mut w = window();
        let p = row_point(&w, Pane::Artists, 1);
        w.pointer_down(p, false, false, 0);
        let (_, a) = w.pointer_down(p, false, false, 200);
        assert_eq!(a, vec![LibAction::PlayAll]);
        assert_eq!(
            w.play_all(),
            LibAction::Play {
                ids: (1..=10).collect(),
                start: 0
            }
        );
    }

    #[test]
    fn rows_select_and_double_click_plays() {
        let mut w = window();
        w.pointer_down(row_point(&w, Pane::Tracks, 1), false, false, 0);
        w.pointer_down(row_point(&w, Pane::Tracks, 3), false, true, 10);
        assert_eq!(w.selected_ids(), vec![2, 3, 4]);
        assert_eq!(
            click_button(&mut w, Button::Enqueue),
            vec![LibAction::Enqueue(vec![2, 3, 4])]
        );
        assert_eq!(
            click_button(&mut w, Button::Play),
            vec![LibAction::Play {
                ids: (1..=10).collect(),
                start: 1
            }]
        );
        let p = row_point(&w, Pane::Tracks, 5);
        w.pointer_down(p, false, false, 1000);
        let (_, a) = w.pointer_down(p, false, false, 1200);
        assert_eq!(
            a,
            vec![LibAction::Play {
                ids: (1..=10).collect(),
                start: 5
            }]
        );
    }

    #[test]
    fn header_click_sorts_and_toggles() {
        let mut w = window();
        let hdr = w.pane_rects(Pane::Tracks).header;
        let (_, plays_x, _) = *w
            .columns()
            .iter()
            .find(|(c, ..)| *c == Column::Plays)
            .unwrap();
        let p = Point::new(plays_x + 5, hdr.y + 4);
        w.pointer_down(p, false, false, 0);
        let plays: Vec<u32> = w.rows().iter().map(|r| r.plays).collect();
        assert!(plays.windows(2).all(|x| x[0] <= x[1]));
        w.pointer_down(p, false, false, 0);
        assert_eq!(w.rows()[0].plays, 3, "descending");
    }

    #[test]
    fn search_typing_and_clear_button() {
        let mut w = window();
        let a = w.key(u32::from('k'), false);
        assert!(matches!(&a[0], LibAction::Query { text, .. } if text == "k"));
        assert_eq!(
            click_button(&mut w, Button::ClearSearch),
            vec![LibAction::Query {
                view: LibView::Audio,
                text: String::new(),
                artist: None,
                album: None,
            }]
        );
        assert!(
            click_button(&mut w, Button::ClearSearch).is_empty(),
            "nothing to clear"
        );
        assert_eq!(
            click_button(&mut w, Button::Library),
            vec![LibAction::LibraryMenu]
        );
    }

    #[test]
    fn status_counts_length_and_size() {
        let mut w = window();
        assert_eq!(w.status_text(), "10 items [17:35] [10.00 MB]");
        w.set_tracks(rows(1));
        assert_eq!(w.status_text(), "1 item [1:41] [1.00 MB]");
        w.set_status("Scanning 3/9");
        assert_eq!(w.status_text(), "Scanning 3/9");
    }

    #[test]
    fn scrolling_each_pane() {
        let mut w = window();
        w.set_tracks(rows(200));
        let t = w.pane_rects(Pane::Tracks).rows;
        w.wheel(Point::new(t.x + 5, t.y + 5), false);
        assert_eq!(w.scroll[2], 3);
        let bar = w.pane_rects(Pane::Tracks).bar;
        w.pointer_down(Point::new(bar.x + 2, bar.y + bar.h - 2), false, false, 0);
        assert_eq!(w.scroll[2], w.max_scroll(Pane::Tracks));
        w.pointer_up(Point::new(bar.x + 2, bar.y + bar.h - 2));
        // Keys keep the selection in view.
        w.key(0xff54, false);
        assert!(w.scroll[2] <= 1);
    }

    #[test]
    fn render_size_and_hash() {
        let skin = coded_skin();
        let mut w = window();
        let b = w.render(&skin, &mut NoFallback, 2);
        assert_eq!((b.width() as i32, b.height() as i32), (w.w * 2, w.h * 2));
        let h = w.render_hash(1);
        w.pointer_down(row_point(&w, Pane::Tracks, 0), false, false, 0);
        assert_ne!(w.render_hash(1), h);
        let small = {
            w.set_size(MIN_W, MIN_H);
            w.render(&skin, &mut NoFallback, 1)
        };
        assert_eq!(small.width() as i32, MIN_W);
    }

    #[test]
    fn close_and_resize() {
        let mut w = window();
        let c = w.close_rect();
        let p = Point::new(c.x + 2, c.y + 2);
        w.pointer_down(p, false, false, 0);
        assert_eq!(w.pointer_up(p), vec![LibAction::Close]);
        let r = w.resize_rect();
        let start = Point::new(r.x + 5, r.y + 5);
        w.pointer_down(start, false, false, 0);
        let a = w.pointer_move(Point::new(start.x + 30, start.y + 40));
        assert_eq!(a, vec![LibAction::Resize(w.w, w.h)]);
        assert_eq!(w.size(), (MIN_W + 15 * STEP_W, MIN_H + 11 * STEP_H));
    }
}
