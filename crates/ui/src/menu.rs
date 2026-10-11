//! Popup menus: the options (O) and visualization (V) menus and the main window's right-click
//! menu. A menu is a stack of levels (the menu and its open submenus), each shown in its own
//! popup window. Everything here is in desktop pixels; the app forwards pointer positions in
//! desktop coordinates and shows each level's bitmap at its rect.

use amp_skin::{Bitmap, Skin};

use crate::text::{TextRenderer, TextStrip};
use amp_dock::{Point, Rect as Area};
use amp_skin::Rect;

/// Keysyms the menu understands (X11 values, the app's portable key codes).
mod keys {
    pub const ESCAPE: u32 = 0xff1b;
    pub const RETURN: u32 = 0xff0d;
    pub const SPACE: u32 = 0x0020;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
}

/// The tick or dot in front of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Check(bool),
    Radio(bool),
}

/// One line of a menu.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry<C> {
    Item(MenuItem<C>),
    Separator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem<C> {
    pub label: String,
    /// Key hint shown on the right ("Ctrl+A").
    pub shortcut: Option<String>,
    pub mark: Mark,
    pub enabled: bool,
    /// What choosing the item does. Items with a submenu have none.
    pub command: Option<C>,
    pub submenu: Vec<MenuEntry<C>>,
}

impl<C> MenuEntry<C> {
    pub fn item(label: impl Into<String>, command: C) -> Self {
        Self::Item(MenuItem {
            label: label.into(),
            shortcut: None,
            mark: Mark::None,
            enabled: true,
            command: Some(command),
            submenu: Vec::new(),
        })
    }

    pub fn submenu(label: impl Into<String>, entries: Vec<MenuEntry<C>>) -> Self {
        Self::Item(MenuItem {
            label: label.into(),
            shortcut: None,
            mark: Mark::None,
            enabled: !entries.is_empty(),
            command: None,
            submenu: entries,
        })
    }

    fn map_item(mut self, f: impl FnOnce(&mut MenuItem<C>)) -> Self {
        if let Self::Item(item) = &mut self {
            f(item);
        }
        self
    }

    pub fn shortcut(self, keys: impl Into<String>) -> Self {
        self.map_item(|i| i.shortcut = Some(keys.into()))
    }

    pub fn checked(self, on: bool) -> Self {
        self.map_item(|i| i.mark = Mark::Check(on))
    }

    pub fn radio(self, on: bool) -> Self {
        self.map_item(|i| i.mark = Mark::Radio(on))
    }

    pub fn enabled(self, enabled: bool) -> Self {
        self.map_item(|i| i.enabled = enabled)
    }
}

/// Menu colors, taken from the skin's playlist colors so menus match the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuColors {
    pub bg: u32,
    pub text: u32,
    pub hover_bg: u32,
    pub hover_text: u32,
    pub disabled: u32,
    pub border: u32,
}

impl MenuColors {
    pub fn from_skin(skin: &Skin) -> Self {
        let style = skin.playlist_style();
        Self {
            bg: style.normal_bg | 0xFF00_0000,
            text: style.normal | 0xFF00_0000,
            hover_bg: style.selected_bg | 0xFF00_0000,
            hover_text: style.current | 0xFF00_0000,
            disabled: mix(style.normal, style.normal_bg) | 0xFF00_0000,
            border: style.normal | 0xFF00_0000,
        }
    }
}

/// Halfway between two colors.
fn mix(a: u32, b: u32) -> u32 {
    let ch = |s: u32| ((((a >> s) & 0xFF) + ((b >> s) & 0xFF)) / 2) << s;
    ch(16) | ch(8) | ch(0)
}

/// What an input did to the menu.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEvent<C> {
    /// Nothing the app needs to act on (the menu may still need redrawing).
    None,
    /// An item was chosen; the menu is closed.
    Chosen(C),
    /// The menu was dismissed.
    Close,
}

/// Sizes in unscaled pixels; multiplied by the UI scale.
const BORDER: i32 = 1;
const ROW_H: i32 = 16;
const TEXT_H: i32 = 12;
const SEPARATOR_H: i32 = 7;
const MARK_W: i32 = 16;
const PAD_R: i32 = 8;
const SHORTCUT_GAP: i32 = 20;
const ARROW_W: i32 = 12;
const MIN_W: i32 = 90;

#[derive(Debug)]
struct Row {
    /// Top relative to the level, and height.
    y: i32,
    h: i32,
    label: Option<[TextStrip; 3]>,
    shortcut: Option<[TextStrip; 3]>,
}

#[derive(Debug)]
struct Level<C> {
    entries: Vec<MenuEntry<C>>,
    rows: Vec<Row>,
    rect: Area,
    hover: Option<usize>,
    /// Which item's submenu is open as the next level.
    open_child: Option<usize>,
}

/// An open menu with its submenus.
#[derive(Debug)]
pub struct MenuStack<C> {
    levels: Vec<Level<C>>,
    scale: i32,
    screen: Area,
    colors: MenuColors,
}

impl<C: Clone> MenuStack<C> {
    /// Opens `entries` with its top-left corner at `at` (desktop pixels), kept inside `screen`.
    pub fn open(
        entries: Vec<MenuEntry<C>>,
        at: Point,
        screen: Area,
        skin: &Skin,
        text: &mut dyn TextRenderer,
        scale: u32,
    ) -> Self {
        let mut stack = Self {
            levels: Vec::new(),
            scale: scale.max(1) as i32,
            screen,
            colors: MenuColors::from_skin(skin),
        };
        let level = stack.build(entries, skin, text);
        let rect = stack.place(level.1, at, None);
        stack.levels.push(Level { rect, ..level.0 });
        stack
    }

    /// Number of open levels (1 plus open submenus).
    pub fn depth(&self) -> usize {
        self.levels.len()
    }

    /// Where level `i` is on the desktop.
    pub fn rect(&self, i: usize) -> Option<Area> {
        self.levels.get(i).map(|l| l.rect)
    }

    fn build(
        &self,
        entries: Vec<MenuEntry<C>>,
        skin: &Skin,
        text: &mut dyn TextRenderer,
    ) -> (Level<C>, (i32, i32)) {
        let u = self.scale;
        let c = self.colors;
        let strips = |text: &mut dyn TextRenderer, s: &str| {
            [c.text, c.hover_text, c.disabled]
                .map(|color| TextStrip::colored(skin, text, s, (TEXT_H * u) as u32, color))
        };
        let mut rows = Vec::with_capacity(entries.len());
        let mut y = BORDER * u;
        let (mut label_w, mut shortcut_w, mut arrow) = (0, 0, false);
        for e in &entries {
            match e {
                MenuEntry::Separator => {
                    rows.push(Row {
                        y,
                        h: SEPARATOR_H * u,
                        label: None,
                        shortcut: None,
                    });
                    y += SEPARATOR_H * u;
                }
                MenuEntry::Item(item) => {
                    let label = strips(text, &item.label);
                    let shortcut = item.shortcut.as_deref().map(|s| strips(text, s));
                    label_w = label_w.max(self.strip_width(&label[0]));
                    if let Some(s) = &shortcut {
                        shortcut_w = shortcut_w.max(self.strip_width(&s[0]));
                    }
                    arrow |= !item.submenu.is_empty();
                    rows.push(Row {
                        y,
                        h: ROW_H * u,
                        label: Some(label),
                        shortcut,
                    });
                    y += ROW_H * u;
                }
            }
        }
        let mut w = MARK_W * u + label_w + PAD_R * u;
        if shortcut_w > 0 {
            w += SHORTCUT_GAP * u + shortcut_w;
        }
        if arrow {
            w += ARROW_W * u;
        }
        let w = w.max(MIN_W * u) + 2 * BORDER * u;
        let h = y + BORDER * u;
        let level = Level {
            entries,
            rows,
            rect: Area::new(0, 0, w, h),
            hover: None,
            open_child: None,
        };
        (level, (w, h))
    }

    /// Width of a strip once drawn (bitmap-font strips are scaled up).
    fn strip_width(&self, s: &TextStrip) -> i32 {
        if s.is_bitmap() {
            s.width() * self.scale
        } else {
            s.width()
        }
    }

    /// A rect of `size` at `at`, moved to stay on screen. Submenus that do not fit on the
    /// right of their parent (`beside`) open on its left instead.
    fn place(&self, (w, h): (i32, i32), at: Point, beside: Option<Area>) -> Area {
        let s = self.screen;
        let mut x = at.x;
        if x + w > s.right() {
            x = match beside {
                Some(parent) => parent.x - w + BORDER * self.scale,
                None => s.right() - w,
            };
        }
        let x = x.max(s.x);
        let y = at.y.min(s.bottom() - h).max(s.y);
        Area::new(x, y, w, h)
    }

    /// The level and row under a desktop point.
    fn hit(&self, p: Point) -> Option<(usize, Option<usize>)> {
        let (li, level) = self
            .levels
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| l.rect.contains(p))?;
        let y = p.y - level.rect.y;
        let row = level
            .rows
            .iter()
            .position(|r| y >= r.y && y < r.y + r.h)
            .filter(|&i| selectable(&level.entries[i]));
        Some((li, row))
    }

    /// Closes levels deeper than `depth`.
    fn truncate(&mut self, depth: usize) {
        self.levels.truncate(depth.max(1));
        if let Some(last) = self.levels.last_mut() {
            last.open_child = None;
        }
    }

    /// Opens the submenu of the hovered item in level `li`, if it has one.
    fn open_submenu(&mut self, li: usize, skin: &Skin, text: &mut dyn TextRenderer) {
        let Some(level) = self.levels.get(li) else {
            return;
        };
        let Some(i) = level.hover else { return };
        if level.open_child == Some(i) && self.levels.len() > li + 1 {
            return;
        }
        let MenuEntry::Item(item) = &level.entries[i] else {
            return;
        };
        if item.submenu.is_empty() || !item.enabled {
            return;
        }
        let entries = item.submenu.clone();
        let parent = level.rect;
        let row_y = parent.y + level.rows[i].y - BORDER * self.scale;
        self.truncate(li + 1);
        let (child, size) = self.build(entries, skin, text);
        let at = Point::new(parent.right() - BORDER * self.scale, row_y);
        let rect = self.place(size, at, Some(parent));
        self.levels[li].open_child = Some(i);
        self.levels.push(Level { rect, ..child });
    }

    /// Pointer moved to `p` (desktop pixels). Returns whether anything changed.
    pub fn pointer_move(&mut self, p: Point, skin: &Skin, text: &mut dyn TextRenderer) -> bool {
        let before = self.snapshot();
        match self.hit(p) {
            Some((li, row)) => {
                self.levels[li].hover = row;
                match row {
                    Some(_) if self.levels[li].open_child == row => {}
                    Some(_) => {
                        self.truncate(li + 1);
                        self.open_submenu(li, skin, text);
                    }
                    // Over a separator or border: keep an open submenu.
                    None => {}
                }
            }
            None => {
                // Off the menus: the deepest level loses its highlight unless it has a child.
                if let Some(last) = self.levels.last_mut() {
                    last.hover = None;
                }
            }
        }
        before != self.snapshot()
    }

    /// A button was pressed at `p`. Presses outside every level close the menu.
    pub fn press(&mut self, p: Point) -> MenuEvent<C> {
        if self.hit(p).is_some() {
            MenuEvent::None
        } else {
            MenuEvent::Close
        }
    }

    /// A button was released at `p`: chooses the item there.
    pub fn release(&mut self, p: Point) -> MenuEvent<C> {
        match self.hit(p) {
            Some((li, Some(row))) => self.choose(li, row),
            _ => MenuEvent::None,
        }
    }

    fn choose(&mut self, li: usize, row: usize) -> MenuEvent<C> {
        match &self.levels[li].entries[row] {
            MenuEntry::Item(MenuItem {
                enabled: true,
                command: Some(c),
                ..
            }) => MenuEvent::Chosen(c.clone()),
            _ => MenuEvent::None,
        }
    }

    /// Keyboard: arrows move and open or close submenus, Return chooses, Escape closes.
    pub fn key(&mut self, keysym: u32, skin: &Skin, text: &mut dyn TextRenderer) -> MenuEvent<C> {
        let li = self.levels.len() - 1;
        match keysym {
            keys::ESCAPE | keys::LEFT if li > 0 => {
                self.truncate(li);
                MenuEvent::None
            }
            keys::ESCAPE => MenuEvent::Close,
            keys::UP | keys::DOWN => {
                let level = &mut self.levels[li];
                let n = level.entries.len();
                let start = level.hover;
                let mut i = start;
                for _ in 0..n {
                    i = Some(match (i, keysym == keys::DOWN) {
                        (None, true) => 0,
                        (None, false) => n - 1,
                        (Some(i), true) => (i + 1) % n,
                        (Some(i), false) => (i + n - 1) % n,
                    });
                    if i.is_some_and(|i| selectable(&level.entries[i])) {
                        break;
                    }
                }
                if i.is_some_and(|i| selectable(&level.entries[i])) {
                    level.hover = i;
                }
                MenuEvent::None
            }
            keys::RIGHT | keys::RETURN | keys::SPACE => {
                let Some(row) = self.levels[li].hover else {
                    return MenuEvent::None;
                };
                let has_sub = matches!(&self.levels[li].entries[row],
                    MenuEntry::Item(item) if !item.submenu.is_empty());
                if has_sub {
                    self.open_submenu(li, skin, text);
                    if let Some(child) = self.levels.get_mut(li + 1) {
                        child.hover = child.entries.iter().position(selectable);
                    }
                    MenuEvent::None
                } else if keysym == keys::RIGHT {
                    MenuEvent::None
                } else {
                    self.choose(li, row)
                }
            }
            _ => MenuEvent::None,
        }
    }

    fn snapshot(&self) -> Vec<(Area, Option<usize>)> {
        self.levels.iter().map(|l| (l.rect, l.hover)).collect()
    }

    /// Changes whenever the rendered levels would change.
    pub fn render_hash(&self) -> u64 {
        crate::render::hash_of(
            &self
                .snapshot()
                .iter()
                .map(|(r, h)| (r.x, r.y, r.w, r.h, *h))
                .collect::<Vec<_>>(),
        )
    }

    /// Draws level `i` at its size.
    pub fn render(&self, i: usize) -> Bitmap {
        let Some(level) = self.levels.get(i) else {
            return Bitmap::new(1, 1, 0xFF00_0000);
        };
        let u = self.scale;
        let c = self.colors;
        let (w, h) = (level.rect.w, level.rect.h);
        let mut b = Bitmap::new(w as u32, h as u32, c.bg);
        // Border.
        b.fill_rect(Rect::new(0, 0, w, u), c.border);
        b.fill_rect(Rect::new(0, h - u, w, u), c.border);
        b.fill_rect(Rect::new(0, 0, u, h), c.border);
        b.fill_rect(Rect::new(w - u, 0, u, h), c.border);
        let inner = w - 2 * BORDER * u;
        for (i, (entry, row)) in level.entries.iter().zip(&level.rows).enumerate() {
            let MenuEntry::Item(item) = entry else {
                let y = row.y + row.h / 2;
                b.fill_rect(
                    Rect::new(BORDER * u + 4 * u, y, inner - 8 * u, u),
                    c.disabled,
                );
                continue;
            };
            let hovered = level.hover == Some(i) || level.open_child == Some(i);
            let (state, fg) = if !item.enabled {
                (2, c.disabled)
            } else if hovered {
                (1, c.hover_text)
            } else {
                (0, c.text)
            };
            if hovered && item.enabled {
                b.fill_rect(Rect::new(BORDER * u, row.y, inner, row.h), c.hover_bg);
            }
            let mid = row.y + row.h / 2;
            match item.mark {
                Mark::Check(true) => {
                    draw_pattern(&mut b, CHECK, BORDER * u + 4 * u, mid - 3 * u, u, fg)
                }
                Mark::Radio(true) => {
                    draw_pattern(&mut b, DOT, BORDER * u + 5 * u, mid - 2 * u, u, fg)
                }
                _ => {}
            }
            if let Some(label) = &row.label {
                self.draw_strip(&mut b, &label[state], BORDER * u + MARK_W * u, mid);
            }
            let mut right = w - BORDER * u - PAD_R * u;
            if !item.submenu.is_empty() {
                draw_pattern(&mut b, ARROW, right - 3 * u, mid - 3 * u, u, fg);
                right -= ARROW_W * u;
            }
            if let Some(shortcut) = &row.shortcut {
                let s = &shortcut[state];
                self.draw_strip(&mut b, s, right - self.strip_width(s), mid);
            }
        }
        b
    }

    /// Draws a strip with its vertical middle at `mid`.
    fn draw_strip(&self, b: &mut Bitmap, s: &TextStrip, x: i32, mid: i32) {
        if s.is_bitmap() {
            // Bitmap-font text: draw unscaled, then scale up.
            let mut small = Bitmap::new(s.width().max(1) as u32, s.height().max(1) as u32, 0);
            s.draw(
                &mut small,
                Rect::new(0, 0, s.width(), s.height()),
                0,
                0,
                false,
                0,
            );
            let big = crate::render::scale_nearest(&small, self.scale as u32);
            let y = mid - big.height() as i32 / 2;
            for sy in 0..big.height() as i32 {
                for sx in 0..big.width() as i32 {
                    if let Some(p) = big.get(sx, sy)
                        && p >> 24 != 0
                    {
                        b.set(x + sx, y + sy, p);
                    }
                }
            }
        } else {
            let y = mid - s.height() / 2;
            s.draw(
                b,
                Rect::new(x, 0, s.width(), b.height() as i32),
                y,
                0,
                false,
                0,
            );
        }
    }
}

fn selectable<C>(e: &MenuEntry<C>) -> bool {
    matches!(e, MenuEntry::Item(item) if item.enabled)
}

const CHECK: &[&str] = &[
    "......#", ".....##", "#...##.", "##.##..", ".###...", "..#....",
];
const DOT: &[&str] = &[".##.", "####", "####", ".##."];
const ARROW: &[&str] = &["#...", "##..", "###.", "####", "###.", "##..", "#..."];

fn draw_pattern(b: &mut Bitmap, rows: &[&str], x: i32, y: i32, u: i32, color: u32) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch == '#' {
                b.fill_rect(Rect::new(x + dx as i32 * u, y + dy as i32 * u, u, u), color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::coded_skin;
    use crate::text::{Mask, NoFallback};

    /// Every character is a solid 4 px wide block.
    struct Blocks;

    impl TextRenderer for Blocks {
        fn render(&mut self, text: &str, height: u32) -> Option<Mask> {
            let width = text.chars().count() as u32 * 4;
            Some(Mask {
                width,
                height,
                alpha: vec![255; (width * height) as usize],
            })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Cmd {
        A,
        B,
        Sub1,
        Sub2,
    }

    fn entries() -> Vec<MenuEntry<Cmd>> {
        vec![
            MenuEntry::item("Alpha", Cmd::A)
                .shortcut("Ctrl+A")
                .checked(true),
            MenuEntry::Separator,
            MenuEntry::item("Disabled", Cmd::B).enabled(false),
            MenuEntry::submenu(
                "More",
                vec![
                    MenuEntry::item("One", Cmd::Sub1).radio(true),
                    MenuEntry::item("Two", Cmd::Sub2).radio(false),
                ],
            ),
        ]
    }

    const SCREEN: Area = Area::new(0, 0, 1000, 800);

    fn skin() -> Skin {
        coded_skin()
    }

    fn open_at(at: Point, scale: u32) -> MenuStack<Cmd> {
        MenuStack::open(entries(), at, SCREEN, &skin(), &mut Blocks, scale)
    }

    /// Desktop point in the middle of row `row` of level `li`.
    fn row_point(m: &MenuStack<Cmd>, li: usize, row: usize) -> Point {
        let l = &m.levels[li];
        let r = &l.rows[row];
        Point::new(l.rect.x + l.rect.w / 2, l.rect.y + r.y + r.h / 2)
    }

    #[test]
    fn layout_rows_and_size() {
        let m = open_at(Point::new(100, 50), 1);
        let r = m.rect(0).unwrap_or_default();
        assert_eq!((r.x, r.y), (100, 50));
        // 3 items and a separator, plus borders.
        assert_eq!(r.h, 3 * ROW_H + SEPARATOR_H + 2);
        assert!(r.w >= MIN_W + 2);
        let m2 = open_at(Point::new(100, 50), 2);
        assert_eq!(m2.rect(0).unwrap_or_default().h, 2 * r.h);
    }

    #[test]
    fn stays_on_screen() {
        let m = open_at(Point::new(990, 790), 1);
        let r = m.rect(0).unwrap_or_default();
        assert_eq!(r.right(), SCREEN.right());
        assert_eq!(r.bottom(), SCREEN.bottom());
    }

    #[test]
    fn hover_opens_submenu_and_click_chooses() {
        let mut m = open_at(Point::new(100, 50), 1);
        assert!(m.pointer_move(row_point(&m, 0, 0), &skin(), &mut Blocks));
        assert_eq!(m.depth(), 1);
        m.pointer_move(row_point(&m, 0, 3), &skin(), &mut Blocks);
        assert_eq!(m.depth(), 2);
        let parent = m.rect(0).unwrap_or_default();
        let child = m.rect(1).unwrap_or_default();
        assert_eq!(child.x, parent.right() - BORDER);
        // Moving into the submenu keeps it open; choosing returns the command.
        let p = row_point(&m, 1, 1);
        m.pointer_move(p, &skin(), &mut Blocks);
        assert_eq!(m.depth(), 2);
        assert_eq!(m.release(p), MenuEvent::Chosen(Cmd::Sub2));
        // Back on another parent item closes the submenu.
        m.pointer_move(row_point(&m, 0, 0), &skin(), &mut Blocks);
        assert_eq!(m.depth(), 1);
    }

    #[test]
    fn submenu_flips_left_at_screen_edge() {
        let mut m = open_at(Point::new(SCREEN.right() - 120, 50), 1);
        m.pointer_move(row_point(&m, 0, 3), &skin(), &mut Blocks);
        let parent = m.rect(0).unwrap_or_default();
        let child = m.rect(1).unwrap_or_default();
        assert!(child.right() <= parent.x + BORDER, "{child:?} {parent:?}");
    }

    #[test]
    fn disabled_and_separators_do_nothing() {
        let mut m = open_at(Point::new(100, 50), 1);
        assert_eq!(m.release(row_point(&m, 0, 1)), MenuEvent::None);
        assert_eq!(m.release(row_point(&m, 0, 2)), MenuEvent::None);
        assert_eq!(m.release(row_point(&m, 0, 0)), MenuEvent::Chosen(Cmd::A));
    }

    #[test]
    fn press_outside_closes() {
        let mut m = open_at(Point::new(100, 50), 1);
        assert_eq!(m.press(Point::new(5, 5)), MenuEvent::Close);
        assert_eq!(m.press(row_point(&m, 0, 0)), MenuEvent::None);
        assert_eq!(m.release(Point::new(5, 5)), MenuEvent::None);
    }

    #[test]
    fn keyboard_navigation() {
        let mut m = open_at(Point::new(100, 50), 1);
        let k = |m: &mut MenuStack<Cmd>, key| m.key(key, &skin(), &mut Blocks);
        k(&mut m, keys::DOWN);
        assert_eq!(m.levels[0].hover, Some(0));
        // Skips the separator and the disabled item.
        k(&mut m, keys::DOWN);
        assert_eq!(m.levels[0].hover, Some(3));
        k(&mut m, keys::RIGHT);
        assert_eq!(m.depth(), 2);
        assert_eq!(m.levels[1].hover, Some(0));
        k(&mut m, keys::UP);
        assert_eq!(m.levels[1].hover, Some(1));
        assert_eq!(k(&mut m, keys::ESCAPE), MenuEvent::None);
        assert_eq!(m.depth(), 1);
        k(&mut m, keys::UP);
        assert_eq!(m.levels[0].hover, Some(0));
        assert_eq!(k(&mut m, keys::RETURN), MenuEvent::Chosen(Cmd::A));
        assert_eq!(k(&mut m, keys::ESCAPE), MenuEvent::Close);
    }

    #[test]
    fn render_draws_hover_and_marks() {
        let mut m = open_at(Point::new(100, 50), 1);
        let c = m.colors;
        let plain = m.render(0);
        assert_eq!(plain.width() as i32, m.rect(0).unwrap_or_default().w);
        assert_eq!(plain.get(0, 0), Some(c.border));
        // The check mark of the first item.
        let (row_y, mid) = {
            let row = &m.levels[0].rows[0];
            (row.y, row.y + row.h / 2)
        };
        assert_eq!(plain.get(BORDER + 4 + 6, mid - 3), Some(c.text));
        let hash = m.render_hash();
        m.pointer_move(row_point(&m, 0, 0), &skin(), &mut Blocks);
        assert_ne!(m.render_hash(), hash);
        let hovered = m.render(0);
        assert_eq!(hovered.get(BORDER + 1, row_y + 1), Some(c.hover_bg));
        assert_eq!(hovered.get(BORDER + 4 + 6, mid - 3), Some(c.hover_text));
    }

    #[test]
    fn bitmap_font_fallback_renders_scaled() {
        let m = MenuStack::open(
            entries(),
            Point::new(0, 0),
            SCREEN,
            &coded_skin(),
            &mut NoFallback,
            2,
        );
        let b = m.render(0);
        assert_eq!(b.width() as i32, m.rect(0).unwrap_or_default().w);
    }
}
