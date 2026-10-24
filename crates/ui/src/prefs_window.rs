//! The preferences window: a generic-window frame with a page list on the left (General,
//! Playback, Skins, Media Library) and that page's settings on the right, as check boxes,
//! option lists and buttons in GENEX colors.
//!
//! The window shows a [`PrefsState`] the app keeps current and reports clicks as
//! [`PrefsAction`]s; it changes no settings itself. Coordinates are unscaled.

use std::collections::HashMap;

use ampcraft_dock::Point;
use ampcraft_skin::generic::{self, GenExColors, button};
use ampcraft_skin::{Bitmap, Rect, Skin};

use crate::text::{TextRenderer, TextStrip};
use crate::vis::VisMode;

pub const WIDTH: i32 = 400;
pub const HEIGHT: i32 = 290;
const ROW_H: i32 = 14;
const TEXT_H: u32 = 10;
const PAGE_LIST_W: i32 = 96;
const CACHE_LIMIT: usize = 512;
/// Output devices listed at most (the rest are reachable with `--device`).
const MAX_DEVICES: usize = 8;

/// Settings pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    #[default]
    General,
    Playback,
    Skins,
    Library,
}

const PAGES: [(Page, &str); 4] = [
    (Page::General, "General"),
    (Page::Playback, "Playback"),
    (Page::Skins, "Skins"),
    (Page::Library, "Media Library"),
];

/// Everything the window shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PrefsState {
    pub always_on_top: bool,
    /// None: picked from the screen at startup.
    pub scale: Option<u8>,
    pub show_remaining: bool,
    pub repeat: bool,
    pub shuffle: bool,
    pub vis: VisMode,
    /// Output devices as (id, name); `device` is the chosen id (None: system default).
    pub devices: Vec<(String, String)>,
    pub device: Option<String>,
    /// Built-in theme names and the one in use.
    pub themes: Vec<String>,
    pub theme: Option<usize>,
    /// The System theme (the desktop's colors) is in use.
    pub system_theme: bool,
    /// A classic skin file in use, shown by name.
    pub skin_file: Option<String>,
    pub include_music: bool,
    pub music_dir: Option<String>,
    pub folders: Vec<String>,
    /// Library summary or scan progress.
    pub library_status: String,
}

/// What a click asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefsAction {
    SetAlwaysOnTop(bool),
    SetScale(Option<u8>),
    SetShowRemaining(bool),
    SetRepeat(bool),
    SetShuffle(bool),
    SetVis(VisMode),
    SetDevice(Option<String>),
    SetTheme(usize),
    /// Follow the desktop's colors.
    SystemTheme,
    LoadSkinFile,
    SetIncludeMusic(bool),
    AddFolder,
    RemoveFolder(usize),
    Rescan,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefsDown {
    Handled,
    DragWindow,
}

/// One line of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Heading(String),
    Note(String),
    Check {
        label: String,
        on: bool,
        action: PrefsAction,
    },
    Radio {
        label: String,
        on: bool,
        action: PrefsAction,
    },
    /// Buttons side by side.
    Buttons(Vec<(String, PrefsAction)>),
    /// A folder with a Remove button.
    Folder {
        path: String,
        remove: PrefsAction,
    },
}

/// Something the pointer can press.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Target {
    Close,
    Page(Page),
    /// Item `index` of the page, and the button within it (0 for rows).
    Item(usize, usize),
}

const BUTTON_W: i32 = 84;
const REMOVE_W: i32 = 52;

#[derive(Debug)]
pub struct PrefsWindow {
    pub focused: bool,
    page: Page,
    state: PrefsState,
    pressed: Option<(Target, bool)>,
    text_cache: HashMap<(String, u32, u32), TextStrip>,
    revision: u64,
}

impl Default for PrefsWindow {
    fn default() -> Self {
        Self::new()
    }
}

fn inside(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
}

impl PrefsWindow {
    pub fn new() -> Self {
        Self {
            focused: false,
            page: Page::General,
            state: PrefsState::default(),
            pressed: None,
            text_cache: HashMap::new(),
            revision: 0,
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (WIDTH, HEIGHT)
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
    }

    pub fn state(&self) -> &PrefsState {
        &self.state
    }

    /// Shows `state`; cheap when nothing changed.
    pub fn set_state(&mut self, state: PrefsState) {
        if state != self.state {
            self.state = state;
            self.revision += 1;
        }
    }

    pub fn skin_changed(&mut self) {
        self.revision += 1;
        self.text_cache.clear();
    }

    pub fn is_interacting(&self) -> bool {
        self.pressed.is_some()
    }

    /// Changes whenever the rendered image would change.
    pub fn render_hash(&self, scale: u32) -> u64 {
        crate::render::hash_of(&(scale, self.focused, self.page, self.revision, &self.pressed))
    }

    // ---- layout ----

    fn client(&self) -> Rect {
        Rect::new(
            generic::LEFT_W,
            generic::TOP_H,
            WIDTH - generic::LEFT_W - generic::RIGHT_W,
            HEIGHT - generic::TOP_H - generic::BOTTOM_H,
        )
    }

    fn page_list_rect(&self) -> Rect {
        let c = self.client();
        Rect::new(c.x + 2, c.y + 2, PAGE_LIST_W, c.h - 4)
    }

    fn page_rect(&self) -> Rect {
        let c = self.client();
        let x = c.x + 2 + PAGE_LIST_W + 4;
        Rect::new(x, c.y + 2, c.x + c.w - 2 - x, c.h - 4)
    }

    fn page_row(&self, i: usize) -> Rect {
        let l = self.page_list_rect();
        Rect::new(l.x + 1, l.y + 1 + i as i32 * ROW_H, l.w - 2, ROW_H)
    }

    fn items(&self) -> Vec<Item> {
        let s = &self.state;
        let check = |label: &str, on: bool, action: PrefsAction| Item::Check {
            label: label.into(),
            on,
            action,
        };
        let radio = |label: &str, on: bool, action: PrefsAction| Item::Radio {
            label: label.into(),
            on,
            action,
        };
        let heading = |t: &str| Item::Heading(t.into());
        match self.page {
            Page::General => vec![
                heading("Windows"),
                check(
                    "Keep the player above other windows",
                    s.always_on_top,
                    PrefsAction::SetAlwaysOnTop(!s.always_on_top),
                ),
                heading("Size"),
                radio(
                    "Automatic (fits the screen)",
                    s.scale.is_none(),
                    PrefsAction::SetScale(None),
                ),
                radio(
                    "Normal (1x)",
                    s.scale == Some(1),
                    PrefsAction::SetScale(Some(1)),
                ),
                radio(
                    "Double (2x)",
                    s.scale == Some(2),
                    PrefsAction::SetScale(Some(2)),
                ),
                radio(
                    "Triple (3x)",
                    s.scale == Some(3),
                    PrefsAction::SetScale(Some(3)),
                ),
                heading("Time display"),
                radio(
                    "Time elapsed",
                    !s.show_remaining,
                    PrefsAction::SetShowRemaining(false),
                ),
                radio(
                    "Time remaining",
                    s.show_remaining,
                    PrefsAction::SetShowRemaining(true),
                ),
            ],
            Page::Playback => {
                let mut items = vec![
                    heading("Playlist"),
                    check("Repeat", s.repeat, PrefsAction::SetRepeat(!s.repeat)),
                    check("Shuffle", s.shuffle, PrefsAction::SetShuffle(!s.shuffle)),
                    heading("Visualizer"),
                ];
                for (label, mode) in [
                    ("Spectrum analyzer", VisMode::Spectrum),
                    ("Oscilloscope", VisMode::Oscilloscope),
                    ("Off", VisMode::Off),
                ] {
                    items.push(radio(label, s.vis == mode, PrefsAction::SetVis(mode)));
                }
                items.push(heading("Output device (used from the next start)"));
                items.push(radio(
                    "System default",
                    s.device.is_none(),
                    PrefsAction::SetDevice(None),
                ));
                for (id, name) in s.devices.iter().take(MAX_DEVICES) {
                    items.push(radio(
                        name,
                        s.device.as_deref() == Some(id.as_str()),
                        PrefsAction::SetDevice(Some(id.clone())),
                    ));
                }
                items
            }
            Page::Skins => {
                let mut items = vec![
                    heading("Built-in themes"),
                    radio(
                        "System colors (follow the desktop)",
                        s.system_theme,
                        PrefsAction::SystemTheme,
                    ),
                ];
                for (i, name) in s.themes.iter().enumerate() {
                    items.push(radio(name, s.theme == Some(i), PrefsAction::SetTheme(i)));
                }
                items.push(heading("Classic skins (.wsz)"));
                if let Some(file) = &s.skin_file {
                    items.push(Item::Note(format!("In use: {file}")));
                }
                items.push(Item::Buttons(vec![(
                    "Load skin...".into(),
                    PrefsAction::LoadSkinFile,
                )]));
                items
            }
            Page::Library => {
                let music = match &s.music_dir {
                    Some(d) => format!("Include the Music folder ({d})"),
                    None => "Include the Music folder".into(),
                };
                let mut items = vec![
                    heading("Folders"),
                    check(
                        &music,
                        s.include_music,
                        PrefsAction::SetIncludeMusic(!s.include_music),
                    ),
                ];
                for (i, f) in s.folders.iter().enumerate() {
                    items.push(Item::Folder {
                        path: f.clone(),
                        remove: PrefsAction::RemoveFolder(i),
                    });
                }
                items.push(Item::Buttons(vec![
                    ("Add folder...".into(), PrefsAction::AddFolder),
                    ("Rescan now".into(), PrefsAction::Rescan),
                ]));
                if !s.library_status.is_empty() {
                    items.push(Item::Note(s.library_status.clone()));
                }
                items
            }
        }
    }

    /// Each item's rect on the page (rows are one line; button rows a little taller).
    fn item_rects(&self, items: &[Item]) -> Vec<Rect> {
        let page = self.page_rect();
        let mut y = page.y + 2;
        items
            .iter()
            .map(|item| {
                let h = match item {
                    Item::Heading(_) => {
                        y += if y > page.y + 2 { 4 } else { 0 };
                        ROW_H
                    }
                    Item::Buttons(_) => button::HEIGHT + 4,
                    Item::Folder { .. } => button::HEIGHT + 1,
                    _ => ROW_H,
                };
                let r = Rect::new(page.x + 4, y, page.w - 8, h);
                y += h;
                r
            })
            .collect()
    }

    /// Button rects within a button row or folder row.
    fn buttons_in(&self, item: &Item, r: Rect) -> Vec<(Rect, PrefsAction)> {
        match item {
            Item::Buttons(list) => list
                .iter()
                .enumerate()
                .map(|(i, (_, a))| {
                    (
                        Rect::new(
                            r.x + 12 + i as i32 * (BUTTON_W + 4),
                            r.y + 2,
                            BUTTON_W,
                            button::HEIGHT,
                        ),
                        a.clone(),
                    )
                })
                .collect(),
            Item::Folder { remove, .. } => vec![(
                Rect::new(r.x + r.w - REMOVE_W, r.y, REMOVE_W, button::HEIGHT),
                remove.clone(),
            )],
            _ => Vec::new(),
        }
    }

    /// What is under `p`, and the action pressing it gives.
    fn target_at(&self, p: Point) -> Option<(Target, Option<PrefsAction>)> {
        if inside(crate::gen_frame::close_rect(WIDTH), p) {
            return Some((Target::Close, Some(PrefsAction::Close)));
        }
        for (i, (page, _)) in PAGES.iter().enumerate() {
            if inside(self.page_row(i), p) {
                return Some((Target::Page(*page), None));
            }
        }
        let items = self.items();
        for (i, (item, r)) in items.iter().zip(self.item_rects(&items)).enumerate() {
            match item {
                Item::Check { action, .. } | Item::Radio { action, .. } if inside(r, p) => {
                    return Some((Target::Item(i, 0), Some(action.clone())));
                }
                Item::Buttons(_) | Item::Folder { .. } => {
                    for (j, (br, a)) in self.buttons_in(item, r).into_iter().enumerate() {
                        if inside(br, p) {
                            return Some((Target::Item(i, j), Some(a)));
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    // ---- input ----

    pub fn pointer_down(&mut self, p: Point) -> PrefsDown {
        if let Some((target, _)) = self.target_at(p) {
            if let Target::Page(page) = target {
                self.page = page;
                return PrefsDown::Handled;
            }
            self.pressed = Some((target, true));
            return PrefsDown::Handled;
        }
        if p.y < generic::TOP_H {
            PrefsDown::DragWindow
        } else {
            PrefsDown::Handled
        }
    }

    pub fn pointer_move(&mut self, p: Point) {
        if let Some((target, _)) = self.pressed.clone() {
            let over = self.target_at(p).is_some_and(|(t, _)| t == target);
            self.pressed = Some((target, over));
        }
    }

    pub fn pointer_up(&mut self, p: Point) -> Vec<PrefsAction> {
        let Some((target, _)) = self.pressed.take() else {
            return Vec::new();
        };
        match self.target_at(p) {
            Some((t, Some(action))) if t == target => vec![action],
            _ => Vec::new(),
        }
    }

    /// Escape closes; Tab moves to the next page.
    pub fn key(&mut self, keysym: u32) -> Vec<PrefsAction> {
        const ESCAPE: u32 = 0xff1b;
        const TAB: u32 = 0xff09;
        match keysym {
            ESCAPE => vec![PrefsAction::Close],
            TAB => {
                let i = PAGES.iter().position(|(p, _)| *p == self.page).unwrap_or(0);
                self.page = PAGES[(i + 1) % PAGES.len()].0;
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

    fn is_pressed(&self, t: &Target) -> bool {
        matches!(&self.pressed, Some((p, true)) if p == t)
    }

    pub fn render(&mut self, skin: &Skin, fallback: &mut dyn TextRenderer, scale: u32) -> Bitmap {
        let scale = scale.max(1);
        let colors = GenExColors::from_skin(skin);
        let mut b = Bitmap::new(WIDTH as u32, HEIGHT as u32, colors.window_background);
        crate::gen_frame::draw_frame(
            &mut b,
            skin,
            (WIDTH, HEIGHT),
            "PREFERENCES",
            self.focused,
            self.is_pressed(&Target::Close),
        );
        b.fill_rect(self.client(), colors.window_background);
        let mut texts: Vec<(String, u32, Rect, bool)> = Vec::new();

        // Page list.
        let list = self.page_list_rect();
        b.fill_rect(list, colors.divider);
        b.fill_rect(
            Rect::new(list.x + 1, list.y + 1, list.w - 2, list.h - 2),
            colors.item_background,
        );
        for (i, (page, name)) in PAGES.iter().enumerate() {
            let r = self.page_row(i);
            let color = if *page == self.page {
                b.fill_rect(r, colors.list_text_selected_background);
                colors.list_text_selected
            } else {
                colors.item_foreground
            };
            texts.push((
                (*name).into(),
                color,
                Rect::new(r.x + 4, r.y, r.w - 6, r.h),
                false,
            ));
        }

        // Page frame and items.
        let page = self.page_rect();
        b.fill_rect(page, colors.divider);
        b.fill_rect(
            Rect::new(page.x + 1, page.y + 1, page.w - 2, page.h - 2),
            colors.item_background,
        );
        let items = self.items();
        let rects = self.item_rects(&items);
        let bottom = page.y + page.h - 2;
        for (i, (item, r)) in items.iter().zip(&rects).enumerate() {
            if r.y + r.h > bottom {
                break;
            }
            let fg = colors.item_foreground;
            match item {
                Item::Heading(t) => {
                    texts.push((t.clone(), colors.list_text_selected, *r, false));
                    b.fill_rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), colors.divider);
                }
                Item::Note(t) => texts.push((
                    t.clone(),
                    fg,
                    Rect::new(r.x + 12, r.y, r.w - 12, r.h),
                    false,
                )),
                Item::Check { label, on, .. } | Item::Radio { label, on, .. } => {
                    let pressed = self.is_pressed(&Target::Item(i, 0));
                    let bx = Rect::new(r.x + 12, r.y + 2, 9, 9);
                    let inner = if pressed {
                        colors.list_text_selected_background
                    } else {
                        colors.item_background
                    };
                    if matches!(item, Item::Radio { .. }) {
                        // A round option button: '#' outline, 'o' inside.
                        for (dy, row) in RADIO.iter().enumerate() {
                            for (dx, ch) in row.chars().enumerate() {
                                let color = match ch {
                                    '#' => fg,
                                    'o' => inner,
                                    _ => continue,
                                };
                                b.set(bx.x + dx as i32, bx.y + dy as i32, color);
                            }
                        }
                    } else {
                        b.fill_rect(bx, fg);
                        b.fill_rect(Rect::new(bx.x + 1, bx.y + 1, 7, 7), inner);
                    }
                    if *on {
                        match item {
                            Item::Check { .. } => {
                                // A tick.
                                for (dx, dy) in [(2, 4), (3, 5), (4, 6), (5, 5), (6, 4), (7, 3)] {
                                    b.fill_rect(Rect::new(bx.x + dx, bx.y + dy - 1, 1, 2), fg);
                                }
                            }
                            _ => {
                                b.fill_rect(Rect::new(bx.x + 3, bx.y + 2, 3, 5), fg);
                                b.fill_rect(Rect::new(bx.x + 2, bx.y + 3, 5, 3), fg);
                            }
                        }
                    }
                    texts.push((
                        label.clone(),
                        fg,
                        Rect::new(r.x + 26, r.y, r.w - 26, r.h),
                        false,
                    ));
                }
                Item::Buttons(list) => {
                    for (j, ((label, _), (br, _))) in
                        list.iter().zip(self.buttons_in(item, *r)).enumerate()
                    {
                        let down = self.is_pressed(&Target::Item(i, j));
                        crate::gen_frame::draw_button(&mut b, skin, br, down);
                        texts.push((label.clone(), colors.button_text, br, true));
                    }
                }
                Item::Folder { path, .. } => {
                    let buttons = self.buttons_in(item, *r);
                    let text_w = buttons.first().map_or(r.w, |(br, _)| br.x - r.x - 4);
                    texts.push((
                        path.clone(),
                        fg,
                        Rect::new(r.x + 12, r.y, text_w - 12, r.h),
                        false,
                    ));
                    for (j, (br, _)) in buttons.into_iter().enumerate() {
                        let down = self.is_pressed(&Target::Item(i, j));
                        crate::gen_frame::draw_button(&mut b, skin, br, down);
                        texts.push(("Remove".into(), colors.button_text, br, true));
                    }
                }
            }
        }

        // Text: skin-font strips now (scaled with the window), system-font ones after.
        let s = scale as i32;
        let th = TEXT_H * scale;
        let mut late = Vec::new();
        for (text, color, clip, center) in texts {
            if text.is_empty() || clip.w <= 0 {
                continue;
            }
            let strip = self.strip(skin, fallback, &text, color, th);
            if strip.is_bitmap() {
                draw_text(&mut b, &strip, clip, center);
            } else {
                late.push((
                    strip,
                    Rect::new(clip.x * s, clip.y * s, clip.w * s, clip.h * s),
                    center,
                ));
            }
        }
        let mut out = crate::render::scale_nearest(&b, scale);
        for (strip, clip, center) in late {
            draw_text(&mut out, &strip, clip, center);
        }
        out
    }
}

/// A 9 px round option button.
const RADIO: [&str; 9] = [
    "..#####..",
    ".#ooooo#.",
    "#ooooooo#",
    "#ooooooo#",
    "#ooooooo#",
    "#ooooooo#",
    "#ooooooo#",
    ".#ooooo#.",
    "..#####..",
];

/// Draws a strip vertically centered in `clip`, at its left or centered.
fn draw_text(dst: &mut Bitmap, strip: &TextStrip, clip: Rect, center: bool) {
    let x = if center {
        clip.x + (clip.w - strip.width()).max(0) / 2
    } else {
        clip.x
    };
    let y = clip.y + (clip.h - strip.height()) / 2;
    strip.draw(
        dst,
        Rect::new(x, clip.y, (clip.x + clip.w - x).max(0), clip.h),
        y,
        0,
        false,
        0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::coded_skin;
    use crate::text::NoFallback;

    fn state() -> PrefsState {
        PrefsState {
            scale: Some(2),
            vis: VisMode::Spectrum,
            devices: vec![
                ("pw".into(), "PipeWire".into()),
                ("hw0".into(), "HDA".into()),
            ],
            device: Some("hw0".into()),
            themes: vec!["Dracula".into(), "Nord".into()],
            theme: Some(1),
            system_theme: false,
            include_music: true,
            music_dir: Some("/home/u/Music".into()),
            folders: vec!["/mnt/a".into(), "/mnt/b".into()],
            ..PrefsState::default()
        }
    }

    fn window(page: Page) -> PrefsWindow {
        let mut w = PrefsWindow::new();
        w.set_state(state());
        w.set_page(page);
        w
    }

    /// Clicks the item whose label is `label` (or a button labeled so) and returns the
    /// actions.
    fn click(w: &mut PrefsWindow, label: &str) -> Vec<PrefsAction> {
        let items = w.items();
        let rects = w.item_rects(&items);
        for (item, r) in items.iter().zip(&rects) {
            let hit = match item {
                Item::Check { label: l, .. } | Item::Radio { label: l, .. } if l == label => {
                    Some(Point::new(r.x + 14, r.y + 5))
                }
                Item::Buttons(list) => list
                    .iter()
                    .zip(w.buttons_in(item, *r))
                    .find(|((l, _), _)| l == label)
                    .map(|(_, (br, _))| Point::new(br.x + 3, br.y + 3)),
                Item::Folder { path, .. } if path == label => {
                    let (br, _) = w.buttons_in(item, *r).remove(0);
                    Some(Point::new(br.x + 3, br.y + 3))
                }
                _ => None,
            };
            if let Some(p) = hit {
                w.pointer_down(p);
                return w.pointer_up(p);
            }
        }
        panic!("no {label}");
    }

    #[test]
    fn general_page_actions() {
        let mut w = window(Page::General);
        assert_eq!(
            click(&mut w, "Keep the player above other windows"),
            vec![PrefsAction::SetAlwaysOnTop(true)]
        );
        assert_eq!(
            click(&mut w, "Automatic (fits the screen)"),
            vec![PrefsAction::SetScale(None)]
        );
        assert_eq!(
            click(&mut w, "Triple (3x)"),
            vec![PrefsAction::SetScale(Some(3))]
        );
        assert_eq!(
            click(&mut w, "Time remaining"),
            vec![PrefsAction::SetShowRemaining(true)]
        );
    }

    #[test]
    fn playback_page_lists_devices() {
        let mut w = window(Page::Playback);
        assert_eq!(
            click(&mut w, "Shuffle"),
            vec![PrefsAction::SetShuffle(true)]
        );
        assert_eq!(
            click(&mut w, "Oscilloscope"),
            vec![PrefsAction::SetVis(VisMode::Oscilloscope)]
        );
        assert_eq!(
            click(&mut w, "PipeWire"),
            vec![PrefsAction::SetDevice(Some("pw".into()))]
        );
        assert_eq!(
            click(&mut w, "System default"),
            vec![PrefsAction::SetDevice(None)]
        );
        let on: Vec<String> = w
            .items()
            .into_iter()
            .filter_map(|i| match i {
                Item::Radio {
                    label, on: true, ..
                } => Some(label),
                _ => None,
            })
            .collect();
        assert_eq!(on, ["Spectrum analyzer", "HDA"]);
    }

    #[test]
    fn skins_and_library_pages() {
        let mut w = window(Page::Skins);
        assert_eq!(click(&mut w, "Dracula"), vec![PrefsAction::SetTheme(0)]);
        assert_eq!(
            click(&mut w, "Load skin..."),
            vec![PrefsAction::LoadSkinFile]
        );
        let mut w = window(Page::Library);
        assert_eq!(
            click(&mut w, "Include the Music folder (/home/u/Music)"),
            vec![PrefsAction::SetIncludeMusic(false)]
        );
        assert_eq!(click(&mut w, "/mnt/b"), vec![PrefsAction::RemoveFolder(1)]);
        assert_eq!(click(&mut w, "Add folder..."), vec![PrefsAction::AddFolder]);
        assert_eq!(click(&mut w, "Rescan now"), vec![PrefsAction::Rescan]);
    }

    #[test]
    fn pages_switch_by_click_and_tab_and_items_fit() {
        let mut w = window(Page::General);
        let r = w.page_row(3);
        w.pointer_down(Point::new(r.x + 5, r.y + 5));
        assert_eq!(w.page(), Page::Library);
        w.key(0xff09);
        assert_eq!(w.page(), Page::General);
        assert_eq!(w.key(0xff1b), vec![PrefsAction::Close]);
        // Every page fits, even with the most devices listed.
        let mut s = state();
        s.devices = (0..20)
            .map(|i| (format!("d{i}"), format!("Device {i}")))
            .collect();
        w.set_state(s);
        let page = w.page_rect();
        for (p, _) in PAGES {
            w.set_page(p);
            let items = w.items();
            let last = *w.item_rects(&items).last().unwrap();
            assert!(last.y + last.h <= page.y + page.h - 2, "{p:?} overflows");
        }
    }

    #[test]
    fn release_elsewhere_cancels_and_title_drags() {
        let mut w = window(Page::General);
        let items = w.items();
        let r = w.item_rects(&items)[1];
        w.pointer_down(Point::new(r.x + 14, r.y + 5));
        assert!(w.is_interacting());
        w.pointer_move(Point::new(5, 5));
        assert!(w.pointer_up(Point::new(5, 200)).is_empty());
        assert_eq!(w.pointer_down(Point::new(150, 8)), PrefsDown::DragWindow);
        let c = crate::gen_frame::close_rect(WIDTH);
        let p = Point::new(c.x + 2, c.y + 2);
        w.pointer_down(p);
        assert_eq!(w.pointer_up(p), vec![PrefsAction::Close]);
    }

    #[test]
    fn render_and_hash() {
        let skin = coded_skin();
        let mut w = window(Page::Library);
        let b = w.render(&skin, &mut NoFallback, 2);
        assert_eq!(
            (b.width(), b.height()),
            (2 * WIDTH as u32, 2 * HEIGHT as u32)
        );
        let h = w.render_hash(2);
        w.set_state(state());
        assert_eq!(w.render_hash(2), h, "same state, same hash");
        let mut s = state();
        s.repeat = true;
        w.set_state(s);
        assert_ne!(w.render_hash(2), h);
    }
}
