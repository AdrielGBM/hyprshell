//! Pure geometry for L and ring paths, kept free of rsx/layout wiring for independent unit testing.

use rsx::{PathData, Point};

use crate::core::config::Corner;

/// Control-point distance for a circular quarter-arc approximated by one cubic Bézier: `4/3 · tan(π/8)`.
const KAPPA: f32 = 0.552_284_8;

fn tangent_toward(p: Point, center: Point, toward: (f32, f32)) -> (f32, f32) {
    let (rx, ry) = (p.x - center.x, p.y - center.y);
    let len = (rx * rx + ry * ry).sqrt().max(f32::EPSILON);
    let (ax, ay) = (-ry, rx);
    if ax * toward.0 + ay * toward.1 >= 0.0 {
        (ax / len, ay / len)
    } else {
        (ry / len, -rx / len)
    }
}

/// Appends a 90° arc; works for convex and concave corners since control points follow travel direction.
fn arc_to(path: PathData, from: Point, to: Point, center: Point, radius: f32) -> PathData {
    let travel = (to.x - from.x, to.y - from.y);
    let t0 = tangent_toward(from, center, travel);
    let t1 = tangent_toward(to, center, travel);
    let k = KAPPA * radius;
    let c1 = Point::new(from.x + k * t0.0, from.y + k * t0.1);
    let c2 = Point::new(to.x - k * t1.0, to.y - k * t1.1);
    path.cubic_to(c1, c2, to)
}

fn along(from: Point, toward: Point, dist: f32) -> Point {
    let (dx, dy) = (toward.x - from.x, toward.y - from.y);
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    Point::new(from.x + dx / len * dist, from.y + dy / len * dist)
}

/// L-shaped path: square minus quarter-disc carved from inner vertex; screen corner stays sharp for continuous bars.
pub fn corner_l_path(corner: Corner, size: f32, radius: f32) -> PathData {
    let s = size.max(0.0);
    let r = radius.clamp(0.0, s);
    let verts = [
        Point::new(0.0, 0.0),
        Point::new(s, 0.0),
        Point::new(s, s),
        Point::new(0.0, s),
    ];
    if r <= 0.0 {
        return PathData::polygon(&verts);
    }
    let inner = match corner {
        Corner::TopLeft => 2,
        Corner::TopRight => 3,
        Corner::BottomLeft => 1,
        Corner::BottomRight => 0,
    };
    // Start the outline one vertex past the carved one, so the walk opens on a sharp vertex and folds the arc in last.
    let start = (inner + 1) % 4;
    let mut path = PathData::new().move_to(verts[start]);
    for k in 1..4 {
        let i = (start + k) % 4;
        if i == inner {
            let v = verts[i];
            let prev = verts[(i + 3) % 4];
            let next = verts[(i + 1) % 4];
            let arc_start = along(v, prev, r);
            let arc_end = along(v, next, r);
            path = path.line_to(arc_start);
            path = arc_to(path, arc_start, arc_end, v, r);
        } else {
            path = path.line_to(verts[i]);
        }
    }
    path.close()
}

/// Inner-content edges: bar/strip thickness for each screen edge; ring fills everything outside the rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InnerEdges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Frame ring: outer rectangle minus inner rounded rectangle; filled with EvenOdd rule to create the border.
pub fn frame_path(size: (f32, f32), inner: InnerEdges, inner_radius: f32) -> PathData {
    let (w, h) = size;
    let mut path = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(w, 0.0))
        .line_to(Point::new(w, h))
        .line_to(Point::new(0.0, h))
        .close();

    let left = inner.left;
    let top = inner.top;
    let right = w - inner.right;
    let bottom = h - inner.bottom;
    if right <= left || bottom <= top {
        // Bars cover entire surface or degenerate case: ring is the full rect.
        return path;
    }
    let max_r = ((right - left).min(bottom - top)) / 2.0;
    let r = inner_radius.clamp(0.0, max_r);
    if r <= 0.0 {
        return path
            .move_to(Point::new(left, top))
            .line_to(Point::new(right, top))
            .line_to(Point::new(right, bottom))
            .line_to(Point::new(left, bottom))
            .close();
    }
    path = path
        .move_to(Point::new(left + r, top))
        .line_to(Point::new(right - r, top));
    path = arc_to(
        path,
        Point::new(right - r, top),
        Point::new(right, top + r),
        Point::new(right - r, top + r),
        r,
    );
    path = path.line_to(Point::new(right, bottom - r));
    path = arc_to(
        path,
        Point::new(right, bottom - r),
        Point::new(right - r, bottom),
        Point::new(right - r, bottom - r),
        r,
    );
    path = path.line_to(Point::new(left + r, bottom));
    path = arc_to(
        path,
        Point::new(left + r, bottom),
        Point::new(left, bottom - r),
        Point::new(left + r, bottom - r),
        r,
    );
    path = path.line_to(Point::new(left, top + r));
    path = arc_to(
        path,
        Point::new(left, top + r),
        Point::new(left + r, top),
        Point::new(left + r, top + r),
        r,
    );
    path.close()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsx::PathVerb;

    fn bounds(p: &PathData) -> (f32, f32, f32, f32) {
        let b = p.bounds().expect("path has geometry");
        (b.x, b.y, b.width, b.height)
    }

    #[test]
    fn corner_l_square_when_radius_zero() {
        let p = corner_l_path(Corner::TopLeft, 30.0, 0.0);
        assert_eq!(p.verbs().len(), 5);
        assert!(matches!(p.verbs().last().unwrap(), PathVerb::Close));
        assert_eq!(bounds(&p), (0.0, 0.0, 30.0, 30.0));
    }

    #[test]
    fn corner_l_fills_square_and_carves_inner_vertex() {
        for corner in Corner::ALL {
            let p = corner_l_path(corner, 40.0, 12.0);
            assert_eq!(bounds(&p), (0.0, 0.0, 40.0, 40.0), "{corner:?} fills the cell");
            let cubics = p
                .verbs()
                .iter()
                .filter(|v| matches!(v, PathVerb::CubicTo { .. }))
                .count();
            assert_eq!(cubics, 1, "{corner:?} carves exactly one concave fillet");
            assert!(matches!(p.verbs().last().unwrap(), PathVerb::Close));
        }
    }

    #[test]
    fn corner_l_keeps_screen_corner_sharp() {
        let p = corner_l_path(Corner::TopLeft, 40.0, 12.0);
        let hits_origin = p.verbs().iter().any(|v| match v {
            PathVerb::MoveTo(pt) | PathVerb::LineTo(pt) => pt.x == 0.0 && pt.y == 0.0,
            _ => false,
        });
        assert!(hits_origin, "the screen corner stays sharp");
    }

    #[test]
    fn frame_path_is_outer_rect_plus_inner_rounded_rect() {
        let inner = InnerEdges {
            left: 34.0,
            top: 34.0,
            right: 34.0,
            bottom: 34.0,
        };
        let p = frame_path((800.0, 600.0), inner, 20.0);
        assert_eq!(bounds(&p), (0.0, 0.0, 800.0, 600.0));
        let closes = p
            .verbs()
            .iter()
            .filter(|v| matches!(v, PathVerb::Close))
            .count();
        assert_eq!(closes, 2);
        let cubics = p
            .verbs()
            .iter()
            .filter(|v| matches!(v, PathVerb::CubicTo { .. }))
            .count();
        assert_eq!(cubics, 4);
    }

    #[test]
    fn frame_inner_rect_tracks_uneven_edges() {
        let inner = InnerEdges {
            left: 40.0,
            top: 30.0,
            right: 44.0,
            bottom: 50.0,
        };
        let (w, h) = (1000.0, 700.0);
        let p = frame_path((w, h), inner, 16.0);
        let (il, it, ir, ib) = (inner.left, inner.top, w - inner.right, h - inner.bottom);
        let after_first_close = p
            .verbs()
            .iter()
            .skip_while(|v| !matches!(v, PathVerb::Close))
            .skip(1);
        for v in after_first_close {
            if let PathVerb::MoveTo(pt) | PathVerb::LineTo(pt) = v {
                assert!(pt.x >= il - 0.01 && pt.x <= ir + 0.01, "x {} in [{il},{ir}]", pt.x);
                assert!(pt.y >= it - 0.01 && pt.y <= ib + 0.01, "y {} in [{it},{ib}]", pt.y);
            }
        }
    }

    #[test]
    fn frame_ring_collapses_to_full_rect_when_bars_cover_surface() {
        let inner = InnerEdges {
            left: 500.0,
            top: 0.0,
            right: 500.0,
            bottom: 0.0,
        };
        let p = frame_path((800.0, 600.0), inner, 20.0);
        let closes = p
            .verbs()
            .iter()
            .filter(|v| matches!(v, PathVerb::Close))
            .count();
        assert_eq!(closes, 1);
    }
}
