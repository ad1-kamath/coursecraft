//! Geometry for drawn paths: flattening, stroke bands, fill and line ends.
//!
//! No egui input or widget types appear here. Everything works in one
//! coordinate space chosen by the caller (slide units for hit testing, screen
//! units for painting) and never panics on hostile points.

pub use coursecraft_course::{
    CONNECTOR_MIN_LEAD, connector_site, connector_site_direction, route_connector,
};
use coursecraft_course::{ConnectorStyle, DashStyle, LineEnd, PathStyle};
use egui::{Color32, Mesh, pos2};

use crate::shape_fill::{fill_contours, stroke_path};

pub use crate::shape_geometry::Pt;

const EPS: f32 = 1e-4;
const FLAT_TOLERANCE: f32 = 0.35;
const MAX_FLAT_POINTS: usize = 8192;
const MAX_SPLIT_DEPTH: usize = 12;

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}

fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}

fn mul(a: Pt, scale: f32) -> Pt {
    (a.0 * scale, a.1 * scale)
}

fn dot(a: Pt, b: Pt) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn cross(a: Pt, b: Pt) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

fn length(a: Pt) -> f32 {
    dot(a, a).sqrt()
}

fn distance(a: Pt, b: Pt) -> f32 {
    length(sub(a, b))
}

fn finite(point: Pt) -> bool {
    point.0.is_finite() && point.1.is_finite()
}

fn direction(from: Pt, to: Pt) -> Option<Pt> {
    let edge = sub(to, from);
    let len = length(edge);
    (len.is_finite() && len > EPS).then(|| mul(edge, 1.0 / len))
}

/// Drop non-finite points and near-duplicates, keeping the first and last.
///
/// A closed path also drops a last point that copies the first, and a final
/// point that would make the closing segment empty.
fn clean(points: &[Pt], closed: bool) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len().min(MAX_FLAT_POINTS));
    for &point in points {
        if !finite(point) {
            continue;
        }
        if out
            .last()
            .is_none_or(|last: &Pt| distance(*last, point) > EPS)
        {
            out.push(point);
        }
        if out.len() >= MAX_FLAT_POINTS {
            break;
        }
    }
    if closed
        && out.len() > 2
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(&first, &last)| distance(first, last) <= EPS)
    {
        out.pop();
    }
    out
}

fn lerp(a: Pt, b: Pt, t: f32) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn point_line_distance(point: Pt, a: Pt, b: Pt) -> f32 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if !len2.is_finite() || len2 <= EPS * EPS {
        return distance(point, a);
    }
    let t = (dot(sub(point, a), ab) / len2).clamp(0.0, 1.0);
    distance(point, lerp(a, b, t))
}

/// Flatten one cubic Bézier iteratively with a hard point cap.
fn flatten_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, out: &mut Vec<Pt>) {
    let mut stack: Vec<(Pt, Pt, Pt, Pt, usize)> = vec![(p0, c1, c2, p1, 0)];
    let mut guard = 0usize;
    while let Some((a, b, c, d, depth)) = stack.pop() {
        guard = guard.saturating_add(1);
        if guard >= MAX_FLAT_POINTS {
            break;
        }
        let flat = point_line_distance(b, a, d).max(point_line_distance(c, a, d));
        if depth >= MAX_SPLIT_DEPTH || !flat.is_finite() || flat <= FLAT_TOLERANCE {
            out.push(d);
            if out.len() >= MAX_FLAT_POINTS {
                break;
            }
            continue;
        }
        // de Casteljau split at 0.5
        let ab = lerp(a, b, 0.5);
        let bc = lerp(b, c, 0.5);
        let cd = lerp(c, d, 0.5);
        let abc = lerp(ab, bc, 0.5);
        let bcd = lerp(bc, cd, 0.5);
        let mid = lerp(abc, bcd, 0.5);
        // Push the right half first so the left half is emitted first.
        stack.push((mid, bcd, cd, d, depth.saturating_add(1)));
        stack.push((a, ab, abc, mid, depth.saturating_add(1)));
    }
}

/// One end of a circular arc, used by the PPTX importer and path tests.
#[must_use]
pub fn arc_points(
    start: Pt,
    rx: f32,
    ry: f32,
    start_angle: f32,
    swing: f32,
    segments: usize,
) -> Vec<Pt> {
    if !start.0.is_finite()
        || !start.1.is_finite()
        || !rx.is_finite()
        || !ry.is_finite()
        || !start_angle.is_finite()
        || !swing.is_finite()
        || rx <= 0.0
        || ry <= 0.0
    {
        return Vec::new();
    }
    let centre = (
        start.0 - rx * start_angle.cos(),
        start.1 - ry * start_angle.sin(),
    );
    let steps = segments.clamp(2, 512);
    let mut out = Vec::with_capacity(steps);
    for i in 0..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / steps as f32;
        let angle = start_angle + swing * t;
        out.push((centre.0 + rx * angle.cos(), centre.1 + ry * angle.sin()));
    }
    out
}

fn catmull_rom(points: &[Pt], closed: bool) -> Vec<Pt> {
    let count = points.len();
    if count < 2 {
        return points.to_vec();
    }
    let mut out = vec![points.first().copied().unwrap_or((0.0, 0.0))];
    let segments = if closed { count } else { count - 1 };
    for i in 0..segments {
        let Some(&p1) = points.get(i) else {
            break;
        };
        let Some(&p2) = points.get((i + 1) % count) else {
            break;
        };
        let p0 = if i > 0 {
            points.get(i - 1).copied().unwrap_or(p1)
        } else if closed {
            points.last().copied().unwrap_or(p1)
        } else {
            p1
        };
        let p3 = if i + 2 < count {
            points.get(i + 2).copied().unwrap_or(p2)
        } else if closed {
            points.get(i + 2 - count).copied().unwrap_or(p2)
        } else {
            p2
        };
        let c1 = add(p1, mul(sub(p2, p0), 1.0 / 6.0));
        let c2 = sub(p2, mul(sub(p3, p1), 1.0 / 6.0));
        flatten_cubic(p1, c1, c2, p2, &mut out);
        if out.len() >= MAX_FLAT_POINTS {
            break;
        }
    }
    out
}

fn elbow(points: &[Pt]) -> Vec<Pt> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let Some(&last) = points.last() else {
        return Vec::new();
    };
    if distance(first, last) <= EPS {
        return vec![first];
    }
    let dx = (last.0 - first.0).abs();
    let dy = (last.1 - first.1).abs();
    if dx <= EPS || dy <= EPS {
        return vec![first, last];
    }
    let mid_x = (first.0 + last.0) * 0.5;
    vec![first, (mid_x, first.1), (mid_x, last.1), last]
}

fn curved_connector(points: &[Pt]) -> Vec<Pt> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let Some(&last) = points.last() else {
        return Vec::new();
    };
    if distance(first, last) <= EPS {
        return vec![first];
    }
    let mid_x = (first.0 + last.0) * 0.5;
    let mut out = vec![first];
    flatten_cubic(first, (mid_x, first.1), (mid_x, last.1), last, &mut out);
    out
}

/// Flatten a path into a polyline.
///
/// Straight joins the cleaned points. `Curved` is a Catmull-Rom spline through
/// them, converted to cubic Béziers and sampled adaptively. A connector routes
/// its two ends as an elbow (right-angle bends through the horizontal midpoint)
/// or one S-shaped cubic. Duplicate points are dropped, so no step divides by a
/// zero length.
#[must_use]
pub fn flatten(
    points: &[Pt],
    style: PathStyle,
    connector: Option<ConnectorStyle>,
    closed: bool,
) -> Vec<Pt> {
    let points = clean(points, closed);
    if points.len() < 2 {
        return points;
    }
    let mut flat = match connector {
        Some(ConnectorStyle::Elbow) => elbow(&points),
        Some(ConnectorStyle::Curved) => curved_connector(&points),
        Some(ConnectorStyle::Straight) => points,
        None => match style {
            PathStyle::Straight => points,
            PathStyle::Curved => catmull_rom(&points, closed),
        },
    };
    if closed
        && flat.len() > 2
        && flat
            .first()
            .zip(flat.last())
            .is_some_and(|(&first, &last)| distance(first, last) > EPS)
        && let Some(&first) = flat.first()
    {
        flat.push(first);
    }
    flat
}

/// Shortest distance from `point` to a polyline. Infinite for an empty path.
#[must_use]
pub fn path_distance(points: &[Pt], point: Pt) -> f32 {
    if !finite(point) {
        return f32::INFINITY;
    }
    let Some(&first) = points.first() else {
        return f32::INFINITY;
    };
    if points.len() == 1 {
        return distance(first, point);
    }
    let mut best = f32::INFINITY;
    for pair in points.windows(2) {
        let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        best = best.min(point_line_distance(point, a, b));
    }
    best
}

/// The closest point on a polyline: `(segment index, distance along it, distance)`.
#[must_use]
pub fn path_closest(points: &[Pt], point: Pt) -> Option<(usize, f32, f32)> {
    if !finite(point) || points.is_empty() {
        return None;
    }
    if points.len() == 1 {
        let first = points.first().copied()?;
        return Some((0, 0.0, distance(first, point)));
    }
    let mut best: Option<(usize, f32, f32)> = None;
    for (index, pair) in points.windows(2).enumerate() {
        let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let ab = sub(b, a);
        let len2 = dot(ab, ab);
        let t = if len2.is_finite() && len2 > EPS * EPS {
            (dot(sub(point, a), ab) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let d = distance(point, lerp(a, b, t));
        if best.is_none_or(|(_, _, best_d)| d < best_d) {
            best = Some((index, t, d));
        }
    }
    best
}

/// Whether `point` is inside a closed polygon, using the even-odd rule.
#[must_use]
pub fn path_contains(points: &[Pt], point: Pt) -> bool {
    if points.len() < 3 || !finite(point) {
        return false;
    }
    let mut inside = false;
    for (index, &a) in points.iter().enumerate() {
        let Some(&b) = points.get((index + 1) % points.len()) else {
            continue;
        };
        if (a.1 > point.1) != (b.1 > point.1) {
            let dy = b.1 - a.1;
            if dy.abs() <= EPS {
                continue;
            }
            let t = (point.1 - a.1) / dy;
            let x = a.0 + t * (b.0 - a.0);
            if point.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

fn trim_start(points: &[Pt], amount: f32) -> Vec<Pt> {
    if amount <= 0.0 || points.len() < 2 {
        return points.to_vec();
    }
    let mut remaining = amount;
    for (index, pair) in points.windows(2).enumerate() {
        let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let len = distance(a, b);
        if !len.is_finite() {
            continue;
        }
        if remaining < len {
            let first = lerp(a, b, remaining / len);
            let mut out = vec![first];
            if let Some(rest) = points.get(index + 1..) {
                out.extend_from_slice(rest);
            }
            return out;
        }
        remaining -= len;
    }
    points.last().copied().map(|p| vec![p]).unwrap_or_default()
}

fn trim_end(points: &[Pt], amount: f32) -> Vec<Pt> {
    if amount <= 0.0 || points.len() < 2 {
        return points.to_vec();
    }
    let mut remaining = amount;
    let mut index = points.len();
    while index > 1 {
        let Some(&b) = points.get(index - 1) else {
            break;
        };
        let Some(&a) = points.get(index - 2) else {
            break;
        };
        let len = distance(a, b);
        if !len.is_finite() {
            index -= 1;
            continue;
        }
        if remaining < len {
            let last = lerp(b, a, remaining / len);
            let mut out = points
                .get(..index - 1)
                .map(<[Pt]>::to_vec)
                .unwrap_or_default();
            out.push(last);
            return out;
        }
        remaining -= len;
        index -= 1;
    }
    points.first().copied().map(|p| vec![p]).unwrap_or_default()
}

fn push_triangle(mesh: &mut Mesh, triangle: [Pt; 3], color: Color32) {
    let base = mesh.vertices.len() as u32;
    for point in triangle {
        mesh.colored_vertex(pos2(point.0, point.1), color);
    }
    mesh.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
}

fn push_quad(mesh: &mut Mesh, quad: [Pt; 4], color: Color32) {
    let base = mesh.vertices.len() as u32;
    for point in quad {
        mesh.colored_vertex(pos2(point.0, point.1), color);
    }
    mesh.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
    mesh.add_triangle(base, base.saturating_add(2), base.saturating_add(3));
}

fn append_mesh(target: &mut Mesh, source: &Mesh) {
    let base = target.vertices.len() as u32;
    target.vertices.extend(source.vertices.iter().copied());
    for [a, b, c] in source.triangles() {
        target.add_triangle(
            base.saturating_add(a),
            base.saturating_add(b),
            base.saturating_add(c),
        );
    }
}

fn add_dot(mesh: &mut Mesh, centre: Pt, radius: f32, color: Color32) {
    let segments = 16usize;
    let step = std::f32::consts::TAU / segments as f32;
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(pos2(centre.0, centre.1), color);
    for i in 0..=segments {
        #[allow(clippy::cast_precision_loss)]
        let angle = step * i as f32;
        mesh.colored_vertex(
            pos2(
                centre.0 + radius * angle.cos(),
                centre.1 + radius * angle.sin(),
            ),
            color,
        );
    }
    for i in 0..segments {
        #[allow(clippy::cast_possible_truncation)]
        let a = i as u32;
        #[allow(clippy::cast_possible_truncation)]
        let b = (i + 1) as u32;
        mesh.add_triangle(
            base,
            base.saturating_add(1).saturating_add(a),
            base.saturating_add(1).saturating_add(b),
        );
    }
}

/// Head size in the same units as the path. Thin lines still show a head.
#[must_use]
pub fn line_end_size(width: f32) -> f32 {
    let width = if width.is_finite() && width > 0.0 {
        width
    } else {
        1.0
    };
    (width * 2.0).max(5.0)
}

fn end_extent(end: LineEnd, width: f32) -> f32 {
    let half = line_end_size(width);
    match end {
        LineEnd::Arrow | LineEnd::OpenArrow | LineEnd::Diamond => half * 2.0,
        LineEnd::Dot => two_thirds(half),
        LineEnd::None => 0.0,
    }
}

fn add_line_end(mesh: &mut Mesh, tip: Pt, forward: Pt, end: LineEnd, width: f32, color: Color32) {
    let half = line_end_size(width);
    let head_len = half * 2.0;
    let back = sub(tip, mul(forward, head_len));
    let side = (forward.1, -forward.0);
    let plus = add(back, mul(side, half));
    let minus = sub(back, mul(side, half));
    match end {
        LineEnd::None => {}
        LineEnd::Arrow => push_triangle(mesh, [tip, plus, minus], color),
        LineEnd::OpenArrow => {
            let stroke = stroke_path(
                &[tip, plus],
                false,
                color,
                width.max(1.0),
                DashStyle::Solid,
                0.0,
            );
            append_mesh(mesh, &stroke);
            let stroke = stroke_path(
                &[tip, minus],
                false,
                color,
                width.max(1.0),
                DashStyle::Solid,
                0.0,
            );
            append_mesh(mesh, &stroke);
        }
        LineEnd::Diamond => {
            let mid_centre = sub(tip, mul(forward, head_len * 0.5));
            let mid_plus = add(mid_centre, mul(side, half));
            let mid_minus = sub(mid_centre, mul(side, half));
            push_quad(mesh, [tip, mid_plus, back, mid_minus], color);
        }
        LineEnd::Dot => add_dot(mesh, tip, two_thirds(half), color),
    }
}

fn two_thirds(value: f32) -> f32 {
    value * (2.0 / 3.0)
}

/// Colours and widths for a path mesh.
#[derive(Debug, Clone, Copy)]
pub struct PathMeshSpec {
    pub color: Color32,
    pub width: f32,
    pub dash: DashStyle,
    pub fill: Option<Color32>,
    pub pixels_per_point: f32,
}

/// Build the fill, stroke band and line ends for a path in one mesh.
///
/// `points` must already be in the target coordinate space; `width` is in the
/// same units. A closed path is filled with the even-odd rule. Degenerate fills
/// produce no triangles but leave the outline intact.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn path_mesh(
    points: &[Pt],
    style: PathStyle,
    closed: bool,
    connector: Option<ConnectorStyle>,
    start_end: LineEnd,
    end_end: LineEnd,
    spec: &PathMeshSpec,
) -> Mesh {
    let mut mesh = Mesh::default();
    let flat = flatten(points, style, connector, closed);
    if flat.len() < 2 {
        return mesh;
    }
    let draw_stroke = spec.width.is_finite() && spec.width > 0.0;

    if closed
        && let Some(fill) = spec.fill
        && fill.a() > 0
        && flat.len() >= 3
    {
        let area: f32 = flat
            .iter()
            .enumerate()
            .map(|(index, &a)| {
                flat.get((index + 1) % flat.len())
                    .map_or(0.0, |&b| cross(a, b))
            })
            .sum::<f32>()
            .abs()
            * 0.5;
        if area.is_finite() && area > EPS && area <= 1e12 {
            let filled = fill_contours(std::slice::from_ref(&flat), fill, spec.pixels_per_point);
            if !filled.is_empty() {
                append_mesh(&mut mesh, &filled);
            }
        }
    }

    if !draw_stroke {
        return mesh;
    }
    let (start_trim, end_trim) = if closed {
        (0.0, 0.0)
    } else {
        (
            end_extent(start_end, spec.width) * 0.9,
            end_extent(end_end, spec.width) * 0.9,
        )
    };
    let start_direction = flat
        .first()
        .copied()
        .zip(flat.get(1).copied())
        .and_then(|(a, b)| direction(a, b));
    let end_direction = flat
        .last()
        .copied()
        .zip(flat.get(flat.len().saturating_sub(2)).copied())
        .and_then(|(a, b)| direction(b, a));

    let trimmed = trim_end(&trim_start(&flat, start_trim), end_trim);
    let stroke = stroke_path(
        &trimmed,
        closed,
        spec.color,
        spec.width,
        spec.dash,
        spec.pixels_per_point,
    );
    append_mesh(&mut mesh, &stroke);

    if !closed {
        if let (Some(&tip), Some(direction)) = (flat.first(), start_direction) {
            add_line_end(&mut mesh, tip, direction, start_end, spec.width, spec.color);
        }
        if let (Some(&tip), Some(direction)) = (flat.last(), end_direction) {
            add_line_end(&mut mesh, tip, direction, end_end, spec.width, spec.color);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite_points(points: &[Pt]) -> bool {
        points.iter().all(|p| p.0.is_finite() && p.1.is_finite())
    }

    #[test]
    fn straight_and_curved_flatten_through_the_points() {
        let points = [(0.0, 0.0), (50.0, 40.0), (100.0, 0.0)];
        let straight = flatten(&points, PathStyle::Straight, None, false);
        assert_eq!(straight.first(), Some(&(0.0, 0.0)));
        assert_eq!(straight.last(), Some(&(100.0, 0.0)));
        let curved = flatten(&points, PathStyle::Curved, None, false);
        assert!(curved.len() > straight.len());
        assert!(finite_points(&curved));
        assert!(curved.iter().any(|p| p.1 > 20.0));
    }

    #[test]
    fn elbow_has_right_angle_bends_and_curved_connector_is_smooth() {
        let points = [(0.0, 0.0), (100.0, 80.0)];
        let elbow = flatten(
            &points,
            PathStyle::Straight,
            Some(ConnectorStyle::Elbow),
            false,
        );
        assert_eq!(
            elbow,
            vec![(0.0, 0.0), (50.0, 0.0), (50.0, 80.0), (100.0, 80.0)]
        );
        let curved = flatten(
            &points,
            PathStyle::Straight,
            Some(ConnectorStyle::Curved),
            false,
        );
        assert!(curved.len() > 4);
        assert!(finite_points(&curved));
        assert_eq!(curved.first(), Some(&(0.0, 0.0)));
        assert_eq!(curved.last(), Some(&(100.0, 80.0)));
    }

    #[test]
    fn every_elbow_site_pair_is_axis_aligned_with_lead() {
        let start = (10.0, 10.0);
        let end = (310.0, 190.0);
        for start_site in 0..4u8 {
            for end_site in 0..4u8 {
                let route = route_connector(
                    start,
                    Some(start_site),
                    end,
                    Some(end_site),
                    ConnectorStyle::Elbow,
                );
                assert!(route.len() >= 2, "{start_site}/{end_site}");
                assert_eq!(route.first(), Some(&start), "{start_site}/{end_site}");
                assert_eq!(route.last(), Some(&end), "{start_site}/{end_site}");
                assert!(finite_points(&route), "{start_site}/{end_site}");
                for pair in route.windows(2) {
                    let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
                        continue;
                    };
                    assert!(
                        (a.0 - b.0).abs() <= EPS || (a.1 - b.1).abs() <= EPS,
                        "site {start_site}/{end_site} bent diagonally: {a:?} -> {b:?}"
                    );
                }
                let first = route.get(1).copied().unwrap_or(start);
                assert!(
                    distance(start, first) >= CONNECTOR_MIN_LEAD - EPS,
                    "site {start_site}/{end_site} leaves too short: {first:?}"
                );
                let last = route
                    .get(route.len().saturating_sub(2))
                    .copied()
                    .unwrap_or(end);
                assert!(
                    distance(last, end) >= CONNECTOR_MIN_LEAD - EPS,
                    "site {start_site}/{end_site} arrives too short: {last:?}"
                );
            }
        }
    }

    #[test]
    fn curved_connector_follows_the_site_directions() {
        let start = (0.0, 0.0);
        let end = (200.0, 100.0);
        let route = route_connector(start, Some(1), end, Some(3), ConnectorStyle::Curved);
        assert!(route.len() > 2);
        let first = route.get(1).copied().unwrap_or(start);
        assert!(first.0 > start.0 && (first.1 - start.1).abs() < 1.0);
        let before_last = route
            .get(route.len().saturating_sub(3))
            .copied()
            .unwrap_or(start);
        assert!(before_last.0 < end.0);
        assert_eq!(route.last(), Some(&end));
    }

    #[test]
    fn connector_sites_and_directions_wrap() {
        let frame = (10.0, 20.0, 100.0, 50.0);
        assert_eq!(connector_site(frame, 0), (60.0, 20.0));
        assert_eq!(connector_site(frame, 1), (110.0, 45.0));
        assert_eq!(connector_site(frame, 2), (60.0, 70.0));
        assert_eq!(connector_site(frame, 3), (10.0, 45.0));
        assert_eq!(connector_site(frame, 7), (10.0, 45.0));
        assert_eq!(connector_site_direction(6), (0.0, 1.0));
    }

    #[test]
    fn duplicate_and_hostile_points_do_not_divide_by_zero() {
        let points = [
            (10.0, 10.0),
            (10.0, 10.0),
            (f32::NAN, 4.0),
            (10.0, 10.0),
            (80.0, 50.0),
            (80.0, 50.0),
        ];
        for style in [PathStyle::Straight, PathStyle::Curved] {
            for connector in [
                None,
                Some(ConnectorStyle::Straight),
                Some(ConnectorStyle::Elbow),
                Some(ConnectorStyle::Curved),
            ] {
                let flat = flatten(&points, style, connector, true);
                assert!(finite_points(&flat));
                let _ = path_distance(&flat, (20.0, 20.0));
                let _ = path_contains(&flat, (20.0, 20.0));
            }
        }
        assert!(path_distance(&[], (0.0, 0.0)).is_infinite());
    }

    #[test]
    fn path_distance_measures_to_the_line_not_the_frame() {
        let line = [(0.0, 0.0), (100.0, 100.0)];
        assert!(path_distance(&line, (50.0, 50.0)) < 0.001);
        assert!(path_distance(&line, (0.0, 100.0)) > 60.0);
    }

    #[test]
    fn line_end_sizes_have_a_floor() {
        assert!(line_end_size(0.5) >= 5.0);
        assert!(line_end_size(10.0) == 20.0);
    }

    #[test]
    fn path_mesh_draws_every_style_and_end() {
        let points = [(0.0, 0.0), (60.0, 40.0), (120.0, 0.0)];
        for style in [PathStyle::Straight, PathStyle::Curved] {
            for end in [
                LineEnd::None,
                LineEnd::Arrow,
                LineEnd::OpenArrow,
                LineEnd::Dot,
                LineEnd::Diamond,
            ] {
                let mesh = path_mesh(
                    &points,
                    style,
                    false,
                    None,
                    end,
                    end,
                    &PathMeshSpec {
                        color: Color32::from_rgb(20, 30, 40),
                        width: 3.0,
                        dash: DashStyle::Solid,
                        fill: None,
                        pixels_per_point: 1.0,
                    },
                );
                assert!(!mesh.is_empty(), "{style:?} {end:?} drew nothing");
            }
        }
    }

    #[test]
    fn figure_eight_and_scribble_fill_without_panicking() {
        let figure_eight = [
            (0.0, 0.0),
            (100.0, 100.0),
            (100.0, 0.0),
            (0.0, 100.0),
            (0.0, 0.0),
        ];
        let scribble = [
            (0.0, 0.0),
            (80.0, 100.0),
            (20.0, 100.0),
            (100.0, 0.0),
            (40.0, 50.0),
            (0.0, 0.0),
        ];
        for points in [figure_eight.as_slice(), scribble.as_slice()] {
            let mesh = path_mesh(
                points,
                PathStyle::Straight,
                true,
                None,
                LineEnd::None,
                LineEnd::None,
                &PathMeshSpec {
                    color: Color32::WHITE,
                    width: 2.0,
                    dash: DashStyle::Solid,
                    fill: Some(Color32::from_rgb(10, 20, 30)),
                    pixels_per_point: 1.0,
                },
            );
            assert!(!mesh.is_empty());
        }
    }
}
