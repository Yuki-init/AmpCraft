//! The classic generic-window frame (GEN.BMP) and GENEX buttons, shared by the media library
//! and preferences windows.

use amp_skin::generic::{self, GenLetters, button};
use amp_skin::{Bitmap, Rect, Sheet, Skin, Sprite};

/// Close button position in a `w` wide generic window.
pub fn close_rect(w: i32) -> Rect {
    Rect::new(w - 11, 3, 9, 9)
}

/// Draws the frame of a `w` x `h` generic window: title bar with `title` in the GEN.BMP
/// letters, sides and bottom.
pub fn draw_frame(
    b: &mut Bitmap,
    skin: &Skin,
    (w, h): (i32, i32),
    title: &str,
    focused: bool,
    close_pressed: bool,
) {
    let draw = |b: &mut Bitmap, s: Sprite, x: i32, y: i32| skin.draw(b, s, x, y);
    let a = focused;
    let pick = |on: bool, x: Sprite, y: Sprite| if on { x } else { y };
    let letters = skin
        .sheet(Sheet::Gen)
        .map(GenLetters::scan)
        .unwrap_or_default();

    // Title bar: corners, fills, and the title centered between two end caps.
    let title_w = letters.width(title) + 8;
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
    letters.draw(skin, b, title, center_x + 4, 4, a);
    if close_pressed {
        let r = close_rect(w);
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

/// Draws a GENEX button filling `r` (its label is drawn by the caller).
pub fn draw_button(b: &mut Bitmap, skin: &Skin, r: Rect, down: bool) {
    if r.w < 8 {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_skin::{code, coded_skin};

    #[test]
    fn frame_corners_and_button_ends() {
        let skin = coded_skin();
        let mut b = Bitmap::new(300, 200, 0);
        draw_frame(&mut b, &skin, (300, 200), "PREFERENCES", true, false);
        let tl = generic::TOP_LEFT_ACTIVE.rect;
        assert_eq!(b.get(0, 0), Some(code(Sheet::Gen, tl.x, tl.y)));
        let br = generic::BOTTOM_RIGHT.rect;
        assert_eq!(
            b.get(300 - 125, 200 - generic::BOTTOM_H),
            Some(code(Sheet::Gen, br.x, br.y))
        );
        let mut b = Bitmap::new(100, 20, 0);
        draw_button(&mut b, &skin, Rect::new(10, 2, 60, button::HEIGHT), true);
        let l = button::LEFT_PRESSED.rect;
        assert_eq!(b.get(10, 2), Some(code(Sheet::GenEx, l.x, l.y)));
    }
}
