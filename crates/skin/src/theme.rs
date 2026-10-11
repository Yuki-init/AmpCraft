//! Built-in skins generated from popular color schemes.
//!
//! Every sheet of the classic skin format is painted in code from a [`Palette`]: frames,
//! buttons, sliders, a 5x6 pixel font, LCD digits and the generic-window letters. All art is
//! original to this project, so these skins can ship with the player. [`Theme::skin`] builds
//! a [`Skin`] directly; [`Theme::export_wsz`] writes a `.wsz` other players can load.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use crate::bitmap::{Bitmap, Rect};
use crate::sprites::Sheet;
use crate::{Skin, font};

/// The colors a theme is painted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Darkest shade: displays, list backgrounds, deep shadows.
    pub crust: u32,
    /// Window background.
    pub base: u32,
    /// Raised surfaces: buttons, frame edges.
    pub surface: u32,
    /// Highlights on raised surfaces.
    pub overlay: u32,
    pub text: u32,
    pub subtext: u32,
    /// Display text, title accents, indicator lights.
    pub accent: u32,
    /// Selected playlist row background.
    pub selection: u32,
    pub red: u32,
    pub orange: u32,
    pub yellow: u32,
    pub green: u32,
    pub cyan: u32,
}

/// A named built-in theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Lowercase id used on the command line and in settings, e.g. `dracula`.
    pub id: &'static str,
    /// Display name, e.g. `Dracula`.
    pub name: &'static str,
    pub palette: Palette,
}

const fn hex(v: u32) -> u32 {
    0xFF00_0000 | v
}

/// The built-in themes, in menu order.
pub const THEMES: &[Theme] = &[
    Theme {
        id: "dracula",
        name: "Dracula",
        palette: Palette {
            crust: hex(0x191a21),
            base: hex(0x282a36),
            surface: hex(0x44475a),
            overlay: hex(0x6272a4),
            text: hex(0xf8f8f2),
            subtext: hex(0xbfbfbf),
            accent: hex(0xbd93f9),
            selection: hex(0x44475a),
            red: hex(0xff5555),
            orange: hex(0xffb86c),
            yellow: hex(0xf1fa8c),
            green: hex(0x50fa7b),
            cyan: hex(0x8be9fd),
        },
    },
    Theme {
        id: "nord",
        name: "Nord",
        palette: Palette {
            crust: hex(0x242933),
            base: hex(0x2e3440),
            surface: hex(0x3b4252),
            overlay: hex(0x4c566a),
            text: hex(0xeceff4),
            subtext: hex(0xd8dee9),
            accent: hex(0x88c0d0),
            selection: hex(0x434c5e),
            red: hex(0xbf616a),
            orange: hex(0xd08770),
            yellow: hex(0xebcb8b),
            green: hex(0xa3be8c),
            cyan: hex(0x8fbcbb),
        },
    },
    Theme {
        id: "gruvbox",
        name: "Gruvbox Dark",
        palette: Palette {
            crust: hex(0x1d2021),
            base: hex(0x282828),
            surface: hex(0x3c3836),
            overlay: hex(0x665c54),
            text: hex(0xebdbb2),
            subtext: hex(0xbdae93),
            accent: hex(0xfabd2f),
            selection: hex(0x504945),
            red: hex(0xfb4934),
            orange: hex(0xfe8019),
            yellow: hex(0xfabd2f),
            green: hex(0xb8bb26),
            cyan: hex(0x8ec07c),
        },
    },
    Theme {
        id: "solarized",
        name: "Solarized Dark",
        palette: Palette {
            crust: hex(0x00212b),
            base: hex(0x002b36),
            surface: hex(0x073642),
            overlay: hex(0x586e75),
            text: hex(0x93a1a1),
            subtext: hex(0x839496),
            accent: hex(0x2aa198),
            selection: hex(0x073642),
            red: hex(0xdc322f),
            orange: hex(0xcb4b16),
            yellow: hex(0xb58900),
            green: hex(0x859900),
            cyan: hex(0x268bd2),
        },
    },
    Theme {
        id: "catppuccin",
        name: "Catppuccin Mocha",
        palette: Palette {
            crust: hex(0x11111b),
            base: hex(0x1e1e2e),
            surface: hex(0x313244),
            overlay: hex(0x585b70),
            text: hex(0xcdd6f4),
            subtext: hex(0xa6adc8),
            accent: hex(0xcba6f7),
            selection: hex(0x45475a),
            red: hex(0xf38ba8),
            orange: hex(0xfab387),
            yellow: hex(0xf9e2af),
            green: hex(0xa6e3a1),
            cyan: hex(0x89dceb),
        },
    },
    Theme {
        id: "tokyonight",
        name: "Tokyo Night",
        palette: Palette {
            crust: hex(0x16161e),
            base: hex(0x1a1b26),
            surface: hex(0x292e42),
            overlay: hex(0x414868),
            text: hex(0xc0caf5),
            subtext: hex(0xa9b1d6),
            accent: hex(0x7aa2f7),
            selection: hex(0x33467c),
            red: hex(0xf7768e),
            orange: hex(0xff9e64),
            yellow: hex(0xe0af68),
            green: hex(0x9ece6a),
            cyan: hex(0x7dcfff),
        },
    },
];

/// Finds a theme by id or display name (case-insensitive), also accepting `theme:<id>`.
pub fn find(name: &str) -> Option<&'static Theme> {
    let n = name.trim();
    let n = n.strip_prefix("theme:").unwrap_or(n).to_lowercase();
    THEMES.iter().find(|t| {
        t.id == n || t.name.to_lowercase() == n || t.name.to_lowercase().replace(' ', "") == n
    })
}

// ---------------------------------------------------------------------------------------
// Painting helpers

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let x = ((a >> s) & 0xFF) as f32;
        let y = ((b >> s) & 0xFF) as f32;
        ((x + (y - x) * t).round() as u32) << s
    };
    0xFF00_0000 | ch(16) | ch(8) | ch(0)
}

/// Relative luminance (WCAG): sRGB channels linearized, then weighted.
fn luma(c: u32) -> f32 {
    let f = |s: u32| {
        let v = ((c >> s) & 0xFF) as f32 / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(16) + 0.7152 * f(8) + 0.0722 * f(0)
}

/// Raised box: light top/left edge, dark bottom/right edge.
fn raised(b: &mut Bitmap, r: Rect, fill: u32, light: u32, dark: u32) {
    b.fill_rect(r, fill);
    b.fill_rect(Rect::new(r.x, r.y, r.w, 1), light);
    b.fill_rect(Rect::new(r.x, r.y, 1, r.h), light);
    b.fill_rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), dark);
    b.fill_rect(Rect::new(r.x + r.w - 1, r.y, 1, r.h), dark);
}

/// Sunken box: dark top/left edge, light bottom/right edge.
fn sunken(b: &mut Bitmap, r: Rect, fill: u32, light: u32, dark: u32) {
    raised(b, r, fill, dark, light);
}

/// Draws a pixel pattern (`#` = set) at (x, y).
fn pattern(b: &mut Bitmap, rows: &[&str], x: i32, y: i32, color: u32) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, c) in row.chars().enumerate() {
            if c == '#' {
                b.set(x + dx as i32, y + dy as i32, color);
            }
        }
    }
}

fn pattern_size(rows: &[&str]) -> (i32, i32) {
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
    (w, rows.len() as i32)
}

/// Draws a pattern centered in `r`, shifted by `nudge` (pressed buttons move down-right).
fn pattern_centered(b: &mut Bitmap, rows: &[&str], r: Rect, color: u32, nudge: i32) {
    let (w, h) = pattern_size(rows);
    pattern(
        b,
        rows,
        r.x + (r.w - w) / 2 + nudge,
        r.y + (r.h - h) / 2 + nudge,
        color,
    );
}

// ---------------------------------------------------------------------------------------
// The 5x6 pixel font (glyphs are at most 4 or 5 wide and 5 tall).

fn glyph_rows(c: char) -> &'static [&'static str] {
    match c.to_ascii_lowercase() {
        'a' => &[".##.", "#..#", "####", "#..#", "#..#"],
        'b' => &["###.", "#..#", "###.", "#..#", "###."],
        'c' => &[".###", "#...", "#...", "#...", ".###"],
        'd' => &["###.", "#..#", "#..#", "#..#", "###."],
        'e' => &["####", "#...", "###.", "#...", "####"],
        'f' => &["####", "#...", "###.", "#...", "#..."],
        'g' => &[".###", "#...", "#.##", "#..#", ".###"],
        'h' => &["#..#", "#..#", "####", "#..#", "#..#"],
        'i' => &["###", ".#.", ".#.", ".#.", "###"],
        'j' => &["..##", "...#", "...#", "#..#", ".##."],
        'k' => &["#..#", "#.#.", "##..", "#.#.", "#..#"],
        'l' => &["#...", "#...", "#...", "#...", "####"],
        'm' => &["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'n' => &["#..#", "##.#", "#.##", "#..#", "#..#"],
        'o' => &[".##.", "#..#", "#..#", "#..#", ".##."],
        'p' => &["###.", "#..#", "###.", "#...", "#..."],
        'q' => &[".##.", "#..#", "#..#", "#.#.", ".#.#"],
        'r' => &["###.", "#..#", "###.", "#.#.", "#..#"],
        's' => &[".###", "#...", ".##.", "...#", "###."],
        't' => &["###", ".#.", ".#.", ".#.", ".#."],
        'u' => &["#..#", "#..#", "#..#", "#..#", ".##."],
        'v' => &["#...#", "#...#", ".#.#.", ".#.#.", "..#.."],
        'w' => &["#...#", "#...#", "#.#.#", "##.##", "#...#"],
        'x' => &["#..#", "#..#", ".##.", "#..#", "#..#"],
        'y' => &["#.#", "#.#", ".#.", ".#.", ".#."],
        'z' => &["####", "..#.", ".#..", "#...", "####"],
        '0' => &[".##.", "#.##", "##.#", "#..#", ".##."],
        '1' => &[".#.", "##.", ".#.", ".#.", "###"],
        '2' => &["###.", "...#", ".##.", "#...", "####"],
        '3' => &["###.", "...#", ".##.", "...#", "###."],
        '4' => &["#..#", "#..#", "####", "...#", "...#"],
        '5' => &["####", "#...", "###.", "...#", "###."],
        '6' => &[".##.", "#...", "###.", "#..#", ".##."],
        '7' => &["####", "...#", "..#.", ".#..", ".#.."],
        '8' => &[".##.", "#..#", ".##.", "#..#", ".##."],
        '9' => &[".##.", "#..#", ".###", "...#", ".##."],
        '"' => &["#.#", "#.#", "...", "...", "..."],
        '@' => &[".##.", "#.##", "#.##", "#...", ".##."],
        '…' => &[".....", ".....", ".....", ".....", "#.#.#"],
        '.' => &[".", ".", ".", ".", "#"],
        ':' => &[".", "#", ".", "#", "."],
        '(' => &[".#", "#.", "#.", "#.", ".#"],
        ')' => &["#.", ".#", ".#", ".#", "#."],
        '-' => &["...", "...", "###", "...", "..."],
        '\'' => &["#", "#", ".", ".", "."],
        '!' => &["#", "#", "#", ".", "#"],
        '_' => &["....", "....", "....", "....", "####"],
        '+' => &["...", ".#.", "###", ".#.", "..."],
        '\\' => &["#..", "#..", ".#.", "..#", "..#"],
        '/' => &["..#", "..#", ".#.", "#..", "#.."],
        '[' => &["##", "#.", "#.", "#.", "##"],
        ']' => &["##", ".#", ".#", ".#", "##"],
        '^' => &[".#.", "#.#", "...", "...", "..."],
        '&' => &[".#..", "#.#.", ".#..", "#.#.", ".#.#"],
        '%' => &["#..#", "..#.", ".#..", "#...", "#..#"],
        ',' => &["..", "..", "..", ".#", "#."],
        '=' => &["...", "###", "...", "###", "..."],
        '$' => &[".###", "##..", ".##.", "..##", "###."],
        '#' => &[".#.#", "####", ".#.#", "####", ".#.#"],
        'å' => &[".##.", ".##.", "#..#", "####", "#..#"],
        'ö' => &["#..#", ".##.", "#..#", "#..#", ".##."],
        'ä' => &["#..#", ".##.", "#..#", "####", "#..#"],
        '?' => &["###.", "...#", ".##.", "....", ".#.."],
        '*' => &["...", "#.#", ".#.", "#.#", "..."],
        _ => &[],
    }
}

/// Width of `text` in the pixel font (glyph widths plus 1 px gaps).
fn text_width(text: &str) -> i32 {
    let mut w = 0;
    for (i, c) in text.chars().enumerate() {
        if i > 0 {
            w += 1;
        }
        w += if c == ' ' {
            3
        } else {
            pattern_size(glyph_rows(c)).0
        };
    }
    w
}

/// Draws `text` with the pixel font at (x, y). Returns the width drawn.
fn text(b: &mut Bitmap, s: &str, x: i32, y: i32, color: u32) -> i32 {
    let mut cx = x;
    for c in s.chars() {
        if c == ' ' {
            cx += 4;
            continue;
        }
        let rows = glyph_rows(c);
        pattern(b, rows, cx, y, color);
        cx += pattern_size(rows).0 + 1;
    }
    cx - x
}

fn text_centered(b: &mut Bitmap, s: &str, r: Rect, color: u32, nudge: i32) {
    let x = r.x + (r.w - text_width(s)) / 2 + nudge;
    let y = r.y + (r.h - 5) / 2 + nudge;
    text(b, s, x, y, color);
}

// ---------------------------------------------------------------------------------------
// Derived colors and icons

#[derive(Clone, Copy)]
struct Colors {
    p: Palette,
    light: u32,
    dark: u32,
    lcd_on: u32,
    lcd_off: u32,
    pressed: u32,
    led_off: u32,
    stripe: u32,
    stripe_off: u32,
}

impl Colors {
    fn new(p: Palette) -> Self {
        Self {
            p,
            light: mix(p.surface, p.text, 0.3),
            dark: p.crust,
            lcd_on: p.accent,
            lcd_off: mix(p.crust, p.accent, 0.14),
            pressed: mix(p.surface, p.crust, 0.55),
            led_off: mix(p.surface, p.crust, 0.4),
            stripe: mix(p.base, p.accent, 0.65),
            stripe_off: mix(p.base, p.overlay, 0.8),
        }
    }

    /// A raised or pressed button with an icon or label drawn by `content`.
    fn button(
        &self,
        b: &mut Bitmap,
        r: Rect,
        down: bool,
        content: impl FnOnce(&mut Bitmap, Rect, i32),
    ) {
        if down {
            sunken(b, r, self.pressed, self.light, self.dark);
        } else {
            raised(b, r, self.p.surface, self.light, self.dark);
        }
        content(b, r, i32::from(down));
    }

    fn led(&self, b: &mut Bitmap, x: i32, y: i32, on: bool) {
        b.fill_rect(
            Rect::new(x, y, 3, 3),
            if on { self.p.green } else { self.led_off },
        );
    }
}

const ICON_PREV: &[&str] = &[
    "#....#", "#...##", "#..###", "#.####", "#..###", "#...##", "#....#",
];
const ICON_PLAY: &[&str] = &["#...", "##..", "###.", "####", "###.", "##..", "#..."];
const ICON_PAUSE: &[&str] = &[
    "##.##", "##.##", "##.##", "##.##", "##.##", "##.##", "##.##",
];
const ICON_STOP: &[&str] = &["######", "######", "######", "######", "######", "######"];
const ICON_NEXT: &[&str] = &[
    "#....#", "##...#", "###..#", "####.#", "###..#", "##...#", "#....#",
];
const ICON_EJECT: &[&str] = &[
    "...#...", "..###..", ".#####.", "#######", ".......", "#######",
];
const ICON_REPEAT: &[&str] = &[
    ".#####.", "#.....#", "#......", "#....#.", "#...###", ".####.#",
];
const MINI_PREV: &[&str] = &["#..#", "#.##", "####", "#.##", "#..#"];
const MINI_PLAY: &[&str] = &["#..", "##.", "###", "##.", "#.."];
const MINI_PAUSE: &[&str] = &["#.#", "#.#", "#.#", "#.#", "#.#"];
const MINI_STOP: &[&str] = &["####", "####", "####", "####"];
const MINI_NEXT: &[&str] = &["#..#", "##.#", "####", "##.#", "#..#"];
const MINI_EJECT: &[&str] = &["..#..", ".###.", "#####", ".....", "#####"];
const BTN_OPTIONS: &[&str] = &["#####", ".....", "#####", ".....", "#####"];
const BTN_MINIMIZE: &[&str] = &[".....", ".....", ".....", ".....", "#####"];
const BTN_SHADE: &[&str] = &["#####", "#####", ".....", ".....", "....."];
const BTN_UNSHADE: &[&str] = &["#####", "#...#", "#...#", "#...#", "#####"];
const BTN_CLOSE: &[&str] = &["#...#", ".#.#.", "..#..", ".#.#.", "#...#"];
const NOTE: &[&str] = &[
    "..####", "..#..#", "..#..#", "..#..#", "###.##", "###.##", "###...", ".#....",
];

/// A 9x9 title-bar button.
fn small_button(c: &Colors, b: &mut Bitmap, x: i32, y: i32, icon: &[&str], down: bool) {
    let r = Rect::new(x, y, 9, 9);
    c.button(b, r, down, |b, r, n| {
        pattern_centered(b, icon, r, c.p.text, n)
    });
}

/// A 14 px title or shade bar with a centered (or left) title and accent stripes.
fn title_bar(c: &Colors, b: &mut Bitmap, bar: Rect, title: &str, active: bool, left_title: bool) {
    let (x, y, w) = (bar.x, bar.y, bar.w);
    b.fill_rect(Rect::new(x, y, w, 14), c.p.base);
    b.fill_rect(Rect::new(x, y, w, 1), c.light);
    b.fill_rect(Rect::new(x, y + 13, w, 1), c.dark);
    let tw = text_width(title);
    let tx = if left_title { x + 20 } else { x + (w - tw) / 2 };
    let (stripe, fg) = if active {
        (c.stripe, c.p.accent)
    } else {
        (c.stripe_off, c.p.subtext)
    };
    let stripes = |b: &mut Bitmap, from: i32, to: i32| {
        if to > from {
            b.fill_rect(Rect::new(from, y + 5, to - from, 1), stripe);
            b.fill_rect(Rect::new(from, y + 8, to - from, 1), stripe);
        }
    };
    stripes(b, x + 18, tx - 5);
    stripes(b, tx + tw + 5, x + w - 34);
    text(b, title, tx, y + 5, fg);
}

/// Seven-segment digit `d` (None = blank, Some(10) = minus) in a 9x13 cell.
fn lcd_digit(c: &Colors, b: &mut Bitmap, cx: i32, d: Option<u8>) {
    // a, b, c, d, e, f, g
    const SEGS: [[bool; 7]; 11] = [
        [true, true, true, true, true, true, false],
        [false, true, true, false, false, false, false],
        [true, true, false, true, true, false, true],
        [true, true, true, true, false, false, true],
        [false, true, true, false, false, true, true],
        [true, false, true, true, false, true, true],
        [true, false, true, true, true, true, true],
        [true, true, true, false, false, false, false],
        [true, true, true, true, true, true, true],
        [true, true, true, true, false, true, true],
        [false, false, false, false, false, false, true],
    ];
    let on = d.map_or([false; 7], |d| SEGS[usize::from(d.min(10))]);
    let col = |lit: bool| if lit { c.lcd_on } else { c.lcd_off };
    let h = |b: &mut Bitmap, y: i32, lit: bool| b.fill_rect(Rect::new(cx + 2, y, 5, 1), col(lit));
    let v =
        |b: &mut Bitmap, x: i32, y: i32, lit: bool| b.fill_rect(Rect::new(x, y, 1, 4), col(lit));
    h(b, 1, on[0]);
    v(b, cx + 7, 2, on[1]);
    v(b, cx + 7, 7, on[2]);
    h(b, 11, on[3]);
    v(b, cx + 1, 7, on[4]);
    v(b, cx + 1, 2, on[5]);
    h(b, 6, on[6]);
}

// ---------------------------------------------------------------------------------------
// Sheets

fn main_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(275, 116, p.base);
    raised(&mut b, Rect::new(0, 0, 275, 116), p.base, c.light, c.dark);
    // Left display: play state, time and visualizer.
    sunken(&mut b, Rect::new(22, 24, 82, 38), p.crust, c.light, c.dark);
    for y in [30, 35] {
        b.fill_rect(Rect::new(73, y, 2, 2), c.lcd_on);
    }
    // Title display and stream info boxes.
    sunken(
        &mut b,
        Rect::new(109, 23, 157, 13),
        p.crust,
        c.light,
        c.dark,
    );
    sunken(&mut b, Rect::new(109, 42, 19, 9), p.crust, c.light, c.dark);
    text(&mut b, "KBPS", 130, 44, p.subtext);
    sunken(&mut b, Rect::new(154, 42, 15, 9), p.crust, c.light, c.dark);
    text(&mut b, "KHZ", 171, 44, p.subtext);
    // Separators and the emblem.
    b.fill_rect(Rect::new(10, 70, 255, 1), mix(p.base, p.crust, 0.6));
    b.fill_rect(Rect::new(10, 85, 255, 1), mix(p.base, p.crust, 0.6));
    pattern_centered(&mut b, NOTE, Rect::new(253, 91, 13, 15), p.accent, 0);
    b
}

fn titlebar_bmp(c: &Colors, name: &str) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(344, 87, p.base);
    for (down, y) in [(false, 0), (true, 9)] {
        small_button(c, &mut b, 0, y, BTN_OPTIONS, down);
        small_button(c, &mut b, 9, y, BTN_MINIMIZE, down);
        small_button(c, &mut b, 18, y, BTN_CLOSE, down);
    }
    small_button(c, &mut b, 0, 18, BTN_SHADE, false);
    small_button(c, &mut b, 9, 18, BTN_SHADE, true);
    small_button(c, &mut b, 0, 27, BTN_UNSHADE, false);
    small_button(c, &mut b, 9, 27, BTN_UNSHADE, true);

    let title = name.to_uppercase();
    for (active, y) in [(true, 0), (false, 15)] {
        title_bar(c, &mut b, Rect::new(27, y, 275, 14), &title, active, false);
        small_button(c, &mut b, 27 + 6, y + 3, BTN_OPTIONS, false);
        small_button(c, &mut b, 27 + 244, y + 3, BTN_MINIMIZE, false);
        small_button(c, &mut b, 27 + 254, y + 3, BTN_SHADE, false);
        small_button(c, &mut b, 27 + 264, y + 3, BTN_CLOSE, false);
    }
    // Shade bars; the inactive one is drawn last (the classic layout shares a row).
    for (active, y) in [(true, 29), (false, 42)] {
        title_bar(c, &mut b, Rect::new(27, y, 275, 14), &title, active, true);
        b.fill_rect(Rect::new(27 + 18, y + 2, 92, 10), p.base);
        text(
            &mut b,
            &title,
            27 + 20,
            y + 5,
            if active { p.accent } else { p.subtext },
        );
        sunken(
            &mut b,
            Rect::new(27 + 125, y + 3, 28, 8),
            p.crust,
            c.light,
            c.dark,
        );
        b.fill_rect(Rect::new(27 + 166, y + 1, 76, 12), p.base);
        for (icon, x) in [
            (MINI_PREV, 169),
            (MINI_PLAY, 177),
            (MINI_PAUSE, 189),
            (MINI_STOP, 197),
            (MINI_NEXT, 206),
            (MINI_EJECT, 217),
        ] {
            pattern(&mut b, icon, 27 + x, y + 4, p.text);
        }
        small_button(c, &mut b, 27 + 244, y + 3, BTN_MINIMIZE, false);
        small_button(c, &mut b, 27 + 254, y + 3, BTN_UNSHADE, false);
        small_button(c, &mut b, 27 + 264, y + 3, BTN_CLOSE, false);
    }
    // Clutter bar: O A I D V, normal and disabled, plus the lit letters. Each letter is
    // centered in its button cell (top relative to the bar, height); lit cells keep the bar's
    // edges so they line up with it.
    let letters = [
        ("O", 3, 8),
        ("A", 11, 7),
        ("I", 18, 7),
        ("D", 25, 8),
        ("V", 33, 7),
    ];
    for (x, color) in [(304, p.subtext), (312, p.overlay)] {
        raised(&mut b, Rect::new(x, 0, 8, 43), p.surface, c.light, c.dark);
        for (l, top, h) in letters {
            let w = text_width(l);
            text(&mut b, l, x + (8 - w) / 2, top + (h - 5) / 2, color);
        }
    }
    for (i, (l, _, h)) in letters.iter().enumerate() {
        let x = 304 + i as i32 * 8;
        let y = [47, 55, 62, 69, 77][i];
        b.fill_rect(Rect::new(x, y, 8, *h), p.accent);
        b.fill_rect(Rect::new(x, y, 1, *h), c.light);
        b.fill_rect(Rect::new(x + 7, y, 1, *h), c.dark);
        let w = text_width(l);
        text(&mut b, l, x + (8 - w) / 2, y + (*h - 5) / 2, p.crust);
    }
    // Shade position slider: groove and three 3x7 thumbs.
    sunken(&mut b, Rect::new(0, 36, 17, 7), p.crust, c.light, c.dark);
    for x in [17, 20, 23] {
        b.fill_rect(Rect::new(x, 36, 3, 7), p.accent);
    }
    b
}

fn cbuttons_bmp(c: &Colors) -> Bitmap {
    let mut b = Bitmap::new(136, 36, c.p.base);
    let specs = [
        (0, 23, ICON_PREV),
        (23, 23, ICON_PLAY),
        (46, 23, ICON_PAUSE),
        (69, 23, ICON_STOP),
        (92, 22, ICON_NEXT),
    ];
    for (down, y) in [(false, 0), (true, 18)] {
        for (x, w, icon) in specs {
            c.button(&mut b, Rect::new(x, y, w, 18), down, |b, r, n| {
                pattern_centered(b, icon, r, c.p.text, n)
            });
        }
        let ey = if down { 16 } else { 0 };
        c.button(&mut b, Rect::new(114, ey, 22, 16), down, |b, r, n| {
            pattern_centered(b, ICON_EJECT, r, c.p.text, n)
        });
    }
    b
}

fn shufrep_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(92, 85, p.base);
    // States in rows: normal, pressed, on, on + pressed.
    for (i, (on, down)) in [(false, false), (false, true), (true, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        let y = i as i32 * 15;
        c.button(&mut b, Rect::new(0, y, 28, 15), down, |b, r, n| {
            c.led(b, r.x + 4 + n, r.y + 6 + n, on);
            pattern(b, ICON_REPEAT, r.x + 11 + n, r.y + 4 + n, p.text);
        });
        c.button(&mut b, Rect::new(28, y, 47, 15), down, |b, r, n| {
            c.led(b, r.x + 4 + n, r.y + 6 + n, on);
            text(b, "SHUFFLE", r.x + 10 + n, r.y + 5 + n, p.text);
        });
    }
    for (x, label) in [(0, "EQ"), (23, "PL")] {
        for (dx, dy, on, down) in [
            (0, 61, false, false),
            (0, 73, true, false),
            (46, 61, false, true),
            (46, 73, true, true),
        ] {
            c.button(&mut b, Rect::new(x + dx, dy, 23, 12), down, |b, r, n| {
                c.led(b, r.x + 3 + n, r.y + 5 + n, on);
                text(b, label, r.x + 9 + n, r.y + 4 + n, p.text);
            });
        }
    }
    b
}

/// Every character TEXT.BMP has a cell for.
const FONT_CHARS: &str = "abcdefghijklmnopqrstuvwxyz\"@0123456789….:()-'!_+\\/[]^&%,=$#åöä?*";

fn text_bmp(c: &Colors) -> Bitmap {
    let mut b = Bitmap::new(155, 18, c.p.crust);
    for ch in FONT_CHARS.chars() {
        if let Some((row, col)) = font::glyph(ch) {
            pattern(
                &mut b,
                glyph_rows(ch),
                col * font::CHAR_W,
                row * font::CHAR_H,
                c.lcd_on,
            );
        }
    }
    b
}

fn numbers_bmp(c: &Colors, extended: bool) -> Bitmap {
    let w = if extended { 108 } else { 99 };
    let mut b = Bitmap::new(w, 13, c.p.crust);
    for d in 0..10u8 {
        lcd_digit(c, &mut b, i32::from(d) * 9, Some(d));
    }
    lcd_digit(c, &mut b, 90, None);
    if extended {
        lcd_digit(c, &mut b, 99, Some(10));
    }
    b
}

fn playpaus_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(42, 9, p.crust);
    pattern(
        &mut b,
        &["#...", "##..", "###.", "####", "###.", "##..", "#..."],
        3,
        1,
        p.green,
    );
    pattern(
        &mut b,
        &[
            "##.##", "##.##", "##.##", "##.##", "##.##", "##.##", "##.##",
        ],
        11,
        1,
        p.yellow,
    );
    pattern(
        &mut b,
        &["#####", "#####", "#####", "#####", "#####"],
        20,
        2,
        p.subtext,
    );
    b.fill_rect(Rect::new(36, 3, 2, 3), p.red);
    b.fill_rect(Rect::new(39, 3, 2, 3), p.green);
    b
}

fn monoster_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(58, 24, p.base);
    for (y, color) in [(0, p.accent), (12, mix(p.base, p.overlay, 0.9))] {
        text_centered(&mut b, "STEREO", Rect::new(0, y, 29, 12), color, 0);
        text_centered(&mut b, "MONO", Rect::new(29, y, 27, 12), color, 0);
    }
    b
}

fn posbar_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(307, 10, p.base);
    sunken(&mut b, Rect::new(0, 3, 248, 4), p.crust, c.light, c.dark);
    for (x, down) in [(248, false), (278, true)] {
        let r = Rect::new(x, 0, 29, 10);
        if down {
            raised(&mut b, r, p.accent, c.light, c.dark);
        } else {
            raised(&mut b, r, p.surface, c.light, c.dark);
        }
        for gx in [x + 11, x + 14, x + 17] {
            b.fill_rect(Rect::new(gx, 2, 1, 6), if down { p.crust } else { c.light });
        }
    }
    b
}

/// VOLUME.BMP or BALANCE.BMP: 28 frames 15 px apart, then the thumbs.
fn slider_bmp(c: &Colors, balance: bool) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(68, 433, p.base);
    for i in 0..28 {
        let level = i as f32 / 27.0;
        let y = i * 15;
        let (x0, w) = if balance { (9, 38) } else { (0, 68) };
        let groove = Rect::new(x0 + 2, y + 4, w - 4, 5);
        sunken(&mut b, groove, p.crust, c.light, c.dark);
        let inner = Rect::new(groove.x + 1, groove.y + 1, groove.w - 2, groove.h - 2);
        let color = mix(p.green, p.red, level);
        if balance {
            let half = inner.w / 2;
            let fill = (level * half as f32).round() as i32;
            let mid = inner.x + half;
            b.fill_rect(
                Rect::new(mid - fill, inner.y, 2 * fill.max(1), inner.h),
                color,
            );
        } else {
            let fill = (level * inner.w as f32).round() as i32;
            b.fill_rect(Rect::new(inner.x, inner.y, fill, inner.h), color);
        }
    }
    for (x, down) in [(15, false), (0, true)] {
        let r = Rect::new(x, 422, 14, 11);
        raised(
            &mut b,
            r,
            if down { p.accent } else { p.surface },
            c.light,
            c.dark,
        );
        for gx in [x + 5, x + 8] {
            b.fill_rect(
                Rect::new(gx, 425, 1, 5),
                if down { p.crust } else { c.light },
            );
        }
    }
    b
}

fn eqmain_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(275, 315, p.base);
    raised(&mut b, Rect::new(0, 0, 275, 116), p.base, c.light, c.dark);
    for (label, y) in [("+12 DB", 40), ("+0 DB", 67), ("-12 DB", 94)] {
        text(&mut b, label, 42, y, p.subtext);
    }
    text_centered(&mut b, "PREAMP", Rect::new(13, 102, 30, 7), p.subtext, 0);
    for (i, label) in [
        "60", "170", "310", "600", "1K", "3K", "6K", "12K", "14K", "16K",
    ]
    .iter()
    .enumerate()
    {
        text_centered(
            &mut b,
            label,
            Rect::new(78 + 18 * i as i32 - 2, 102, 18, 7),
            p.subtext,
            0,
        );
    }
    sunken(&mut b, Rect::new(85, 16, 115, 21), p.crust, c.light, c.dark);
    for (y, active) in [(134, true), (149, false)] {
        title_bar(
            c,
            &mut b,
            Rect::new(0, y, 275, 14),
            "EQUALIZER",
            active,
            false,
        );
        small_button(c, &mut b, 254, y + 3, BTN_SHADE, false);
        small_button(c, &mut b, 264, y + 3, BTN_CLOSE, false);
    }
    small_button(c, &mut b, 0, 116, BTN_CLOSE, false);
    small_button(c, &mut b, 0, 125, BTN_CLOSE, true);
    // ON and AUTO: off, on, pressed, on + pressed.
    for (x_on, x_auto, on, down) in [
        (10, 36, false, false),
        (69, 95, true, false),
        (128, 154, false, true),
        (187, 213, true, true),
    ] {
        c.button(&mut b, Rect::new(x_on, 119, 26, 12), down, |b, r, n| {
            c.led(b, r.x + 4 + n, r.y + 5 + n, on);
            text(b, "ON", r.x + 11 + n, r.y + 4 + n, p.text);
        });
        c.button(&mut b, Rect::new(x_auto, 119, 32, 12), down, |b, r, n| {
            c.led(b, r.x + 4 + n, r.y + 5 + n, on);
            text(b, "AUTO", r.x + 10 + n, r.y + 4 + n, p.text);
        });
    }
    for (y, down) in [(164, false), (176, true)] {
        c.button(&mut b, Rect::new(224, y, 44, 12), down, |b, r, n| {
            text_centered(b, "PRESETS", r, p.text, n)
        });
    }
    // Slider backgrounds: 28 frames, bar filled from the bottom.
    for n in 0..28 {
        let x = 13 + (n % 14) * 15;
        let y = 164 + (n / 14) * 65;
        let level = n as f32 / 27.0;
        let groove = Rect::new(x + 5, y + 2, 4, 59);
        sunken(&mut b, groove, p.crust, c.light, c.dark);
        let fill = (level * 57.0).round() as i32;
        b.fill_rect(
            Rect::new(x + 6, y + 60 - fill, 2, fill),
            mix(p.green, p.red, level),
        );
    }
    for (y, down) in [(164, false), (176, true)] {
        let r = Rect::new(0, y, 11, 11);
        raised(
            &mut b,
            r,
            if down { p.accent } else { p.surface },
            c.light,
            c.dark,
        );
        b.fill_rect(
            Rect::new(2, y + 5, 7, 1),
            if down { p.crust } else { c.light },
        );
    }
    // Graph background, line colors (top to bottom) and the preamp line.
    b.fill_rect(Rect::new(0, 294, 113, 19), p.crust);
    for x in (0..113).step_by(4) {
        b.set(x, 294 + 9, mix(p.crust, p.overlay, 0.6));
    }
    for y in 0..19 {
        let t = y as f32 / 18.0;
        let color = if t < 0.5 {
            mix(p.red, p.yellow, t * 2.0)
        } else {
            mix(p.yellow, p.green, t * 2.0 - 1.0)
        };
        b.set(115, 294 + y, color);
    }
    b.fill_rect(Rect::new(0, 314, 113, 1), p.subtext);
    b
}

fn eq_ex_bmp(c: &Colors) -> Bitmap {
    let mut b = Bitmap::new(275, 82, c.p.base);
    for (y, active) in [(0, true), (15, false)] {
        title_bar(
            c,
            &mut b,
            Rect::new(0, y, 275, 14),
            "EQUALIZER",
            active,
            true,
        );
        small_button(c, &mut b, 254, y + 3, BTN_UNSHADE, false);
        small_button(c, &mut b, 264, y + 3, BTN_CLOSE, false);
    }
    small_button(c, &mut b, 1, 38, BTN_UNSHADE, true);
    small_button(c, &mut b, 11, 38, BTN_CLOSE, false);
    small_button(c, &mut b, 11, 47, BTN_CLOSE, true);
    b
}

fn pledit_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(280, 186, p.base);
    for (y, active) in [(0, true), (21, false)] {
        let (stripe, fg) = if active {
            (c.stripe, p.accent)
        } else {
            (c.stripe_off, p.subtext)
        };
        for (x, w) in [(0, 25), (26, 100), (127, 25), (153, 25)] {
            b.fill_rect(Rect::new(x, y, w, 20), p.base);
            b.fill_rect(Rect::new(x, y, w, 1), c.light);
        }
        for (x, w) in [(26, 100), (127, 25), (153, 25)] {
            b.fill_rect(Rect::new(x, y + 7, w, 1), stripe);
            b.fill_rect(Rect::new(x, y + 10, w, 1), stripe);
        }
        b.fill_rect(Rect::new(0, y + 7, 25, 1), stripe);
        b.fill_rect(Rect::new(0, y + 10, 25, 1), stripe);
        b.fill_rect(Rect::new(48, y + 4, 56, 12), p.base);
        text_centered(&mut b, "PLAYLIST", Rect::new(26, y + 2, 100, 14), fg, 0);
        b.fill_rect(Rect::new(153 + 2, y + 2, 22, 11), p.base);
        small_button(c, &mut b, 153 + 4, y + 3, BTN_SHADE, false);
        small_button(c, &mut b, 153 + 14, y + 3, BTN_CLOSE, false);
    }
    // Sides: plain left edge; right edge carries the scrollbar groove.
    b.fill_rect(Rect::new(0, 42, 12, 29), p.base);
    b.fill_rect(Rect::new(11, 42, 1, 29), c.dark);
    b.fill_rect(Rect::new(31, 42, 20, 29), p.base);
    b.fill_rect(Rect::new(31, 42, 1, 29), c.light);
    b.fill_rect(Rect::new(36, 42, 8, 29), mix(p.crust, p.base, 0.4));
    // Scroll handles.
    raised(&mut b, Rect::new(52, 53, 8, 18), p.surface, c.light, c.dark);
    raised(&mut b, Rect::new(61, 53, 8, 18), p.accent, c.light, c.dark);
    small_button(c, &mut b, 52, 42, BTN_CLOSE, true);
    small_button(c, &mut b, 62, 42, BTN_SHADE, true);
    // Window-shade bar pieces.
    for (x, y, w) in [(72, 42, 25), (72, 57, 25), (99, 57, 50), (99, 42, 50)] {
        b.fill_rect(Rect::new(x, y, w, 14), p.base);
        b.fill_rect(Rect::new(x, y, w, 1), c.light);
        b.fill_rect(Rect::new(x, y + 13, w, 1), c.dark);
    }
    for y in [42, 57] {
        small_button(c, &mut b, 99 + 29, y + 3, BTN_UNSHADE, false);
        small_button(c, &mut b, 99 + 39, y + 3, BTN_CLOSE, false);
    }
    small_button(c, &mut b, 150, 42, BTN_UNSHADE, true);
    // Bottom pieces.
    for (x, w) in [(179, 25), (205, 75)] {
        b.fill_rect(Rect::new(x, 0, w, 38), p.base);
        b.fill_rect(Rect::new(x, 37, w, 1), c.dark);
    }
    sunken(&mut b, Rect::new(207, 10, 71, 20), p.crust, c.light, c.dark);
    b.fill_rect(Rect::new(0, 72, 125, 38), p.base);
    b.fill_rect(Rect::new(0, 109, 125, 1), c.dark);
    for (x, label) in [(14, "ADD"), (43, "REM"), (72, "SEL"), (101, "MISC")] {
        c.button(&mut b, Rect::new(x, 80, 22, 18), false, |b, r, n| {
            text_centered(b, label, r, p.text, n)
        });
    }
    b.fill_rect(Rect::new(126, 72, 150, 38), p.base);
    b.fill_rect(Rect::new(126, 109, 150, 1), c.dark);
    sunken(
        &mut b,
        Rect::new(126 + 5, 72 + 8, 64, 9),
        p.crust,
        c.light,
        c.dark,
    );
    sunken(
        &mut b,
        Rect::new(126 + 64, 72 + 21, 34, 9),
        p.crust,
        c.light,
        c.dark,
    );
    for (i, icon) in [
        MINI_PREV, MINI_PLAY, MINI_PAUSE, MINI_STOP, MINI_NEXT, MINI_EJECT,
    ]
    .iter()
    .enumerate()
    {
        pattern_centered(
            &mut b,
            icon,
            Rect::new(126 + 3 + i as i32 * 10, 72 + 22, 10, 10),
            p.text,
            0,
        );
    }
    c.button(
        &mut b,
        Rect::new(126 + 106, 72 + 8, 22, 18),
        false,
        |b, r, n| text_centered(b, "LIST", r, p.text, n),
    );
    for i in 0..4 {
        b.fill_rect(Rect::new(126 + 146 - i * 3, 72 + 34, 1 + i * 3, 1), c.light);
    }
    // Bottom-bar button pressed states (drawn over the buttons while held).
    for (x, label) in [
        (0, "ADD"),
        (54, "REM"),
        (104, "SEL"),
        (154, "MISC"),
        (204, "LIST"),
    ] {
        c.button(&mut b, Rect::new(x, 149, 22, 18), false, |b, r, n| {
            text_centered(b, label, r, p.text, n)
        });
        c.button(&mut b, Rect::new(x + 23, 149, 22, 18), true, |b, r, n| {
            text_centered(b, label, r, p.text, n)
        });
    }
    b
}

/// Separator color for GEN.BMP letter rows; never used in the art itself.
const GEN_SEPARATOR: u32 = 0xFFFF_00FF;

fn gen_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(194, 109, p.base);
    for (y, active) in [(0, true), (21, false)] {
        let stripe = if active { c.stripe } else { c.stripe_off };
        for x in [0, 26, 52, 78, 104, 130] {
            b.fill_rect(Rect::new(x, y, 25, 20), p.base);
            b.fill_rect(Rect::new(x, y, 25, 1), c.light);
            if x != 52 {
                b.fill_rect(Rect::new(x, y + 7, 25, 1), stripe);
                b.fill_rect(Rect::new(x, y + 10, 25, 1), stripe);
            }
        }
        small_button(c, &mut b, 130 + 14, y + 3, BTN_CLOSE, false);
    }
    for (y, h) in [(42, 14), (57, 14)] {
        b.fill_rect(Rect::new(0, y, 125, h), p.base);
        b.fill_rect(Rect::new(0, y + h - 1, 125, 1), c.dark);
    }
    for i in 0..4 {
        b.fill_rect(Rect::new(121 - i * 3, 57 + 11, 1 + i * 3, 1), c.light);
    }
    b.fill_rect(Rect::new(127, 72, 25, 14), p.base);
    b.fill_rect(Rect::new(127, 85, 25, 1), c.dark);
    for (x, w, h) in [(127, 11, 29), (158, 11, 24)] {
        b.fill_rect(Rect::new(x, 42, w, h), p.base);
        b.fill_rect(Rect::new(x + w - 1, 42, 1, h), c.dark);
    }
    for (x, w, h) in [(139, 8, 29), (170, 8, 24)] {
        b.fill_rect(Rect::new(x, 42, w, h), p.base);
        b.fill_rect(Rect::new(x, 42, 1, h), c.light);
    }
    small_button(c, &mut b, 148, 42, BTN_CLOSE, true);
    // Title letters A..Z: cells with their own background between separator columns.
    for (y, color) in [(88, p.accent), (96, p.subtext)] {
        b.fill_rect(Rect::new(0, y, 194, 7), GEN_SEPARATOR);
        let mut x = 1;
        for ch in 'a'..='z' {
            let rows = glyph_rows(ch);
            let w = pattern_size(rows).0 + 2;
            b.fill_rect(Rect::new(x, y, w, 7), p.base);
            pattern(&mut b, rows, x + 1, y + 1, color);
            x += w + 1;
        }
    }
    b
}

fn genex_bmp(c: &Colors) -> Bitmap {
    let p = c.p;
    let mut b = Bitmap::new(130, 75, p.base);
    raised(&mut b, Rect::new(0, 0, 47, 15), p.surface, c.light, c.dark);
    sunken(&mut b, Rect::new(0, 16, 47, 15), c.pressed, c.light, c.dark);
    let colors = [
        p.crust,
        p.text,
        p.base,
        p.text,
        p.text,
        p.overlay,
        p.selection,
        p.surface,
        p.text,
        c.light,
        c.dark,
        c.dark,
        p.base,
        p.surface,
        p.overlay,
        p.accent,
        p.accent,
        p.base,
        p.text,
        p.selection,
        p.text,
        p.selection,
    ];
    for (i, color) in colors.iter().enumerate() {
        b.set(48 + 2 * i as i32, 0, *color);
    }
    b
}

fn rgb_hex(c: u32) -> String {
    format!("#{:06X}", c & 0xFF_FFFF)
}

impl Theme {
    fn colors(&self) -> Colors {
        Colors::new(self.palette)
    }

    /// Every sheet of the skin.
    pub fn sheets(&self) -> BTreeMap<Sheet, Bitmap> {
        let c = self.colors();
        let mut m = BTreeMap::new();
        m.insert(Sheet::Main, main_bmp(&c));
        m.insert(Sheet::Titlebar, titlebar_bmp(&c, self.name));
        m.insert(Sheet::Cbuttons, cbuttons_bmp(&c));
        m.insert(Sheet::Shufrep, shufrep_bmp(&c));
        m.insert(Sheet::Text, text_bmp(&c));
        m.insert(Sheet::Numbers, numbers_bmp(&c, false));
        m.insert(Sheet::NumsEx, numbers_bmp(&c, true));
        m.insert(Sheet::Playpaus, playpaus_bmp(&c));
        m.insert(Sheet::Monoster, monoster_bmp(&c));
        m.insert(Sheet::Posbar, posbar_bmp(&c));
        m.insert(Sheet::Volume, slider_bmp(&c, false));
        m.insert(Sheet::Balance, slider_bmp(&c, true));
        m.insert(Sheet::EqMain, eqmain_bmp(&c));
        m.insert(Sheet::EqEx, eq_ex_bmp(&c));
        m.insert(Sheet::Pledit, pledit_bmp(&c));
        m.insert(Sheet::Gen, gen_bmp(&c));
        m.insert(Sheet::GenEx, genex_bmp(&c));
        m
    }

    /// pledit.txt contents.
    pub fn pledit_txt(&self) -> String {
        let p = self.palette;
        format!(
            "[Text]\r\nNormal={}\r\nCurrent={}\r\nNormalBG={}\r\nSelectedBG={}\r\nFont=DejaVu Sans\r\n",
            rgb_hex(p.text),
            rgb_hex(p.accent),
            rgb_hex(p.crust),
            rgb_hex(p.selection)
        )
    }

    /// viscolor.txt contents: background, grid, 16 spectrum rows (top red to bottom green),
    /// 5 oscilloscope shades, peaks.
    pub fn viscolor_txt(&self) -> String {
        let p = self.palette;
        let mut colors = vec![p.crust, mix(p.crust, p.overlay, 0.6)];
        for i in 0..16 {
            let t = i as f32 / 15.0;
            colors.push(if t < 1.0 / 3.0 {
                mix(p.red, p.orange, t * 3.0)
            } else if t < 2.0 / 3.0 {
                mix(p.orange, p.yellow, t * 3.0 - 1.0)
            } else {
                mix(p.yellow, p.green, t * 3.0 - 2.0)
            });
        }
        for k in 0..5 {
            colors.push(mix(p.text, p.accent, k as f32 * 0.2));
        }
        colors.push(p.text);
        colors
            .iter()
            .map(|c| format!("{},{},{},\r\n", (c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF))
            .collect()
    }

    /// The theme as a ready-to-use skin.
    pub fn skin(&self) -> Skin {
        Skin::from_sheets(self.sheets()).with_text_files(&self.pledit_txt(), &self.viscolor_txt())
    }

    /// The theme as a `.wsz` archive (BMP sheets plus the text files).
    pub fn export_wsz(&self) -> Result<Vec<u8>, crate::SkinError> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (sheet, bitmap) in self.sheets() {
            let img = image::RgbImage::from_fn(bitmap.width(), bitmap.height(), |x, y| {
                let px = bitmap.get(x as i32, y as i32).unwrap_or(0);
                image::Rgb([(px >> 16) as u8, (px >> 8) as u8, px as u8])
            });
            let mut bmp = Cursor::new(Vec::new());
            img.write_to(&mut bmp, image::ImageFormat::Bmp)?;
            zip.start_file(sheet.file_name().to_uppercase(), opts)?;
            zip.write_all(bmp.get_ref())?;
        }
        for (name, body) in [
            ("PLEDIT.TXT", self.pledit_txt()),
            ("VISCOLOR.TXT", self.viscolor_txt()),
            (
                "README.TXT",
                format!(
                    "{} theme for OpenAMP, generated from the {} color scheme.\r\nOriginal artwork drawn by the player's built-in theme generator.\r\n",
                    self.name, self.name
                ),
            ),
        ] {
            zip.start_file(name, opts)?;
            zip.write_all(body.as_bytes())?;
        }
        Ok(zip.finish()?.into_inner())
    }

    /// Contrast ratio between display text and its background (WCAG formula).
    pub fn display_contrast(&self) -> f32 {
        let (a, b) = (luma(self.palette.accent), luma(self.palette.crust));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::{self, GenExColors, GenLetters};
    use crate::sprites::{self, main};

    fn sheet_size(sheet: Sheet) -> (u32, u32) {
        match sheet {
            Sheet::Main => (275, 116),
            Sheet::Cbuttons => (136, 36),
            Sheet::Titlebar => (344, 87),
            Sheet::Shufrep => (92, 85),
            Sheet::Text => (155, 18),
            Sheet::Numbers => (99, 13),
            Sheet::NumsEx => (108, 13),
            Sheet::Playpaus => (42, 9),
            Sheet::Monoster => (58, 24),
            Sheet::Posbar => (307, 10),
            Sheet::Volume | Sheet::Balance => (68, 433),
            Sheet::EqMain => (275, 315),
            Sheet::EqEx => (275, 82),
            Sheet::Pledit => (280, 186),
            Sheet::Gen => (194, 109),
            Sheet::GenEx => (130, 75),
        }
    }

    #[test]
    fn lit_clutter_buttons_line_up_with_the_bar() {
        // Lit cells (sheet x, y, height) and where they sit in the bar (top offset).
        let cells = [
            (304, 47, 8, 3),
            (312, 55, 7, 11),
            (320, 62, 7, 18),
            (328, 69, 8, 25),
            (336, 77, 7, 33),
        ];
        for t in THEMES {
            let sheets = t.sheets();
            let b = &sheets[&Sheet::Titlebar];
            let bar = |x, y| b.get(304 + x, y);
            for (lx, ly, h, top) in cells {
                for dy in 0..h {
                    // The bar's left and right edges show through the lit cell.
                    for x in [0, 7] {
                        assert_eq!(b.get(lx + x, ly + dy), bar(x, top + dy), "{} edge", t.id);
                    }
                }
                // The lit letter is on the same rows as the unlit one.
                let rows = |x0: i32, y0: i32, bg: Option<u32>| -> Vec<i32> {
                    (0..h)
                        .filter(|&dy| (2..6).any(|dx| b.get(x0 + dx, y0 + dy) != bg))
                        .collect()
                };
                let lit = rows(lx, ly, b.get(lx + 1, ly));
                let unlit = rows(304, top, bar(1, top + h / 2));
                assert_eq!(lit, unlit, "{} letter at {lx}", t.id);
                assert!(!lit.is_empty());
            }
        }
    }

    #[test]
    fn every_theme_has_every_sheet_at_classic_size() {
        for t in THEMES {
            let sheets = t.sheets();
            for sheet in Sheet::ALL {
                let b = &sheets[&sheet];
                assert_eq!(
                    (b.width(), b.height()),
                    sheet_size(sheet),
                    "{} {sheet:?}",
                    t.id
                );
            }
        }
    }

    #[test]
    fn every_sprite_fits_and_is_painted() {
        let skin = THEMES[0].skin();
        let all = main::ALL
            .iter()
            .chain(sprites::playlist::ALL)
            .chain(sprites::eq::ALL)
            .chain(generic::ALL);
        for s in all {
            let sheet = skin.sheet(s.sheet).unwrap();
            assert!(sheet.contains_rect(s.rect), "{s:?}");
        }
    }

    #[test]
    fn font_covers_every_mapped_character() {
        for ch in FONT_CHARS.chars() {
            assert!(!glyph_rows(ch).is_empty(), "{ch:?} has no glyph");
            let (w, h) = pattern_size(glyph_rows(ch));
            assert!(w <= 5 && h <= 5, "{ch:?} is {w}x{h}");
        }
        assert!(FONT_CHARS.chars().all(|c| font::glyph(c).is_some()));
    }

    #[test]
    fn generic_letters_and_colors_are_readable() {
        for t in THEMES {
            let skin = t.skin();
            let letters = GenLetters::scan(skin.sheet(Sheet::Gen).unwrap());
            assert_eq!(letters.len(), 26, "{}", t.id);
            let c = GenExColors::from_skin(&skin);
            assert_eq!(c.item_background, t.palette.crust);
            assert_ne!(c.list_text_selected, c.list_text_selected_background);
        }
    }

    #[test]
    fn text_files_parse_to_theme_colors() {
        for t in THEMES {
            let skin = t.skin();
            assert_eq!(skin.playlist_style().normal_bg, t.palette.crust);
            assert_eq!(skin.playlist_style().current, t.palette.accent);
            assert_eq!(skin.vis_colors()[0], t.palette.crust);
            assert_eq!(
                skin.vis_colors()[2],
                t.palette.red,
                "{}: top of the spectrum",
                t.id
            );
        }
    }

    #[test]
    fn displays_have_readable_contrast() {
        for t in THEMES {
            assert!(
                t.display_contrast() >= 4.5,
                "{}: {:.1}",
                t.id,
                t.display_contrast()
            );
        }
    }

    #[test]
    fn export_round_trips_through_the_loader() {
        let t = find("Tokyo Night").unwrap();
        let wsz = t.export_wsz().unwrap();
        let skin = Skin::from_wsz(&wsz).unwrap();
        assert!(skin.warnings().is_empty(), "{:?}", skin.warnings());
        assert_eq!(skin.sheet(Sheet::Main), t.skin().sheet(Sheet::Main));
        assert_eq!(skin.playlist_style(), t.skin().playlist_style());
    }

    #[test]
    fn lookup_by_id_name_and_prefix() {
        assert_eq!(find("dracula").map(|t| t.id), Some("dracula"));
        assert_eq!(find("Catppuccin Mocha").map(|t| t.id), Some("catppuccin"));
        assert_eq!(find("theme:nord").map(|t| t.id), Some("nord"));
        assert_eq!(find("SolarizedDark").map(|t| t.id), Some("solarized"));
        assert!(find("winamp").is_none());
    }
}
