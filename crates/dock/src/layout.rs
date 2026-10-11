//! Panel graph: where each panel is, which panels are docked together, and how drags,
//! resizes and scale changes move them.
//!
//! Classic behavior modeled here:
//! - Panels are docked when they share an edge segment. Docking is transitive: a cluster is
//!   every visible panel reachable through shared edges.
//! - Dragging the leader (the main window) moves its whole cluster. Dragging any other panel
//!   moves only that panel, which undocks it.
//! - While dragging, the moving panels snap to other panels and to screen edges.
//! - When a panel grows or shrinks (playlist resize, window shade), panels hanging off its
//!   bottom or right edge move with that edge, so they stay docked.
//! - Nothing gets stranded off screen: drags are clamped so every moving panel stays
//!   reachable (see [`crate::reach`]), and [`Layout::ensure_reachable`] pulls stranded panels
//!   back after monitor, size or scale changes.

use std::collections::{BTreeSet, VecDeque};

use crate::geom::{Point, Rect, Side, Size, attached_side};
use crate::reach::{DEFAULT_MIN_VISIBLE, constrain, group_reachable_at};
use crate::snap::{DEFAULT_SNAP_DISTANCE, snap_offset};

/// Identifies a panel. The caller picks the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PanelId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Panel {
    id: PanelId,
    rect: Rect,
    visible: bool,
}

/// An in-progress drag. Created by [`Layout::begin_drag`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drag {
    panel: PanelId,
    grab: Point,
    /// Panels moving together and their rectangles when the drag began.
    members: Vec<(PanelId, Rect)>,
}

impl Drag {
    /// The panel the pointer grabbed.
    pub fn panel(&self) -> PanelId {
        self.panel
    }

    /// Pointer position (desktop coordinates) when the drag began.
    pub fn grab(&self) -> Point {
        self.grab
    }

    /// Every panel that moves with this drag, including the grabbed one.
    pub fn members(&self) -> impl Iterator<Item = PanelId> + '_ {
        self.members.iter().map(|(id, _)| *id)
    }
}

/// All panels and screens, plus the docking rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    panels: Vec<Panel>,
    screens: Vec<Rect>,
    leader: Option<PanelId>,
    snap_distance: i32,
    min_visible: Size,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            screens: Vec::new(),
            leader: None,
            snap_distance: DEFAULT_SNAP_DISTANCE,
            min_visible: DEFAULT_MIN_VISIBLE,
        }
    }

    /// Snap distance in pixels. Scale it with the UI (10 at 1x, 20 at 2x). Zero disables it.
    pub fn set_snap_distance(&mut self, d: i32) {
        self.snap_distance = d.max(0);
    }

    pub fn snap_distance(&self) -> i32 {
        self.snap_distance
    }

    /// How much of each panel must stay on screen (scaled with the UI by [`Self::rescale`]).
    pub fn set_min_visible(&mut self, size: Size) {
        self.min_visible = Size::new(size.w.max(1), size.h.max(1));
    }

    pub fn min_visible(&self) -> Size {
        self.min_visible
    }

    /// Work areas of the connected monitors. Panels left stranded (for example on a monitor
    /// that went away) are moved back on screen.
    pub fn set_screens(&mut self, screens: Vec<Rect>) {
        self.screens = screens;
        self.ensure_reachable();
    }

    pub fn screens(&self) -> &[Rect] {
        &self.screens
    }

    /// The panel whose drags carry its docked cluster along (the main window).
    pub fn set_leader(&mut self, leader: Option<PanelId>) {
        self.leader = leader;
    }

    pub fn leader(&self) -> Option<PanelId> {
        self.leader
    }

    /// Adds a visible panel, or replaces the rectangle of an existing one.
    pub fn insert(&mut self, id: PanelId, rect: Rect) {
        match self.panel_mut(id) {
            Some(p) => p.rect = rect,
            None => self.panels.push(Panel {
                id,
                rect,
                visible: true,
            }),
        }
    }

    pub fn remove(&mut self, id: PanelId) -> bool {
        let before = self.panels.len();
        self.panels.retain(|p| p.id != id);
        self.panels.len() != before
    }

    /// Hidden panels keep their place but never dock, snap or move with a cluster.
    pub fn set_visible(&mut self, id: PanelId, visible: bool) {
        if let Some(p) = self.panel_mut(id) {
            p.visible = visible;
        }
    }

    pub fn is_visible(&self, id: PanelId) -> bool {
        self.panel(id).is_some_and(|p| p.visible)
    }

    pub fn rect(&self, id: PanelId) -> Option<Rect> {
        self.panel(id).map(|p| p.rect)
    }

    /// Moves a panel without snapping or moving anything else.
    pub fn set_rect(&mut self, id: PanelId, rect: Rect) {
        if let Some(p) = self.panel_mut(id) {
            p.rect = rect;
        }
    }

    /// All panels as (id, rect, visible), in insertion order.
    pub fn panels(&self) -> impl Iterator<Item = (PanelId, Rect, bool)> + '_ {
        self.panels.iter().map(|p| (p.id, p.rect, p.visible))
    }

    /// Visible panels sharing an edge with `id`, and which side of `id` they are on.
    pub fn neighbors(&self, id: PanelId) -> Vec<(PanelId, Side)> {
        let Some(me) = self.panel(id).filter(|p| p.visible) else {
            return Vec::new();
        };
        self.visible()
            .filter(|p| p.id != id)
            .filter_map(|p| attached_side(&me.rect, &p.rect).map(|s| (p.id, s)))
            .collect()
    }

    /// Every visible panel docked to `id` directly or through others, including `id`.
    pub fn cluster(&self, id: PanelId) -> BTreeSet<PanelId> {
        let mut seen = BTreeSet::new();
        if !self.is_visible(id) {
            return seen;
        }
        let mut queue = VecDeque::from([id]);
        seen.insert(id);
        while let Some(cur) = queue.pop_front() {
            for (n, _) in self.neighbors(cur) {
                if seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        seen
    }

    /// Starts dragging `id` with the pointer at `pointer`. Returns None for unknown or
    /// hidden panels.
    pub fn begin_drag(&self, id: PanelId, pointer: Point) -> Option<Drag> {
        if !self.is_visible(id) {
            return None;
        }
        let ids: Vec<PanelId> = if Some(id) == self.leader {
            self.cluster(id).into_iter().collect()
        } else {
            vec![id]
        };
        let members = ids
            .into_iter()
            .filter_map(|m| self.rect(m).map(|r| (m, r)))
            .collect();
        Some(Drag {
            panel: id,
            grab: pointer,
            members,
        })
    }

    /// Moves the dragged panels so the grab point follows `pointer`, snapping to other panels
    /// and screen edges. Always computed from the drag's start, so a snap releases once the
    /// pointer moves past the snap distance. The move is clamped so every moving panel stays
    /// reachable. Returns the grabbed panel's new rectangle.
    pub fn drag_to(&mut self, drag: &Drag, pointer: Point) -> Option<Rect> {
        let dx = pointer.x - drag.grab.x;
        let dy = pointer.y - drag.grab.y;
        let moved: Vec<Rect> = drag
            .members
            .iter()
            .map(|(_, r)| r.translated(dx, dy))
            .collect();
        let targets: Vec<Rect> = self
            .visible()
            .filter(|p| !drag.members.iter().any(|(m, _)| *m == p.id))
            .map(|p| p.rect)
            .collect();
        let (sx, sy) = snap_offset(&moved, &targets, &self.screens, self.snap_distance);
        let (tx, ty) = (dx + sx, dy + sy);
        let starts: Vec<Rect> = drag.members.iter().map(|(_, r)| *r).collect();
        let (fx, fy) = match constrain(&starts, &self.screens, self.min_visible, tx, ty) {
            Some(d) => d,
            // Nowhere fits. If the group started stranded, let it move freely so it can be
            // rescued; otherwise hold the current position.
            None if !group_reachable_at(&starts, &self.screens, self.min_visible, 0, 0) => (tx, ty),
            None => return self.rect(drag.panel),
        };
        for (id, start) in &drag.members {
            self.set_rect(*id, start.translated(fx, fy));
        }
        self.rect(drag.panel)
    }

    /// Records that something other than us (usually the window manager) moved panel `id` to
    /// `origin`. The rest of its docked cluster is shifted by the same amount so it stays
    /// docked. Returns the other panels that must now move.
    pub fn moved_externally(&mut self, id: PanelId, origin: Point) -> Vec<(PanelId, Rect)> {
        let Some(old) = self.rect(id) else {
            return Vec::new();
        };
        let (dx, dy) = (origin.x - old.x, origin.y - old.y);
        if (dx, dy) == (0, 0) {
            return Vec::new();
        }
        let group = self.cluster(id);
        let mut moves = Vec::new();
        for member in group {
            if let Some(r) = self.rect(member) {
                let moved = r.translated(dx, dy);
                self.set_rect(member, moved);
                if member != id {
                    moves.push((member, moved));
                }
            }
        }
        if !self.is_visible(id) {
            self.set_rect(id, old.translated(dx, dy));
        }
        moves
    }

    /// Moves any stranded panel back on screen. The leader's cluster moves as one block, as
    /// does every other docked cluster, so docking survives. A group that cannot fit anywhere
    /// is moved to the top-left of the first screen. Returns true if anything moved.
    pub fn ensure_reachable(&mut self) -> bool {
        if self.screens.is_empty() {
            return false;
        }
        let mut groups: Vec<Vec<PanelId>> = Vec::new();
        let mut assigned = BTreeSet::new();
        let order = self
            .leader
            .into_iter()
            .chain(self.visible().map(|p| p.id).collect::<Vec<_>>());
        for id in order {
            if assigned.contains(&id) || !self.is_visible(id) {
                continue;
            }
            let cluster = self.cluster(id);
            assigned.extend(cluster.iter().copied());
            groups.push(cluster.into_iter().collect());
        }

        let mut moved = false;
        for group in groups {
            let rects: Vec<Rect> = group.iter().filter_map(|id| self.rect(*id)).collect();
            let Some(bounds) = rects.iter().copied().reduce(|a, r| a.union(&r)) else {
                continue;
            };
            if group_reachable_at(&rects, &self.screens, self.min_visible, 0, 0) {
                continue;
            }
            let home = self.screens[0];
            let (dx, dy) = constrain(&rects, &self.screens, self.min_visible, 0, 0)
                .unwrap_or((home.x - bounds.x, home.y - bounds.y));
            for id in &group {
                if let Some(r) = self.rect(*id) {
                    self.set_rect(*id, r.translated(dx, dy));
                }
            }
            moved = true;
        }
        moved
    }

    /// Resizes `id`, keeping its top-left corner. Panels docked below it (directly or in a
    /// chain) move with its bottom edge, and panels docked to its right move with its right
    /// edge.
    pub fn resize(&mut self, id: PanelId, size: Size) {
        let Some(old) = self.rect(id) else {
            return;
        };
        let dw = size.w - old.w;
        let dh = size.h - old.h;
        let below = if dh != 0 {
            self.chain(id, Side::Bottom)
        } else {
            BTreeSet::new()
        };
        let right = if dw != 0 {
            self.chain(id, Side::Right)
        } else {
            BTreeSet::new()
        };
        self.set_rect(id, old.with_size(size));
        for p in &mut self.panels {
            let mx = if right.contains(&p.id) { dw } else { 0 };
            let my = if below.contains(&p.id) { dh } else { 0 };
            p.rect = p.rect.translated(mx, my);
        }
        self.ensure_reachable();
    }

    /// Visible panels reached from `id` by repeatedly stepping to panels attached on `side`.
    fn chain(&self, id: PanelId, side: Side) -> BTreeSet<PanelId> {
        let mut found = BTreeSet::new();
        let mut queue = VecDeque::from([id]);
        while let Some(cur) = queue.pop_front() {
            let Some(cur_rect) = self.rect(cur) else {
                continue;
            };
            for p in self.visible() {
                if p.id != id
                    && attached_side(&cur_rect, &p.rect) == Some(side)
                    && found.insert(p.id)
                {
                    queue.push_back(p.id);
                }
            }
        }
        found
    }

    /// Changes the UI scale from `from` to `to` (for example 1 to 2 for double size). Every
    /// panel's size scales. The leader's cluster keeps its shape around the leader's top-left
    /// corner, so docked panels stay docked; other panels keep their top-left corner.
    pub fn rescale(&mut self, from: i32, to: i32) {
        if from <= 0 || to <= 0 || from == to {
            return;
        }
        let scale = |v: i32| (i64::from(v) * i64::from(to) / i64::from(from)) as i32;
        let cluster = self.leader.map(|l| self.cluster(l)).unwrap_or_default();
        let anchor = self
            .leader
            .and_then(|l| self.rect(l))
            .map(|r| r.origin())
            .unwrap_or_default();
        for p in &mut self.panels {
            let size = Size::new(scale(p.rect.w), scale(p.rect.h));
            let origin = if cluster.contains(&p.id) {
                Point::new(
                    anchor.x + scale(p.rect.x - anchor.x),
                    anchor.y + scale(p.rect.y - anchor.y),
                )
            } else {
                p.rect.origin()
            };
            p.rect = Rect::new(origin.x, origin.y, size.w, size.h);
        }
        self.snap_distance = scale(self.snap_distance);
        self.min_visible = Size::new(scale(self.min_visible.w), scale(self.min_visible.h));
        self.ensure_reachable();
    }

    fn visible(&self) -> impl Iterator<Item = &Panel> + '_ {
        self.panels.iter().filter(|p| p.visible)
    }

    fn panel(&self, id: PanelId) -> Option<&Panel> {
        self.panels.iter().find(|p| p.id == id)
    }

    fn panel_mut(&mut self, id: PanelId) -> Option<&mut Panel> {
        self.panels.iter_mut().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: PanelId = PanelId(1);
    const EQ: PanelId = PanelId(2);
    const PL: PanelId = PanelId(3);
    const OTHER: PanelId = PanelId(4);

    /// Classic stack: main, EQ below it, playlist below the EQ.
    fn stack() -> Layout {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(100, 100, 275, 116));
        l.insert(EQ, Rect::new(100, 216, 275, 116));
        l.insert(PL, Rect::new(100, 332, 275, 232));
        l.set_leader(Some(MAIN));
        l
    }

    fn ids(v: &[PanelId]) -> BTreeSet<PanelId> {
        v.iter().copied().collect()
    }

    #[test]
    fn insert_replaces_and_remove_works() {
        let mut l = stack();
        l.insert(EQ, Rect::new(0, 0, 10, 10));
        assert_eq!(l.rect(EQ), Some(Rect::new(0, 0, 10, 10)));
        assert_eq!(l.panels().count(), 3);
        assert!(l.remove(EQ));
        assert!(!l.remove(EQ));
        assert_eq!(l.rect(EQ), None);
    }

    #[test]
    fn neighbors_report_sides() {
        let l = stack();
        assert_eq!(l.neighbors(MAIN), vec![(EQ, Side::Bottom)]);
        let mut eq = l.neighbors(EQ);
        eq.sort_by_key(|(id, _)| *id);
        assert_eq!(eq, vec![(MAIN, Side::Top), (PL, Side::Bottom)]);
    }

    #[test]
    fn cluster_is_transitive() {
        let l = stack();
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
        assert_eq!(l.cluster(PL), ids(&[MAIN, EQ, PL]));
    }

    #[test]
    fn hidden_panel_breaks_the_chain() {
        let mut l = stack();
        l.set_visible(EQ, false);
        assert_eq!(l.cluster(MAIN), ids(&[MAIN]));
        assert!(l.cluster(EQ).is_empty());
        assert!(l.begin_drag(EQ, Point::new(0, 0)).is_none());
    }

    #[test]
    fn leader_drag_moves_whole_cluster() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(1000, 1000, 100, 100));
        let d = l.begin_drag(MAIN, Point::new(150, 110)).unwrap();
        assert_eq!(d.members().collect::<BTreeSet<_>>(), ids(&[MAIN, EQ, PL]));
        l.drag_to(&d, Point::new(450, 160));
        assert_eq!(l.rect(MAIN), Some(Rect::new(400, 150, 275, 116)));
        assert_eq!(l.rect(EQ), Some(Rect::new(400, 266, 275, 116)));
        assert_eq!(l.rect(PL), Some(Rect::new(400, 382, 275, 232)));
        assert_eq!(l.rect(OTHER), Some(Rect::new(1000, 1000, 100, 100)));
    }

    #[test]
    fn non_leader_drag_moves_alone_and_undocks() {
        let mut l = stack();
        let d = l.begin_drag(EQ, Point::new(200, 250)).unwrap();
        assert_eq!(d.members().collect::<Vec<_>>(), vec![EQ]);
        l.drag_to(&d, Point::new(800, 250));
        assert_eq!(l.rect(EQ), Some(Rect::new(700, 216, 275, 116)));
        assert_eq!(l.rect(MAIN), Some(Rect::new(100, 100, 275, 116)));
        assert_eq!(l.rect(PL), Some(Rect::new(100, 332, 275, 232)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN]));
    }

    #[test]
    fn no_leader_means_every_drag_is_single() {
        let mut l = stack();
        l.set_leader(None);
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        assert_eq!(d.members().count(), 1);
    }

    #[test]
    fn dragged_panel_snaps_and_docks() {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(100, 100, 275, 116));
        l.insert(EQ, Rect::new(600, 600, 275, 116));
        l.set_leader(Some(MAIN));
        let d = l.begin_drag(EQ, Point::new(650, 610)).unwrap();
        // Drop the EQ 7 px below and 3 px left of the docked position.
        l.drag_to(&d, Point::new(650 - 503, 610 - 377));
        assert_eq!(l.rect(EQ), Some(Rect::new(100, 216, 275, 116)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ]));
    }

    #[test]
    fn snap_releases_past_distance() {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(0, 0, 275, 116));
        l.insert(EQ, Rect::new(0, 116, 275, 116));
        let d = l.begin_drag(EQ, Point::new(10, 120)).unwrap();
        l.drag_to(&d, Point::new(10, 128));
        assert_eq!(l.rect(EQ).unwrap().y, 116, "still within snap distance");
        l.drag_to(&d, Point::new(10, 131));
        assert_eq!(l.rect(EQ).unwrap().y, 127, "released");
    }

    #[test]
    fn group_does_not_snap_to_its_own_members() {
        let mut l = stack();
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        // A small move would otherwise snap back onto the members' old edges.
        l.drag_to(&d, Point::new(3, 4));
        assert_eq!(l.rect(MAIN), Some(Rect::new(103, 104, 275, 116)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
    }

    #[test]
    fn group_snaps_to_screen_by_lowest_member() {
        let mut l = stack();
        l.set_screens(vec![Rect::new(0, 0, 1920, 1080)]);
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        // Playlist bottom would land at 1074, 6 px above the screen bottom.
        l.drag_to(&d, Point::new(0, 510));
        assert_eq!(l.rect(PL).unwrap().bottom(), 1080);
        assert_eq!(l.rect(MAIN).unwrap().y, 616);
    }

    #[test]
    fn hidden_panels_are_not_snap_targets() {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(0, 0, 275, 116));
        l.insert(EQ, Rect::new(500, 500, 275, 116));
        l.set_visible(MAIN, false);
        let d = l.begin_drag(EQ, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(-500, -380));
        assert_eq!(l.rect(EQ), Some(Rect::new(0, 120, 275, 116)));
    }

    #[test]
    fn shading_main_pulls_stack_up_and_unshading_pushes_down() {
        let mut l = stack();
        l.resize(MAIN, Size::new(275, 14));
        assert_eq!(l.rect(EQ), Some(Rect::new(100, 114, 275, 116)));
        assert_eq!(l.rect(PL), Some(Rect::new(100, 230, 275, 232)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
        l.resize(MAIN, Size::new(275, 116));
        assert_eq!(l, stack());
    }

    #[test]
    fn widening_pushes_right_neighbors() {
        let mut l = Layout::new();
        l.insert(PL, Rect::new(0, 0, 275, 116));
        l.insert(OTHER, Rect::new(275, 0, 100, 116));
        l.insert(EQ, Rect::new(0, 116, 275, 116));
        l.resize(PL, Size::new(400, 116));
        assert_eq!(l.rect(OTHER), Some(Rect::new(400, 0, 100, 116)));
        assert_eq!(
            l.rect(EQ),
            Some(Rect::new(0, 116, 275, 116)),
            "below: width change only"
        );
    }

    #[test]
    fn resize_ignores_panels_above_and_unattached() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(900, 900, 50, 50));
        l.resize(EQ, Size::new(275, 50));
        assert_eq!(l.rect(MAIN), Some(Rect::new(100, 100, 275, 116)));
        assert_eq!(l.rect(PL), Some(Rect::new(100, 266, 275, 232)));
        assert_eq!(l.rect(OTHER), Some(Rect::new(900, 900, 50, 50)));
    }

    #[test]
    fn double_size_keeps_cluster_docked_and_round_trips() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(900, 900, 50, 50));
        let original = l.clone();
        l.rescale(1, 2);
        assert_eq!(l.rect(MAIN), Some(Rect::new(100, 100, 550, 232)));
        assert_eq!(l.rect(EQ), Some(Rect::new(100, 332, 550, 232)));
        assert_eq!(l.rect(PL), Some(Rect::new(100, 564, 550, 464)));
        assert_eq!(l.rect(OTHER), Some(Rect::new(900, 900, 100, 100)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
        assert_eq!(l.snap_distance(), 20);
        l.rescale(2, 1);
        assert_eq!(l, original);
    }

    #[test]
    fn side_by_side_cluster_moves_together() {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(0, 0, 275, 116));
        l.insert(PL, Rect::new(275, 0, 275, 300));
        l.insert(EQ, Rect::new(0, 116, 275, 116));
        l.set_leader(Some(MAIN));
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(500, 500));
        assert_eq!(l.rect(PL), Some(Rect::new(775, 500, 275, 300)));
        assert_eq!(l.rect(EQ), Some(Rect::new(500, 616, 275, 116)));
    }

    #[test]
    fn external_move_carries_the_cluster() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(900, 900, 50, 50));
        // The window manager nudged the playlist up by 9 px.
        let mut moves = l.moved_externally(PL, Point::new(100, 323));
        moves.sort_by_key(|(id, _)| *id);
        assert_eq!(
            moves,
            vec![
                (MAIN, Rect::new(100, 91, 275, 116)),
                (EQ, Rect::new(100, 207, 275, 116))
            ]
        );
        assert_eq!(l.rect(PL), Some(Rect::new(100, 323, 275, 232)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
        assert_eq!(l.rect(OTHER), Some(Rect::new(900, 900, 50, 50)));
    }

    #[test]
    fn external_move_of_lone_panel_and_no_op() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(900, 900, 50, 50));
        assert!(l.moved_externally(OTHER, Point::new(800, 800)).is_empty());
        assert_eq!(l.rect(OTHER), Some(Rect::new(800, 800, 50, 50)));
        assert!(l.moved_externally(MAIN, Point::new(100, 100)).is_empty());
        assert!(l.moved_externally(PanelId(99), Point::new(0, 0)).is_empty());
    }

    const SCREEN: Rect = Rect::new(0, 0, 1920, 1080);

    fn all_reachable(l: &Layout) -> bool {
        l.panels()
            .filter(|(_, _, v)| *v)
            .all(|(_, r, _)| crate::reach::is_reachable(&r, l.screens(), l.min_visible()))
    }

    #[test]
    fn drag_is_clamped_at_screen_edges() {
        let mut l = Layout::new();
        l.insert(EQ, Rect::new(100, 100, 275, 116));
        l.set_screens(vec![SCREEN]);
        let d = l.begin_drag(EQ, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(9000, 0));
        assert_eq!(
            l.rect(EQ).unwrap().x,
            1920 - 28,
            "28 px stays visible on the right"
        );
        l.drag_to(&d, Point::new(-9000, 0));
        assert_eq!(
            l.rect(EQ).unwrap().right(),
            28,
            "28 px stays visible on the left"
        );
        l.drag_to(&d, Point::new(0, 9000));
        assert_eq!(
            l.rect(EQ).unwrap().y,
            1080 - 14,
            "title bar stays visible at the bottom"
        );
    }

    #[test]
    fn title_bar_never_goes_above_the_top() {
        let mut l = Layout::new();
        l.insert(EQ, Rect::new(100, 100, 275, 116));
        l.set_screens(vec![SCREEN]);
        let d = l.begin_drag(EQ, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(0, -9000));
        assert_eq!(l.rect(EQ).unwrap().y, 0);
    }

    #[test]
    fn group_drag_is_limited_by_every_member() {
        let mut l = stack();
        l.set_screens(vec![SCREEN]);
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(0, 9000));
        assert_eq!(
            l.rect(PL).unwrap().y,
            1080 - 14,
            "lowest member keeps its title bar"
        );
        assert_eq!(l.rect(MAIN).unwrap().y, 1080 - 14 - 232);
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
        l.drag_to(&d, Point::new(0, -9000));
        assert_eq!(l.rect(MAIN).unwrap().y, 0);
    }

    #[test]
    fn unplugged_monitor_brings_cluster_back_docked() {
        let mut l = stack();
        l.insert(OTHER, Rect::new(2500, 300, 100, 100));
        l.set_screens(vec![SCREEN, Rect::new(1920, 0, 1920, 1080)]);
        // Move the whole stack onto the second monitor.
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(2000, 0));
        assert_eq!(l.rect(MAIN).unwrap().x, 2100);
        l.set_screens(vec![SCREEN]);
        assert!(all_reachable(&l));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]), "still docked");
        assert_eq!(
            l.rect(MAIN).unwrap().x,
            1920 - 28,
            "moved the shortest distance"
        );
        assert_eq!(l.rect(OTHER), Some(Rect::new(1920 - 28, 300, 100, 100)));
    }

    #[test]
    fn unshading_near_bottom_lifts_the_stack() {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(100, 900, 275, 14));
        l.insert(EQ, Rect::new(100, 914, 275, 116));
        l.insert(PL, Rect::new(100, 1030, 275, 232));
        l.set_leader(Some(MAIN));
        l.set_screens(vec![SCREEN]);
        l.resize(MAIN, Size::new(275, 116));
        assert!(all_reachable(&l));
        assert_eq!(l.rect(PL).unwrap().y, 1080 - 14);
        assert_eq!(
            l.rect(MAIN),
            Some(Rect::new(100, 1080 - 14 - 116 - 116, 275, 116))
        );
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
    }

    #[test]
    fn double_size_near_corner_stays_reachable() {
        let mut l = stack();
        l.set_screens(vec![SCREEN]);
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(1600, 500));
        l.rescale(1, 2);
        assert!(all_reachable(&l));
        assert_eq!(l.min_visible(), Size::new(56, 28));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
    }

    #[test]
    fn stranded_panel_can_still_be_dragged_back() {
        let mut l = Layout::new();
        l.set_screens(vec![SCREEN]);
        // Inserted after the screens were set, so nothing has rescued it yet.
        l.insert(EQ, Rect::new(5000, 5000, 275, 116));
        let d = l.begin_drag(EQ, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(-4800, -4800));
        assert_eq!(l.rect(EQ), Some(Rect::new(200, 200, 275, 116)));
    }

    #[test]
    fn ensure_reachable_reports_changes() {
        let mut l = stack();
        l.set_screens(vec![SCREEN]);
        assert!(!l.ensure_reachable());
        l.insert(OTHER, Rect::new(-900, 50, 100, 100));
        assert!(l.ensure_reachable());
        assert_eq!(l.rect(OTHER), Some(Rect::new(28 - 100, 50, 100, 100)));
    }

    #[test]
    fn group_too_tall_for_screen_goes_to_its_corner() {
        let mut l = stack();
        let d = l.begin_drag(MAIN, Point::new(0, 0)).unwrap();
        l.drag_to(&d, Point::new(5000, 5000));
        l.set_screens(vec![Rect::new(0, 0, 800, 200)]);
        assert_eq!(l.rect(MAIN), Some(Rect::new(0, 0, 275, 116)));
        assert_eq!(l.cluster(MAIN), ids(&[MAIN, EQ, PL]));
    }

    #[test]
    fn hidden_panels_are_left_alone() {
        let mut l = Layout::new();
        l.insert(EQ, Rect::new(5000, 5000, 275, 116));
        l.set_visible(EQ, false);
        l.set_screens(vec![SCREEN]);
        assert_eq!(l.rect(EQ), Some(Rect::new(5000, 5000, 275, 116)));
        l.set_visible(EQ, true);
        assert!(l.ensure_reachable());
        assert!(all_reachable(&l));
    }

    /// Small deterministic PRNG so the randomized test is reproducible.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn range(&mut self, lo: i32, hi: i32) -> i32 {
            lo + (self.next() % (hi - lo) as u64) as i32
        }
    }

    #[test]
    fn random_drags_keep_the_dragged_group_rigid() {
        let mut rng = Rng(0x5EED_1234_ABCD_0042);
        for _ in 0..500 {
            let mut l = stack();
            l.insert(
                OTHER,
                Rect::new(rng.range(-500, 1500), rng.range(-500, 1500), 200, 150),
            );
            l.set_screens(vec![Rect::new(0, 0, 1920, 1080)]);
            let who = [MAIN, EQ, PL, OTHER][rng.range(0, 4) as usize];
            let before = l.clone();
            let d = l.begin_drag(who, Point::new(0, 0)).unwrap();
            for _ in 0..5 {
                l.drag_to(&d, Point::new(rng.range(-800, 800), rng.range(-800, 800)));
            }
            // Every member moved by the same amount; everyone else did not move.
            let r0 = before.rect(who).unwrap();
            let r1 = l.rect(who).unwrap();
            let (dx, dy) = (r1.x - r0.x, r1.y - r0.y);
            for (id, rect, _) in before.panels() {
                let now = l.rect(id).unwrap();
                if d.members().any(|m| m == id) {
                    assert_eq!(now, rect.translated(dx, dy));
                } else {
                    assert_eq!(now, rect);
                }
            }
            assert_eq!(r1.size(), r0.size());
            if crate::reach::is_reachable(&r0, l.screens(), l.min_visible()) {
                assert!(crate::reach::is_reachable(
                    &r1,
                    l.screens(),
                    l.min_visible()
                ));
            }
        }
    }
}
