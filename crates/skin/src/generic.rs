//! Generic windows (GEN.BMP and GENEX.BMP): the frame used by the media library and other
//! plugin windows, the title-bar letters, and the list and button colors.
//!
//! Frame coordinates and the GENEX color positions follow the classic format, cross-checked
//! against Webamp (MIT, Copyright Jordan Eldredge). Letter positions are found by scanning
//! GEN.BMP, since their widths vary between skins.

use crate::bitmap::{Rect, rgb};
use crate::sprites::{Sheet, Sprite};
use crate::{Bitmap, Skin};

const fn s(x: i32, y: i32, w: i32, h: i32) -> Sprite {
    Sprite {
        sheet: Sheet::Gen,
        rect: Rect::new(x, y, w, h),
    }
}

pub const TOP_LEFT: Sprite = s(0, 21, 25, 20);
pub const TOP_LEFT_ACTIVE: Sprite = s(0, 0, 25, 20);
pub const TOP_LEFT_END: Sprite = s(26, 21, 25, 20);
pub const TOP_LEFT_END_ACTIVE: Sprite = s(26, 0, 25, 20);
pub const TOP_CENTER_FILL: Sprite = s(52, 21, 25, 20);
pub const TOP_CENTER_FILL_ACTIVE: Sprite = s(52, 0, 25, 20);
pub const TOP_RIGHT_END: Sprite = s(78, 21, 25, 20);
pub const TOP_RIGHT_END_ACTIVE: Sprite = s(78, 0, 25, 20);
pub const TOP_FILL: Sprite = s(104, 21, 25, 20);
pub const TOP_FILL_ACTIVE: Sprite = s(104, 0, 25, 20);
pub const TOP_RIGHT: Sprite = s(130, 21, 25, 20);
pub const TOP_RIGHT_ACTIVE: Sprite = s(130, 0, 25, 20);
pub const BOTTOM_LEFT: Sprite = s(0, 42, 125, 14);
pub const BOTTOM_RIGHT: Sprite = s(0, 57, 125, 14);
pub const BOTTOM_FILL: Sprite = s(127, 72, 25, 14);
pub const MIDDLE_LEFT: Sprite = s(127, 42, 11, 29);
pub const MIDDLE_LEFT_BOTTOM: Sprite = s(158, 42, 11, 24);
pub const MIDDLE_RIGHT: Sprite = s(139, 42, 8, 29);
pub const MIDDLE_RIGHT_BOTTOM: Sprite = s(170, 42, 8, 24);
pub const CLOSE_PRESSED: Sprite = s(148, 42, 9, 9);

pub const ALL: &[Sprite] = &[
    TOP_LEFT,
    TOP_LEFT_ACTIVE,
    TOP_LEFT_END,
    TOP_LEFT_END_ACTIVE,
    TOP_CENTER_FILL,
    TOP_CENTER_FILL_ACTIVE,
    TOP_RIGHT_END,
    TOP_RIGHT_END_ACTIVE,
    TOP_FILL,
    TOP_FILL_ACTIVE,
    TOP_RIGHT,
    TOP_RIGHT_ACTIVE,
    BOTTOM_LEFT,
    BOTTOM_RIGHT,
    BOTTOM_FILL,
    MIDDLE_LEFT,
    MIDDLE_LEFT_BOTTOM,
    MIDDLE_RIGHT,
    MIDDLE_RIGHT_BOTTOM,
    CLOSE_PRESSED,
];

/// Frame thickness: title bar, left, right and bottom.
pub const TOP_H: i32 = 20;
pub const LEFT_W: i32 = 11;
pub const RIGHT_W: i32 = 8;
pub const BOTTOM_H: i32 = 14;

/// Rows of the title letters in GEN.BMP (active, inactive) and their height.
const LETTER_ROWS: [i32; 2] = [88, 96];
const LETTER_H: i32 = 7;

/// Title-bar letters A..Z, active and inactive variants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenLetters {
    active: Vec<Rect>,
    inactive: Vec<Rect>,
}

impl GenLetters {
    /// Finds the letters by scanning GEN.BMP: each letter is a run of columns that differ
    /// from the separator color found at the start of the row.
    pub fn scan(genbmp: &Bitmap) -> Self {
        let row = |y: i32| -> Vec<Rect> {
            let Some(sep) = genbmp.get(0, y + LETTER_H / 2) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            let mut start: Option<i32> = None;
            for x in 0..=genbmp.width() as i32 {
                // Separators are whole columns of the background color; letters have their own
                // background, so the middle row is enough (other rows may touch neighbors).
                let is_letter = genbmp.get(x, y + LETTER_H / 2).is_some_and(|p| p != sep);
                match (is_letter, start) {
                    (true, None) => start = Some(x),
                    (false, Some(s)) => {
                        out.push(Rect::new(s, y, x - s, LETTER_H));
                        start = None;
                    }
                    _ => {}
                }
            }
            out.truncate(26);
            out
        };
        Self {
            active: row(LETTER_ROWS[0]),
            inactive: row(LETTER_ROWS[1]),
        }
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    fn glyph(&self, c: char, active: bool) -> Option<Rect> {
        let i = (c.to_ascii_uppercase() as u32).checked_sub('A' as u32)? as usize;
        let set = if active { &self.active } else { &self.inactive };
        set.get(i).copied()
    }

    /// Width of `text` in letters (spaces are 5 px, unknown characters skipped).
    pub fn width(&self, text: &str) -> i32 {
        text.chars()
            .map(|c| match c {
                ' ' => 5,
                _ => self.glyph(c, true).map_or(0, |r| r.w),
            })
            .sum()
    }

    /// Draws `text` at (x, y) from GEN.BMP.
    pub fn draw(&self, skin: &Skin, dst: &mut Bitmap, text: &str, x: i32, y: i32, active: bool) {
        let Some(genbmp) = skin.sheet(Sheet::Gen) else {
            return;
        };
        let mut cx = x;
        for c in text.chars() {
            if c == ' ' {
                cx += 5;
                continue;
            }
            if let Some(r) = self.glyph(c, active) {
                dst.blit(genbmp, r, cx, y);
                cx += r.w;
            }
        }
    }
}

/// List, window and button colors read from GENEX.BMP's first row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenExColors {
    pub item_background: u32,
    pub item_foreground: u32,
    pub window_background: u32,
    pub button_text: u32,
    pub window_text: u32,
    pub divider: u32,
    pub playlist_selection: u32,
    pub list_header_background: u32,
    pub list_header_text: u32,
    pub list_header_frame_top_left: u32,
    pub list_header_frame_bottom_right: u32,
    pub list_header_frame_pressed: u32,
    pub list_header_dead_area: u32,
    pub scrollbar_one: u32,
    pub scrollbar_two: u32,
    pub pressed_scrollbar_one: u32,
    pub pressed_scrollbar_two: u32,
    pub scrollbar_dead_area: u32,
    pub list_text_highlighted: u32,
    pub list_text_highlighted_background: u32,
    pub list_text_selected: u32,
    pub list_text_selected_background: u32,
}

impl Default for GenExColors {
    /// Close to the stock skin, for skins without GENEX.BMP.
    fn default() -> Self {
        Self {
            item_background: rgb(0, 0, 0),
            item_foreground: rgb(0, 255, 0),
            window_background: rgb(56, 55, 87),
            button_text: rgb(57, 57, 66),
            window_text: rgb(255, 255, 255),
            divider: rgb(117, 116, 139),
            playlist_selection: rgb(0, 0, 198),
            list_header_background: rgb(72, 72, 120),
            list_header_text: rgb(255, 255, 255),
            list_header_frame_top_left: rgb(108, 108, 180),
            list_header_frame_bottom_right: rgb(36, 36, 60),
            list_header_frame_pressed: rgb(18, 18, 30),
            list_header_dead_area: rgb(36, 36, 60),
            scrollbar_one: rgb(36, 36, 60),
            scrollbar_two: rgb(72, 72, 120),
            pressed_scrollbar_one: rgb(121, 130, 150),
            pressed_scrollbar_two: rgb(78, 88, 110),
            scrollbar_dead_area: rgb(36, 36, 60),
            list_text_highlighted: rgb(0, 248, 255),
            list_text_highlighted_background: rgb(0, 0, 198),
            list_text_selected: rgb(255, 255, 255),
            list_text_selected_background: rgb(0, 0, 198),
        }
    }
}

impl GenExColors {
    /// Reads the colors from GENEX.BMP (pixels at x = 48, 50, ... 90 on row 0).
    pub fn from_skin(skin: &Skin) -> Self {
        let Some(genex) = skin.sheet(Sheet::GenEx) else {
            return Self::default();
        };
        let d = Self::default();
        let at = |i: i32, fallback: u32| {
            genex
                .get(48 + 2 * i, 0)
                .map_or(fallback, |p| p | 0xFF00_0000)
        };
        let mut c = Self {
            item_background: at(0, d.item_background),
            item_foreground: at(1, d.item_foreground),
            window_background: at(2, d.window_background),
            button_text: at(3, d.button_text),
            window_text: at(4, d.window_text),
            divider: at(5, d.divider),
            playlist_selection: at(6, d.playlist_selection),
            list_header_background: at(7, d.list_header_background),
            list_header_text: at(8, d.list_header_text),
            list_header_frame_top_left: at(9, d.list_header_frame_top_left),
            list_header_frame_bottom_right: at(10, d.list_header_frame_bottom_right),
            list_header_frame_pressed: at(11, d.list_header_frame_pressed),
            list_header_dead_area: at(12, d.list_header_dead_area),
            scrollbar_one: at(13, d.scrollbar_one),
            scrollbar_two: at(14, d.scrollbar_two),
            pressed_scrollbar_one: at(15, d.pressed_scrollbar_one),
            pressed_scrollbar_two: at(16, d.pressed_scrollbar_two),
            scrollbar_dead_area: at(17, d.scrollbar_dead_area),
            list_text_highlighted: at(18, d.list_text_highlighted),
            list_text_highlighted_background: at(19, d.list_text_highlighted_background),
            list_text_selected: at(20, d.list_text_selected),
            list_text_selected_background: at(21, d.list_text_selected_background),
        };
        // Older skins (including the stock 2.91 one) leave the last colors undefined, so they
        // read as one background color and selected text would vanish. Use the playlist
        // selection color with white text instead.
        if c.list_text_selected == c.list_text_selected_background {
            c.list_text_selected = rgb(255, 255, 255);
            c.list_text_selected_background = c.playlist_selection;
        }
        if c.list_text_highlighted == c.list_text_highlighted_background {
            c.list_text_highlighted = rgb(255, 255, 255);
            c.list_text_highlighted_background = c.playlist_selection;
        }
        c
    }
}

/// GENEX button pieces (unpressed left, center, right; and the pressed background).
pub mod button {
    use super::Rect;
    use crate::sprites::{Sheet, Sprite};

    const fn s(x: i32, y: i32, w: i32, h: i32) -> Sprite {
        Sprite {
            sheet: Sheet::GenEx,
            rect: Rect::new(x, y, w, h),
        }
    }

    pub const LEFT: Sprite = s(0, 0, 4, 15);
    pub const CENTER: Sprite = s(4, 0, 39, 15);
    pub const RIGHT: Sprite = s(43, 0, 4, 15);
    pub const LEFT_PRESSED: Sprite = s(0, 16, 4, 15);
    pub const CENTER_PRESSED: Sprite = s(4, 16, 39, 15);
    pub const RIGHT_PRESSED: Sprite = s(43, 16, 4, 15);
    pub const HEIGHT: i32 = 15;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_found_between_separator_columns() {
        // Separator cyan; three letters of widths 4, 2 and 5 on the active row, 3 on the other.
        let sep = rgb(0, 255, 255);
        let mut genbmp = Bitmap::new(40, 110, sep);
        let mut x = 1;
        for w in [4, 2, 5] {
            genbmp.fill_rect(Rect::new(x, 88, w, 7), rgb(10, 10, 10));
            genbmp.fill_rect(Rect::new(x, 96, w, 7), rgb(20, 20, 20));
            x += w + 1;
        }
        let letters = GenLetters::scan(&genbmp);
        assert_eq!(letters.len(), 3);
        assert_eq!(letters.glyph('a', true), Some(Rect::new(1, 88, 4, 7)));
        assert_eq!(letters.glyph('C', false), Some(Rect::new(9, 96, 5, 7)));
        assert_eq!(letters.glyph('D', true), None);
        assert_eq!(letters.width("AB C"), 4 + 2 + 5 + 5);
    }

    #[test]
    fn genex_colors_from_row_zero_with_defaults() {
        let mut genex = Bitmap::new(130, 75, 0);
        genex.set(48, 0, rgb(1, 2, 3));
        let skin = Skin::from_sheets([(Sheet::GenEx, genex)]);
        let c = GenExColors::from_skin(&skin);
        assert_eq!(c.item_background, rgb(1, 2, 3));
        // Row 0 is otherwise black, so selected text equals its background: fallback applies.
        assert_eq!(c.list_text_selected, rgb(255, 255, 255));
        assert_eq!(c.list_text_selected_background, c.playlist_selection);
        let mut genex = Bitmap::new(130, 75, 0);
        genex.set(88, 0, rgb(9, 9, 9));
        genex.set(90, 0, rgb(4, 5, 6));
        let c = GenExColors::from_skin(&Skin::from_sheets([(Sheet::GenEx, genex)]));
        assert_eq!(
            c.list_text_selected_background,
            rgb(4, 5, 6),
            "defined colors are kept"
        );
        let plain = GenExColors::from_skin(&Skin::from_sheets([]));
        assert_eq!(plain, GenExColors::default());
    }
}
