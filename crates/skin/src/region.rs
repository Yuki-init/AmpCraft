//! Turning region.txt polygons into a window shape.
//!
//! A pixel belongs to the shape when its center lies inside any polygon (even-odd rule
//! within a polygon, union across polygons). For axis-aligned outlines this matches the
//! classic convention that a polygon covers its top and left edges but not its bottom and
//! right ones: the rectangle 0,0 275,0 275,14 0,14 covers pixels x 0..275, y 0..14.
//! The result is a list of rectangles, with rows that have identical spans merged into one
//! band so the shape request stays small.

use crate::Rect;
use crate::config::Polygon;

/// Pixel spans `[x0, x1)` covered on row `y` by `polys`, sorted and merged.
fn row_spans(polys: &[Polygon], y: i32, width: i32) -> Vec<(i32, i32)> {
    let yc = y as f64 + 0.5;
    let mut spans: Vec<(i32, i32)> = Vec::new();
    for poly in polys.iter().filter(|p| p.len() >= 3) {
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..poly.len() {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % poly.len()];
            let (x0, y0, x1, y1) = (x0 as f64, y0 as f64, x1 as f64, y1 as f64);
            // Half-open in y so shared vertices are counted once.
            if (y0 <= yc && yc < y1) || (y1 <= yc && yc < y0) {
                xs.push(x0 + (yc - y0) * (x1 - x0) / (y1 - y0));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            // Pixels whose centers lie in [a, b).
            let a = ((pair[0] - 0.5).ceil() as i32).clamp(0, width);
            let b = ((pair[1] - 0.5).ceil() as i32).clamp(0, width);
            if b > a {
                spans.push((a, b));
            }
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(i32, i32)> = Vec::new();
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

/// Rasterizes `polys` for a window of `width` x `height` (1x) and scales the result by
/// `scale`. Returns None when there are no usable polygons (the window stays rectangular).
pub fn shape_rects(polys: &[Polygon], width: i32, height: i32, scale: i32) -> Option<Vec<Rect>> {
    if polys.iter().all(|p| p.len() < 3) {
        return None;
    }
    let s = scale.max(1);
    let mut rects = Vec::new();
    // Open bands: spans and the row they started on.
    let mut band: Option<(Vec<(i32, i32)>, i32)> = None;
    let flush = |band: &mut Option<(Vec<(i32, i32)>, i32)>, end: i32, rects: &mut Vec<Rect>| {
        if let Some((spans, start)) = band.take() {
            for (a, b) in spans {
                rects.push(Rect::new(a * s, start * s, (b - a) * s, (end - start) * s));
            }
        }
    };
    for y in 0..height {
        let spans = row_spans(polys, y, width);
        match &band {
            Some((current, _)) if *current == spans => {}
            _ => {
                flush(&mut band, y, &mut rects);
                if !spans.is_empty() {
                    band = Some((spans, y));
                }
            }
        }
    }
    flush(&mut band, height, &mut rects);
    Some(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(points: &[(i32, i32)]) -> Polygon {
        points.to_vec()
    }

    #[test]
    fn rectangle_covers_top_left_edges_only() {
        let r = shape_rects(
            &[poly(&[(0, 0), (275, 0), (275, 14), (0, 14)])],
            275,
            116,
            1,
        )
        .unwrap();
        assert_eq!(r, vec![Rect::new(0, 0, 275, 14)]);
    }

    #[test]
    fn documented_main_window_example() {
        // From the stock skin's region.txt documentation: a title bar a row down, then a body
        // inset by one pixel on each side.
        let p = poly(&[
            (0, 1),
            (275, 1),
            (275, 14),
            (274, 14),
            (274, 115),
            (1, 115),
            (1, 14),
            (0, 14),
        ]);
        let r = shape_rects(&[p], 275, 116, 1).unwrap();
        assert_eq!(
            r,
            vec![Rect::new(0, 1, 275, 13), Rect::new(1, 14, 273, 101)]
        );
    }

    #[test]
    fn polygons_union_and_scale() {
        let a = poly(&[(0, 0), (10, 0), (10, 10), (0, 10)]);
        let b = poly(&[(5, 5), (20, 5), (20, 10), (5, 10)]);
        let r = shape_rects(&[a, b], 30, 10, 2).unwrap();
        assert_eq!(r, vec![Rect::new(0, 0, 20, 10), Rect::new(0, 10, 40, 10)]);
    }

    #[test]
    fn holes_from_even_odd_and_clipping() {
        // A square with a square hole drawn as one polygon walking in and out.
        let ring = poly(&[
            (0, 0),
            (10, 0),
            (10, 10),
            (0, 10),
            (0, 0),
            (3, 3),
            (3, 7),
            (7, 7),
            (7, 3),
            (3, 3),
        ]);
        let r = shape_rects(&[ring], 10, 10, 1).unwrap();
        assert!(r.contains(&Rect::new(0, 3, 3, 4)));
        assert!(r.contains(&Rect::new(7, 3, 3, 4)));
        // Coordinates beyond the window are clipped.
        let big = poly(&[(-5, -5), (500, -5), (500, 500), (-5, 500)]);
        assert_eq!(
            shape_rects(&[big], 20, 10, 1).unwrap(),
            vec![Rect::new(0, 0, 20, 10)]
        );
    }

    #[test]
    fn diagonal_edges_step_per_row() {
        // Right triangle: row y covers x 0..(10 - y) roughly.
        let tri = poly(&[(0, 0), (10, 0), (0, 10)]);
        let r = shape_rects(&[tri], 10, 10, 1).unwrap();
        // Row y has pixel centers inside up to x = 9.5 - y: 9 px on row 0, none on row 9.
        assert_eq!(r.len(), 9, "one band per row");
        assert_eq!(r[0], Rect::new(0, 0, 9, 1));
        assert_eq!(r[8], Rect::new(0, 8, 1, 1));
    }

    #[test]
    fn nothing_usable_means_rectangular() {
        assert_eq!(shape_rects(&[], 10, 10, 1), None);
        assert_eq!(shape_rects(&[poly(&[(0, 0), (5, 5)])], 10, 10, 1), None);
    }
}
