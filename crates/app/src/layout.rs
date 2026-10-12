//! The window layout at startup, from saved positions or the classic stack.

use amp_dock::{Layout, PanelId, Point, Rect, Size};

/// One window's starting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelStart {
    pub id: PanelId,
    /// Unscaled size, and position in the classic stack relative to the origin.
    pub default: Rect,
    /// Saved desktop position (at the saved scale), if any.
    pub saved: Option<Point>,
}

/// Builds the starting layout at `scale`. Saved positions were recorded at `saved_scale`
/// (None for settings from before it was stored: taken as `scale`); when that differs, the
/// layout is first rebuilt at the saved scale and then rescaled the same way the double-size
/// toggle does it, so docked windows stay docked to the main window (`leader`).
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
    }
    layout.set_leader(Some(leader));
    layout.set_screens(screens);
    layout.rescale(from, scale);
    layout
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
            },
            PanelStart {
                id: EQ,
                default: Rect::new(0, 116, 275, 116),
                saved: Some(Point::new(250, 432)),
            },
            PanelStart {
                id: PL,
                default: Rect::new(0, 232, 275, 232),
                saved: Some(Point::new(250, 664)),
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
        assert_eq!(
            rects(&three),
            vec![
                Rect::new(250, 200, 825, 348),
                Rect::new(250, 548, 825, 348),
                Rect::new(250, 896, 825, 696),
            ]
        );
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
