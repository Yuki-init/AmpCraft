//! Edge snapping.
//!
//! A moving rectangle snaps when one of its edges comes within the snap distance of:
//! - the facing edge of another panel (right to left, bottom to top, and so on), when the two
//!   are close on the other axis, so they dock side by side;
//! - the same edge of a panel it is stacked against, so stacked panels line up;
//! - the inside edge of a screen.
//!
//! Each axis is corrected independently, picking the smallest correction found.

use crate::geom::Rect;

/// Classic snap distance in pixels at 1x scale.
pub const DEFAULT_SNAP_DISTANCE: i32 = 10;

/// Keeps the smallest correction within `dist`.
#[derive(Debug, Clone, Copy)]
struct Best {
    dist: i32,
    offset: Option<i32>,
}

impl Best {
    fn new(dist: i32) -> Self {
        Self { dist, offset: None }
    }

    fn offer(&mut self, offset: i32) {
        if offset.abs() > self.dist {
            return;
        }
        if self.offset.is_none_or(|o| offset.abs() < o.abs()) {
            self.offset = Some(offset);
        }
    }

    fn get(self) -> i32 {
        self.offset.unwrap_or(0)
    }
}

/// The (dx, dy) to add to every rectangle in `moving` so that the group snaps to `targets`
/// or `screens`. Returns (0, 0) when nothing is within `dist`.
pub fn snap_offset(moving: &[Rect], targets: &[Rect], screens: &[Rect], dist: i32) -> (i32, i32) {
    let mut bx = Best::new(dist);
    let mut by = Best::new(dist);

    for m in moving {
        for t in targets {
            // Close enough vertically to dock side by side.
            if m.y_overlap(t) >= -dist {
                bx.offer(t.left() - m.right());
                bx.offer(t.right() - m.left());
            }
            // Stacked on top of each other: line up the left or right edges.
            if (m.top() - t.bottom()).abs() <= dist || (m.bottom() - t.top()).abs() <= dist {
                bx.offer(t.left() - m.left());
                bx.offer(t.right() - m.right());
            }
            // The same two rules with the axes swapped.
            if m.x_overlap(t) >= -dist {
                by.offer(t.top() - m.bottom());
                by.offer(t.bottom() - m.top());
            }
            if (m.left() - t.right()).abs() <= dist || (m.right() - t.left()).abs() <= dist {
                by.offer(t.top() - m.top());
                by.offer(t.bottom() - m.bottom());
            }
        }
        for s in screens {
            if m.y_overlap(s) > 0 {
                bx.offer(s.left() - m.left());
                bx.offer(s.right() - m.right());
            }
            if m.x_overlap(s) > 0 {
                by.offer(s.top() - m.top());
                by.offer(s.bottom() - m.bottom());
            }
        }
    }
    (bx.get(), by.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: i32 = DEFAULT_SNAP_DISTANCE;
    const MAIN: Rect = Rect::new(100, 100, 275, 116);

    fn snap(m: Rect, targets: &[Rect]) -> (i32, i32) {
        snap_offset(&[m], targets, &[], D)
    }

    #[test]
    fn snaps_below_and_aligns_left_edges() {
        // EQ dropped 6 px below main and 4 px to the right.
        let eq = Rect::new(104, 222, 275, 116);
        assert_eq!(snap(eq, &[MAIN]), (-4, -6));
    }

    #[test]
    fn snaps_above() {
        let pl = Rect::new(100, -20, 275, 116);
        assert_eq!(snap(pl, &[MAIN]), (0, 4));
    }

    #[test]
    fn snaps_to_the_right_and_left() {
        let right = Rect::new(382, 100, 275, 116);
        assert_eq!(snap(right, &[MAIN]), (-7, 0));
        let left = Rect::new(-170, 103, 275, 116);
        assert_eq!(snap(left, &[MAIN]), (-5, -3));
    }

    #[test]
    fn aligns_right_edges_when_stacked() {
        // Narrower panel stacked below, right edge 3 px short of main's right edge.
        let p = Rect::new(197, 216, 175, 50);
        assert_eq!(snap(p, &[MAIN]), (3, 0));
    }

    #[test]
    fn nothing_beyond_snap_distance() {
        assert_eq!(snap(Rect::new(100, 216 + D + 1, 275, 116), &[MAIN]), (0, 0));
        assert_eq!(snap(Rect::new(375 + D + 1, 100, 275, 116), &[MAIN]), (0, 0));
    }

    #[test]
    fn exactly_at_snap_distance_still_snaps() {
        assert_eq!(snap(Rect::new(100, 216 + D, 275, 116), &[MAIN]), (0, -D));
    }

    #[test]
    fn far_apart_on_other_axis_does_not_dock() {
        // Horizontally adjacent but 300 px lower: no side-by-side docking.
        assert_eq!(snap(Rect::new(378, 600, 275, 116), &[MAIN]), (0, 0));
    }

    #[test]
    fn picks_the_nearest_candidate() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(0, 200, 100, 100);
        // Moving panel between them, 2 px from b and 8 px from a.
        let m = Rect::new(0, 108, 100, 90);
        assert_eq!(snap(m, &[a, b]), (0, 2));
    }

    #[test]
    fn snaps_to_screen_inside_edges() {
        let screen = Rect::new(0, 0, 1920, 1080);
        let m = Rect::new(6, 1080 - 116 - 9, 275, 116);
        assert_eq!(snap_offset(&[m], &[], &[screen], D), (-6, 9));
        let m = Rect::new(1920 - 275 + 4, -8, 275, 116);
        assert_eq!(snap_offset(&[m], &[], &[screen], D), (-4, 8));
    }

    #[test]
    fn second_monitor_edges() {
        let screens = [Rect::new(0, 0, 1920, 1080), Rect::new(1920, 0, 1280, 1024)];
        let m = Rect::new(1925, 500, 275, 116);
        assert_eq!(snap_offset(&[m], &[], &screens, D), (-5, 0));
    }

    #[test]
    fn group_snaps_by_any_member() {
        let target = Rect::new(1000, 116, 275, 116);
        let group = [Rect::new(0, 0, 275, 116), Rect::new(0, 116, 275, 116)];
        // The group's second member overlaps the target's left edge by 5 px.
        let moved: Vec<Rect> = group.iter().map(|r| r.translated(730, 0)).collect();
        assert_eq!(snap_offset(&moved, &[target], &[], D), (-5, 0));
    }

    #[test]
    fn zero_distance_disables_snapping() {
        let eq = Rect::new(104, 222, 275, 116);
        assert_eq!(snap_offset(&[eq], &[MAIN], &[], 0), (0, 0));
    }
}
