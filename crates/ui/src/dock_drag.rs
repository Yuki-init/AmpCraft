//! Turns pointer events into docking moves.
//!
//! The platform reports a button press on a panel's drag area, pointer motion and release,
//! all in desktop coordinates. This controller feeds them to the [`Layout`] and returns only
//! the panels whose rectangles changed, so the platform moves no more windows than needed.

use ampcraft_dock::{Drag, Layout, PanelId, Point, Rect};

/// Panel moves to apply to real windows.
pub type Moves = Vec<(PanelId, Rect)>;

#[derive(Debug, Default)]
pub struct DockDrag {
    drag: Option<Drag>,
}

impl DockDrag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The panel being dragged, if any.
    pub fn panel(&self) -> Option<PanelId> {
        self.drag.as_ref().map(Drag::panel)
    }

    /// Starts a drag of `panel` with the pointer at `root`. Returns false if the panel cannot
    /// be dragged (unknown or hidden).
    pub fn press(&mut self, layout: &Layout, panel: PanelId, root: Point) -> bool {
        self.drag = layout.begin_drag(panel, root);
        self.drag.is_some()
    }

    /// Follows the pointer. Returns the panels that moved.
    pub fn motion(&mut self, layout: &mut Layout, root: Point) -> Moves {
        let Some(drag) = &self.drag else {
            return Vec::new();
        };
        let before: Moves = drag
            .members()
            .filter_map(|id| layout.rect(id).map(|r| (id, r)))
            .collect();
        layout.drag_to(drag, root);
        before
            .into_iter()
            .filter_map(|(id, old)| layout.rect(id).filter(|r| *r != old).map(|r| (id, r)))
            .collect()
    }

    /// Ends the drag at `root`. Returns the panels that moved in the final step.
    pub fn release(&mut self, layout: &mut Layout, root: Point) -> Moves {
        let moves = self.motion(layout, root);
        self.drag = None;
        moves
    }

    /// Abandons the drag and puts the panels back where they started.
    pub fn cancel(&mut self, layout: &mut Layout) -> Moves {
        let Some(grab) = self.drag.as_ref().map(Drag::grab) else {
            return Vec::new();
        };
        let moves = self.motion(layout, grab);
        self.drag = None;
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: PanelId = PanelId(1);
    const EQ: PanelId = PanelId(2);

    fn layout() -> Layout {
        let mut l = Layout::new();
        l.insert(MAIN, Rect::new(100, 100, 275, 116));
        l.insert(EQ, Rect::new(600, 600, 275, 116));
        l.set_leader(Some(MAIN));
        l
    }

    #[test]
    fn motion_without_press_does_nothing() {
        let mut l = layout();
        let mut d = DockDrag::new();
        assert!(d.motion(&mut l, Point::new(5, 5)).is_empty());
        assert!(!d.is_dragging());
    }

    #[test]
    fn drag_reports_only_changed_panels_and_snaps() {
        let mut l = layout();
        let mut d = DockDrag::new();
        assert!(d.press(&l, EQ, Point::new(610, 605)));
        assert_eq!(d.panel(), Some(EQ));
        let moves = d.motion(&mut l, Point::new(115, 225));
        // 5 px right and 9 px below main's bottom-left: snaps flush under it.
        assert_eq!(moves, vec![(EQ, Rect::new(100, 216, 275, 116))]);
        assert!(
            d.motion(&mut l, Point::new(116, 224)).is_empty(),
            "still snapped: no change"
        );
        assert!(d.release(&mut l, Point::new(116, 224)).is_empty());
        assert!(!d.is_dragging());
    }

    #[test]
    fn leader_drag_moves_docked_panels_together() {
        let mut l = layout();
        l.set_rect(EQ, Rect::new(100, 216, 275, 116));
        let mut d = DockDrag::new();
        d.press(&l, MAIN, Point::new(0, 0));
        let mut moves = d.motion(&mut l, Point::new(50, 60));
        moves.sort_by_key(|(id, _)| *id);
        assert_eq!(
            moves,
            vec![
                (MAIN, Rect::new(150, 160, 275, 116)),
                (EQ, Rect::new(150, 276, 275, 116))
            ]
        );
    }

    #[test]
    fn cancel_restores_start() {
        let mut l = layout();
        let mut d = DockDrag::new();
        d.press(&l, EQ, Point::new(0, 0));
        d.motion(&mut l, Point::new(-300, 40));
        let moves = d.cancel(&mut l);
        assert_eq!(moves, vec![(EQ, Rect::new(600, 600, 275, 116))]);
        assert!(!d.is_dragging());
    }

    #[test]
    fn hidden_panel_cannot_be_dragged() {
        let mut l = layout();
        l.set_visible(EQ, false);
        let mut d = DockDrag::new();
        assert!(!d.press(&l, EQ, Point::new(0, 0)));
    }
}
