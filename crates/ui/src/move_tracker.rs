//! Telling our own window moves apart from moves made by the window manager.
//!
//! Every move we request comes back later as a position report from the server, and during a
//! drag several requests are in flight at once. A report that matches a request still in
//! flight is just that request landing. A report that matches none means someone else (the
//! window manager) placed the window, and the layout has to follow. Overrides are kept until
//! collected with [`MoveTracker::take_overrides`] or superseded by a newer request, because
//! during a drag the app must not act on them until the drag ends.

use std::collections::{BTreeMap, VecDeque};

use amp_dock::Point;

/// Most requests kept per window; older ones are assumed lost.
const MAX_IN_FLIGHT: usize = 64;

#[derive(Debug, Clone)]
pub struct MoveTracker<K: Ord> {
    in_flight: BTreeMap<K, VecDeque<Point>>,
    overrides: BTreeMap<K, Point>,
}

impl<K: Ord> Default for MoveTracker<K> {
    fn default() -> Self {
        Self {
            in_flight: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }
}

impl<K: Ord + Copy> MoveTracker<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call when asking the platform to move `window` to `pos`.
    pub fn requested(&mut self, window: K, pos: Point) {
        // A newer request supersedes any override seen so far.
        self.overrides.remove(&window);
        let q = self.in_flight.entry(window).or_default();
        if q.len() == MAX_IN_FLIGHT {
            q.pop_front();
        }
        q.push_back(pos);
    }

    /// Call when the platform reports `window` at `pos`. Returns true if the move was not ours.
    pub fn observed(&mut self, window: K, pos: Point) -> bool {
        let q = self.in_flight.entry(window).or_default();
        match q.iter().position(|p| *p == pos) {
            Some(i) => {
                // This request and every older one have landed.
                q.drain(..=i);
                false
            }
            None => {
                // The window manager overrode us; whatever we still expect is moot.
                q.clear();
                self.overrides.insert(window, pos);
                true
            }
        }
    }

    /// Positions the window manager imposed and nothing has superseded yet. Clears them.
    pub fn take_overrides(&mut self) -> Vec<(K, Point)> {
        std::mem::take(&mut self.overrides).into_iter().collect()
    }

    /// Whether moves to `window` are still expected to land.
    pub fn pending(&self, window: K) -> bool {
        self.in_flight.get(&window).is_some_and(|q| !q.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn our_moves_are_not_external() {
        let mut t = MoveTracker::new();
        t.requested(1u32, p(10, 10));
        assert!(!t.observed(1, p(10, 10)));
        assert!(!t.pending(1));
    }

    #[test]
    fn stale_reports_during_a_drag_are_ours() {
        let mut t = MoveTracker::new();
        for x in [10, 20, 30, 40] {
            t.requested(1u32, p(x, 0));
        }
        // Reports arrive late and possibly skipping some.
        assert!(!t.observed(1, p(20, 0)));
        assert!(t.pending(1));
        assert!(!t.observed(1, p(40, 0)));
        assert!(!t.pending(1));
    }

    #[test]
    fn unexpected_position_is_external() {
        let mut t = MoveTracker::new();
        t.requested(1u32, p(100, 1010));
        assert!(t.observed(1, p(100, 1001)));
        assert!(!t.pending(1), "expectations reset after the override");
        // A report for a window we never moved is external too (user or WM placed it).
        assert!(t.observed(2u32, p(5, 5)));
    }

    #[test]
    fn overrides_wait_until_taken_and_are_superseded_by_requests() {
        let mut t = MoveTracker::new();
        t.requested(1u32, p(0, 1010));
        t.requested(2u32, p(0, 500));
        assert!(t.observed(1, p(0, 1001)));
        assert!(t.observed(2, p(0, 490)));
        // A newer request for window 2 makes its override stale.
        t.requested(2, p(0, 600));
        assert_eq!(t.take_overrides(), vec![(1, p(0, 1001))]);
        assert!(t.take_overrides().is_empty());
    }

    #[test]
    fn windows_are_independent() {
        let mut t = MoveTracker::new();
        t.requested(1u32, p(1, 1));
        t.requested(2u32, p(2, 2));
        assert!(!t.observed(2, p(2, 2)));
        assert!(t.pending(1));
    }

    #[test]
    fn in_flight_queue_is_bounded() {
        let mut t = MoveTracker::new();
        for x in 0..200 {
            t.requested(1u32, p(x, 0));
        }
        assert!(t.observed(1, p(0, 0)), "oldest requests were dropped");
    }
}
