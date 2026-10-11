//! The `TEXT.BMP` bitmap font: 5x6 glyphs in three rows of 31.
//!
//! The character map follows the classic skin format, cross-checked against Webamp's
//! `FONT_LOOKUP` (MIT, Copyright Jordan Eldredge). Letters have one case. Characters the font
//! cannot show are reported by [`glyph`] so the UI can fall back to a system font.

use crate::bitmap::{Bitmap, Rect};

pub const CHAR_W: i32 = 5;
pub const CHAR_H: i32 = 6;

/// (row, column) of `c` in TEXT.BMP, or None if the font has no glyph for it.
pub fn glyph(c: char) -> Option<(i32, i32)> {
    let c = c.to_lowercase().next().unwrap_or(c);
    let pos = match c {
        'a'..='z' => (0, c as i32 - 'a' as i32),
        '"' => (0, 26),
        '@' => (0, 27),
        ' ' => (0, 30),
        '0'..='9' => (1, c as i32 - '0' as i32),
        '…' => (1, 10),
        '.' => (1, 11),
        ':' => (1, 12),
        '(' => (1, 13),
        ')' => (1, 14),
        '-' => (1, 15),
        '\'' => (1, 16),
        '!' => (1, 17),
        '_' => (1, 18),
        '+' => (1, 19),
        '\\' => (1, 20),
        '/' => (1, 21),
        '[' | '<' | '{' => (1, 22),
        ']' | '>' | '}' => (1, 23),
        '^' => (1, 24),
        '&' => (1, 25),
        '%' => (1, 26),
        ',' => (1, 27),
        '=' => (1, 28),
        '$' => (1, 29),
        '#' => (1, 30),
        'å' => (2, 0),
        'ö' => (2, 1),
        'ä' => (2, 2),
        '?' => (2, 3),
        '*' => (2, 4),
        _ => return None,
    };
    Some(pos)
}

/// Whether every character of `text` has a glyph.
pub fn can_render(text: &str) -> bool {
    text.chars().all(|c| glyph(c).is_some())
}

/// Sprite rectangle for `c`. Characters without a glyph render as a space.
pub fn glyph_rect(c: char) -> Rect {
    let (row, col) = glyph(c).unwrap_or((0, 30));
    Rect::new(col * CHAR_W, row * CHAR_H, CHAR_W, CHAR_H)
}

/// Width in pixels of `text` in this font.
pub fn text_width(text: &str) -> i32 {
    text.chars().count() as i32 * CHAR_W
}

/// Draws `text` at (x, y). Returns the width drawn.
pub fn draw_text(dst: &mut Bitmap, font: &Bitmap, text: &str, x: i32, y: i32) -> i32 {
    let mut cx = x;
    for c in text.chars() {
        dst.blit(font, glyph_rect(c), cx, y);
        cx += CHAR_W;
    }
    cx - x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_have_one_case() {
        assert_eq!(glyph('a'), Some((0, 0)));
        assert_eq!(glyph('Z'), Some((0, 25)));
        assert_eq!(glyph('Ö'), glyph('ö'));
        assert_eq!(glyph('Å'), Some((2, 0)));
    }

    #[test]
    fn punctuation_and_aliases() {
        assert_eq!(glyph('9'), Some((1, 9)));
        assert_eq!(glyph('…'), Some((1, 10)));
        assert_eq!(glyph('<'), glyph('['));
        assert_eq!(glyph('}'), glyph(']'));
        assert_eq!(glyph('#'), Some((1, 30)));
        assert_eq!(glyph('*'), Some((2, 4)));
    }

    #[test]
    fn unsupported_characters() {
        assert_eq!(glyph('日'), None);
        assert_eq!(glyph('é'), None);
        assert!(can_render("Hello, World! 1:23"));
        assert!(!can_render("ハレ晴レユカイ"));
        assert_eq!(glyph_rect('日'), glyph_rect(' '));
    }

    #[test]
    fn rects_and_width() {
        assert_eq!(glyph_rect('b'), Rect::new(5, 0, 5, 6));
        assert_eq!(glyph_rect('1'), Rect::new(5, 6, 5, 6));
        assert_eq!(glyph_rect('?'), Rect::new(15, 12, 5, 6));
        assert_eq!(text_width("abc"), 15);
        assert_eq!(text_width("日本"), 10, "width counts characters, not bytes");
    }
}
