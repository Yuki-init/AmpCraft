//! Resize grips: the bottom-right corner handle and the right and bottom edges of a
//! resizable window (playlist, media library). Sizes snap to the classic steps.

use amp_dock::Point;
use amp_skin::Rect;

/// How far in from the right and bottom edges a press resizes.
pub const EDGE: i32 = 3;

/// Which part of the window a resize drags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grip {
    Right,
    Bottom,
    Corner,
}

impl Grip {
    /// Whether the drag changes the width and the height.
    pub fn axes(self) -> (bool, bool) {
        match self {
            Grip::Right => (true, false),
            Grip::Bottom => (false, true),
            Grip::Corner => (true, true),
        }
    }
}

/// The grip under `p` in a `w` x `h` window whose corner handle is `corner`. The right edge
/// starts below the title bar (`top`), leaving its buttons alone.
pub fn grip_at(w: i32, h: i32, p: Point, corner: Rect, top: i32) -> Option<Grip> {
    let in_corner = p.x >= corner.x
        && p.x < corner.x + corner.w
        && p.y >= corner.y
        && p.y < corner.y + corner.h;
    if in_corner {
        return Some(Grip::Corner);
    }
    let right = p.x >= w - EDGE && p.x < w && p.y >= top && p.y < h;
    let bottom = p.y >= h - EDGE && p.y < h && p.x >= 0 && p.x < w;
    match (right, bottom) {
        (true, true) => Some(Grip::Corner),
        (true, false) => Some(Grip::Right),
        (false, true) => Some(Grip::Bottom),
        _ => None,
    }
}

fn snap(v: i32, min: i32, step: i32) -> i32 {
    min + ((v - min).max(0) / step) * step
}

/// A resize in progress: where it started and the size then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resize {
    pub grip: Grip,
    start: Point,
    w0: i32,
    h0: i32,
}

impl Resize {
    pub fn new(grip: Grip, start: Point, w: i32, h: i32) -> Self {
        Self {
            grip,
            start,
            w0: w,
            h0: h,
        }
    }

    /// The size for the pointer at `p`, snapped to `min + n * step` on each axis the grip
    /// changes.
    pub fn size(
        &self,
        p: Point,
        (min_w, step_w): (i32, i32),
        (min_h, step_h): (i32, i32),
    ) -> (i32, i32) {
        let (dw, dh) = self.grip.axes();
        let w = if dw {
            snap(self.w0 + p.x - self.start.x, min_w, step_w)
        } else {
            self.w0
        };
        let h = if dh {
            snap(self.h0 + p.y - self.start.y, min_h, step_h)
        } else {
            self.h0
        };
        (w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORNER: Rect = Rect::new(80, 80, 20, 20);

    #[test]
    fn grips_by_position() {
        let at = |x, y| grip_at(100, 100, Point::new(x, y), CORNER, 20);
        assert_eq!(at(85, 85), Some(Grip::Corner));
        assert_eq!(at(98, 50), Some(Grip::Right));
        assert_eq!(at(98, 10), None, "title bar buttons stay clickable");
        assert_eq!(at(40, 99), Some(Grip::Bottom));
        assert_eq!(at(50, 50), None);
        assert_eq!(at(100, 50), None, "outside");
    }

    #[test]
    fn edges_resize_one_axis() {
        let steps = ((275, 25), (116, 29));
        let r = Resize::new(Grip::Right, Point::new(0, 0), 300, 145);
        assert_eq!(r.size(Point::new(30, 90), steps.0, steps.1), (325, 145));
        let b = Resize::new(Grip::Bottom, Point::new(0, 0), 300, 145);
        assert_eq!(b.size(Point::new(90, 30), steps.0, steps.1), (300, 174));
        let c = Resize::new(Grip::Corner, Point::new(0, 0), 300, 145);
        assert_eq!(
            c.size(Point::new(-500, 30), steps.0, steps.1),
            (275, 174),
            "minimum"
        );
    }
}
