//! Title text: the skin's bitmap font, with a system-font fallback for characters it lacks.
//!
//! TEXT.BMP only covers ASCII letters, digits, some punctuation and a few accented capitals.
//! Titles with anything else (Japanese, most accents) are drawn entirely with a
//! [`TextRenderer`] supplied by the app, tinted with the skin's text colors, as the classic
//! player does when its bitmap font cannot show a title.

use amp_skin::{Bitmap, Rect, Sheet, Skin, font};

/// Coverage mask produced by a fallback renderer: one alpha byte per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

/// Draws text the bitmap font cannot.
pub trait TextRenderer {
    /// Renders `text` into a mask `height` pixels tall, or None if it cannot.
    fn render(&mut self, text: &str, height: u32) -> Option<Mask>;
}

/// A renderer that never renders: titles fall back to the bitmap font, with unknown
/// characters shown as spaces.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFallback;

impl TextRenderer for NoFallback {
    fn render(&mut self, _text: &str, _height: u32) -> Option<Mask> {
        None
    }
}

/// Background and foreground colors of the skin's font, read from TEXT.BMP: the background
/// is the space glyph's color and the foreground the most common other color in the glyphs.
pub fn text_colors(skin: &Skin) -> (u32, u32) {
    let default = (amp_skin::rgb(0, 0, 0), amp_skin::rgb(0, 226, 0));
    let Some(text) = skin.sheet(Sheet::Text) else {
        return default;
    };
    let space = font::glyph_rect(' ');
    let Some(bg) = text.get(space.x + 2, space.y + 2) else {
        return default;
    };
    let mut counts: std::collections::BTreeMap<u32, usize> = Default::default();
    for y in 0..font::CHAR_H * 2 {
        for x in 0..(26 * font::CHAR_W).min(text.width() as i32) {
            if let Some(p) = text.get(x, y).filter(|p| *p != bg) {
                *counts.entry(p).or_default() += 1;
            }
        }
    }
    let fg = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map_or(default.1, |(c, _)| c);
    (bg, fg)
}

fn blend(dst: u32, src: u32, alpha: u8) -> u32 {
    let a = u32::from(alpha);
    let mix = |shift: u32| {
        let d = (dst >> shift) & 0xFF;
        let s = (src >> shift) & 0xFF;
        ((s * a + d * (255 - a)) / 255) << shift
    };
    0xFF00_0000 | mix(16) | mix(8) | mix(0)
}

/// A rendered line of text, ready to be scrolled through a clip box.
#[derive(Debug, Clone)]
pub struct TextStrip {
    pixels: Bitmap,
    /// Pixels where the text was drawn (the rest is transparent).
    used: Vec<bool>,
}

impl TextStrip {
    /// Renders `text` with the skin's bitmap font, or the fallback when the font lacks a
    /// character. `fallback_height` is the pixel size for fallback text.
    pub fn new(
        skin: &Skin,
        fallback: &mut dyn TextRenderer,
        text: &str,
        fallback_height: u32,
    ) -> Self {
        let use_bitmap = font::can_render(text) || skin.sheet(Sheet::Text).is_none();
        if !use_bitmap && let Some(mask) = fallback.render(text, fallback_height) {
            let (_, fg) = text_colors(skin);
            let pixels = Bitmap::new(mask.width, mask.height, fg);
            let used = mask.alpha.iter().map(|a| *a > 0).collect();
            let mut pixels = pixels;
            // Store alpha in the top byte; blended when drawn.
            for (p, a) in pixels.pixels_mut().iter_mut().zip(&mask.alpha) {
                *p = (*p & 0x00FF_FFFF) | u32::from(*a) << 24;
            }
            return Self { pixels, used };
        }
        let width = font::text_width(text).max(0) as u32;
        let mut pixels = Bitmap::new(width, font::CHAR_H as u32, 0);
        skin.draw_text(&mut pixels, text, 0, 0);
        let used = vec![true; pixels.pixels().len()];
        // Bitmap-font pixels are opaque.
        for p in pixels.pixels_mut() {
            *p |= 0xFF00_0000;
        }
        Self { pixels, used }
    }

    pub fn width(&self) -> i32 {
        self.pixels.width() as i32
    }

    pub fn height(&self) -> i32 {
        self.pixels.height() as i32
    }

    /// Draws the strip into `dst` clipped to `clip`, starting `offset` pixels into the strip
    /// and wrapping around with `gap` blank pixels between repeats when `wrap` is set.
    pub fn draw(&self, dst: &mut Bitmap, clip: Rect, y: i32, offset: i32, wrap: bool, gap: i32) {
        let period = self.width() + gap;
        for dx in 0..clip.w {
            let sx = if wrap && period > 0 {
                (offset + dx).rem_euclid(period)
            } else {
                offset + dx
            };
            if sx < 0 || sx >= self.width() {
                continue;
            }
            for sy in 0..self.height() {
                let (x, yy) = (clip.x + dx, y + sy);
                if yy < clip.y || yy >= clip.y + clip.h {
                    continue;
                }
                let i = (sy * self.width() + sx) as usize;
                if !self.used[i] {
                    continue;
                }
                let src = self.pixels.pixels()[i];
                let alpha = (src >> 24) as u8;
                if alpha == 0xFF {
                    dst.set(x, yy, src);
                } else if alpha > 0
                    && let Some(d) = dst.get(x, yy)
                {
                    dst.set(x, yy, blend(d, src, alpha));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fallback that draws each character as a 3-wide solid block.
    struct Blocks;

    impl TextRenderer for Blocks {
        fn render(&mut self, text: &str, height: u32) -> Option<Mask> {
            let width = text.chars().count() as u32 * 4;
            let mut alpha = vec![0; (width * height) as usize];
            for (i, _) in text.chars().enumerate() {
                for y in 0..height {
                    for x in 0..3 {
                        alpha[(y * width + i as u32 * 4 + x) as usize] = 255;
                    }
                }
            }
            Some(Mask {
                width,
                height,
                alpha,
            })
        }
    }

    fn skin_with_font() -> Skin {
        // TEXT.BMP: black background, green glyph pixels in the top-left of each cell.
        let mut text = Bitmap::new(155, 18, amp_skin::rgb(0, 0, 0));
        for row in 0..3 {
            for col in 0..30 {
                text.set(col * 5, row * 6, amp_skin::rgb(0, 200, 0));
            }
        }
        crate::test_skin::skin_from(&[(Sheet::Text, text)])
    }

    #[test]
    fn colors_come_from_text_bmp() {
        let (bg, fg) = text_colors(&skin_with_font());
        assert_eq!(bg, amp_skin::rgb(0, 0, 0));
        assert_eq!(fg, amp_skin::rgb(0, 200, 0));
    }

    #[test]
    fn ascii_uses_bitmap_font() {
        let skin = skin_with_font();
        let strip = TextStrip::new(&skin, &mut Blocks, "abc", 11);
        assert_eq!((strip.width(), strip.height()), (15, 6));
    }

    #[test]
    fn unsupported_text_uses_fallback_in_skin_color() {
        let skin = skin_with_font();
        let strip = TextStrip::new(&skin, &mut Blocks, "日本", 11);
        assert_eq!((strip.width(), strip.height()), (8, 11));
        let mut dst = Bitmap::new(20, 20, amp_skin::rgb(9, 9, 9));
        strip.draw(&mut dst, Rect::new(0, 0, 20, 20), 2, 0, false, 0);
        assert_eq!(dst.get(0, 2), Some(amp_skin::rgb(0, 200, 0)));
        assert_eq!(
            dst.get(3, 2),
            Some(amp_skin::rgb(9, 9, 9)),
            "gap left untouched"
        );
    }

    #[test]
    fn without_fallback_unknown_chars_become_spaces() {
        let skin = skin_with_font();
        let strip = TextStrip::new(&skin, &mut NoFallback, "a日", 11);
        assert_eq!(strip.width(), 10);
    }

    #[test]
    fn draw_clips_and_wraps() {
        let skin = skin_with_font();
        let strip = TextStrip::new(&skin, &mut Blocks, "日", 4);
        let mut dst = Bitmap::new(20, 10, 0);
        // Clip box 10 wide, strip 4 wide plus 2 gap: repeats at 0, 6.
        strip.draw(&mut dst, Rect::new(5, 0, 10, 10), 0, 0, true, 2);
        let row: Vec<bool> = (0..20).map(|x| dst.get(x, 0) != Some(0)).collect();
        let expected: Vec<bool> = (0..20)
            .map(|x| (5..15).contains(&x) && matches!((x - 5) % 6, 0..=2))
            .collect();
        assert_eq!(row, expected);
    }

    #[test]
    fn blend_mixes_channels() {
        assert_eq!(blend(0xFF00_0000, 0x00FF_FFFF, 255), 0xFFFF_FFFF);
        assert_eq!(blend(0xFF00_0000, 0x00FF_FFFF, 0), 0xFF00_0000);
        assert_eq!(blend(0xFF00_0000, 0x00FE_FEFE, 128) & 0xFF, 127);
    }
}
