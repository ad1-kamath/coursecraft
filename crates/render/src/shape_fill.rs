//! Even-odd polygon filling with a one-pixel feather fringe.
//!
//! epaint's [`egui::epaint::PathShape`] only fills convex paths, so shapes use
//! this module instead. Ear clipping with hole bridging handles concave outlines
//! and holes; nothing here panics on hostile input, and the loops are capped so
//! bad geometry ends rather than hangs.

use egui::{Color32, Mesh, pos2};

pub use crate::shape_geometry::Pt;

const EPS: f32 = 1e-6;

fn cross(a: Pt, b: Pt) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

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

fn distance(a: Pt, b: Pt) -> f32 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn finite(p: Pt) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

fn signed_area(poly: &[Pt]) -> f32 {
    let mut sum = 0.0;
    for (i, &a) in poly.iter().enumerate() {
        let Some(&b) = poly.get((i + 1) % poly.len()) else {
            return 0.0;
        };
        sum += cross(a, b);
    }
    sum * 0.5
}

fn point_in_polygon(point: Pt, poly: &[Pt]) -> bool {
    if poly.len() < 3 || !finite(point) {
        return false;
    }
    let mut inside = false;
    for (i, &a) in poly.iter().enumerate() {
        let Some(&b) = poly.get((i + 1) % poly.len()) else {
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

fn clean_contour(points: &[Pt]) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
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
    }
    if out.len() > 2
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(&first, &last)| distance(first, last) <= EPS)
    {
        out.pop();
    }
    // Drop vertices that sit exactly on the line between their neighbours.
    let mut changed = true;
    while changed && out.len() > 3 {
        changed = false;
        for i in 0..out.len() {
            let n = out.len();
            let Some(&prev) = out.get((i + n - 1) % n) else {
                continue;
            };
            let Some(&cur) = out.get(i) else {
                continue;
            };
            let Some(&next) = out.get((i + 1) % n) else {
                continue;
            };
            let a = sub(cur, prev);
            let b = sub(next, cur);
            if cross(a, b).abs() <= EPS && dot(a, b) >= 0.0 {
                out.remove(i);
                changed = true;
                break;
            }
        }
    }
    out
}

fn clean_contours(contours: &[Vec<Pt>]) -> Vec<Vec<Pt>> {
    contours
        .iter()
        .map(|contour| clean_contour(contour))
        .filter(|contour| contour.len() >= 3)
        .collect()
}

fn orient(poly: &mut [Pt], want_ccw: bool) {
    if poly.len() < 3 {
        return;
    }
    let area = signed_area(poly);
    if (area > 0.0) != want_ccw {
        poly.reverse();
    }
}

fn on_segment(a: Pt, b: Pt, p: Pt) -> bool {
    let ab = sub(b, a);
    let ap = sub(p, a);
    cross(ab, ap).abs() <= EPS
        && p.0 >= a.0.min(b.0) - EPS
        && p.0 <= a.0.max(b.0) + EPS
        && p.1 >= a.1.min(b.1) - EPS
        && p.1 <= a.1.max(b.1) + EPS
}

fn segments_touch(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let ab_c = cross(sub(b, a), sub(c, a));
    let ab_d = cross(sub(b, a), sub(d, a));
    let cd_a = cross(sub(d, c), sub(a, c));
    let cd_b = cross(sub(d, c), sub(b, c));

    let opposite = |p: f32, q: f32| (p > EPS && q < -EPS) || (p < -EPS && q > EPS);
    if opposite(ab_c, ab_d) && opposite(cd_a, cd_b) {
        return true;
    }
    (ab_c.abs() <= EPS && on_segment(a, b, c))
        || (ab_d.abs() <= EPS && on_segment(a, b, d))
        || (cd_a.abs() <= EPS && on_segment(c, d, a))
        || (cd_b.abs() <= EPS && on_segment(c, d, b))
}

fn point_in_triangle(p: Pt, a: Pt, b: Pt, c: Pt) -> bool {
    let c1 = cross(sub(b, a), sub(p, a));
    let c2 = cross(sub(c, b), sub(p, b));
    let c3 = cross(sub(a, c), sub(p, c));
    c1 > EPS && c2 > EPS && c3 > EPS
}

/// Return `true` when `points` is a strictly convex, simple, counter-clockwise
/// polygon (no collinear/reflex vertices and exterior turning angle sums to `2π`,
/// rejecting self-intersecting star polygons).
pub(crate) fn is_strictly_convex_ccw(points: &[Pt]) -> bool {
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut total_turn = 0.0_f64;
    for i in 0..n {
        let (Some(&a), Some(&b), Some(&c)) = (
            points.get(i),
            points.get((i + 1) % n),
            points.get((i + 2) % n),
        ) else {
            return false;
        };
        if !finite(a) || !finite(b) || !finite(c) {
            return false;
        }
        let e1 = sub(b, a);
        let e2 = sub(c, b);
        let cr = cross(e1, e2);
        let dt = dot(e1, e2);
        if !cr.is_finite() || !dt.is_finite() || cr <= EPS {
            return false;
        }
        let turn = cr.atan2(dt);
        if !turn.is_finite() || turn <= 0.0 {
            return false;
        }
        total_turn += f64::from(turn);
    }
    (total_turn - std::f64::consts::TAU).abs() <= 1e-2
}

/// Triangulate one simple polygon with a convex fan fast-path or ear clipping.
/// Returns triangles in counter-clockwise order and never loops more than a fixed cap.
fn triangulate_polygon(poly: &[Pt]) -> Vec<[Pt; 3]> {
    let mut points = clean_contour(poly);
    if points.len() < 3 {
        return Vec::new();
    }
    if signed_area(&points) < 0.0 {
        points.reverse();
    }
    if is_strictly_convex_ccw(&points) {
        let Some(&first) = points.first() else {
            return Vec::new();
        };
        let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
        for pair in points.get(1..).unwrap_or_default().windows(2) {
            if let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) {
                triangles.push([first, a, b]);
            }
        }
        return triangles;
    }
    let total = points.len();
    let mut indices: Vec<usize> = (0..total).collect();
    let mut triangles = Vec::new();
    let cap = total
        .saturating_mul(total)
        .saturating_mul(4)
        .saturating_add(64);
    let mut guard = 0usize;

    while indices.len() > 3 && guard < cap {
        guard = guard.saturating_add(1);
        let count = indices.len();
        let mut clipped = false;
        for k in 0..count {
            let Some(&i_prev) = indices.get((k + count - 1) % count) else {
                continue;
            };
            let Some(&i_cur) = indices.get(k) else {
                continue;
            };
            let Some(&i_next) = indices.get((k + 1) % count) else {
                continue;
            };
            let Some(&a) = points.get(i_prev) else {
                continue;
            };
            let Some(&b) = points.get(i_cur) else {
                continue;
            };
            let Some(&c) = points.get(i_next) else {
                continue;
            };
            if cross(sub(b, a), sub(c, b)) <= EPS {
                continue;
            }
            let mut ear = true;
            for m in 0..count {
                if m == k || m == (k + count - 1) % count || m == (k + 1) % count {
                    continue;
                }
                let Some(&index) = indices.get(m) else {
                    continue;
                };
                let Some(&p) = points.get(index) else {
                    continue;
                };
                if point_in_triangle(p, a, b, c) {
                    ear = false;
                    break;
                }
            }
            if ear {
                triangles.push([a, b, c]);
                indices.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Bad or degenerate input: clip any convex vertex so progress is
            // guaranteed, then let the cap end the loop.
            for k in 0..count {
                let Some(&i_prev) = indices.get((k + count - 1) % count) else {
                    continue;
                };
                let Some(&i_cur) = indices.get(k) else {
                    continue;
                };
                let Some(&i_next) = indices.get((k + 1) % count) else {
                    continue;
                };
                let (Some(&a), Some(&b), Some(&c)) =
                    (points.get(i_prev), points.get(i_cur), points.get(i_next))
                else {
                    continue;
                };
                if cross(sub(b, a), sub(c, b)) > EPS {
                    triangles.push([a, b, c]);
                    indices.remove(k);
                    clipped = true;
                    break;
                }
            }
        }
        if !clipped {
            break;
        }
    }

    if indices.len() == 3 {
        let (Some(&a), Some(&b), Some(&c)) = (
            indices.first().and_then(|i| points.get(*i)),
            indices.get(1).and_then(|i| points.get(*i)),
            indices.get(2).and_then(|i| points.get(*i)),
        ) else {
            return triangles;
        };
        triangles.push([a, b, c]);
    } else if indices.len() > 3 {
        // Last resort for hostile input: fan the remainder. This is still a
        // bounded amount of work and never panics.
        let Some(&first) = indices.first().and_then(|i| points.get(*i)) else {
            return triangles;
        };
        for pair in indices.windows(2).skip(1) {
            let (Some(&a), Some(&b)) = (
                pair.first().and_then(|i| points.get(*i)),
                pair.get(1).and_then(|i| points.get(*i)),
            ) else {
                continue;
            };
            triangles.push([first, a, b]);
        }
    }
    triangles
}

fn bridge_is_clear(
    hole_point: Pt,
    outer_point: Pt,
    outer: &[Pt],
    outer_index: usize,
    hole: &[Pt],
    hole_index: usize,
    other_holes: &[Vec<Pt>],
) -> bool {
    let mid = (
        (hole_point.0 + outer_point.0) * 0.5,
        (hole_point.1 + outer_point.1) * 0.5,
    );
    if !point_in_polygon(mid, outer) || point_in_polygon(mid, hole) {
        return false;
    }
    for other in other_holes {
        if point_in_polygon(mid, other) {
            return false;
        }
    }
    for (i, &a) in outer.iter().enumerate() {
        if i == outer_index || (i + 1) % outer.len() == outer_index {
            continue;
        }
        let Some(&b) = outer.get((i + 1) % outer.len()) else {
            continue;
        };
        if segments_touch(hole_point, outer_point, a, b) {
            return false;
        }
    }
    for (i, &a) in hole.iter().enumerate() {
        if i == hole_index || (i + 1) % hole.len() == hole_index {
            continue;
        }
        let Some(&b) = hole.get((i + 1) % hole.len()) else {
            continue;
        };
        if segments_touch(hole_point, outer_point, a, b) {
            return false;
        }
    }
    for other in other_holes {
        for (i, &a) in other.iter().enumerate() {
            let Some(&b) = other.get((i + 1) % other.len()) else {
                continue;
            };
            if segments_touch(hole_point, outer_point, a, b) {
                return false;
            }
        }
    }
    true
}

fn find_bridge(outer: &[Pt], hole: &[Pt], other_holes: &[Vec<Pt>]) -> Option<(usize, usize)> {
    let mut best: Option<(f32, usize, usize)> = None;
    for (hi, &hole_point) in hole.iter().enumerate() {
        for (oi, &outer_point) in outer.iter().enumerate() {
            let dx = hole_point.0 - outer_point.0;
            let dy = hole_point.1 - outer_point.1;
            let d2 = dx * dx + dy * dy;
            if best.is_some_and(|(best_d2, ..)| d2 >= best_d2) {
                continue;
            }
            if bridge_is_clear(hole_point, outer_point, outer, oi, hole, hi, other_holes) {
                best = Some((d2, oi, hi));
            }
        }
    }
    best.map(|(_, oi, hi)| (oi, hi))
}

fn splice_hole(outer: &[Pt], hole: &[Pt], outer_index: usize, hole_index: usize) -> Vec<Pt> {
    let Some(&outer_point) = outer.get(outer_index) else {
        return outer.to_vec();
    };
    let mut merged = Vec::with_capacity(outer.len() + hole.len() + 2);
    merged.extend_from_slice(&outer[..=outer_index.min(outer.len().saturating_sub(1))]);
    if hole.is_empty() {
        return outer.to_vec();
    }
    let hole_index = hole_index.min(hole.len().saturating_sub(1));
    for k in 0..hole.len() {
        if let Some(&point) = hole.get((hole_index + k) % hole.len()) {
            merged.push(point);
        }
    }
    if let Some(&point) = hole.get(hole_index) {
        merged.push(point);
    }
    merged.push(outer_point);
    if let Some(rest) = outer.get(outer_index.saturating_add(1)..) {
        merged.extend_from_slice(rest);
    }
    merged
}

fn merge_holes(mut outer: Vec<Pt>, holes: Vec<Vec<Pt>>) -> Vec<Pt> {
    let mut remaining: Vec<Vec<Pt>> = holes;
    let mut guard = 0usize;
    while !remaining.is_empty() && guard < remaining.len().saturating_add(32) {
        guard = guard.saturating_add(1);
        let hole = remaining.remove(0);
        let others = remaining.clone();
        if let Some((outer_index, hole_index)) = find_bridge(&outer, &hole, &others) {
            outer = splice_hole(&outer, &hole, outer_index, hole_index);
        }
    }
    outer
}

/// The even-odd area of a set of contours, using nesting parity.
#[must_use]
pub fn even_odd_area(contours: &[Vec<Pt>]) -> f32 {
    let cleaned = clean_contours(contours);
    let mut total = 0.0;
    for (i, contour) in cleaned.iter().enumerate() {
        let Some(&sample) = contour.first() else {
            continue;
        };
        let depth = cleaned
            .iter()
            .enumerate()
            .filter(|(j, other)| *j != i && point_in_polygon(sample, other))
            .count();
        let area = signed_area(contour).abs();
        if depth % 2 == 0 {
            total += area;
        } else {
            total -= area;
        }
    }
    total.abs()
}

/// Triangulate `contours` with the even-odd rule and return a mesh.
///
/// `color` already carries dimming and opacity. `pixels_per_point` sizes the
/// anti-aliasing fringe: one physical pixel, so points already in logical units
/// stay one pixel wide at every zoom. A non-finite or non-positive
/// `pixels_per_point` turns the fringe off, which keeps tests and callers that
/// pass frame units exact.
#[must_use]
pub fn fill_contours(contours: &[Vec<Pt>], color: Color32, pixels_per_point: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let cleaned = clean_contours(contours);
    if cleaned.is_empty() {
        return mesh;
    }

    let mut depth = Vec::with_capacity(cleaned.len());
    for (i, contour) in cleaned.iter().enumerate() {
        let Some(&sample) = contour.first() else {
            depth.push(0);
            continue;
        };
        let nesting = cleaned
            .iter()
            .enumerate()
            .filter(|(j, other)| *j != i && point_in_polygon(sample, other))
            .count();
        depth.push(nesting);
    }

    let mut oriented = cleaned.clone();
    for (i, contour) in oriented.iter_mut().enumerate() {
        let is_hole = depth.get(i).copied().unwrap_or(0) % 2 == 1;
        orient(contour, !is_hole);
    }

    // Group each hole with the smallest even-parity contour that contains it.
    let mut outers: Vec<(usize, Vec<Pt>)> = Vec::new();
    let mut holes: Vec<Vec<Pt>> = Vec::new();
    let mut hole_targets: Vec<usize> = Vec::new();
    // Collect every outer first: a hole may be listed before its outer.
    for (i, contour) in oriented.iter().enumerate() {
        if depth.get(i).copied().unwrap_or(0) % 2 == 0 {
            outers.push((i, contour.clone()));
        }
    }
    for (i, contour) in oriented.iter().enumerate() {
        if depth.get(i).copied().unwrap_or(0) % 2 == 0 {
            continue;
        }
        let Some(&sample) = contour.first() else {
            continue;
        };
        let mut best: Option<(f32, usize)> = None;
        for (outer_position, (_, outer)) in outers.iter().enumerate() {
            if point_in_polygon(sample, outer) {
                let area = signed_area(outer).abs();
                if best.is_none_or(|(best_area, _)| area < best_area) {
                    best = Some((area, outer_position));
                }
            }
        }
        if let Some((_, target)) = best {
            holes.push(contour.clone());
            hole_targets.push(target);
        }
    }

    let mut groups: Vec<Vec<Vec<Pt>>> = vec![Vec::new(); outers.len()];
    for (hole, &target) in holes.iter().zip(hole_targets.iter()) {
        if let Some(group) = groups.get_mut(target) {
            group.push(hole.clone());
        }
    }

    // The fringe goes down first so the solid triangles drawn afterwards cover
    // any fringe that reaches into the material at a sharp inner corner. With
    // the inclusive edge test the two layers meet exactly on the outline.
    let feather = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        1.0 / pixels_per_point
    } else {
        0.0
    };
    if feather > 0.0 && color.a() > 0 {
        add_fringe(&mut mesh, &oriented, color, feather);
    }

    for ((_, outer), group) in outers.into_iter().zip(groups) {
        let merged = merge_holes(outer, group);
        for triangle in triangulate_polygon(&merged) {
            push_triangle(&mut mesh, triangle, color);
        }
    }

    mesh
}

/// Upper bound on vertices built for a shaded gradient fill.
pub const MAX_SHADED_FILL_VERTICES: usize = 24_000;

/// Triangulate `contours` and colour every vertex from `color_at`.
///
/// Triangles are split until their longest edge is at most `max_edge` points,
/// so a multi-stop gradient has enough samples to look smooth. The vertex count
/// is capped at [`MAX_SHADED_FILL_VERTICES`], so a hostile or huge shape stays
/// bounded. The feather fringe is kept and shaded the same way.
#[must_use]
pub fn fill_contours_shaded(
    contours: &[Vec<Pt>],
    pixels_per_point: f32,
    max_edge: f32,
    color_at: impl Fn(Pt) -> Color32,
) -> Mesh {
    let mut mesh = fill_contours(contours, Color32::WHITE, pixels_per_point);
    subdivide_mesh(&mut mesh, max_edge);
    for vertex in &mut mesh.vertices {
        let coverage = vertex.color.a();
        let mut color = color_at((vertex.pos.x, vertex.pos.y));
        if coverage < 255 {
            color = color.gamma_multiply(f32::from(coverage) / 255.0);
        }
        vertex.color = color;
    }
    mesh
}

fn lerp_vertex(a: &egui::epaint::Vertex, b: &egui::epaint::Vertex, u: f32) -> egui::epaint::Vertex {
    let u = u.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * u).round() as u8;
    egui::epaint::Vertex {
        pos: pos2(
            a.pos.x + (b.pos.x - a.pos.x) * u,
            a.pos.y + (b.pos.y - a.pos.y) * u,
        ),
        uv: pos2(
            a.uv.x + (b.uv.x - a.uv.x) * u,
            a.uv.y + (b.uv.y - a.uv.y) * u,
        ),
        color: Color32::from_rgba_premultiplied(
            mix(a.color.r(), b.color.r()),
            mix(a.color.g(), b.color.g()),
            mix(a.color.b(), b.color.b()),
            mix(a.color.a(), b.color.a()),
        ),
    }
}

fn split_mesh_at_cuts(mesh: &mut Mesh, split_ts: &[f32], t_at: &impl Fn(Pt) -> f32) {
    const CUT_EPS: f32 = 1e-5;
    for &cut in split_ts {
        if !cut.is_finite() || mesh.indices.len() < 3 {
            continue;
        }
        let mut out = Mesh {
            texture_id: mesh.texture_id,
            ..Mesh::default()
        };
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let (Some(a), Some(b), Some(c)) = (
                mesh.vertices.get(triangle[0] as usize).copied(),
                mesh.vertices.get(triangle[1] as usize).copied(),
                mesh.vertices.get(triangle[2] as usize).copied(),
            ) else {
                continue;
            };
            let ta = t_at((a.pos.x, a.pos.y));
            let tb = t_at((b.pos.x, b.pos.y));
            let tc = t_at((c.pos.x, c.pos.y));
            let has_below = ta < cut - CUT_EPS || tb < cut - CUT_EPS || tc < cut - CUT_EPS;
            let has_above = ta > cut + CUT_EPS || tb > cut + CUT_EPS || tc > cut + CUT_EPS;
            if !has_below || !has_above || out.vertices.len() + 9 > MAX_SHADED_FILL_VERTICES {
                let base = out.vertices.len() as u32;
                out.vertices.extend([a, b, c]);
                out.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
                continue;
            }
            let verts = [(a, ta), (b, tb), (c, tc)];
            // Case 1: one vertex is on the cut, the other two are on opposite sides.
            if let Some(on_idx) = (0..3).find(|&i| (verts[i].1 - cut).abs() <= CUT_EPS) {
                let (v0, _) = verts[on_idx];
                let (v1, t1) = verts[(on_idx + 1) % 3];
                let (v2, t2) = verts[(on_idx + 2) % 3];
                let u = ((cut - t1) / (t2 - t1)).clamp(0.0, 1.0);
                let p = lerp_vertex(&v1, &v2, u);
                let base = out.vertices.len() as u32;
                out.vertices.extend([v0, v1, p, v0, p, v2]);
                out.add_triangle(base, base + 1, base + 2);
                out.add_triangle(base + 3, base + 4, base + 5);
                continue;
            }
            // Case 2: one vertex is alone on its side, two are on the other side.
            let below_count = usize::from(ta < cut) + usize::from(tb < cut) + usize::from(tc < cut);
            let want_below = below_count == 1;
            let lone_idx = (0..3)
                .find(|&i| (verts[i].1 < cut) == want_below)
                .unwrap_or(0);
            let (v0, t0) = verts[lone_idx];
            let (v1, t1) = verts[(lone_idx + 1) % 3];
            let (v2, t2) = verts[(lone_idx + 2) % 3];
            let p1 = lerp_vertex(&v0, &v1, (cut - t0) / (t1 - t0));
            let p2 = lerp_vertex(&v0, &v2, (cut - t0) / (t2 - t0));
            let base = out.vertices.len() as u32;
            out.vertices.extend([v0, p1, p2, p1, v1, v2, p1, v2, p2]);
            out.add_triangle(base, base + 1, base + 2);
            out.add_triangle(base + 3, base + 4, base + 5);
            out.add_triangle(base + 6, base + 7, base + 8);
        }
        *mesh = out;
    }
}

/// Triangulate `contours`, split triangles at hard-step iso-lines `split_ts`,
/// and colour every triangle's vertices from `color_at(point, tri_mid_t)`.
#[must_use]
pub fn fill_contours_linear_shaded(
    contours: &[Vec<Pt>],
    pixels_per_point: f32,
    max_edge: f32,
    split_ts: &[f32],
    t_at: impl Fn(Pt) -> f32,
    color_at: impl Fn(Pt, f32) -> Color32,
) -> Mesh {
    let mut mesh = fill_contours(contours, Color32::WHITE, pixels_per_point);
    subdivide_mesh(&mut mesh, max_edge);
    if !split_ts.is_empty() {
        split_mesh_at_cuts(&mut mesh, split_ts, &t_at);
    }
    for tri in mesh.indices.as_chunks::<3>().0 {
        let (Some(i0), Some(i1), Some(i2)) = (
            Some(tri[0] as usize),
            Some(tri[1] as usize),
            Some(tri[2] as usize),
        ) else {
            continue;
        };
        if i2 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i0 >= mesh.vertices.len() {
            continue;
        }
        let t0 = t_at((mesh.vertices[i0].pos.x, mesh.vertices[i0].pos.y));
        let t1 = t_at((mesh.vertices[i1].pos.x, mesh.vertices[i1].pos.y));
        let t2 = t_at((mesh.vertices[i2].pos.x, mesh.vertices[i2].pos.y));
        let tri_mid_t = (t0 + t1 + t2) / 3.0;
        for idx in [i0, i1, i2] {
            let v = &mut mesh.vertices[idx];
            let coverage = v.color.a();
            let mut color = color_at((v.pos.x, v.pos.y), tri_mid_t);
            if coverage < 255 {
                color = color.gamma_multiply(f32::from(coverage) / 255.0);
            }
            v.color = color;
        }
    }
    mesh
}

/// Triangulate `contours` and set each vertex's texture coordinates from `uv_at`.
///
/// The mesh keeps its default (white) colour, so the caller can tint it by
/// multiplying `Vertex::color` afterwards. Used by picture fills clipped to a
/// shape outline.
#[must_use]
pub fn fill_contours_uv(
    contours: &[Vec<Pt>],
    pixels_per_point: f32,
    uv_at: impl Fn(Pt) -> (f32, f32),
) -> Mesh {
    let mut mesh = fill_contours(contours, Color32::WHITE, pixels_per_point);
    for vertex in &mut mesh.vertices {
        let (u, v) = uv_at((vertex.pos.x, vertex.pos.y));
        vertex.uv = pos2(
            if u.is_finite() { u } else { 0.0 },
            if v.is_finite() { v } else { 0.0 },
        );
    }
    mesh
}

fn midpoint(a: &egui::epaint::Vertex, b: &egui::epaint::Vertex) -> egui::epaint::Vertex {
    let mix = |x: u8, y: u8| ((u16::from(x) + u16::from(y)) / 2) as u8;
    egui::epaint::Vertex {
        pos: pos2((a.pos.x + b.pos.x) * 0.5, (a.pos.y + b.pos.y) * 0.5),
        uv: pos2((a.uv.x + b.uv.x) * 0.5, (a.uv.y + b.uv.y) * 0.5),
        color: Color32::from_rgba_premultiplied(
            mix(a.color.r(), b.color.r()),
            mix(a.color.g(), b.color.g()),
            mix(a.color.b(), b.color.b()),
            mix(a.color.a(), b.color.a()),
        ),
    }
}

fn subdivide_mesh(mesh: &mut Mesh, max_edge: f32) {
    if !max_edge.is_finite() || max_edge <= 0.0 || mesh.indices.len() < 3 {
        return;
    }
    let max_edge_sq = max_edge * max_edge;
    let texture_id = mesh.texture_id;
    let mut out = Mesh {
        texture_id,
        ..Mesh::default()
    };
    let mut queue: std::collections::VecDeque<[egui::epaint::Vertex; 3]> =
        std::collections::VecDeque::new();
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let (Some(a), Some(b), Some(c)) = (
            mesh.vertices.get(triangle[0] as usize).copied(),
            mesh.vertices.get(triangle[1] as usize).copied(),
            mesh.vertices.get(triangle[2] as usize).copied(),
        ) else {
            continue;
        };
        queue.push_back([a, b, c]);
    }
    while let Some([a, b, c]) = queue.pop_front() {
        let ab = (a.pos.x - b.pos.x).powi(2) + (a.pos.y - b.pos.y).powi(2);
        let bc = (b.pos.x - c.pos.x).powi(2) + (b.pos.y - c.pos.y).powi(2);
        let ca = (c.pos.x - a.pos.x).powi(2) + (c.pos.y - a.pos.y).powi(2);
        let longest = ab.max(bc).max(ca);
        if longest > max_edge_sq
            && out.vertices.len() + queue.len() * 3 + 3 <= MAX_SHADED_FILL_VERTICES
        {
            if longest == ab {
                let m = midpoint(&a, &b);
                queue.push_back([a, m, c]);
                queue.push_back([m, b, c]);
            } else if longest == bc {
                let m = midpoint(&b, &c);
                queue.push_back([a, b, m]);
                queue.push_back([a, m, c]);
            } else {
                let m = midpoint(&c, &a);
                queue.push_back([a, b, m]);
                queue.push_back([m, b, c]);
            }
        } else {
            let base = out.vertices.len() as u32;
            out.vertices.push(a);
            out.vertices.push(b);
            out.vertices.push(c);
            out.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
        }
    }
    *mesh = out;
}

/// Default miter limit (half-widths) matching PowerPoint's `<a:miter lim="800000"/>`.
const DEFAULT_MITER_LIMIT: f32 = 8.0;
/// Segments in one round dot.
const DOT_SEGMENTS: usize = 12;

/// How corners of a stroked path are joined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeJoin {
    pub kind: coursecraft_course::LineJoin,
    pub miter_limit: f32,
}

impl Default for StrokeJoin {
    fn default() -> Self {
        Self {
            kind: coursecraft_course::LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        }
    }
}

impl StrokeJoin {
    /// Build a [`StrokeJoin`] from an outline's join kind and optional miter limit.
    #[must_use]
    pub fn from_outline(kind: coursecraft_course::LineJoin, miter_limit: Option<f32>) -> Self {
        let limit = miter_limit
            .filter(|v| v.is_finite() && *v >= 1.0)
            .unwrap_or(DEFAULT_MITER_LIMIT);
        Self {
            kind,
            miter_limit: limit,
        }
    }
}

/// Build a mesh for a band of `width` centred on each closed contour.
///
/// Every contour, including a hole, gets its own closed band. Joins are mitred
/// up to [`DEFAULT_MITER_LIMIT`] and bevelled past it. `dash` breaks the band into
/// dashes, round dots or dash-dot groups. The band is a single ring polygon fed
/// through [`fill_contours`], so it gets the same one-pixel feather and never
/// overlaps itself at a join. Non-finite or non-positive widths draw nothing.
#[must_use]
pub fn stroke_contours(
    contours: &[Vec<Pt>],
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    pixels_per_point: f32,
) -> Mesh {
    stroke_contours_pattern(contours, color, width, dash, None, pixels_per_point)
}

/// Like [`stroke_contours`], with an optional custom alternating
/// `[dash, gap, ...]` pattern in multiples of `width`.
#[must_use]
pub fn stroke_contours_pattern(
    contours: &[Vec<Pt>],
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: Option<&[f32]>,
    pixels_per_point: f32,
) -> Mesh {
    stroke_contours_with_join(
        contours,
        color,
        width,
        dash,
        dash_pattern,
        StrokeJoin::default(),
        pixels_per_point,
    )
}

/// Like [`stroke_contours_pattern`], with an explicit [`StrokeJoin`].
#[must_use]
pub fn stroke_contours_with_join(
    contours: &[Vec<Pt>],
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: Option<&[f32]>,
    join: StrokeJoin,
    pixels_per_point: f32,
) -> Mesh {
    let mut mesh = Mesh::default();
    if !width.is_finite() || width <= 0.0 {
        return mesh;
    }
    let half = width * 0.5;
    if !half.is_finite() || half <= 0.0 {
        return mesh;
    }
    let cleaned = clean_contours(contours);
    if cleaned.is_empty() {
        return mesh;
    }
    let feather = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        1.0 / pixels_per_point
    } else {
        0.0
    };
    for contour in &cleaned {
        add_patterned_stroke(
            &mut mesh,
            contour,
            true,
            width,
            half,
            feather,
            color,
            pixels_per_point,
            dash,
            dash_pattern,
            join,
        );
    }
    mesh
}

/// Build a mesh for a band of `width` centred on an open or closed polyline.
///
/// This is the path tool's stroke: the same band builder, dash and dot rules as
/// [`stroke_contours`], but the polyline may stay open. Non-finite or
/// non-positive widths draw nothing.
#[must_use]
pub fn stroke_path(
    points: &[Pt],
    closed: bool,
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    pixels_per_point: f32,
) -> Mesh {
    stroke_path_pattern(points, closed, color, width, dash, None, pixels_per_point)
}

/// Like [`stroke_path`], with an optional custom alternating `[dash, gap, ...]`
/// pattern in multiples of `width`.
#[must_use]
pub fn stroke_path_pattern(
    points: &[Pt],
    closed: bool,
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: Option<&[f32]>,
    pixels_per_point: f32,
) -> Mesh {
    stroke_path_with_join(
        points,
        closed,
        color,
        width,
        dash,
        dash_pattern,
        StrokeJoin::default(),
        pixels_per_point,
    )
}

/// Like [`stroke_path_pattern`], with an explicit [`StrokeJoin`].
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn stroke_path_with_join(
    points: &[Pt],
    closed: bool,
    color: Color32,
    width: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: Option<&[f32]>,
    join: StrokeJoin,
    pixels_per_point: f32,
) -> Mesh {
    let mut mesh = Mesh::default();
    if !width.is_finite() || width <= 0.0 {
        return mesh;
    }
    let half = width * 0.5;
    if !half.is_finite() || half <= 0.0 {
        return mesh;
    }
    let cleaned = clean_open_or_closed(points, closed);
    if cleaned.len() < 2 {
        return mesh;
    }
    let feather = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        1.0 / pixels_per_point
    } else {
        0.0
    };
    add_patterned_stroke(
        &mut mesh,
        &cleaned,
        closed && cleaned.len() > 2,
        width,
        half,
        feather,
        color,
        pixels_per_point,
        dash,
        dash_pattern,
        join,
    );
    mesh
}

#[allow(clippy::too_many_arguments)]
fn add_patterned_stroke(
    mesh: &mut Mesh,
    points: &[Pt],
    closed: bool,
    width: f32,
    half: f32,
    feather: f32,
    color: Color32,
    pixels_per_point: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: Option<&[f32]>,
    join: StrokeJoin,
) {
    if dash == coursecraft_course::DashStyle::Solid {
        add_band(mesh, points, closed, half, feather, color, join);
        return;
    }
    let pattern: &[f32] = match dash_pattern {
        Some(p)
            if p.len() >= 2 && p.len() % 2 == 0 && p.iter().all(|v| v.is_finite() && *v > 0.0) =>
        {
            p
        }
        _ => dash.default_pattern(),
    };
    if pattern.len() < 2 {
        add_band(mesh, points, closed, half, feather, color, join);
        return;
    }
    let segs = segments(points, closed);
    if segs.is_empty() {
        return;
    }
    let pair_count = pattern.len() / 2;
    let mut pair_idx = 0usize;
    let mut in_dash = true;
    let mut phase = 0.0f32;
    let mut current: Vec<Pt> = Vec::new();
    let mut dot_mid_recorded = false;

    for (a, b, len) in segs {
        if !len.is_finite() || len <= EPS {
            continue;
        }
        let mut t = 0.0f32;
        let mut guard = 0usize;
        while t < len - EPS && guard < 4096 {
            guard = guard.saturating_add(1);
            let d_mult = pattern[pair_idx * 2];
            let g_mult = pattern[pair_idx * 2 + 1];
            let is_dot = d_mult <= 1.05;
            let phase_len = (if in_dash { d_mult } else { g_mult }) * width;
            if !phase_len.is_finite() || phase_len <= EPS {
                if !in_dash {
                    pair_idx = (pair_idx + 1) % pair_count;
                }
                in_dash = !in_dash;
                phase = 0.0;
                dot_mid_recorded = false;
                continue;
            }
            let remaining = (phase_len - phase).max(0.0);
            let step = remaining.min(len - t);
            if in_dash {
                if is_dot {
                    let half_dash = phase_len * 0.5;
                    if !dot_mid_recorded && phase + step >= half_dash - EPS {
                        let dt = coursecraft_course::safe_clamp(half_dash - phase, 0.0, step);
                        let center = lerp(a, b, ((t + dt) / len).clamp(0.0, 1.0));
                        let dot_half = half * d_mult.clamp(0.4, 1.0);
                        let dot = circle_contour(center.0, center.1, dot_half, DOT_SEGMENTS);
                        if dot.len() >= 3 {
                            let filled = fill_contours(&[dot], color, pixels_per_point);
                            append_mesh(mesh, &filled);
                        }
                        dot_mid_recorded = true;
                    }
                } else {
                    let p0 = lerp(a, b, t / len);
                    let p1 = lerp(a, b, (t + step) / len);
                    if current
                        .last()
                        .is_none_or(|last: &Pt| distance(*last, p0) > EPS)
                    {
                        current.push(p0);
                    }
                    if current
                        .last()
                        .is_none_or(|last: &Pt| distance(*last, p1) > EPS)
                    {
                        current.push(p1);
                    }
                }
            }
            t += step;
            phase += step;
            if phase >= phase_len - EPS {
                if in_dash {
                    if !is_dot && current.len() >= 2 {
                        add_band(mesh, &current, false, half, feather, color, join);
                    }
                    current.clear();
                    dot_mid_recorded = false;
                } else {
                    pair_idx = (pair_idx + 1) % pair_count;
                }
                in_dash = !in_dash;
                phase = 0.0;
            }
        }
    }
    if in_dash && current.len() >= 2 {
        add_band(mesh, &current, false, half, feather, color, join);
    }
}

/// Drop non-finite points and points that sit on their predecessor. A closed
/// path also drops a final point that touches the first.
fn clean_open_or_closed(points: &[Pt], closed: bool) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
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

fn circle_contour(cx: f32, cy: f32, radius: f32, segments: usize) -> Vec<Pt> {
    let segments = segments.clamp(3, 64);
    let mut out = Vec::with_capacity(segments);
    let step = std::f32::consts::TAU / segments as f32;
    for i in 0..segments {
        #[allow(clippy::cast_precision_loss)]
        let angle = step * i as f32;
        out.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
    }
    out
}

fn lerp(a: Pt, b: Pt, t: f32) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn segments(points: &[Pt], closed: bool) -> Vec<(Pt, Pt, f32)> {
    let mut out = Vec::new();
    for (i, &a) in points.iter().enumerate() {
        let next = if i + 1 < points.len() {
            points.get(i + 1).copied()
        } else if closed {
            points.first().copied()
        } else {
            None
        };
        let Some(b) = next else {
            continue;
        };
        let len = distance(a, b);
        if len.is_finite() && len > EPS {
            out.push((a, b, len));
        }
    }
    out
}

/// One cross-section of a band: its two edges, either side of the path.
#[derive(Clone, Copy)]
struct Station {
    centre: Pt,
    plus: Pt,
    minus: Pt,
}

/// Unit direction from `a` to `b`, or `None` for points on top of each other.
fn direction(a: Pt, b: Pt) -> Option<Pt> {
    let edge = sub(b, a);
    let len = length(edge);
    (len.is_finite() && len > EPS).then(|| mul(edge, 1.0 / len))
}

/// The band's side for a travel direction. A clockwise turn on screen has this
/// side on the outside of the corner.
fn side(d: Pt) -> Pt {
    (d.1, -d.0)
}

/// Cross-sections at one point of the path: one for a mitred corner, two for a bevel,
/// or an arc of stations for a round join.
fn corner_stations(
    prev: Option<Pt>,
    cur: Pt,
    next: Option<Pt>,
    half: f32,
    join: StrokeJoin,
) -> Vec<Station> {
    let incoming = prev.and_then(|prev| direction(prev, cur));
    let outgoing = next.and_then(|next| direction(cur, next));
    let station = |normal: Pt, along: f32, inner: f32| Station {
        centre: cur,
        plus: add(cur, mul(normal, along)),
        minus: sub(cur, mul(normal, inner)),
    };
    let (d_in, d_out) = match (incoming, outgoing) {
        (Some(d_in), Some(d_out)) => (d_in, d_out),
        (Some(d), None) | (None, Some(d)) => return vec![station(side(d), half, half)],
        (None, None) => return Vec::new(),
    };
    let (n_in, n_out) = (side(d_in), side(d_out));
    let sum = add(n_in, n_out);
    let sum_len = length(sum);
    // How far the corner's inside edge can reach before it passes the shorter
    // neighbouring segment.
    let reach = |other: Option<Pt>| other.map_or(half, |other| distance(cur, other));
    let inner_limit = reach(prev).min(reach(next)).max(half.min(1.0));
    if sum_len <= EPS {
        // The path doubles straight back on itself.
        if join.kind == coursecraft_course::LineJoin::Round {
            let a0 = n_in.1.atan2(n_in.0);
            let steps = 6usize;
            return (0..=steps)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let angle = a0 + std::f32::consts::PI * (i as f32 / steps as f32);
                    station((angle.cos(), angle.sin()), half, half)
                })
                .collect();
        }
        return vec![station(n_in, half, half), station(n_out, half, half)];
    }
    let miter = mul(sum, 1.0 / sum_len);
    let cos = dot(miter, n_in).max(EPS);
    let miter_len = half / cos;
    let inner_len = miter_len.min(inner_limit);
    let turn = cross(d_in, d_out);
    if join.kind == coursecraft_course::LineJoin::Miter
        && miter_len <= half * join.miter_limit.max(1.0)
    {
        return if turn >= 0.0 {
            vec![station(miter, miter_len, inner_len)]
        } else {
            vec![station(miter, inner_len, miter_len)]
        };
    }
    let inner = mul(miter, inner_len);
    if join.kind == coursecraft_course::LineJoin::Round {
        let a0 = n_in.1.atan2(n_in.0);
        let a1 = n_out.1.atan2(n_out.0);
        let da = if turn >= 0.0 {
            (a1 - a0).rem_euclid(std::f32::consts::TAU)
        } else {
            -((a0 - a1).rem_euclid(std::f32::consts::TAU))
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((da.abs() / (std::f32::consts::PI / 6.0)).ceil() as usize).clamp(2, 12);
        return (0..=steps)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let angle = a0 + da * (i as f32 / steps as f32);
                let u = (angle.cos(), angle.sin());
                if turn >= 0.0 {
                    Station {
                        centre: cur,
                        plus: add(cur, mul(u, half)),
                        minus: sub(cur, inner),
                    }
                } else {
                    Station {
                        centre: cur,
                        plus: add(cur, inner),
                        minus: sub(cur, mul(u, half)),
                    }
                }
            })
            .collect();
    }
    // Bevel (explicit or miter exceeding limit): the outside of the corner gets two points.
    if turn >= 0.0 {
        let bevel = |normal: Pt| Station {
            centre: cur,
            plus: add(cur, mul(normal, half)),
            minus: sub(cur, inner),
        };
        vec![bevel(n_in), bevel(n_out)]
    } else {
        let bevel = |normal: Pt| Station {
            centre: cur,
            plus: add(cur, inner),
            minus: sub(cur, mul(normal, half)),
        };
        vec![bevel(n_in), bevel(n_out)]
    }
}

/// Extend `edge` away from `centre` by `by`.
fn pushed_out(centre: Pt, edge: Pt, by: f32) -> Pt {
    direction(centre, edge).map_or(edge, |d| add(edge, mul(d, by)))
}

/// Add a stroke along `points` as a strip of quads with a feathered edge.
///
/// A strip has no triangulation step to fail, and its quads meet without
/// overlapping, so a translucent colour stays even. Every part of the path is
/// always covered.
fn add_band(
    mesh: &mut Mesh,
    points: &[Pt],
    closed: bool,
    half: f32,
    feather: f32,
    color: Color32,
    join: StrokeJoin,
) {
    let count = points.len();
    if count < 2 {
        return;
    }
    let mut stations = Vec::with_capacity(count + 4);
    for (i, &cur) in points.iter().enumerate() {
        let prev = if i > 0 {
            points.get(i - 1).copied()
        } else if closed {
            points.last().copied()
        } else {
            None
        };
        let next = if i + 1 < count {
            points.get(i + 1).copied()
        } else if closed {
            points.first().copied()
        } else {
            None
        };
        stations.extend(corner_stations(prev, cur, next, half, join));
    }
    if stations.len() < 2 {
        return;
    }
    let transparent = Color32::TRANSPARENT;
    let base = mesh.vertices.len() as u32;
    for station in &stations {
        let plus_out = pushed_out(station.centre, station.plus, feather);
        let minus_out = pushed_out(station.centre, station.minus, feather);
        mesh.colored_vertex(pos2(plus_out.0, plus_out.1), transparent);
        mesh.colored_vertex(pos2(station.plus.0, station.plus.1), color);
        mesh.colored_vertex(pos2(station.minus.0, station.minus.1), color);
        mesh.colored_vertex(pos2(minus_out.0, minus_out.1), transparent);
    }
    let vertex = |station: usize, offset: u32| {
        base.saturating_add(u32::try_from(station).unwrap_or(u32::MAX).saturating_mul(4))
            .saturating_add(offset)
    };
    let last = stations.len() - 1;
    let spans = if closed { stations.len() } else { last };
    for from in 0..spans {
        let to = if from == last { 0 } else { from + 1 };
        for strip in 0..3 {
            mesh.add_triangle(
                vertex(from, strip),
                vertex(to, strip),
                vertex(to, strip + 1),
            );
            mesh.add_triangle(
                vertex(from, strip),
                vertex(to, strip + 1),
                vertex(from, strip + 1),
            );
        }
    }
    if !closed {
        // Feather the two square ends.
        let ends = [
            (stations.first().copied(), stations.get(1).copied()),
            (
                stations.last().copied(),
                stations.get(last.saturating_sub(1)).copied(),
            ),
        ];
        for (end, inside) in ends {
            let (Some(end), Some(inside)) = (end, inside) else {
                continue;
            };
            let Some(out) = direction(inside.centre, end.centre) else {
                continue;
            };
            let cap = mesh.vertices.len() as u32;
            let push = mul(out, feather);
            mesh.colored_vertex(pos2(end.plus.0, end.plus.1), color);
            mesh.colored_vertex(pos2(end.minus.0, end.minus.1), color);
            let (minus_out, plus_out) = (add(end.minus, push), add(end.plus, push));
            mesh.colored_vertex(pos2(minus_out.0, minus_out.1), transparent);
            mesh.colored_vertex(pos2(plus_out.0, plus_out.1), transparent);
            mesh.add_triangle(cap, cap.saturating_add(1), cap.saturating_add(2));
            mesh.add_triangle(cap, cap.saturating_add(2), cap.saturating_add(3));
        }
    }
}

fn length(a: Pt) -> f32 {
    dot(a, a).sqrt()
}

fn push_triangle(mesh: &mut Mesh, triangle: [Pt; 3], color: Color32) {
    let base = mesh.vertices.len() as u32;
    for point in triangle {
        mesh.colored_vertex(pos2(point.0, point.1), color);
    }
    mesh.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
}

fn add_fringe(mesh: &mut Mesh, contours: &[Vec<Pt>], color: Color32, feather: f32) {
    let transparent = Color32::TRANSPARENT;
    for contour in contours {
        for (i, &a) in contour.iter().enumerate() {
            let Some(&b) = contour.get((i + 1) % contour.len()) else {
                continue;
            };
            let edge = sub(b, a);
            let len = dot(edge, edge).sqrt();
            if !len.is_finite() || len <= EPS {
                continue;
            }
            // For a CCW outer and a CW hole the right-hand normal points away
            // from the filled material.
            let normal = (edge.1 / len, -edge.0 / len);
            let a_out = (a.0 + normal.0 * feather, a.1 + normal.1 * feather);
            let b_out = (b.0 + normal.0 * feather, b.1 + normal.1 * feather);

            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(pos2(a.0, a.1), color);
            mesh.colored_vertex(pos2(b.0, b.1), color);
            mesh.colored_vertex(pos2(b_out.0, b_out.1), transparent);
            mesh.colored_vertex(pos2(a_out.0, a_out.1), transparent);
            mesh.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
            mesh.add_triangle(base, base.saturating_add(2), base.saturating_add(3));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(triangles: &[[Pt; 3]]) -> f32 {
        triangles
            .iter()
            .map(|[a, b, c]| cross(sub(*b, *a), sub(*c, *a)).abs() * 0.5)
            .sum()
    }

    #[test]
    fn concave_polygon_fills_its_true_area() {
        let poly = vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (6.0, 10.0),
            (6.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ];
        let triangles = triangulate_polygon(&poly);
        let expected = even_odd_area(&[poly]);
        assert!((area(&triangles) - expected).abs() <= expected * 0.01);
    }

    #[test]
    fn polygon_with_a_hole_fills_its_true_area() {
        let outer = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let hole = vec![(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)];
        let mesh = fill_contours(&[outer.clone(), hole.clone()], Color32::WHITE, 0.0);
        let expected = even_odd_area(&[outer, hole]);
        let got: f32 = mesh
            .triangles()
            .map(|[a, b, c]| {
                let (Some(&pa), Some(&pb), Some(&pc)) = (
                    mesh.vertices.get(a as usize),
                    mesh.vertices.get(b as usize),
                    mesh.vertices.get(c as usize),
                ) else {
                    return 0.0;
                };
                let a = (pa.pos.x, pa.pos.y);
                let b = (pb.pos.x, pb.pos.y);
                let c = (pc.pos.x, pc.pos.y);
                cross(sub(b, a), sub(c, a)).abs() * 0.5
            })
            .sum();
        assert!((got - expected).abs() <= expected * 0.01);
    }

    #[test]
    fn degenerate_and_hostile_input_is_safe() {
        assert!(fill_contours(&[], Color32::WHITE, 1.0).is_empty());
        assert!(fill_contours(&[vec![(0.0, 0.0)]], Color32::WHITE, 1.0).is_empty());
        assert!(
            fill_contours(
                &[vec![(0.0, 0.0), (0.0, 0.0), (0.0, 0.0)]],
                Color32::WHITE,
                1.0
            )
            .is_empty()
        );
        let _ = fill_contours(
            &[vec![
                (f32::NAN, 0.0),
                (1.0, f32::INFINITY),
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
            ]],
            Color32::WHITE,
            1.0,
        );
        let _ = fill_contours(
            &[vec![(0.0, 0.0), (1e30, 0.0), (1e30, 1e30), (0.0, 1e30)]],
            Color32::WHITE,
            1.0,
        );
        let _ = fill_contours(
            &[vec![
                (0.0, 0.0),
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
            ]],
            Color32::WHITE,
            1.0,
        );
    }

    #[test]
    fn fringe_adds_transparent_outer_vertices() {
        let mesh = fill_contours(
            &[vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]],
            Color32::from_rgb(10, 20, 30),
            2.0,
        );
        assert!(mesh.vertices.iter().any(|v| v.color.a() == 0));
        assert!(mesh.vertices.iter().any(|v| v.color.a() > 0));
    }

    #[test]
    fn m31_11_patterned_stroke_hostile_inputs_do_not_panic() {
        let mut mesh = Mesh::default();
        add_patterned_stroke(
            &mut mesh,
            &[(0.0, 0.0), (1e-4, 0.0), (10.0, 0.0)],
            false,
            1e-4,
            5e-5,
            0.0,
            Color32::WHITE,
            1.0,
            coursecraft_course::DashStyle::Dot,
            None,
            StrokeJoin::default(),
        );
        add_patterned_stroke(
            &mut mesh,
            &[(0.0, 0.0), (f32::NAN, 1.0), (10.0, 0.0)],
            false,
            2.0,
            1.0,
            0.5,
            Color32::WHITE,
            1.0,
            coursecraft_course::DashStyle::DashDot,
            Some(&[0.5, 0.5]),
            StrokeJoin {
                kind: coursecraft_course::LineJoin::Round,
                ..StrokeJoin::default()
            },
        );
    }
}
