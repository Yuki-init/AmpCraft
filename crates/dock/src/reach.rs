//! Keeping panels reachable.
//!
//! A panel is reachable when, on at least one screen, its top edge (the title bar) is not
//! above the screen's top and at least `min_visible` of it lies inside the screen. A group is
//! reachable when every member is. Moves that would strand a panel are clamped to the nearest
//! reachable position instead.

use crate::geom::{Rect, Size};

/// Default part of a panel that must stay on screen at 1x scale: enough of the 14 px title
/// bar to grab.
pub const DEFAULT_MIN_VISIBLE: Size = Size::new(28, 14);

/// Range of translations (inclusive) that keep one rectangle reachable on one screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeltaBox {
    dx_lo: i32,
    dx_hi: i32,
    dy_lo: i32,
    dy_hi: i32,
}

impl DeltaBox {
    fn for_screen(r: &Rect, s: &Rect, min: Size) -> Option<Self> {
        // Never require more than the panel or the screen actually has.
        let mw = min.w.min(r.w).min(s.w).max(1);
        let mh = min.h.min(r.h).min(s.h).max(1);
        let b = Self {
            dx_lo: s.left() + mw - r.right(),
            dx_hi: s.right() - mw - r.left(),
            dy_lo: s.top() - r.top(),
            dy_hi: s.bottom() - mh - r.top(),
        };
        (b.dx_lo <= b.dx_hi && b.dy_lo <= b.dy_hi).then_some(b)
    }

    fn contains(&self, dx: i32, dy: i32) -> bool {
        (self.dx_lo..=self.dx_hi).contains(&dx) && (self.dy_lo..=self.dy_hi).contains(&dy)
    }

    fn clamp(&self, dx: i32, dy: i32) -> (i32, i32) {
        (
            dx.clamp(self.dx_lo, self.dx_hi),
            dy.clamp(self.dy_lo, self.dy_hi),
        )
    }

    fn intersect(&self, o: &Self) -> Option<Self> {
        let b = Self {
            dx_lo: self.dx_lo.max(o.dx_lo),
            dx_hi: self.dx_hi.min(o.dx_hi),
            dy_lo: self.dy_lo.max(o.dy_lo),
            dy_hi: self.dy_hi.min(o.dy_hi),
        };
        (b.dx_lo <= b.dx_hi && b.dy_lo <= b.dy_hi).then_some(b)
    }
}

/// Whether `r` translated by (dx, dy) is reachable on some screen.
fn reachable_at(r: &Rect, screens: &[Rect], min: Size, dx: i32, dy: i32) -> bool {
    screens
        .iter()
        .filter_map(|s| DeltaBox::for_screen(r, s, min))
        .any(|b| b.contains(dx, dy))
}

/// Whether `r` is reachable where it is. Always true when there are no screens.
pub fn is_reachable(r: &Rect, screens: &[Rect], min: Size) -> bool {
    screens.is_empty() || reachable_at(r, screens, min, 0, 0)
}

/// Whether every rectangle in `group` is reachable after moving by (dx, dy).
pub fn group_reachable_at(group: &[Rect], screens: &[Rect], min: Size, dx: i32, dy: i32) -> bool {
    screens.is_empty() || group.iter().all(|r| reachable_at(r, screens, min, dx, dy))
}

/// The translation closest to (dx, dy) that leaves every rectangle in `group` reachable.
/// Returns (dx, dy) itself when it is already fine, and None when no candidate works (for
/// example a group larger than any screen).
pub fn constrain(
    group: &[Rect],
    screens: &[Rect],
    min: Size,
    dx: i32,
    dy: i32,
) -> Option<(i32, i32)> {
    if group_reachable_at(group, screens, min, dx, dy) {
        return Some((dx, dy));
    }
    let mut candidates = Vec::new();
    // Whole group on one screen.
    for s in screens {
        let mut acc: Option<DeltaBox> = None;
        let mut ok = true;
        for r in group {
            let Some(b) = DeltaBox::for_screen(r, s, min) else {
                ok = false;
                break;
            };
            acc = match acc {
                None => Some(b),
                Some(a) => match a.intersect(&b) {
                    Some(i) => Some(i),
                    None => {
                        ok = false;
                        break;
                    }
                },
            };
        }
        if let (true, Some(b)) = (ok, acc) {
            candidates.push(b.clamp(dx, dy));
        }
    }
    // Groups spanning monitors: pull one member onto one screen and see if the rest fit.
    for r in group {
        for s in screens {
            if let Some(b) = DeltaBox::for_screen(r, s, min) {
                candidates.push(b.clamp(dx, dy));
            }
        }
    }
    candidates
        .into_iter()
        .filter(|(cx, cy)| group_reachable_at(group, screens, min, *cx, *cy))
        .min_by_key(|(cx, cy)| {
            let (ex, ey) = (i64::from(cx - dx), i64::from(cy - dy));
            ex * ex + ey * ey
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Size = DEFAULT_MIN_VISIBLE;
    const SCREEN: Rect = Rect::new(0, 0, 1920, 1080);
    const MAIN: Rect = Rect::new(100, 100, 275, 116);

    #[test]
    fn on_screen_is_reachable() {
        assert!(is_reachable(&MAIN, &[SCREEN], M));
        assert!(
            is_reachable(&MAIN, &[], M),
            "no screens means no constraint"
        );
    }

    #[test]
    fn mostly_off_screen_but_grabbable_is_reachable() {
        // Only 28 px showing on the right edge, 14 px at the bottom.
        assert!(is_reachable(
            &Rect::new(1920 - 28, 1080 - 14, 275, 116),
            &[SCREEN],
            M
        ));
        assert!(is_reachable(
            &Rect::new(28 - 275, 500, 275, 116),
            &[SCREEN],
            M
        ));
    }

    #[test]
    fn hidden_title_bar_or_too_little_showing_is_not() {
        assert!(!is_reachable(&Rect::new(100, -1, 275, 116), &[SCREEN], M));
        assert!(!is_reachable(
            &Rect::new(1920 - 27, 500, 275, 116),
            &[SCREEN],
            M
        ));
        assert!(!is_reachable(
            &Rect::new(100, 1080 - 13, 275, 116),
            &[SCREEN],
            M
        ));
        assert!(!is_reachable(
            &Rect::new(5000, 5000, 275, 116),
            &[SCREEN],
            M
        ));
    }

    #[test]
    fn constrain_clamps_to_nearest_edge() {
        assert_eq!(
            constrain(&[MAIN], &[SCREEN], M, 5000, 0),
            Some((1920 - 28 - 100, 0))
        );
        assert_eq!(constrain(&[MAIN], &[SCREEN], M, 0, -500), Some((0, -100)));
        assert_eq!(
            constrain(&[MAIN], &[SCREEN], M, -5000, 5000),
            Some((28 - 375, 1080 - 14 - 100))
        );
        assert_eq!(constrain(&[MAIN], &[SCREEN], M, 30, 40), Some((30, 40)));
    }

    #[test]
    fn group_limited_by_most_constrained_member() {
        let group = [
            MAIN,
            Rect::new(100, 216, 275, 116),
            Rect::new(100, 332, 275, 232),
        ];
        // Moving down: the playlist hits the bottom limit last, the main window's top first
        // when moving up.
        assert_eq!(
            constrain(&group, &[SCREEN], M, 0, 5000),
            Some((0, 1080 - 14 - 332))
        );
        assert_eq!(constrain(&group, &[SCREEN], M, 0, -5000), Some((0, -100)));
    }

    #[test]
    fn dead_zone_between_monitors_clamps_onto_one() {
        // Right monitor is shorter: below 768 on the right side is not on any screen.
        let screens = [SCREEN, Rect::new(1920, 0, 1024, 768)];
        let r = Rect::new(1000, 500, 275, 116);
        // Would land at x=2500, y=900: off the right monitor's bottom.
        let (dx, dy) = constrain(&[r], &screens, M, 1500, 400).unwrap();
        assert_eq!((dx, dy), (1500, 768 - 14 - 500));
        // Fully onto the right monitor is fine.
        assert_eq!(constrain(&[r], &screens, M, 1500, 0), Some((1500, 0)));
    }

    #[test]
    fn group_spanning_two_monitors_is_allowed() {
        let screens = [SCREEN, Rect::new(1920, 0, 1920, 1080)];
        let group = [
            Rect::new(1800, 100, 275, 116),
            Rect::new(2075, 100, 275, 116),
        ];
        assert!(group_reachable_at(&group, &screens, M, 0, 0));
        assert_eq!(constrain(&group, &screens, M, 0, 10), Some((0, 10)));
    }

    #[test]
    fn tiny_screen_requirements_shrink() {
        // Panel narrower than the minimum still counts when fully visible.
        let small = Rect::new(10, 10, 20, 10);
        assert!(is_reachable(&small, &[SCREEN], M));
    }

    #[test]
    fn impossible_group_returns_none() {
        // Two members stacked 3000 px apart cannot both be on a 1080 px tall screen.
        let group = [Rect::new(0, 0, 100, 100), Rect::new(0, 3000, 100, 100)];
        assert_eq!(constrain(&group, &[SCREEN], M, 0, 0), None);
    }
}
