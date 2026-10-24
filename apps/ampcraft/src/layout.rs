//! The window layout at startup, from saved positions or the classic stack.

use ampcraft_dock::{Layout, PanelId, Point, Rect, Size};

/// One window's starting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelStart {
    pub id: PanelId,
    /// Unscaled size, and position in the classic stack relative to the origin.
    pub default: Rect,
    /// Saved desktop position (at the saved scale), if any.
    pub saved: Option<Point>,
    /// Hidden windows keep their place but do not dock.
    pub visible: bool,
}

/// Builds the starting layout at `scale`. Saved positions were recorded at `saved_scale`
/// (None for settings from before it was stored: taken as `scale`); when that differs, the
/// layout is first rebuilt at the saved scale and then rescaled the same way the double-size
/// toggle does it, so docked windows stay docked to the main window (`leader`). A group that
/// grew past the edge of its monitor that way is then moved back onto it if it fits.
pub fn restore(
    panels: &[PanelStart],
    saved_scale: Option<i32>,
    scale: i32,
    origin: Point,
    screens: Vec<Rect>,
    leader: PanelId,
) -> Layout {
    let scale = scale.clamp(1, 3);
    let from = saved_scale.unwrap_or(scale).clamp(1, 3);
    let mut layout = Layout::new();
    layout.set_snap_distance(10 * from);
    layout.set_min_visible(Size::new(28 * from, 14 * from));
    for p in panels {
        let d = p.default;
        let size = (d.w * from, d.h * from);
        let pos = p
            .saved
            .unwrap_or_else(|| Point::new(origin.x + d.x * from, origin.y + d.y * from));
        layout.insert(p.id, Rect::new(pos.x, pos.y, size.0, size.1));
        layout.set_visible(p.id, p.visible);
    }
    layout.set_leader(Some(leader));
    layout.set_screens(screens);
    if from != scale {
        layout.rescale(from, scale);
        keep_group_on_screen(&mut layout, leader);
    }
    layout
}

/// Moves `leader`'s docked group so it lies within the work area of the monitor it is on
/// (the one holding most of the leader), as far as it fits; past that the top-left corner
/// stays visible, so title bars can be reached.
fn keep_group_on_screen(layout: &mut Layout, leader: PanelId) {
    let group = layout.cluster(leader);
    let rects: Vec<(PanelId, Rect)> = group
        .iter()
        .filter_map(|&id| layout.rect(id).map(|r| (id, r)))
        .collect();
    let Some(lead) = layout.rect(leader) else {
        return;
    };
    let overlap = |s: &Rect| {
        let w = lead.right().min(s.right()) - lead.left().max(s.left());
        let h = lead.bottom().min(s.bottom()) - lead.top().max(s.top());
        i64::from(w.max(0)) * i64::from(h.max(0))
    };
    let Some(screen) = layout.screens().iter().copied().max_by_key(overlap) else {
        return;
    };
    if overlap(&screen) == 0 {
        return;
    }
    let left = rects
        .iter()
        .map(|(_, r)| r.left())
        .min()
        .unwrap_or(lead.left());
    let top = rects
        .iter()
        .map(|(_, r)| r.top())
        .min()
        .unwrap_or(lead.top());
    let right = rects
        .iter()
        .map(|(_, r)| r.right())
        .max()
        .unwrap_or(lead.right());
    let bottom = rects
        .iter()
        .map(|(_, r)| r.bottom())
        .max()
        .unwrap_or(lead.bottom());
    // Pull back from the far edge first, then make sure the near edge is on screen.
    let shift = |lo: i32, hi: i32, s_lo: i32, s_hi: i32| {
        let mut d = (s_hi - hi).min(0);
        if lo + d < s_lo {
            d = s_lo - lo;
        }
        d
    };
    let dx = shift(left, right, screen.left(), screen.right());
    let dy = shift(top, bottom, screen.top(), screen.bottom());
    if (dx, dy) == (0, 0) {
        return;
    }
    for (id, r) in rects {
        layout.set_rect(id, Rect::new(r.x + dx, r.y + dy, r.w, r.h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: PanelId = PanelId(1);
    const EQ: PanelId = PanelId(2);
    const PL: PanelId = PanelId(3);

    fn screen() -> Vec<Rect> {
        vec![Rect::new(0, 0, 2560, 1392)]
    }

    /// The stack the Windows test saved at 2x: main at (250, 200), EQ and playlist below.
    fn saved_at_2x() -> Vec<PanelStart> {
        vec![
            PanelStart {
                id: MAIN,
                default: Rect::new(0, 0, 275, 116),
                saved: Some(Point::new(250, 200)),
                visible: true,
            },
            PanelStart {
                id: EQ,
                default: Rect::new(0, 116, 275, 116),
                saved: Some(Point::new(250, 432)),
                visible: true,
            },
            PanelStart {
                id: PL,
                default: Rect::new(0, 232, 275, 232),
                saved: Some(Point::new(250, 664)),
                visible: true,
            },
        ]
    }

    fn rects(l: &Layout) -> Vec<Rect> {
        [MAIN, EQ, PL].iter().filter_map(|&p| l.rect(p)).collect()
    }

    #[test]
    fn docked_stack_survives_a_scale_change() {
        // Regression: starting at 1x or 3x after saving at 2x left 116 px gaps at 1x and
        // overlaps hiding the transport buttons at 3x.
        let one = restore(&saved_at_2x(), Some(2), 1, Point::new(0, 0), screen(), MAIN);
        assert_eq!(
            rects(&one),
            vec![
                Rect::new(250, 200, 275, 116),
                Rect::new(250, 316, 275, 116),
                Rect::new(250, 432, 275, 232),
            ]
        );
        let three = restore(&saved_at_2x(), Some(2), 3, Point::new(0, 0), screen(), MAIN);
        // 1392 px tall at 3x: moved up to fit the 1392 px work area exactly, still docked.
        assert_eq!(
            rects(&three),
            vec![
                Rect::new(250, 0, 825, 348),
                Rect::new(250, 348, 825, 348),
                Rect::new(250, 696, 825, 696),
            ]
        );
    }

    #[test]
    fn a_group_too_tall_for_the_screen_keeps_its_top_visible() {
        let small = vec![Rect::new(0, 0, 1920, 1040)];
        let l = restore(&saved_at_2x(), Some(2), 3, Point::new(0, 0), small, MAIN);
        // Title bars stay reachable: the group starts at the top of the work area.
        assert_eq!(l.rect(MAIN), Some(Rect::new(250, 0, 825, 348)));
        assert_eq!(l.rect(EQ), Some(Rect::new(250, 348, 825, 348)));
    }

    #[test]
    fn groups_on_a_left_hand_monitor_stay_there() {
        let screens = vec![Rect::new(0, 0, 2560, 1392), Rect::new(-2560, 0, 2560, 1392)];
        let mut panels = saved_at_2x();
        for (p, y) in panels.iter_mut().zip([200, 432, 664]) {
            p.saved = Some(Point::new(-700, y));
        }
        let l = restore(&panels, Some(2), 3, Point::new(0, 0), screens, MAIN);
        // 825 wide from -700 crosses into the primary monitor: moved left onto its own.
        assert_eq!(l.rect(MAIN), Some(Rect::new(-825, 0, 825, 348)));
    }

    #[test]
    fn hidden_windows_do_not_count_as_docked() {
        let mut panels = saved_at_2x();
        panels[2].visible = false;
        let l = restore(&panels, Some(2), 1, Point::new(0, 0), screen(), MAIN);
        // The hidden playlist keeps its saved place (only resized).
        assert_eq!(l.rect(PL), Some(Rect::new(250, 664, 275, 232)));
        assert_eq!(l.rect(EQ), Some(Rect::new(250, 316, 275, 116)));
    }

    #[test]
    fn same_scale_keeps_saved_positions() {
        let l = restore(&saved_at_2x(), Some(2), 2, Point::new(0, 0), screen(), MAIN);
        assert_eq!(rects(&l)[1], Rect::new(250, 432, 550, 232));
        // Settings from before the scale was saved behave as before.
        let old = restore(&saved_at_2x(), None, 2, Point::new(0, 0), screen(), MAIN);
        assert_eq!(rects(&old), rects(&l));
    }

    #[test]
    fn undocked_windows_keep_their_place() {
        let mut panels = saved_at_2x();
        // The equalizer was dragged away from the stack.
        panels[1].saved = Some(Point::new(1500, 300));
        panels[2].saved = Some(Point::new(250, 432));
        let l = restore(&panels, Some(2), 1, Point::new(0, 0), screen(), MAIN);
        assert_eq!(l.rect(EQ), Some(Rect::new(1500, 300, 275, 116)));
        // The playlist was docked under the main window and follows it.
        assert_eq!(l.rect(PL), Some(Rect::new(250, 316, 275, 232)));
    }

    #[test]
    fn first_start_is_the_classic_stack() {
        let mut panels = saved_at_2x();
        for p in &mut panels {
            p.saved = None;
        }
        let l = restore(&panels, None, 2, Point::new(100, 100), screen(), MAIN);
        assert_eq!(
            rects(&l),
            vec![
                Rect::new(100, 100, 550, 232),
                Rect::new(100, 332, 550, 232),
                Rect::new(100, 564, 550, 464),
            ]
        );
    }
}
