//! Integer screen geometry. Rectangles are half-open: `x..x + w` by `y..y + h`.

/// A point in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A size in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }
}

/// An axis-aligned rectangle in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn left(&self) -> i32 {
        self.x
    }

    pub const fn top(&self) -> i32 {
        self.y
    }

    /// One past the last column.
    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    /// One past the last row.
    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub const fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    pub const fn translated(&self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub const fn with_origin(&self, p: Point) -> Self {
        Self::new(p.x, p.y, self.w, self.h)
    }

    pub const fn with_size(&self, s: Size) -> Self {
        Self::new(self.x, self.y, s.w, s.h)
    }

    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.left() && p.x < self.right() && p.y >= self.top() && p.y < self.bottom()
    }

    /// Overlap length of the horizontal spans (negative when apart).
    pub fn x_overlap(&self, other: &Rect) -> i32 {
        self.right().min(other.right()) - self.left().max(other.left())
    }

    /// Overlap length of the vertical spans (negative when apart).
    pub fn y_overlap(&self, other: &Rect) -> i32 {
        self.bottom().min(other.bottom()) - self.top().max(other.top())
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x_overlap(other) > 0 && self.y_overlap(other) > 0
    }

    /// Smallest rectangle containing both.
    pub fn union(&self, other: &Rect) -> Rect {
        let l = self.left().min(other.left());
        let t = self.top().min(other.top());
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(l, t, r - l, b - t)
    }
}

/// Which side of a rectangle another one is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The side of `a` that `b` is flush against, if they share an edge segment of positive length.
/// Touching only at a corner does not count.
pub fn attached_side(a: &Rect, b: &Rect) -> Option<Side> {
    if a.y_overlap(b) > 0 {
        if a.right() == b.left() {
            return Some(Side::Right);
        }
        if a.left() == b.right() {
            return Some(Side::Left);
        }
    }
    if a.x_overlap(b) > 0 {
        if a.bottom() == b.top() {
            return Some(Side::Bottom);
        }
        if a.top() == b.bottom() {
            return Some(Side::Top);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_containment_are_half_open() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!((r.left(), r.top(), r.right(), r.bottom()), (10, 20, 40, 60));
        assert!(r.contains(Point::new(10, 20)));
        assert!(r.contains(Point::new(39, 59)));
        assert!(!r.contains(Point::new(40, 59)));
        assert!(!r.contains(Point::new(39, 60)));
    }

    #[test]
    fn overlap_and_intersection() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.x_overlap(&Rect::new(5, 50, 10, 10)), 5);
        assert_eq!(a.x_overlap(&Rect::new(10, 0, 10, 10)), 0);
        assert_eq!(a.x_overlap(&Rect::new(13, 0, 10, 10)), -3);
        assert!(a.intersects(&Rect::new(9, 9, 5, 5)));
        assert!(!a.intersects(&Rect::new(10, 0, 5, 5)));
    }

    #[test]
    fn union_covers_both() {
        let u = Rect::new(0, 0, 10, 10).union(&Rect::new(20, -5, 5, 5));
        assert_eq!(u, Rect::new(0, -5, 25, 15));
    }

    #[test]
    fn attached_sides() {
        let a = Rect::new(0, 0, 275, 116);
        assert_eq!(
            attached_side(&a, &Rect::new(0, 116, 275, 116)),
            Some(Side::Bottom)
        );
        assert_eq!(
            attached_side(&a, &Rect::new(0, -116, 275, 116)),
            Some(Side::Top)
        );
        assert_eq!(
            attached_side(&a, &Rect::new(275, 50, 100, 100)),
            Some(Side::Right)
        );
        assert_eq!(
            attached_side(&a, &Rect::new(-100, 0, 100, 10)),
            Some(Side::Left)
        );
    }

    #[test]
    fn corner_contact_and_gaps_are_not_attached() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(attached_side(&a, &Rect::new(10, 10, 5, 5)), None);
        assert_eq!(attached_side(&a, &Rect::new(11, 0, 5, 5)), None);
        assert_eq!(attached_side(&a, &Rect::new(0, 11, 5, 5)), None);
        assert_eq!(attached_side(&a, &Rect::new(-5, -5, 5, 5)), None);
    }
}
