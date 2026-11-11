//! Shape outlines as closed contours in frame units.
//!
//! Every function here works on plain `(f32, f32)` values, never egui types, so
//! the same geometry drives the editor gallery, the canvas, offscreen rendering
//! and the wasm player. Contours are filled with the even-odd rule: a contour
//! inside another is a hole.
//!
//! Details are measured from `w.min(h)` rather than stretched from a unit shape,
//! so an arrowhead keeps its proportions when its frame is stretched.

use std::f32::consts::TAU;

use coursecraft_course::{ShapeKind, adjust_spec, clamp_adjust};

/// A point in frame units.
pub type Pt = (f32, f32);

/// Segments used for a full circle. Cheap because this only runs for shapes on
/// screen, and smooth enough at the sizes the editor and player draw.
const CIRCLE_SEGMENTS: usize = 32;

fn finite(value: f32) -> bool {
    value.is_finite()
}

/// A usable adjustment value, falling back to the kind's default.
fn value(adjust: &[f32], index: usize, fallback: f32) -> f32 {
    adjust
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

/// Expand empty values to the kind's defaults and clamp everything else.
fn effective_adjust(kind: ShapeKind, adjust: &[f32]) -> Vec<f32> {
    let spec = adjust_spec(kind);
    if spec.count() == 0 {
        return Vec::new();
    }
    let clamped = clamp_adjust(kind, adjust);
    if clamped.len() == spec.count() {
        clamped
    } else {
        spec.defaults.to_vec()
    }
}

fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}

fn mul(a: Pt, scale: f32) -> Pt {
    (a.0 * scale, a.1 * scale)
}

fn dot(a: Pt, b: Pt) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn length(a: Pt) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: Pt) -> Option<Pt> {
    let len = length(a);
    if len.is_finite() && len > 1e-6 {
        Some((a.0 / len, a.1 / len))
    } else {
        None
    }
}

fn distance(a: Pt, b: Pt) -> f32 {
    length(sub(a, b))
}

/// Map every point with `f`, dropping non-finite results.
fn map_points<F: Fn(Pt) -> Pt>(points: &[Pt], f: F) -> Vec<Pt> {
    points
        .iter()
        .copied()
        .map(f)
        .filter(|(x, y)| finite(*x) && finite(*y))
        .collect()
}

/// Translate points by `(dx, dy)`.
#[must_use]
pub fn translate(points: &[Pt], dx: f32, dy: f32) -> Vec<Pt> {
    map_points(points, |(x, y)| (x + dx, y + dy))
}

/// Scale points by `(sx, sy)`.
#[must_use]
pub fn scale(points: &[Pt], sx: f32, sy: f32) -> Vec<Pt> {
    map_points(points, |(x, y)| (x * sx, y * sy))
}

/// A circular or elliptical arc, flattened at a fixed angle step.
///
/// A full turn returns exactly [`CIRCLE_SEGMENTS`] points without repeating the
/// first; a partial arc includes both endpoints and enough intermediate points
/// for the requested sweep.
#[must_use]
pub fn arc(cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32) -> Vec<Pt> {
    if !(finite(cx) && finite(cy) && finite(rx) && finite(ry) && finite(start) && finite(end))
        || rx <= 0.0
        || ry <= 0.0
    {
        return Vec::new();
    }
    let sweep = end - start;
    if !finite(sweep) || sweep.abs() < 1e-6 {
        return vec![(cx + rx * start.cos(), cy + ry * start.sin())];
    }
    let full = (sweep.abs() - TAU).abs() < 1e-4;
    let steps = ((sweep.abs() / (TAU / CIRCLE_SEGMENTS as f32)).ceil() as usize).clamp(1, 256);
    let count = if full { steps } else { steps.saturating_add(1) };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let t = i as f32 / steps as f32;
        let angle = start + sweep * t;
        out.push((cx + rx * angle.cos(), cy + ry * angle.sin()));
    }
    out
}

/// A normalised regular polygon with `n` points, radius 1, apex at the top.
#[must_use]
pub fn regular_polygon(n: usize) -> Vec<Pt> {
    if !(3..=256).contains(&n) {
        return Vec::new();
    }
    let step = TAU / n as f32;
    let start = -TAU / 4.0;
    (0..n)
        .map(|i| {
            let angle = start + i as f32 * step;
            (angle.cos(), angle.sin())
        })
        .collect()
}

/// A star with `points` outer points and radius 1, apex at the top.
#[must_use]
pub fn star(points: usize, inner_ratio: f32) -> Vec<Pt> {
    if !(3..=256).contains(&points) || !finite(inner_ratio) || !(0.0..1.0).contains(&inner_ratio) {
        return Vec::new();
    }
    let step = TAU / (points * 2) as f32;
    let start = -TAU / 4.0;
    let mut out = Vec::with_capacity(points * 2);
    for i in 0..(points * 2) {
        let radius = if i % 2 == 0 { 1.0 } else { inner_ratio };
        let angle = start + i as f32 * step;
        out.push((radius * angle.cos(), radius * angle.sin()));
    }
    out
}

/// A closed band around an open polyline, `thickness` wide.
///
/// Useful for brackets, braces and other open glyphs. Returns an empty band for
/// fewer than two points or a non-positive thickness.
#[must_use]
pub fn stroke_polyline(points: &[Pt], thickness: f32) -> Vec<Pt> {
    if points.len() < 2 || !finite(thickness) || thickness <= 0.0 {
        return Vec::new();
    }
    let half = thickness * 0.5;
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    for (i, &point) in points.iter().enumerate() {
        if !finite(point.0) || !finite(point.1) {
            return Vec::new();
        }
        let tangent = if i == 0 {
            sub(points[1], points[0])
        } else if i + 1 == points.len() {
            sub(points[i], points[i - 1])
        } else {
            let before = normalize(sub(points[i], points[i - 1]));
            let after = normalize(sub(points[i + 1], points[i]));
            match (before, after) {
                (Some(a), Some(b)) => normalize(add(a, b)).unwrap_or(a),
                (Some(a), _) => a,
                (_, Some(b)) => b,
                _ => return Vec::new(),
            }
        };
        let Some(unit) = normalize(tangent) else {
            continue;
        };
        let normal = (-unit.1, unit.0);
        left.push(add(point, mul(normal, half)));
        right.push(sub(point, mul(normal, half)));
    }
    if left.len() < 2 {
        return Vec::new();
    }
    right.reverse();
    left.extend(right);
    left
}

fn bezier2(a: Pt, control: Pt, b: Pt, steps: usize) -> Vec<Pt> {
    let steps = steps.max(1);
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            (
                u * u * a.0 + 2.0 * u * t * control.0 + t * t * b.0,
                u * u * a.1 + 2.0 * u * t * control.1 + t * t * b.1,
            )
        })
        .collect()
}

fn push_corner(
    out: &mut Vec<Pt>,
    vertex: usize,
    points: &[Pt],
    snip: &[usize],
    round: &[usize],
    radius: f32,
) {
    let n = points.len();
    if n < 3 {
        return;
    }
    let Some(&p) = points.get(vertex) else {
        return;
    };
    let Some(&prev) = points.get((vertex + n - 1) % n) else {
        return;
    };
    let Some(&next) = points.get((vertex + 1) % n) else {
        return;
    };
    let Some(v_in) = normalize(sub(p, prev)) else {
        out.push(p);
        return;
    };
    let Some(v_out) = normalize(sub(next, p)) else {
        out.push(p);
        return;
    };
    let len_in = distance(prev, p);
    let len_out = distance(p, next);

    if snip.contains(&vertex) {
        let cut = (radius.max(0.0)).min(len_in * 0.5).min(len_out * 0.5);
        out.push(sub(p, mul(v_in, cut)));
        out.push(add(p, mul(v_out, cut)));
    } else if round.contains(&vertex) {
        let cut = (radius.max(0.0)).min(len_in * 0.5).min(len_out * 0.5);
        let a = sub(p, mul(v_in, cut));
        let b = add(p, mul(v_out, cut));
        out.extend(bezier2(a, p, b, 4));
    } else {
        out.push(p);
    }
}

/// A polygon with the listed corners rounded by `radius`.
#[must_use]
pub fn rounded_polygon(points: &[Pt], radius: f32) -> Vec<Pt> {
    let all: Vec<usize> = (0..points.len()).collect();
    rounded_polygon_masked(points, &all, radius)
}

fn rounded_polygon_masked(points: &[Pt], round: &[usize], radius: f32) -> Vec<Pt> {
    if points.len() < 3 || !finite(radius) || radius <= 0.0 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 3);
    for i in 0..points.len() {
        push_corner(&mut out, i, points, &[], round, radius);
    }
    out
}

fn snip_polygon(points: &[Pt], snip: &[usize], cut_size: f32) -> Vec<Pt> {
    let mut out = Vec::with_capacity(points.len() + snip.len());
    for i in 0..points.len() {
        push_corner(&mut out, i, points, snip, &[], cut_size);
    }
    out
}

fn rectangle_points(w: f32, h: f32) -> Vec<Pt> {
    vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
}

fn detail(w: f32, h: f32) -> f32 {
    w.min(h).max(0.0)
}

fn rect_at(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Pt> {
    arc(cx, cy, rx, ry, 0.0, TAU)
}

fn ellipse_from_top(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Pt> {
    arc(cx, cy, rx, ry, -TAU * 0.25, TAU * 0.75)
}

fn ellipse_in_frame(w: f32, h: f32) -> Vec<Pt> {
    ellipse(w * 0.5, h * 0.5, w * 0.5, h * 0.5)
}

fn regular_in_frame(n: usize, w: f32, h: f32) -> Vec<Pt> {
    if !(3..=256).contains(&n) {
        return Vec::new();
    }
    // Odd polygons point up like PowerPoint's pentagon and heptagon; even ones
    // sit on a flat top like its hexagon, octagon and dodecagon.
    let step = TAU / n as f32;
    let start = if n.is_multiple_of(2) {
        -TAU * 0.25 + step * 0.5
    } else {
        -TAU * 0.25
    };
    (0..n)
        .map(|i| {
            let angle = start + i as f32 * step;
            (
                w * 0.5 + angle.cos() * w * 0.5,
                h * 0.5 + angle.sin() * h * 0.5,
            )
        })
        .collect()
}

fn star_in_frame(points: usize, inner_ratio: f32, w: f32, h: f32) -> Vec<Pt> {
    map_points(&star(points, inner_ratio), |(x, y)| {
        (w * 0.5 + x * w * 0.5, h * 0.5 + y * h * 0.5)
    })
}

fn star_in_circle(points: usize, inner_ratio: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Pt> {
    map_points(&star(points, inner_ratio), |(x, y)| {
        (cx + x * rx, cy + y * ry)
    })
}

fn rotate_about(point: Pt, center: Pt, angle: f32) -> Pt {
    let (sin, cos) = angle.sin_cos();
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    (
        center.0 + dx * cos - dy * sin,
        center.1 + dx * sin + dy * cos,
    )
}

/// A plus/cross with arm half-length `arm` and half-thickness `half`.
///
/// Rotating it by 45 degrees gives the union of two crossing diagonal bands,
/// which is the only practical way to cut a single X-shaped hole.
fn plus_polygon(cx: f32, cy: f32, arm: f32, half: f32, angle: f32) -> Vec<Pt> {
    let arm = arm.max(half);
    let base = [
        (-half, -arm),
        (half, -arm),
        (half, -half),
        (arm, -half),
        (arm, half),
        (half, half),
        (half, arm),
        (-half, arm),
        (-half, half),
        (-arm, half),
        (-arm, -half),
        (-half, -half),
    ];
    map_points(&base, |(x, y)| {
        rotate_about((cx + x, cy + y), (cx, cy), angle)
    })
}

/// A band along an elliptical arc, from `start` to `end`, `thickness` wide.
fn arc_band(cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32, thickness: f32) -> Vec<Pt> {
    let thin = thickness.max(0.0).min(rx * 0.8).min(ry * 0.8);
    if thin <= 0.0 || rx <= 0.0 || ry <= 0.0 {
        return Vec::new();
    }
    let inner_rx = (rx - thin).max(rx * 0.05);
    let inner_ry = (ry - thin).max(ry * 0.05);
    let mut out = arc(cx, cy, rx, ry, start, end);
    out.extend(arc(cx, cy, inner_rx, inner_ry, end, start));
    out
}

fn ring_contours(cx: f32, cy: f32, rx: f32, ry: f32, inner: f32) -> Vec<Vec<Pt>> {
    let ratio = inner.clamp(0.05, 0.95);
    vec![
        ellipse(cx, cy, rx, ry),
        ellipse(cx, cy, rx * ratio, ry * ratio),
    ]
}

fn shrink_polygon(points: &[Pt], amount: f32) -> Vec<Pt> {
    if points.len() < 3 || !finite(amount) || amount <= 0.0 {
        return points.to_vec();
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for &(x, y) in points {
        cx += x;
        cy += y;
    }
    let n = points.len() as f32;
    cx /= n;
    cy /= n;
    map_points(points, |(x, y)| {
        let dx = x - cx;
        let dy = y - cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 1e-6 {
            (x, y)
        } else {
            let step = amount.min(len * 0.45);
            (x - dx / len * step, y - dy / len * step)
        }
    })
}

fn wavy_document(w: f32, h: f32) -> Vec<Pt> {
    let mut out = vec![(0.0, 0.0), (w, 0.0), (w, h * 0.82)];
    let mut first = bezier2((w, h * 0.82), (w * 0.75, h * 1.02), (w * 0.5, h * 0.92), 12);
    if !first.is_empty() {
        first.remove(0);
    }
    out.extend(first);
    let mut second = bezier2(
        (w * 0.5, h * 0.92),
        (w * 0.25, h * 0.82),
        (0.0, h * 0.9),
        12,
    );
    if !second.is_empty() {
        second.remove(0);
    }
    out.extend(second);
    out.push((0.0, 0.0));
    out
}

fn wave_edge(x0: f32, x1: f32, y: f32, amplitude: f32, cycles: f32, invert: bool) -> Vec<Pt> {
    let steps = 48usize;
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let x = x0 + (x1 - x0) * t;
            let bump = (t * TAU * cycles).sin() * amplitude;
            (x, if invert { y - bump } else { y + bump })
        })
        .collect()
}

/// A solid flag about 75% of the frame tall. The top and bottom edges carry
/// the same sine wave, so the body keeps a constant height.
fn solid_wave(w: f32, h: f32, cycles: f32) -> Vec<Pt> {
    let half = h * 0.375;
    let amplitude = h * 0.08;
    let mid = h * 0.5;
    let steps = 64usize;
    let mut out = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let bump = (t * TAU * cycles).sin() * amplitude;
        out.push((w * t, mid - half + bump));
    }
    for i in (0..=steps).rev() {
        let t = i as f32 / steps as f32;
        let bump = (t * TAU * cycles).sin() * amplitude;
        out.push((w * t, mid + half + bump));
    }
    out
}

fn punched_tape(w: f32, h: f32) -> Vec<Pt> {
    let amp = h * 0.08;
    let mut out = wave_edge(0.0, w, h * 0.12, amp, 2.0, false);
    out.push((w, h * 0.88));
    let bottom = wave_edge(0.0, w, h * 0.88, amp, 2.0, true);
    out.extend(bottom.into_iter().rev().skip(1));
    out
}

fn stored_data(w: f32, h: f32) -> Vec<Pt> {
    let d = detail(w, h);
    let cap = (d * 0.18).min(w * 0.25).max(0.01);
    let bow = (d * 0.18).min(w * 0.3).max(0.01);
    let cy = h * 0.5;
    let pi = std::f32::consts::PI;
    // Round on the left: a half ellipse from top to bottom via the left edge.
    let mut out = vec![(cap, 0.0)];
    out.extend(arc(cap, cy, cap, h * 0.5, -pi * 0.5, -pi * 1.5));
    out.push((w, h));
    // The right edge bows inward, towards the left.
    out.extend(arc(w, cy, bow, h * 0.5, pi * 0.5, pi * 1.5));
    out
}

/// The outer intersection of two overlapping circles, picked away from the
/// cloud's centre so the arcs trace the outside of the union.
fn outer_circle_hit(a: (f32, f32, f32), b: (f32, f32, f32), center: Pt) -> Pt {
    let (ax, ay, ar) = a;
    let (bx, by, br) = b;
    let dx = bx - ax;
    let dy = by - ay;
    let d = (dx * dx + dy * dy).sqrt();
    if finite(d) && d > 1e-6 && finite(ar) && finite(br) {
        let along = (ar * ar - br * br + d * d) / (2.0 * d);
        let h2 = ar * ar - along * along;
        if finite(along) && h2 >= 0.0 {
            let h = h2.sqrt();
            let mx = ax + along * dx / d;
            let my = ay + along * dy / d;
            let first = (mx - dy / d * h, my + dx / d * h);
            let second = (mx + dy / d * h, my - dx / d * h);
            return if distance(first, center) >= distance(second, center) {
                first
            } else {
                second
            };
        }
    }
    let Some(dir) = normalize((ax - center.0, ay - center.1)) else {
        return (ax, ay);
    };
    (ax + dir.0 * ar, ay + dir.1 * ar)
}

fn arc_mid_distance(points: &[Pt], center: Pt) -> f32 {
    let mid = points.get(points.len() / 2).copied().unwrap_or(center);
    distance(mid, center)
}

/// The cloud body: seven overlapping circles whose outer arcs meet in sharp
/// inward creases. Built in unit space so the bumps stay round when the frame
/// is not square.
fn cloud_body(w: f32, h: f32) -> Vec<Pt> {
    const CIRCLES: [(f32, f32, f32); 7] = [
        (0.24, 0.40, 0.18),
        (0.45, 0.25, 0.21),
        (0.69, 0.30, 0.19),
        (0.84, 0.52, 0.15),
        (0.67, 0.72, 0.19),
        (0.43, 0.76, 0.18),
        (0.21, 0.62, 0.17),
    ];
    let n = CIRCLES.len();
    let mut gx = 0.0;
    let mut gy = 0.0;
    for &(x, y, _) in &CIRCLES {
        gx += x;
        gy += y;
    }
    let count = n as f32;
    gx /= count;
    gy /= count;
    let center = (gx, gy);
    let mut out: Vec<Pt> = Vec::new();
    for i in 0..n {
        let Some(&(cx, cy, r)) = CIRCLES.get(i) else {
            continue;
        };
        let Some(&prev) = CIRCLES.get((i + n - 1) % n) else {
            continue;
        };
        let Some(&next) = CIRCLES.get((i + 1) % n) else {
            continue;
        };
        let start = outer_circle_hit((cx, cy, r), prev, center);
        let end = outer_circle_hit((cx, cy, r), next, center);
        let a0 = (start.1 - cy).atan2(start.0 - cx);
        let a1 = (end.1 - cy).atan2(end.0 - cx);
        let sweep = a1 - a0;
        let other = if sweep >= 0.0 {
            sweep - TAU
        } else {
            sweep + TAU
        };
        let forward = arc(cx, cy, r, r, a0, a0 + sweep);
        let backward = arc(cx, cy, r, r, a0, a0 + other);
        let chosen = if arc_mid_distance(&forward, center) >= arc_mid_distance(&backward, center) {
            forward
        } else {
            backward
        };
        for &point in &chosen {
            if out
                .last()
                .is_none_or(|last: &Pt| distance(*last, point) > 1e-6)
            {
                out.push(point);
            }
        }
    }
    if out.len() > 2
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(&first, &last)| distance(first, last) < 1e-4)
    {
        out.pop();
    }
    map_points(&out, |(x, y)| (x * w, y * h))
}

fn heart(w: f32, h: f32) -> Vec<Pt> {
    let steps = 72usize;
    let mut points = Vec::with_capacity(steps);
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for i in 0..steps {
        let t = i as f32 / steps as f32 * TAU;
        let x = 16.0 * t.sin().powi(3);
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        points.push((x, y));
    }
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    map_points(&points, |(x, y)| {
        ((x - min_x) / span_x * w, (max_y - y) / span_y * h)
    })
}

fn lightning_bolt(w: f32, h: f32) -> Vec<Pt> {
    let base = [
        (0.62, 0.0),
        (0.2, 0.54),
        (0.5, 0.54),
        (0.34, 1.0),
        (0.86, 0.42),
        (0.56, 0.42),
    ];
    map_points(&base, |(x, y)| (x * w, y * h))
}

fn moon(w: f32, h: f32) -> Vec<Pt> {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let rx = w * 0.5;
    let ry = h * 0.5;
    let mut out = arc(cx, cy, rx, ry, TAU * 0.25, TAU * 0.75);
    let r2x = rx * 0.95;
    let r2y = ry * 0.95;
    out.extend(arc(cx + rx * 0.45, cy, r2x, r2y, -TAU * 0.25, -TAU * 0.75));
    out
}

fn ribbon(w: f32, h: f32, up: bool) -> Vec<Vec<Pt>> {
    // Drawn for the raised-centre case, then mirrored for `ribbon_down`.
    let gap = (detail(w, h) * 0.05).min(w * 0.02).max(0.001);
    let cx0 = w * 0.2;
    let cx1 = w * 0.8;
    let (cy0, cy1) = (h * 0.08, h * 0.66);
    let (ty0, ty1) = (h * 0.34, h * 0.92);
    let notch = (w * 0.07).min(cx0 * 0.5).max(0.001);
    let mid = (ty0 + ty1) * 0.5;
    let fold_w = w * 0.08;
    let contours = vec![
        rect_at(cx0, cy0, cx1, cy1),
        vec![
            (0.0, ty0),
            (cx0 - gap, ty0),
            (cx0 - gap, ty1),
            (0.0, ty1),
            (notch, mid),
        ],
        vec![
            (w, ty0),
            (w - notch, mid),
            (w, ty1),
            (cx1 + gap, ty1),
            (cx1 + gap, ty0),
        ],
        vec![(cx0, cy1 + gap), (cx0 + fold_w, cy1 + gap), (cx0, ty1)],
        vec![(cx1, cy1 + gap), (cx1, ty1), (cx1 - fold_w, cy1 + gap)],
    ];
    if up {
        contours
    } else {
        contours
            .into_iter()
            .map(|contour| map_points(&contour, |(x, y)| (x, h - y)))
            .collect()
    }
}

fn curved_ribbon(w: f32, h: f32, up: bool) -> Vec<Vec<Pt>> {
    // Bend the straight ribbon along a shallow sine arc, keeping the whole
    // shape inside the frame.
    let sign = if up { 1.0 } else { -1.0 };
    let amplitude = h * 0.15;
    let pi = std::f32::consts::PI;
    ribbon(w, h, up)
        .into_iter()
        .map(|contour| {
            map_points(&contour, |(x, y)| {
                let bend = (pi * x / w).sin() * amplitude * sign;
                (x, h * 0.5 + (y - h * 0.5) * 0.8 + bend)
            })
        })
        .collect()
}

fn scroll_vertical(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let body = rect_at(w * 0.16, h * 0.18, w * 0.84, h * 0.82);
    let roll = |y0: f32, y1: f32| {
        rounded_polygon(&rect_at(w * 0.06, y0, w * 0.94, y1), detail(w, h) * 0.08)
    };
    vec![body, roll(0.0, h * 0.16), roll(h * 0.84, h)]
}

fn scroll_horizontal(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let body = rect_at(w * 0.18, h * 0.16, w * 0.82, h * 0.84);
    let roll = |x0: f32, x1: f32| {
        rounded_polygon(&rect_at(x0, h * 0.06, x1, h * 0.94), detail(w, h) * 0.08)
    };
    vec![body, roll(0.0, w * 0.16), roll(w * 0.84, w)]
}

fn cross(a: Pt, b: Pt) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

/// The edge (and fraction along it) closest to `point`.
fn nearest_edge(points: &[Pt], point: Pt) -> (usize, f32) {
    let n = points.len();
    let mut best = (0usize, 0.0f32);
    let mut best_distance = f32::INFINITY;
    for i in 0..n {
        let Some(&a) = points.get(i) else {
            continue;
        };
        let Some(&b) = points.get((i + 1) % n) else {
            continue;
        };
        let edge = sub(b, a);
        let len2 = dot(edge, edge);
        let frac = if len2 > 1e-12 {
            (dot(sub(point, a), edge) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let hit = add(a, mul(edge, frac));
        let d = distance(hit, point);
        if finite(d) && d < best_distance {
            best_distance = d;
            best = (i, frac);
        }
    }
    best
}

fn point_at(
    points: &[Pt],
    cumulative: &[f32],
    lengths: &[f32],
    total: f32,
    distance_along: f32,
) -> Pt {
    if total <= 0.0 || points.is_empty() {
        return points.first().copied().unwrap_or((0.0, 0.0));
    }
    let s = distance_along.rem_euclid(total);
    for i in 0..points.len() {
        let Some(&start) = cumulative.get(i) else {
            continue;
        };
        let Some(&len) = lengths.get(i) else {
            continue;
        };
        if len <= 0.0 || s < start || s > start + len {
            continue;
        }
        let Some(&a) = points.get(i) else {
            continue;
        };
        let Some(&b) = points.get((i + 1) % points.len()) else {
            continue;
        };
        return add(a, mul(sub(b, a), (s - start) / len));
    }
    points.first().copied().unwrap_or((0.0, 0.0))
}

/// Walk forward along the boundary from `from` to `to`, returning every vertex
/// passed and the end point (but not the start point).
fn walk_boundary(
    points: &[Pt],
    cumulative: &[f32],
    lengths: &[f32],
    total: f32,
    from: f32,
    to: f32,
) -> Vec<Pt> {
    let mut out = Vec::new();
    if total <= 0.0 {
        return out;
    }
    let span = (to - from).rem_euclid(total);
    if span <= 1e-6 {
        return out;
    }
    let limit = from + span;
    let mut passed: Vec<(f32, Pt)> = Vec::new();
    for i in 0..points.len() {
        let Some(&start) = cumulative.get(i) else {
            continue;
        };
        let Some(&point) = points.get(i) else {
            continue;
        };
        let mut at = start;
        while at <= from + 1e-6 {
            at += total;
        }
        if at < limit - 1e-6 {
            passed.push((at, point));
        }
    }
    passed.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.extend(passed.into_iter().map(|(_, point)| point));
    out.push(point_at(points, cumulative, lengths, total, to));
    out
}

/// Splice a wedge with its tip at `tip` into the nearest part of `body`.
///
/// The body's boundary is cut between two points a fixed distance either side of
/// where the ray from `center` to `tip` leaves the body; the gap is replaced by
/// the tip. The result is one simple contour, so an outline never draws through
/// the bubble.
fn splice_wedge(body: &[Pt], center: Pt, tip: Pt, half: f32) -> Vec<Pt> {
    let points: Vec<Pt> = body
        .iter()
        .copied()
        .filter(|(x, y)| finite(*x) && finite(*y))
        .collect();
    let n = points.len();
    if n < 3 || !finite(tip.0) || !finite(tip.1) {
        return points;
    }
    let mut lengths = Vec::with_capacity(n);
    let mut cumulative = Vec::with_capacity(n);
    let mut total = 0.0f32;
    for i in 0..n {
        let Some(&a) = points.get(i) else {
            continue;
        };
        let Some(&b) = points.get((i + 1) % n) else {
            continue;
        };
        cumulative.push(total);
        let len = distance(a, b);
        lengths.push(len);
        total += len;
    }
    if !finite(total) || total <= 1e-6 {
        return points;
    }

    let direction = sub(tip, center);
    let mut hit: Option<(f32, usize, f32)> = None;
    if length(direction) > 1e-6 {
        for i in 0..n {
            let Some(&a) = points.get(i) else {
                continue;
            };
            let Some(&b) = points.get((i + 1) % n) else {
                continue;
            };
            let edge = sub(b, a);
            let denom = cross(direction, edge);
            if denom.abs() <= 1e-9 {
                continue;
            }
            let offset = sub(a, center);
            let t = cross(offset, edge) / denom;
            let u = cross(offset, direction) / denom;
            if finite(t)
                && finite(u)
                && t > 1e-6
                && (-1e-6..=1.0 + 1e-6).contains(&u)
                && hit.is_none_or(|(best, _, _)| t < best)
            {
                hit = Some((t, i, u.clamp(0.0, 1.0)));
            }
        }
    }
    let (edge, fraction) = hit
        .map(|(_, edge, fraction)| (edge, fraction))
        .unwrap_or_else(|| nearest_edge(&points, tip));
    let start = cumulative.get(edge).copied().unwrap_or(0.0);
    let len = lengths.get(edge).copied().unwrap_or(0.0);
    let attach = start + len * fraction;
    let half = half.max(0.0).min(total * 0.45);
    let forward = (attach + half).rem_euclid(total);
    let backward = (attach - half).rem_euclid(total);

    let first = point_at(&points, &cumulative, &lengths, total, forward);
    let mut out = Vec::with_capacity(n + 2);
    out.push(tip);
    out.push(first);
    out.extend(walk_boundary(
        &points,
        &cumulative,
        &lengths,
        total,
        forward,
        backward,
    ));
    out
}

fn callout_wedge(w: f32, h: f32, body: &[Pt], tip: Pt) -> Vec<Vec<Pt>> {
    let half = detail(w, h) * 0.10;
    let center = (w * 0.5, h * 0.36);
    vec![splice_wedge(body, center, tip, half)]
}

fn line_callout_tail(w: f32, h: f32, kind: ShapeKind, adjust: &[f32]) -> Vec<Pt> {
    let t = callout_line_thickness(w, h);
    let y0 = h * 0.7 + t * 0.6;
    let point = |index: usize, fallback: (f32, f32)| -> Pt {
        (
            value(adjust, index * 2, fallback.0) * w,
            value(adjust, index * 2 + 1, fallback.1) * h,
        )
    };
    match kind {
        ShapeKind::CalloutLine => vec![(w * 0.04, y0), point(0, (0.18, 0.97))],
        ShapeKind::CalloutLineBent => vec![
            (w * 0.04, y0),
            point(0, (0.17, 0.7)),
            point(1, (0.17, 0.97)),
        ],
        _ => vec![
            (w * 0.04, y0),
            point(0, (0.13, 0.8)),
            point(1, (0.24, 0.8)),
            point(2, (0.24, 0.97)),
        ],
    }
}

fn right_arrow_polygon(w: f32, h: f32, start: f32) -> Vec<Pt> {
    let d = detail(w, h);
    let mid = h * 0.5;
    let shaft = d * 0.2;
    let head_half = d * 0.24;
    let start = start.clamp(0.0, w * 0.95);
    let head = (w * 0.18)
        .max(d * 0.1)
        .min((w - start) * 0.5)
        .max((w - start) * 0.15);
    let head_start = w - head;
    // Start at the head base, outside the callout body, so the even-odd
    // nesting check sees this as a second outer contour rather than a hole.
    vec![
        (head_start, mid - shaft * 0.5),
        (head_start, mid - head_half),
        (w, mid),
        (head_start, mid + head_half),
        (head_start, mid + shaft * 0.5),
        (start, mid + shaft * 0.5),
        (start, mid - shaft * 0.5),
    ]
}

fn arrow_callout(w: f32, h: f32, kind: ShapeKind) -> Vec<Vec<Pt>> {
    let body = w * 0.65;
    let parts = || {
        vec![
            rect_at(0.0, 0.0, body, h),
            right_arrow_polygon(w, h, body * 0.9),
        ]
    };
    match kind {
        ShapeKind::CalloutRightArrow => parts(),
        ShapeKind::CalloutLeftArrow => parts()
            .into_iter()
            .map(|contour| map_points(&contour, |(x, y)| (w - x, y)))
            .collect(),
        ShapeKind::CalloutDownArrow => {
            let source = {
                let body = h * 0.65;
                vec![
                    rect_at(0.0, 0.0, body, w),
                    right_arrow_polygon(h, w, body * 0.9),
                ]
            };
            source
                .into_iter()
                .map(|contour| map_points(&contour, |(x, y)| (w - y, x)))
                .collect()
        }
        ShapeKind::CalloutUpArrow => {
            let source = {
                let body = h * 0.65;
                vec![
                    rect_at(0.0, 0.0, body, w),
                    right_arrow_polygon(h, w, body * 0.9),
                ]
            };
            source
                .into_iter()
                .map(|contour| map_points(&contour, |(x, y)| (y, h - x)))
                .collect()
        }
        _ => vec![arrow_quad(w, h)],
    }
}

fn arrow_callout_left_right(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let box_left = w * 0.32;
    let box_right = w * 0.68;
    let right_arrow = right_arrow_polygon(w, h, box_right * 0.9);
    let left_arrow = map_points(
        &right_arrow_polygon(w, h, (w - box_left) * 0.9),
        |(x, y)| (w - x, y),
    );
    vec![
        rect_at(box_left, 0.0, box_right, h),
        right_arrow,
        left_arrow,
    ]
}

fn arrow_callout_quad(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let side = (d * 0.40).min(w * 0.8).min(h * 0.8).max(0.01);
    let left = (w - side) * 0.5;
    let right = left + side;
    let top = (h - side) * 0.5;
    let bottom = top + side;
    let mx = w * 0.5;
    let my = h * 0.5;
    let shaft = (d * 0.05).max(0.01);
    let head_half = (d * 0.10).max(shaft);
    let head_len = (d * 0.15).max(shaft);
    let right_start = (w - head_len).max(right);
    let down_start = (h - head_len).max(bottom);
    let up_start = head_len.min(top);

    let right_arrow = vec![
        (right_start, my - shaft * 0.5),
        (right_start, my - head_half),
        (w, my),
        (right_start, my + head_half),
        (right_start, my + shaft * 0.5),
        (right, my + shaft * 0.5),
        (right, my - shaft * 0.5),
    ];
    let left_arrow = map_points(&right_arrow, |(x, y)| (w - x, y));
    let down_arrow = vec![
        (mx - shaft * 0.5, down_start),
        (mx - head_half, down_start),
        (mx, h),
        (mx + head_half, down_start),
        (mx + shaft * 0.5, down_start),
        (mx + shaft * 0.5, bottom),
        (mx - shaft * 0.5, bottom),
    ];
    let up_arrow = vec![
        (mx + shaft * 0.5, up_start),
        (mx + head_half, up_start),
        (mx, 0.0),
        (mx - head_half, up_start),
        (mx - shaft * 0.5, up_start),
        (mx - shaft * 0.5, top),
        (mx + shaft * 0.5, top),
    ];
    vec![
        rect_at(left, top, right, bottom),
        right_arrow,
        left_arrow,
        down_arrow,
        up_arrow,
    ]
}

fn arrow_points(direction: ArrowDirection, w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    match direction {
        ArrowDirection::Right => arrow_right_adjusted(w, h, adjust),
        ArrowDirection::Left => {
            let source = arrow_right_adjusted(w, h, adjust);
            map_points(&source, |(x, y)| (w - x, y))
        }
        ArrowDirection::Up => {
            let source = arrow_right_adjusted(h, w, adjust);
            map_points(&source, |(x, y)| (y, h - x))
        }
        ArrowDirection::Down => {
            let source = arrow_right_adjusted(h, w, adjust);
            map_points(&source, |(x, y)| (w - y, x))
        }
    }
}

#[derive(Clone, Copy)]
enum ArrowDirection {
    Right,
    Left,
    Up,
    Down,
}

fn arrow_right_from(w: f32, h: f32, x0: f32, head_frac: f32, tail_frac: f32) -> Vec<Pt> {
    let d = w.min(h);
    let mid = h * 0.5;
    let tail = (d * tail_frac).min(h * 0.45);
    let x0 = x0.clamp(0.0, w * 0.5);
    let head = (d * head_frac).min((w - x0) * 0.9);
    vec![
        (x0, mid - tail),
        (w - head, mid - tail),
        (w - head, 0.0),
        (w, mid),
        (w - head, h),
        (w - head, mid + tail),
        (x0, mid + tail),
    ]
}

fn arrow_right(w: f32, h: f32) -> Vec<Pt> {
    arrow_right_from(w, h, 0.0, 0.5, 0.25)
}

fn arrow_right_adjusted(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    arrow_right_from(w, h, 0.0, value(adjust, 0, 0.5), value(adjust, 1, 0.25))
}

fn arrow_left_right_adjusted(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let d = w.min(h);
    let mid = h * 0.5;
    let tail = (d * value(adjust, 1, 0.25)).min(h * 0.45);
    let head = (d * value(adjust, 0, 0.5)).min(w * 0.4);
    let hh = h * 0.5;
    vec![
        (0.0, mid),
        (head, mid - hh),
        (head, mid - tail),
        (w - head, mid - tail),
        (w - head, mid - hh),
        (w, mid),
        (w - head, mid + hh),
        (w - head, mid + tail),
        (head, mid + tail),
        (head, mid + hh),
    ]
}

fn arrow_up_down_adjusted(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let source = arrow_left_right_adjusted(h, w, adjust);
    map_points(&source, |(x, y)| (y, h - x))
}

/// Quad arrow in unit space: a 16% shaft, 36%-wide and 18%-long heads.
fn arrow_quad(w: f32, h: f32) -> Vec<Pt> {
    let points = [
        (0.50, 0.00),
        (0.68, 0.18),
        (0.58, 0.18),
        (0.58, 0.42),
        (0.82, 0.42),
        (0.82, 0.32),
        (1.00, 0.50),
        (0.82, 0.68),
        (0.82, 0.58),
        (0.58, 0.58),
        (0.58, 0.82),
        (0.68, 0.82),
        (0.50, 1.00),
        (0.32, 0.82),
        (0.42, 0.82),
        (0.42, 0.58),
        (0.18, 0.58),
        (0.18, 0.68),
        (0.00, 0.50),
        (0.18, 0.32),
        (0.18, 0.42),
        (0.42, 0.42),
        (0.42, 0.18),
        (0.32, 0.18),
    ];
    map_points(&points, |(x, y)| (x * w, y * h))
}

fn arrow_left_right_up(w: f32, h: f32) -> Vec<Pt> {
    let points = [
        (0.50, 0.00),
        (0.68, 0.18),
        (0.58, 0.18),
        (0.58, 0.42),
        (0.82, 0.42),
        (0.82, 0.32),
        (1.00, 0.50),
        (0.82, 0.68),
        (0.82, 0.58),
        (0.58, 0.58),
        (0.58, 1.00),
        (0.42, 1.00),
        (0.42, 0.58),
        (0.18, 0.58),
        (0.18, 0.68),
        (0.00, 0.50),
        (0.18, 0.32),
        (0.18, 0.42),
        (0.42, 0.42),
        (0.42, 0.18),
        (0.32, 0.18),
    ];
    map_points(&points, |(x, y)| (x * w, y * h))
}

fn arrow_bent(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let tail = (d * 0.3).min(h * 0.4);
    let head = (d * 0.5).min(w * 0.4);
    let hh = (d * 0.35).min(h * 0.45);
    let arm = hh - tail * 0.5;
    vec![
        (0.0, h),
        (0.0, arm),
        (w - head, arm),
        (w - head, 0.0),
        (w, hh),
        (w - head, hh * 2.0),
        (w - head, arm + tail),
        (tail, arm + tail),
        (tail, h),
    ]
}

fn arrow_u_turn(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let tail = (d * 0.18).min(w * 0.2).min(h * 0.2);
    let head = (d * 0.35).min(h * 0.4);
    let hh = ((w - tail) * 0.5).min(d * 0.35);
    let center = w - hh;
    vec![
        (0.0, h),
        (0.0, 0.0),
        (center + tail * 0.5, 0.0),
        (center + tail * 0.5, h - head),
        (center + hh, h - head),
        (center, h),
        (center - hh, h - head),
        (center - tail * 0.5, h - head),
        (center - tail * 0.5, tail),
        (tail, tail),
        (tail, h),
    ]
}

fn arrow_left_up(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let tail = d * 0.2;
    let head = (d * 0.35).min(w * 0.3);
    let hh = (d * 0.3).min(h * 0.25);
    let center = w - hh;
    let y0 = (h * 0.62).max(hh + tail).min(h - hh - tail);
    vec![
        (center, 0.0),
        (center + hh, head),
        (center + tail * 0.5, head),
        (center + tail * 0.5, h),
        (center - tail * 0.5, h),
        (center - tail * 0.5, y0 + tail * 0.5),
        (head, y0 + tail * 0.5),
        (head, y0 + hh),
        (0.0, y0),
        (head, y0 - hh),
        (head, y0 - tail * 0.5),
        (center - tail * 0.5, y0 - tail * 0.5),
        (center - tail * 0.5, head),
        (center - hh, head),
    ]
}

fn arrow_bent_up(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let tail = d * 0.2;
    let head = (d * 0.35).min(h * 0.4);
    let hh = (d * 0.3).min(w * 0.3);
    let center = w * 0.5;
    vec![
        (0.0, h),
        (0.0, h - tail),
        (center - tail * 0.5, h - tail),
        (center - tail * 0.5, head),
        (center - hh, head),
        (center, 0.0),
        (center + hh, head),
        (center + tail * 0.5, head),
        (center + tail * 0.5, h),
    ]
}

fn sample_quad(p0: Pt, p1: Pt, p2: Pt) -> Vec<Pt> {
    bezier2(p0, p1, p2, 20)
}

fn tapered_arrow(centerline: &[Pt], thickness: f32, head_half: f32, head_len: f32) -> Vec<Pt> {
    if centerline.len() < 2 || !finite(thickness) || thickness <= 0.0 {
        return Vec::new();
    }
    let half = thickness * 0.5;
    let mut base = 0;
    let mut walked = 0.0;
    for i in (0..centerline.len() - 1).rev() {
        walked += distance(centerline[i], centerline[i + 1]);
        if walked >= head_len {
            base = i;
            break;
        }
    }
    if base == 0 {
        base = centerline.len().saturating_sub(2);
    }
    if base == 0 {
        return Vec::new();
    }

    let mut left = Vec::with_capacity(base + 1);
    let mut right = Vec::with_capacity(base + 1);
    let mut base_normal = (0.0, 0.0);
    for i in 0..=base {
        let tangent = if i == 0 {
            sub(centerline[1], centerline[0])
        } else if i + 1 == centerline.len() {
            sub(centerline[i], centerline[i - 1])
        } else {
            sub(centerline[i + 1], centerline[i - 1])
        };
        let Some(unit) = normalize(tangent) else {
            continue;
        };
        let normal = (-unit.1, unit.0);
        if i == base {
            base_normal = normal;
        }
        left.push(add(centerline[i], mul(normal, half)));
        right.push(sub(centerline[i], mul(normal, half)));
    }
    if left.len() < 2 || right.len() < 2 {
        return Vec::new();
    }
    let tip = *centerline.last().unwrap_or(&(0.0, 0.0));
    let base_point = centerline[base];
    let head_left = add(base_point, mul(base_normal, head_half));
    let head_right = sub(base_point, mul(base_normal, head_half));

    let mut out = left;
    out.push(head_left);
    out.push(tip);
    out.push(head_right);
    out.extend(right.into_iter().rev());
    out
}

fn curved_arrow(w: f32, h: f32, kind: ShapeKind) -> Vec<Pt> {
    let d = w.min(h);
    let thickness = d * 0.18;
    let head_half = d * 0.3;
    let head_len = d * 0.3;
    let (centerline, _direction) = match kind {
        ShapeKind::ArrowCurvedRight => (
            sample_quad((0.0, h * 0.6), (w * 0.5, h * 0.75), (w, h * 0.45)),
            ArrowDirection::Right,
        ),
        ShapeKind::ArrowCurvedLeft => (
            sample_quad((w, h * 0.6), (w * 0.5, h * 0.75), (0.0, h * 0.45)),
            ArrowDirection::Left,
        ),
        ShapeKind::ArrowCurvedUp => (
            sample_quad((w * 0.3, h), (w * 0.5, h * 0.55), (w * 0.55, 0.0)),
            ArrowDirection::Up,
        ),
        _ => (
            sample_quad((w * 0.3, 0.0), (w * 0.5, h * 0.45), (w * 0.55, h)),
            ArrowDirection::Down,
        ),
    };
    tapered_arrow(&centerline, thickness, head_half, head_len)
}

fn striped_right_arrow(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let mid = h * 0.5;
    let tail = (d * 0.25).min(h * 0.45);
    let bar = (d * 0.05).max(0.0);
    let gap = (d * 0.04).max(0.0);
    let start = (bar * 2.0 + gap * 2.0).min(w * 0.45);
    if bar <= 0.0 || gap <= 0.0 || tail <= 0.0 {
        return vec![arrow_right(w, h)];
    }
    vec![
        rect_at(0.0, mid - tail, bar, mid + tail),
        rect_at(bar + gap, mid - tail, bar * 2.0 + gap, mid + tail),
        arrow_right_from(w, h, start, 0.5, 0.25),
    ]
}

fn notched_right_arrow(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let mid = h * 0.5;
    let tail = (d * 0.25).min(h * 0.45);
    let head = (d * 0.5).min(w * 0.9);
    let notch = (d * 0.25).min(w * 0.2);
    vec![
        (0.0, mid - tail),
        (w - head, mid - tail),
        (w - head, 0.0),
        (w, mid),
        (w - head, h),
        (w - head, mid + tail),
        (0.0, mid + tail),
        (notch, mid),
    ]
}

fn pentagon_arrow(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let d = w.min(h);
    let head = (d * value(adjust, 0, 0.5)).min(w * 0.9);
    vec![
        (0.0, 0.0),
        (w - head, 0.0),
        (w, h * 0.5),
        (w - head, h),
        (0.0, h),
    ]
}

fn chevron(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let d = w.min(h);
    let default_head = (d * 0.35).min(w * 0.45);
    let default_notch = (d * 0.25).min(w * 0.35);
    let head = (d * value(adjust, 0, 0.35)).min(w * 0.45);
    // Keep the default notch while moving the point with the handle.
    let notch = if default_head > 1e-6 {
        (default_notch * (head / default_head)).min(w * 0.35)
    } else {
        default_notch
    };
    vec![
        (0.0, 0.0),
        (w - head, 0.0),
        (w, h * 0.5),
        (w - head, h),
        (0.0, h),
        (notch, h * 0.5),
    ]
}

fn circular_arrow(w: f32, h: f32) -> Vec<Pt> {
    let d = w.min(h);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let radius = d * 0.34;
    let centerline = arc(cx, cy, radius, radius, -TAU * 0.17, TAU * 0.66);
    tapered_arrow(&centerline, d * 0.18, d * 0.32, d * 0.32)
}

fn line_thickness(w: f32, h: f32) -> f32 {
    let d = detail(w, h);
    (3.0_f32.min(d * 0.5)).max(d * 0.04).max(0.01)
}

fn line_band(w: f32, h: f32, points: &[Pt]) -> Vec<Pt> {
    stroke_polyline(points, line_thickness(w, h))
}

fn line_arrow_horizontal(w: f32, h: f32, both: bool) -> Vec<Pt> {
    let t = line_thickness(w, h);
    let mid = h * 0.5;
    let head = (12.0_f32.min(w * 0.42)).max(t * 3.0).min(w * 0.48);
    let head_half = (t * 2.4).min(h * 0.35).max(t * 1.2);
    if both {
        vec![
            (0.0, mid),
            (head, mid - head_half),
            (head, mid - t * 0.5),
            (w - head, mid - t * 0.5),
            (w - head, mid - head_half),
            (w, mid),
            (w - head, mid + head_half),
            (w - head, mid + t * 0.5),
            (head, mid + t * 0.5),
            (head, mid + head_half),
        ]
    } else {
        vec![
            (0.0, mid - t * 0.5),
            (w - head, mid - t * 0.5),
            (w - head, mid - head_half),
            (w, mid),
            (w - head, mid + head_half),
            (w - head, mid + t * 0.5),
            (0.0, mid + t * 0.5),
        ]
    }
}

fn line_arrow_vertical(w: f32, h: f32, both: bool) -> Vec<Pt> {
    let source = line_arrow_horizontal(h, w, both);
    map_points(&source, |(x, y)| (y, h - x))
}

fn pie(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let start = value(adjust, 0, 0.0);
    let end = value(adjust, 1, TAU * 0.75);
    let mut out = vec![(w * 0.5, h * 0.5)];
    out.extend(arc(w * 0.5, h * 0.5, w * 0.5, h * 0.5, start, end));
    out
}

fn chord(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let start = value(adjust, 0, 0.0);
    let end = value(adjust, 1, TAU * 0.75);
    arc(w * 0.5, h * 0.5, w * 0.5, h * 0.5, start, end)
}

fn teardrop(w: f32, h: f32) -> Vec<Pt> {
    // A round body whose upper-right quarter is stretched into a point at the
    // frame's top-right corner.
    let r = detail(w, h) * 0.45;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let tip = (w, 0.0);
    let right = (cx + r, cy);
    let top = (cx, cy - r);
    let mut out = vec![tip];
    let mut right_curve = bezier2(tip, (w * 0.98, cy * 0.55), right, 12);
    if !right_curve.is_empty() {
        right_curve.remove(0);
    }
    out.extend(right_curve);
    let mut body = arc(cx, cy, r, r, 0.0, TAU * 0.75);
    if !body.is_empty() {
        body.remove(0);
    }
    out.extend(body);
    let mut top_curve = bezier2(top, (cx + r * 0.7, 0.0), tip, 12);
    if !top_curve.is_empty() {
        top_curve.remove(0);
    }
    out.extend(top_curve);
    out
}

fn frame_contours(w: f32, h: f32, adjust: &[f32]) -> Vec<Vec<Pt>> {
    let inset = detail(w, h) * value(adjust, 0, 0.125);
    if inset <= 0.0 || inset * 2.0 >= w.min(h) {
        return vec![rectangle_points(w, h)];
    }
    vec![
        rectangle_points(w, h),
        rect_at(inset, inset, w - inset, h - inset),
    ]
}

fn half_frame(w: f32, h: f32) -> Vec<Pt> {
    // Top bar and left bar only, meeting at the top-left, with the two free
    // ends cut at an angle.
    let d = detail(w, h);
    let bar = (d * 0.18).min(w * 0.45).min(h * 0.45);
    let slant = (bar * 0.8).min(w * 0.4).min(h * 0.4);
    vec![
        (0.0, 0.0),
        (w, 0.0),
        (w - slant, bar),
        (bar, bar),
        (bar, h - slant),
        (0.0, h),
    ]
}

fn corner_shape(w: f32, h: f32) -> Vec<Pt> {
    let arm = detail(w, h) * 0.3;
    vec![
        (0.0, 0.0),
        (w, 0.0),
        (w, arm),
        (arm, arm),
        (arm, h),
        (0.0, h),
    ]
}

fn plaque(w: f32, h: f32) -> Vec<Pt> {
    // Concave corners: quarter circles centred on the frame corners, so the
    // material is cut back into the shape rather than snip-cut out of it.
    let r = (detail(w, h) * 0.18).min(w * 0.45).min(h * 0.45);
    let pi = std::f32::consts::PI;
    let mut out = vec![(r, 0.0)];
    out.extend(arc(w, 0.0, r, r, pi, pi * 0.5));
    out.extend(arc(w, h, r, r, -pi * 0.5, -pi));
    out.extend(arc(0.0, h, r, r, 0.0, -pi * 0.5));
    out.extend(arc(0.0, 0.0, r, r, pi * 0.5, 0.0));
    out
}

fn can(w: f32, h: f32, adjust: &[f32]) -> Vec<Vec<Pt>> {
    let ry = (h * value(adjust, 0, 0.12)).min(w * 0.4).max(0.01);
    let gap = detail(w, h) * 0.04;
    let top = ellipse(w * 0.5, ry, w * 0.5, ry);
    let cy_bottom = h - ry;
    let body_top = (ry * 2.0 + gap).min(cy_bottom);
    let mut body = vec![(0.0, body_top), (w, body_top)];
    body.extend(arc(
        w * 0.5,
        cy_bottom,
        w * 0.5,
        ry,
        0.0,
        std::f32::consts::PI,
    ));
    vec![top, body]
}

fn cube(w: f32, h: f32, adjust: &[f32]) -> Vec<Vec<Pt>> {
    let depth = value(adjust, 0, 0.25);
    let top = (w * 0.5, 0.0);
    let top_right = (w, h * depth);
    let bottom_right = (w, h * (1.0 - depth));
    let bottom = (w * 0.5, h);
    let bottom_left = (0.0, h * (1.0 - depth));
    let top_left = (0.0, h * depth);
    let center = (w * 0.5, h * 0.5);
    let gap = detail(w, h) * 0.02;
    let faces = [
        vec![top_left, top, top_right, center],
        vec![top_left, center, bottom, bottom_left],
        vec![center, top_right, bottom_right, bottom],
    ];
    faces.iter().map(|face| shrink_polygon(face, gap)).collect()
}

fn folded_corner(w: f32, h: f32, adjust: &[f32]) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let fold = (d * value(adjust, 0, 0.28)).min(w * 0.45).min(h * 0.45);
    let gap = (d * 0.08).min(fold * 0.4);
    let page = vec![(0.0, 0.0), (w, 0.0), (w, h - fold), (w - fold, h), (0.0, h)];
    // A smaller triangle inside the cut, offset by the gap all round.
    let diagonal = gap * std::f32::consts::FRAC_1_SQRT_2;
    let triangle = vec![
        (w, h - fold + gap),
        (w - fold + gap, h),
        (w - diagonal, h - diagonal),
    ];
    vec![page, triangle]
}

fn smiley(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let mut contours = vec![ellipse_in_frame(w, h)];
    let eye = (d * 0.075).max(0.01);
    contours.push(ellipse(w * 0.34, h * 0.36, eye, eye));
    contours.push(ellipse(w * 0.66, h * 0.36, eye, eye));
    // A smile low on the face: the lower arc of a circle, opening upward.
    contours.push(arc_band(
        w * 0.5,
        h * 0.45,
        w * 0.30,
        h * 0.30,
        std::f32::consts::PI * 0.18,
        std::f32::consts::PI * 0.82,
        (d * 0.09).max(0.01),
    ));
    contours
}

fn sun(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let body_x = w * 0.26;
    let body_y = h * 0.26;
    let mut contours = vec![ellipse(cx, cy, body_x, body_y)];
    let rays = 8usize;
    let inner = 0.36;
    let outer = 0.5;
    for i in 0..rays {
        let angle = -TAU * 0.25 + i as f32 * TAU / rays as f32;
        let half = TAU / rays as f32 * 0.16;
        let a = (cx + angle.cos() * w * inner, cy + angle.sin() * h * inner);
        let b = (
            cx + (angle - half).cos() * w * outer,
            cy + (angle - half).sin() * h * outer,
        );
        let c = (
            cx + (angle + half).cos() * w * outer,
            cy + (angle + half).sin() * h * outer,
        );
        contours.push(vec![a, b, c]);
    }
    contours
}

fn arc_shape(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    // The top-right quarter of the frame's ellipse, drawn as a band.
    arc_band(
        w * 0.5,
        h * 0.5,
        w * 0.5,
        h * 0.5,
        value(adjust, 0, -std::f32::consts::PI * 0.5),
        value(adjust, 1, 0.0),
        (detail(w, h) * 0.12).max(0.01),
    )
}

fn block_arc(w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    let d = detail(w, h);
    let cx = w * 0.5;
    let cy = h * 0.55;
    let rx = w * 0.5;
    let ry = cy.max(h * 0.01);
    let thickness = (d * value(adjust, 0, 0.2))
        .max(0.01)
        .min(rx * 0.9)
        .min(ry * 0.9);
    let inner_rx = (rx - thickness).max(rx * 0.05);
    let inner_ry = (ry - thickness).max(ry * 0.05);
    let start = value(adjust, 1, std::f32::consts::PI);
    let end = value(adjust, 2, TAU);
    let mut out = arc(cx, cy, rx, ry, start, end);
    out.extend(arc(cx, cy, inner_rx, inner_ry, end, start));
    out
}

fn bracket(w: f32, h: f32, left: bool, arm_frac: f32) -> Vec<Pt> {
    let thickness = detail(w, h) * 0.14;
    let arm = (w * arm_frac).max(thickness * 2.0);
    let points = if left {
        vec![(arm, 0.0), (0.0, 0.0), (0.0, h), (arm, h)]
    } else {
        vec![(w - arm, 0.0), (w, 0.0), (w, h), (w - arm, h)]
    };
    stroke_polyline(&points, thickness)
}

fn brace(w: f32, h: f32, left: bool, arm: f32) -> Vec<Vec<Pt>> {
    let thickness = (detail(w, h) * 0.11).max(0.01);
    let arm = arm.max(thickness * 2.0).min(w * 0.92).max(0.01);
    let spine = arm * 0.30;
    let tip = (thickness * 0.95).min(arm * 0.55);
    let mid = h * 0.5;
    let neck = (h * 0.10).min(h * 0.3);
    let mut top = bezier2((arm, 0.0), (spine, 0.0), (spine, h * 0.14), 12);
    let top_curve = bezier2((spine, h * 0.14), (spine, mid - neck * 0.6), (tip, mid), 16);
    top.extend(top_curve.into_iter().skip(1));
    let mut bottom = bezier2((arm, h), (spine, h), (spine, h * 0.86), 12);
    let bottom_curve = bezier2((spine, h * 0.86), (spine, mid + neck * 0.6), (tip, mid), 16);
    bottom.extend(bottom_curve.into_iter().skip(1));
    let head = thickness * 1.2;
    let head_half = thickness * 0.55;
    let halves = vec![
        tapered_arrow(&top, thickness, head_half, head),
        tapered_arrow(&bottom, thickness, head_half, head),
    ];
    if left {
        halves
    } else {
        halves
            .into_iter()
            .map(|contour| map_points(&contour, |(x, y)| (w - x, y)))
            .collect()
    }
}

fn math_bars(w: f32, h: f32, count: usize) -> Vec<Vec<Pt>> {
    let mut contours = Vec::new();
    let bar_h = h * 0.13;
    let start = if count == 2 { 0.3 } else { 0.44 };
    for i in 0..count {
        let y = h * (start + i as f32 * 0.26);
        contours.push(rect_at(w * 0.12, y, w * 0.88, y + bar_h));
    }
    contours
}

fn flow_predefined(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let bar = (d * 0.03).max(0.01);
    let left = w * 0.14;
    let right = w * 0.86;
    vec![
        rect_at(0.0, 0.0, left - bar, h),
        rect_at(left + bar, 0.0, right - bar, h),
        rect_at(right + bar, 0.0, w, h),
    ]
}

fn flow_internal_storage(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let bar = (d * 0.025).max(0.01);
    let left = w * 0.14;
    let top = h * 0.14;
    vec![
        rect_at(0.0, 0.0, left - bar, h),
        rect_at(left + bar, 0.0, w, top - bar),
        rect_at(left + bar, top + bar, w, h),
    ]
}

fn flow_multidocument(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let step = (d * 0.16).min(w * 0.2).min(h * 0.2).max(0.01);
    let gap = (d * 0.08).min(step * 0.5).max(0.001);
    let front = translate(
        &wavy_document(w - 2.0 * step, h - 2.0 * step),
        0.0,
        2.0 * step,
    );
    let middle = vec![
        (step, step),
        (w - step, step),
        (w - step, h - step),
        (w - 2.0 * step + gap, h - step),
        (w - 2.0 * step + gap, 2.0 * step - gap),
        (step, 2.0 * step - gap),
    ];
    let back = vec![
        (2.0 * step, 0.0),
        (w, 0.0),
        (w, h - 2.0 * step),
        (w - step + gap, h - 2.0 * step),
        (w - step + gap, step - gap),
        (2.0 * step, step - gap),
    ];
    vec![front, middle, back]
}

fn flow_collate(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        vec![(0.0, 0.0), (w, 0.0), (w * 0.5, h * 0.44)],
        vec![(w * 0.5, h * 0.56), (w, h), (0.0, h)],
    ]
}

fn flow_sort(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        vec![(w * 0.5, 0.0), (w, h * 0.46), (0.0, h * 0.46)],
        vec![(0.0, h * 0.54), (w, h * 0.54), (w * 0.5, h)],
    ]
}

fn flow_delay(w: f32, h: f32) -> Vec<Pt> {
    let r = (h * 0.5).min(w * 0.45);
    let mut out = vec![(0.0, 0.0), (w - r, 0.0)];
    out.extend(arc(w - r, h * 0.5, r, h * 0.5, -TAU * 0.25, TAU * 0.25));
    out.push((0.0, h));
    out
}

fn flow_terminator(w: f32, h: f32) -> Vec<Pt> {
    let pi = std::f32::consts::PI;
    if w >= h {
        let r = h * 0.5;
        let cy = h * 0.5;
        let mut out = arc(r, cy, r, r, pi * 0.5, pi * 1.5);
        out.extend(arc(w - r, cy, r, r, -pi * 0.5, pi * 0.5));
        out
    } else {
        let r = w * 0.5;
        let cx = w * 0.5;
        let mut out = arc(cx, r, r, r, 0.0, -pi);
        out.extend(arc(cx, h - r, r, r, pi, 0.0));
        out
    }
}

fn flow_sequential_access(w: f32, h: f32) -> Vec<Vec<Pt>> {
    // A circle whose lower right quadrant is squared off into the tape's tail.
    let cx = w * 0.5;
    let cy = h * 0.5;
    let r = (detail(w, h) * 0.5).max(0.01);
    let mut out = arc(cx, cy, r, r, std::f32::consts::FRAC_PI_2, TAU);
    out.push((cx + r, cy + r));
    vec![out]
}

fn flow_magnetic_disk(w: f32, h: f32) -> Vec<Vec<Pt>> {
    // A vertical cylinder, with the gap under the top ellipse that `can` has.
    can(w, h, &[])
}

fn flow_direct_access(w: f32, h: f32) -> Vec<Vec<Pt>> {
    stored_data_split(w, h)
}

fn stored_data_split(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let bar = (d * 0.02).max(0.01);
    let split = w * 0.78;
    let cap_x = (w * 0.18).min(w * 0.45);
    let cy = h * 0.5;
    let mut left_piece = vec![(cap_x, 0.0), (split - bar, 0.0), (split - bar, h)];
    let mut left_cap = arc(cap_x, cy, cap_x, cy, TAU * 0.25, TAU * 0.75);
    if !left_cap.is_empty() {
        left_cap.remove(0);
    }
    left_piece.extend(left_cap);
    let mut right_piece = vec![(split + bar, 0.0)];
    right_piece.extend(arc(w - cap_x, cy, cap_x, cy, -TAU * 0.25, TAU * 0.25));
    right_piece.push((split + bar, h));
    vec![left_piece, right_piece]
}

fn flow_display(w: f32, h: f32) -> Vec<Pt> {
    let d = detail(w, h);
    let cap = (h * 0.5).min(w * 0.45).max(0.01);
    let point = (d * 0.15).max(0.01).min(w * 0.3);
    let right = (w - cap).max(point);
    let mut out = vec![(point, 0.0), (right, 0.0)];
    out.extend(arc(
        right,
        h * 0.5,
        cap,
        h * 0.5,
        -std::f32::consts::PI * 0.5,
        std::f32::consts::PI * 0.5,
    ));
    out.push((point, h));
    out.push((0.0, h * 0.5));
    out
}

/// Fixed, uneven burst vertices as `(turns, radius)`. Angles only increase, so
/// the polygon is star-shaped and cannot self-intersect.
const BURST_1: [(f32, f32); 24] = [
    (0.000, 1.00),
    (0.045, 0.46),
    (0.090, 0.86),
    (0.130, 0.54),
    (0.170, 0.97),
    (0.220, 0.40),
    (0.270, 0.78),
    (0.305, 0.52),
    (0.340, 0.93),
    (0.390, 0.44),
    (0.440, 0.88),
    (0.485, 0.56),
    (0.530, 1.00),
    (0.570, 0.41),
    (0.610, 0.82),
    (0.655, 0.50),
    (0.700, 0.95),
    (0.745, 0.45),
    (0.790, 0.84),
    (0.830, 0.53),
    (0.870, 0.98),
    (0.910, 0.42),
    (0.950, 0.87),
    (0.985, 0.55),
];

const BURST_2: [(f32, f32); 28] = [
    (0.000, 0.98),
    (0.035, 0.45),
    (0.070, 0.84),
    (0.105, 0.53),
    (0.140, 0.95),
    (0.180, 0.41),
    (0.215, 0.79),
    (0.250, 0.55),
    (0.285, 1.00),
    (0.325, 0.43),
    (0.360, 0.86),
    (0.395, 0.50),
    (0.430, 0.92),
    (0.470, 0.46),
    (0.505, 0.81),
    (0.540, 0.54),
    (0.575, 0.97),
    (0.615, 0.42),
    (0.650, 0.83),
    (0.690, 0.51),
    (0.725, 0.99),
    (0.760, 0.44),
    (0.795, 0.85),
    (0.835, 0.56),
    (0.870, 0.94),
    (0.905, 0.40),
    (0.940, 0.80),
    (0.975, 0.52),
];

fn burst(w: f32, h: f32, table: &[(f32, f32)]) -> Vec<Pt> {
    let mut out = Vec::with_capacity(table.len());
    for &(turn, radius) in table {
        let angle = turn * TAU;
        out.push((
            w * 0.5 + angle.cos() * radius * w * 0.5,
            h * 0.5 + angle.sin() * radius * h * 0.5,
        ));
    }
    out
}

fn callout_line_thickness(w: f32, h: f32) -> f32 {
    (3.0_f32).min(detail(w, h) * 0.5).max(0.01)
}

fn callout_body(w: f32, h: f32, kind: ShapeKind, adjust: &[f32]) -> Vec<Vec<Pt>> {
    let tip = (value(adjust, 0, 0.14) * w, value(adjust, 1, 1.0) * h);
    match kind {
        ShapeKind::CalloutRectangle => callout_wedge(w, h, &rect_at(0.0, 0.0, w, h * 0.72), tip),
        ShapeKind::CalloutRounded => callout_wedge(
            w,
            h,
            &rounded_polygon(&rect_at(0.0, 0.0, w, h * 0.72), detail(w, h) * 0.12),
            tip,
        ),
        ShapeKind::CalloutOval => {
            callout_wedge(w, h, &ellipse(w * 0.5, h * 0.36, w * 0.5, h * 0.36), tip)
        }
        ShapeKind::CalloutCloud => {
            let d = detail(w, h);
            vec![
                cloud_body(w, h * 0.74),
                ellipse(w * 0.34, h * 0.85, d * 0.07, d * 0.07),
                ellipse(w * 0.21, h * 0.93, d * 0.05, d * 0.05),
                ellipse(w * 0.11, h * 0.99, d * 0.035, d * 0.035),
            ]
        }
        ShapeKind::CalloutLine | ShapeKind::CalloutLineBent | ShapeKind::CalloutLineDoubleBent => {
            let t = callout_line_thickness(w, h);
            vec![
                rect_at(0.0, 0.0, w, h * 0.7),
                stroke_polyline(&line_callout_tail(w, h, kind, adjust), t),
            ]
        }
        _ => Vec::new(),
    }
}

fn action_base(w: f32, h: f32) -> Vec<Pt> {
    rounded_polygon(&rectangle_points(w, h), detail(w, h) * 0.18)
}

fn glyph_box(w: f32, h: f32) -> (f32, f32, f32) {
    let s = detail(w, h) * 0.62;
    (w * 0.5 - s * 0.5, h * 0.5 - s * 0.5, s)
}

fn glyph_point(w: f32, h: f32, x: f32, y: f32) -> Pt {
    let (x0, y0, s) = glyph_box(w, h);
    (x0 + x * s, y0 + y * s)
}

fn glyph_rect(w: f32, h: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    let a = glyph_point(w, h, x0, y0);
    let b = glyph_point(w, h, x1, y1);
    rect_at(a.0, a.1, b.0, b.1)
}

fn glyph_polygon(w: f32, h: f32, points: &[(f32, f32)]) -> Vec<Pt> {
    map_points(points, |(x, y)| glyph_point(w, h, x, y))
}

fn glyph_band(w: f32, h: f32, points: &[(f32, f32)], thickness: f32) -> Vec<Pt> {
    let mapped = glyph_polygon(w, h, points);
    let (_, _, s) = glyph_box(w, h);
    stroke_polyline(&mapped, s * thickness)
}

fn question_holes(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        glyph_band(
            w,
            h,
            &[
                (0.3, 0.32),
                (0.34, 0.16),
                (0.5, 0.08),
                (0.66, 0.16),
                (0.7, 0.32),
                (0.64, 0.44),
                (0.5, 0.52),
                (0.5, 0.58),
            ],
            0.15,
        ),
        glyph_circle(w, h, 0.5, 0.86, 0.07),
    ]
}

fn glyph_circle(w: f32, h: f32, x: f32, y: f32, r: f32) -> Vec<Pt> {
    let (_, _, s) = glyph_box(w, h);
    let c = glyph_point(w, h, x, y);
    ellipse(c.0, c.1, s * r, s * r)
}

fn info_holes(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        glyph_circle(w, h, 0.5, 0.18, 0.07),
        glyph_rect(w, h, 0.44, 0.34, 0.56, 0.86),
    ]
}

fn action_holes(w: f32, h: f32, kind: ShapeKind) -> Vec<Vec<Pt>> {
    match kind {
        ShapeKind::ActionBlank => Vec::new(),
        ShapeKind::ActionHome => vec![glyph_polygon(
            w,
            h,
            &[
                (0.5, 0.05),
                (0.95, 0.44),
                (0.82, 0.44),
                (0.82, 0.95),
                (0.58, 0.95),
                (0.58, 0.66),
                (0.42, 0.66),
                (0.42, 0.95),
                (0.18, 0.95),
                (0.18, 0.44),
                (0.05, 0.44),
            ],
        )],
        ShapeKind::ActionHelp => question_holes(w, h),
        ShapeKind::ActionInformation => info_holes(w, h),
        ShapeKind::ActionBack => vec![glyph_polygon(
            w,
            h,
            &[
                (0.18, 0.5),
                (0.46, 0.18),
                (0.46, 0.38),
                (0.86, 0.38),
                (0.86, 0.62),
                (0.46, 0.62),
                (0.46, 0.82),
            ],
        )],
        ShapeKind::ActionForward => vec![glyph_polygon(
            w,
            h,
            &[
                (0.82, 0.5),
                (0.54, 0.18),
                (0.54, 0.38),
                (0.14, 0.38),
                (0.14, 0.62),
                (0.54, 0.62),
                (0.54, 0.82),
            ],
        )],
        ShapeKind::ActionBeginning => vec![
            glyph_rect(w, h, 0.14, 0.2, 0.24, 0.8),
            glyph_polygon(w, h, &[(0.3, 0.5), (0.84, 0.2), (0.84, 0.8)]),
        ],
        ShapeKind::ActionEnd => vec![
            glyph_polygon(w, h, &[(0.16, 0.2), (0.7, 0.5), (0.16, 0.8)]),
            glyph_rect(w, h, 0.76, 0.2, 0.86, 0.8),
        ],
        ShapeKind::ActionReturn => {
            let start = glyph_point(w, h, 0.82, 0.8);
            let control = glyph_point(w, h, 0.82, 0.18);
            let end = glyph_point(w, h, 0.3, 0.28);
            let centerline = bezier2(start, control, end, 20);
            let (_, _, s) = glyph_box(w, h);
            vec![tapered_arrow(&centerline, s * 0.16, s * 0.22, s * 0.22)]
        }
        ShapeKind::ActionDocument => vec![glyph_polygon(
            w,
            h,
            &[
                (0.15, 0.1),
                (0.68, 0.1),
                (0.85, 0.27),
                (0.85, 0.9),
                (0.15, 0.9),
            ],
        )],
        ShapeKind::ActionSound => {
            let (_, _, s) = glyph_box(w, h);
            let mut holes = vec![glyph_polygon(
                w,
                h,
                &[
                    (0.12, 0.42),
                    (0.28, 0.42),
                    (0.48, 0.2),
                    (0.48, 0.8),
                    (0.28, 0.58),
                    (0.12, 0.58),
                ],
            )];
            let c = glyph_point(w, h, 0.48, 0.5);
            holes.push(arc_band(
                c.0,
                c.1,
                s * 0.2,
                s * 0.24,
                -TAU * 0.2,
                TAU * 0.2,
                s * 0.055,
            ));
            holes.push(arc_band(
                c.0,
                c.1,
                s * 0.3,
                s * 0.34,
                -TAU * 0.2,
                TAU * 0.2,
                s * 0.055,
            ));
            holes
        }
        ShapeKind::ActionMovie => {
            let mut holes = vec![glyph_rect(w, h, 0.12, 0.32, 0.88, 0.68)];
            for i in 0..3 {
                let x = 0.18 + i as f32 * 0.26;
                holes.push(glyph_rect(w, h, x, 0.36, x + 0.1, 0.44));
                holes.push(glyph_rect(w, h, x, 0.56, x + 0.1, 0.64));
            }
            holes
        }
        _ => Vec::new(),
    }
}

fn action_button(w: f32, h: f32, kind: ShapeKind) -> Vec<Vec<Pt>> {
    let mut contours = vec![action_base(w, h)];
    contours.extend(action_holes(w, h, kind));
    contours
}

fn check_hole(w: f32, h: f32) -> Vec<Pt> {
    let d = detail(w, h);
    stroke_polyline(
        &[
            (w * 0.26, h * 0.52),
            (w * 0.43, h * 0.7),
            (w * 0.74, h * 0.32),
        ],
        (d * 0.11).max(0.01),
    )
}

fn cross_hole(w: f32, h: f32) -> Vec<Pt> {
    let d = detail(w, h);
    plus_polygon(
        w * 0.5,
        h * 0.5,
        d * 0.28,
        (d * 0.055).max(0.01),
        TAU * 0.125,
    )
}

fn question_mark_hole(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        stroke_polyline(
            &[
                (w * 0.34, h * 0.36),
                (w * 0.38, h * 0.22),
                (w * 0.5, h * 0.16),
                (w * 0.62, h * 0.22),
                (w * 0.66, h * 0.36),
                (w * 0.6, h * 0.48),
                (w * 0.5, h * 0.56),
                (w * 0.5, h * 0.66),
            ],
            (d * 0.11).max(0.01),
        ),
        ellipse(w * 0.5, h * 0.86, d * 0.06, d * 0.06),
    ]
}

fn info_holes_frame(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        ellipse(w * 0.5, h * 0.2, d * 0.06, d * 0.06),
        rect_at(w * 0.44, h * 0.34, w * 0.56, h * 0.84),
    ]
}

fn warning_triangle(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let top = (d * 0.10).max(0.01);
    let bottom = (d * 0.14).max(top);
    let cx = w * 0.5;
    let y0 = h * 0.32;
    let y1 = h * 0.68;
    vec![
        vec![(cx, 0.0), (w, h), (0.0, h)],
        vec![
            (cx - top * 0.5, y0),
            (cx + top * 0.5, y0),
            (cx + bottom * 0.5, y1),
            (cx - bottom * 0.5, y1),
        ],
        ellipse(cx, h * 0.84, d * 0.07, d * 0.07),
    ]
}

fn lightbulb(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let cx = w * 0.5;
    let r = (w.min(h) * 0.34).max(0.01);
    let cy = r + h * 0.01;
    let neck_half = r * 0.48;
    let neck_y = cy + r * 1.2;
    let mut bulb = arc(
        cx,
        cy,
        r,
        r,
        std::f32::consts::PI * 0.8,
        std::f32::consts::PI * 2.2,
    );
    bulb.push((cx + neck_half, neck_y));
    bulb.push((cx - neck_half, neck_y));
    let bar_h = (h * 0.07).max(0.01);
    let gap = (h * 0.035).max(0.01);
    let y1 = neck_y + gap;
    let y2 = y1 + bar_h + gap;
    vec![
        bulb,
        rect_at(cx - neck_half, y1, cx + neck_half, (y1 + bar_h).min(h)),
        rect_at(
            cx - neck_half * 0.75,
            y2.min(h - 0.01),
            cx + neck_half * 0.75,
            (y2 + bar_h).min(h),
        ),
    ]
}

fn target(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let ratios = [1.0, 0.78, 0.56, 0.34, 0.14];
    let cx = w * 0.5;
    let cy = h * 0.5;
    ratios
        .iter()
        .map(|r| ellipse(cx, cy, w * 0.5 * r, h * 0.5 * r))
        .collect()
}

fn ring_handle(cx: f32, cy: f32, outer: f32, inner: f32, left: bool) -> Vec<Vec<Pt>> {
    let start = if left { std::f32::consts::PI } else { 0.0 };
    vec![
        arc(cx, cy, outer, outer, start, start + TAU),
        arc(cx, cy, inner, inner, start, start + TAU),
    ]
}

fn trophy(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let cup_top = h * 0.10;
    let cup_bottom = h * 0.50;
    let mut cup = vec![
        (w * 0.18, cup_top),
        (w * 0.82, cup_top),
        (w * 0.64, cup_bottom),
    ];
    let rounded_bottom = bezier2(
        (w * 0.64, cup_bottom),
        (w * 0.5, h * 0.56),
        (w * 0.36, cup_bottom),
        10,
    );
    cup.extend(rounded_bottom.into_iter().skip(1));
    let stem = rect_at(w * 0.45, h * 0.58, w * 0.55, h * 0.74);
    let base = rounded_polygon(
        &rect_at(w * 0.32, h * 0.80, w * 0.68, h * 0.92),
        (d * 0.03).max(0.01),
    );
    let handle_r = (d * 0.13).max(0.01).min(w * 0.2);
    let handle_inner = (handle_r * 0.55).max(0.001);
    let mut contours = vec![cup, stem, base];
    let left = ring_handle(w * 0.22, h * 0.24, handle_r, handle_inner, true);
    let right = ring_handle(w * 0.78, h * 0.24, handle_r, handle_inner, false);
    contours.extend(left);
    contours.extend(right);
    contours
}

fn medal(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let cx = w * 0.5;
    let cy = h * 0.58;
    let r = (d * 0.30).max(0.01).min(w * 0.45).min(h * 0.45);
    let gap = (d * 0.08).max(0.001).min(r * 0.3);
    let strap_bottom = (cy - r - gap).max(0.0);
    let left_strap = vec![
        (w * 0.30, 0.0),
        (w * 0.42, 0.0),
        (cx - d * 0.04, strap_bottom),
        (cx - d * 0.15, strap_bottom),
    ];
    let right_strap = vec![
        (w * 0.58, 0.0),
        (w * 0.70, 0.0),
        (cx + d * 0.15, strap_bottom),
        (cx + d * 0.04, strap_bottom),
    ];
    let star = star_in_circle(5, 0.40, cx, cy, r * 0.55, r * 0.55);
    vec![left_strap, right_strap, ellipse(cx, cy, r, r), star]
}

fn flag(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        rect_at(w * 0.12, 0.0, w * 0.2, h),
        vec![
            (w * 0.2, h * 0.05),
            (w * 0.92, h * 0.12),
            (w * 0.76, h * 0.32),
            (w * 0.92, h * 0.52),
            (w * 0.2, h * 0.52),
        ],
    ]
}

fn location_pin(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let cx = w * 0.5;
    let cy = h * 0.38;
    let rx = w * 0.4;
    let ry = h * 0.38;
    let mut outline = arc(cx, cy, rx, ry, std::f32::consts::PI * 0.8, TAU * 1.2);
    outline.push((w * 0.5, h));
    vec![outline, ellipse(cx, cy, w * 0.15, h * 0.14)]
}

fn clock(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        ellipse_in_frame(w, h),
        stroke_polyline(
            &[(w * 0.5, h * 0.22), (w * 0.5, h * 0.5), (w * 0.72, h * 0.5)],
            (d * 0.09).max(0.01),
        ),
    ]
}

fn lock(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let body = rounded_polygon(
        &rect_at(w * 0.16, h * 0.44, w * 0.84, h * 0.94),
        (d * 0.08).max(0.01),
    );
    vec![
        arc_band(
            w * 0.5,
            h * 0.4,
            w * 0.24,
            h * 0.2,
            std::f32::consts::PI,
            TAU,
            (d * 0.06).max(0.01),
        ),
        body,
        ellipse(w * 0.5, h * 0.65, d * 0.05, d * 0.05),
        rect_at(w * 0.47, h * 0.65, w * 0.53, h * 0.8),
    ]
}

fn key(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let mut contours = vec![
        ellipse_from_top(w * 0.28, h * 0.5, w * 0.2, h * 0.2),
        ellipse_from_top(w * 0.28, h * 0.5, w * 0.1, h * 0.1),
        vec![
            (w * 0.96, h * 0.44),
            (w * 0.96, h * 0.56),
            (w * 0.42, h * 0.56),
            (w * 0.42, h * 0.44),
        ],
    ];
    contours.push(vec![
        (w * 0.78, h * 0.72),
        (w * 0.86, h * 0.72),
        (w * 0.86, h * 0.56),
        (w * 0.78, h * 0.56),
    ]);
    contours.push(vec![
        (w * 0.88, h * 0.7),
        (w * 0.96, h * 0.7),
        (w * 0.96, h * 0.56),
        (w * 0.88, h * 0.56),
    ]);
    contours
}

fn shield(w: f32, h: f32) -> Vec<Pt> {
    vec![
        (w * 0.5, 0.0),
        (w * 0.92, h * 0.14),
        (w * 0.86, h * 0.6),
        (w * 0.5, h),
        (w * 0.14, h * 0.6),
        (w * 0.08, h * 0.14),
    ]
}

fn book(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        vec![
            (w * 0.06, h * 0.18),
            (w * 0.48, h * 0.12),
            (w * 0.48, h * 0.84),
            (w * 0.06, h * 0.92),
        ],
        vec![
            (w * 0.52, h * 0.12),
            (w * 0.94, h * 0.18),
            (w * 0.94, h * 0.92),
            (w * 0.52, h * 0.84),
        ],
    ]
}

fn graduation_cap(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        vec![
            (w * 0.5, h * 0.1),
            (w * 0.97, h * 0.34),
            (w * 0.5, h * 0.58),
            (w * 0.03, h * 0.34),
        ],
        vec![
            (w * 0.34, h * 0.78),
            (w * 0.66, h * 0.78),
            (w * 0.72, h * 0.44),
            (w * 0.28, h * 0.44),
        ],
        stroke_polyline(
            &[(w * 0.86, h * 0.72), (w * 0.86, h * 0.3)],
            (d * 0.025).max(0.01),
        ),
        ellipse(w * 0.86, h * 0.76, d * 0.04, d * 0.04),
    ]
}

fn person_body(w: f32, h: f32) -> Vec<Pt> {
    rounded_polygon(
        &[
            (w * 0.18, h),
            (w * 0.24, h * 0.62),
            (w * 0.4, h * 0.5),
            (w * 0.6, h * 0.5),
            (w * 0.76, h * 0.62),
            (w * 0.82, h),
        ],
        detail(w, h) * 0.1,
    )
}

fn person_parts(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        ellipse(w * 0.5, h * 0.2, w * 0.2, h * 0.2),
        person_body(w, h),
    ]
}

fn person(w: f32, h: f32) -> Vec<Vec<Pt>> {
    person_parts(w, h)
}

fn people(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let half = w * 0.5;
    let left_h = h * 0.92;
    let left_offset = h * 0.08;
    let left_head_r = (half.min(left_h) * 0.2).max(0.01);
    let right_head_r = (half.min(h) * 0.2).max(0.01);
    vec![
        ellipse(
            half * 0.5,
            left_offset + left_h * 0.2,
            left_head_r,
            left_head_r,
        ),
        translate(&person_body(half, left_h), 0.0, left_offset),
        ellipse(half * 1.5, h * 0.2, right_head_r, right_head_r),
        translate(&person_body(half, h), half, 0.0),
    ]
}

fn thumbs_up(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    let gap = (d * 0.08).max(0.001);
    let fist_left = w * 0.20;
    let fist_right = w * 0.84;
    let fist_top = h * 0.34;
    let fist_bottom = h * 0.94;
    let cuff_right = (fist_left - gap).max(w * 0.02);
    let mut contours = vec![rounded_polygon(
        &rect_at(w * 0.02, h * 0.36, cuff_right, h * 0.88),
        (d * 0.05).max(0.01),
    )];
    contours.push(rounded_polygon(
        &rect_at(fist_left, fist_top, fist_right, fist_bottom),
        (d * 0.08).max(0.01),
    ));
    let slot_half = (d * 0.04).max(0.005);
    let slot_x0 = w * 0.42;
    let slot_x1 = (fist_right - gap).max(slot_x0 + d * 0.01);
    for i in 0..4 {
        let cy = h * (0.45 + i as f32 * 0.14);
        contours.push(rounded_polygon(
            &rect_at(slot_x0, cy - slot_half, slot_x1, cy + slot_half),
            slot_half * 0.45,
        ));
    }
    contours.push(rounded_polygon(
        &[
            (w * 0.38, h * 0.12),
            (w * 0.48, h * 0.16),
            (w * 0.50, h * 0.36),
            (w * 0.46, h * 0.42),
            (w * 0.30, h * 0.42),
            (w * 0.27, h * 0.34),
            (w * 0.29, h * 0.20),
        ],
        (d * 0.05).max(0.01),
    ));
    contours
}

fn speech_bubble(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let body = rounded_polygon(
        &rect_at(w * 0.04, h * 0.06, w * 0.96, h * 0.7),
        detail(w, h) * 0.12,
    );
    vec![splice_wedge(
        &body,
        (w * 0.5, h * 0.38),
        (w * 0.2, h * 0.94),
        detail(w, h) * 0.08,
    )]
}

fn thought_bubble(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        cloud_body(w, h * 0.74),
        ellipse(w * 0.34, h * 0.85, d * 0.07, d * 0.07),
        ellipse(w * 0.21, h * 0.93, d * 0.05, d * 0.05),
        ellipse(w * 0.11, h * 0.99, d * 0.035, d * 0.035),
    ]
}

fn play_circle(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        ellipse_in_frame(w, h),
        vec![
            (w * 0.38, h * 0.28),
            (w * 0.38, h * 0.72),
            (w * 0.72, h * 0.5),
        ],
    ]
}

fn pause_circle(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        ellipse_in_frame(w, h),
        rect_at(w * 0.34, h * 0.28, w * 0.44, h * 0.72),
        rect_at(w * 0.56, h * 0.28, w * 0.66, h * 0.72),
    ]
}

fn gear(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let teeth = 8usize;
    let step = TAU / teeth as f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let rx = w * 0.48;
    let ry = h * 0.48;
    let mut points = Vec::with_capacity(teeth * 4);
    for i in 0..teeth {
        let a = -TAU * 0.25 + i as f32 * step;
        for (offset, radius) in [(0.1, 0.72), (0.25, 1.0), (0.75, 1.0), (0.9, 0.72)] {
            let angle = a + step * offset;
            points.push((
                cx + angle.cos() * rx * radius,
                cy + angle.sin() * ry * radius,
            ));
        }
    }
    vec![points, ellipse(cx, cy, w * 0.22, h * 0.22)]
}

fn magnifier(w: f32, h: f32) -> Vec<Vec<Pt>> {
    let d = detail(w, h);
    vec![
        ellipse(w * 0.42, h * 0.4, w * 0.3, h * 0.3),
        ellipse(w * 0.42, h * 0.4, w * 0.2, h * 0.2),
        stroke_polyline(
            &[(w * 0.92, h * 0.92), (w * 0.64, h * 0.62)],
            (d * 0.09).max(0.01),
        ),
    ]
}

fn star_badge(w: f32, h: f32) -> Vec<Vec<Pt>> {
    vec![
        ellipse_in_frame(w, h),
        ellipse(w * 0.5, h * 0.5, w * 0.4, h * 0.4),
        star_in_circle(5, 0.45, w * 0.5, h * 0.5, w * 0.28, h * 0.28),
    ]
}

/// The frame text occupies inside a shape, in frame units: `(x, y, w, h)`.
///
/// The default is the frame inset by 6 slide px. Kinds whose usable area is
/// smaller or off-centre override it so text sits on the visible body: inside
/// an ellipse's inscribed rectangle, a diamond's middle, a callout's bubble,
/// a block arrow's shaft, and so on. The result always lies inside the frame;
/// `Lines` kinds return an empty rect because they take no text.
#[must_use]
pub fn shape_text_rect(kind: ShapeKind, w: f32, h: f32, _adjust: &[f32]) -> (f32, f32, f32, f32) {
    if !finite(w) || !finite(h) || w <= 0.0 || h <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    if !coursecraft_course::shape_allows_text(kind) {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let inset = 6.0_f32.min(w * 0.25).min(h * 0.25);
    let (mut x, mut y, mut rw, mut rh) = (inset, inset, w - inset * 2.0, h - inset * 2.0);
    let mut set = |fx: f32, fy: f32, fw: f32, fh: f32| {
        x = w * fx;
        y = h * fy;
        rw = w * fw;
        rh = h * fh;
    };
    match kind {
        // The largest axis-aligned rectangle inside an ellipse.
        ShapeKind::Ellipse
        | ShapeKind::FlowConnector
        | ShapeKind::CheckCircle
        | ShapeKind::CrossCircle
        | ShapeKind::InfoCircle
        | ShapeKind::QuestionCircle
        | ShapeKind::PlayCircle
        | ShapeKind::PauseCircle => {
            let side = std::f32::consts::FRAC_1_SQRT_2;
            let margin = (1.0 - side) * 0.5;
            set(margin, margin, side, side);
        }
        ShapeKind::Diamond | ShapeKind::FlowDecision => set(0.25, 0.25, 0.5, 0.5),
        ShapeKind::Triangle | ShapeKind::FlowExtract => set(0.18, 0.42, 0.64, 0.52),
        ShapeKind::RightTriangle => set(0.22, 0.35, 0.7, 0.6),
        ShapeKind::FlowMerge => set(0.2, 0.1, 0.6, 0.5),
        ShapeKind::CalloutRectangle | ShapeKind::CalloutRounded | ShapeKind::CalloutCloud => {
            set(0.08, 0.06, 0.84, 0.6);
        }
        // The oval bubble is only 72% of the frame tall; use its inscribed
        // rectangle so the text stays inside the ellipse.
        ShapeKind::CalloutOval => set(
            0.5 - std::f32::consts::FRAC_1_SQRT_2 * 0.5,
            0.36 - std::f32::consts::FRAC_1_SQRT_2 * 0.36,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2 * 0.72,
        ),
        ShapeKind::CalloutLine | ShapeKind::CalloutLineBent | ShapeKind::CalloutLineDoubleBent => {
            set(0.06, 0.05, 0.88, 0.6)
        }
        ShapeKind::ArrowRight | ShapeKind::ArrowStripedRight | ShapeKind::ArrowNotchedRight => {
            set(0.05, 0.28, 0.65, 0.44);
        }
        ShapeKind::ArrowLeft => set(0.3, 0.28, 0.65, 0.44),
        ShapeKind::ArrowUp => set(0.28, 0.05, 0.44, 0.65),
        ShapeKind::ArrowDown => set(0.28, 0.3, 0.44, 0.65),
        ShapeKind::ArrowLeftRight => set(0.18, 0.3, 0.64, 0.4),
        ShapeKind::ArrowUpDown => set(0.3, 0.18, 0.4, 0.64),
        ShapeKind::ArrowQuad => set(0.28, 0.28, 0.44, 0.44),
        ShapeKind::ArrowChevron => set(0.22, 0.15, 0.6, 0.7),
        ShapeKind::ArrowPentagon | ShapeKind::Pentagon => set(0.18, 0.22, 0.64, 0.62),
        ShapeKind::Star4
        | ShapeKind::Star5
        | ShapeKind::Star6
        | ShapeKind::Star7
        | ShapeKind::Star8
        | ShapeKind::Star10
        | ShapeKind::Star12
        | ShapeKind::Star16
        | ShapeKind::Star24
        | ShapeKind::Star32
        | ShapeKind::Explosion1
        | ShapeKind::Explosion2
        | ShapeKind::StarBadge
        | ShapeKind::Sun => set(0.28, 0.28, 0.44, 0.44),
        ShapeKind::Cloud => set(0.15, 0.28, 0.7, 0.45),
        ShapeKind::Heart => set(0.18, 0.18, 0.64, 0.5),
        ShapeKind::Plus | ShapeKind::MathPlus | ShapeKind::MathMultiply => {
            set(0.28, 0.28, 0.44, 0.44);
        }
        ShapeKind::MathMinus | ShapeKind::MathEqual | ShapeKind::MathNotEqual => {
            set(0.18, 0.38, 0.64, 0.24);
        }
        ShapeKind::FlowTerminator => set(0.1, 0.1, 0.8, 0.8),
        ShapeKind::FlowInputOutput => set(0.12, 0.1, 0.76, 0.8),
        ShapeKind::FlowInternalStorage => set(0.12, 0.12, 0.76, 0.76),
        ShapeKind::FlowDocument | ShapeKind::FlowMultidocument => set(0.08, 0.05, 0.84, 0.7),
        ShapeKind::FlowPreparation => set(0.2, 0.15, 0.6, 0.7),
        ShapeKind::FlowManualInput => set(0.08, 0.15, 0.84, 0.75),
        ShapeKind::FlowManualOperation => set(0.15, 0.08, 0.7, 0.84),
        ShapeKind::FlowOffpageConnector => set(0.1, 0.05, 0.8, 0.55),
        ShapeKind::FlowCard => set(0.08, 0.12, 0.84, 0.8),
        ShapeKind::FlowPunchedTape => set(0.1, 0.15, 0.8, 0.65),
        ShapeKind::FlowSummingJunction | ShapeKind::FlowOr => set(0.22, 0.22, 0.56, 0.56),
        ShapeKind::FlowCollate | ShapeKind::FlowSort => set(0.2, 0.15, 0.6, 0.7),
        ShapeKind::FlowStoredData => set(0.1, 0.15, 0.8, 0.7),
        ShapeKind::FlowDelay => set(0.12, 0.15, 0.7, 0.7),
        ShapeKind::FlowSequentialAccess
        | ShapeKind::FlowMagneticDisk
        | ShapeKind::FlowDirectAccess => set(0.12, 0.12, 0.76, 0.76),
        ShapeKind::FlowDisplay => set(0.15, 0.1, 0.7, 0.7),
        ShapeKind::ActionBlank
        | ShapeKind::ActionHome
        | ShapeKind::ActionHelp
        | ShapeKind::ActionInformation
        | ShapeKind::ActionBack
        | ShapeKind::ActionForward
        | ShapeKind::ActionBeginning
        | ShapeKind::ActionEnd
        | ShapeKind::ActionReturn
        | ShapeKind::ActionDocument
        | ShapeKind::ActionSound
        | ShapeKind::ActionMovie
        | ShapeKind::CheckMark
        | ShapeKind::CrossMark
        | ShapeKind::Target
        | ShapeKind::Trophy
        | ShapeKind::Medal
        | ShapeKind::Flag
        | ShapeKind::LocationPin
        | ShapeKind::Clock
        | ShapeKind::Lock
        | ShapeKind::Key
        | ShapeKind::Shield
        | ShapeKind::Book
        | ShapeKind::GraduationCap
        | ShapeKind::Person
        | ShapeKind::People
        | ShapeKind::ThumbsUp
        | ShapeKind::SpeechBubble
        | ShapeKind::ThoughtBubble
        | ShapeKind::Gear
        | ShapeKind::Magnifier => set(0.25, 0.25, 0.5, 0.5),
        _ => {}
    }
    // Clamp to the frame so hostile sizes and overrides never escape it.
    let x = x.clamp(0.0, w);
    let y = y.clamp(0.0, h);
    let rw = rw.clamp(0.0, w - x);
    let rh = rh.clamp(0.0, h - y);
    (x, y, rw, rh)
}

fn clean_contour(mut points: Vec<Pt>, w: f32, h: f32, allow_outside: bool) -> Vec<Pt> {
    points.retain(|(x, y)| finite(*x) && finite(*y));
    if !allow_outside {
        for point in &mut points {
            point.0 = point.0.clamp(0.0, w);
            point.1 = point.1.clamp(0.0, h);
        }
    }
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
    for point in points {
        if out
            .last()
            .is_none_or(|last: &Pt| distance(*last, point) > 1e-6)
        {
            out.push(point);
        }
    }
    if out.len() > 2
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(&first, &last)| distance(first, last) <= 1e-6)
    {
        out.pop();
    }
    out
}

fn nearest_angle(current: f32, raw: f32) -> f32 {
    if !finite(raw) {
        return current;
    }
    raw + ((current - raw) / TAU).round() * TAU
}

fn star_handle(points: usize, inner: f32, w: f32, h: f32) -> Pt {
    let step = TAU / (points * 2) as f32;
    let angle = -TAU / 4.0 + step;
    (
        w * 0.5 + angle.cos() * inner * w * 0.5,
        h * 0.5 + angle.sin() * inner * h * 0.5,
    )
}

/// Names shown while hovering an adjustment handle.
#[must_use]
pub fn adjust_handle_name(kind: ShapeKind, index: usize) -> &'static str {
    match kind {
        ShapeKind::SnipRound => match index {
            0 => "Corner cut",
            1 => "Corner radius",
            _ => "Adjust",
        },
        ShapeKind::RoundedRectangle
        | ShapeKind::SnipSingleCorner
        | ShapeKind::SnipSameSide
        | ShapeKind::SnipDiagonal
        | ShapeKind::RoundSingleCorner
        | ShapeKind::RoundSameSide
        | ShapeKind::RoundDiagonal => "Corner size",
        ShapeKind::CalloutRectangle | ShapeKind::CalloutRounded | ShapeKind::CalloutOval => {
            "Tail tip"
        }
        ShapeKind::CalloutLine => "Tail point",
        ShapeKind::CalloutLineBent | ShapeKind::CalloutLineDoubleBent => {
            if index + 2 >= 4 {
                "Tail point"
            } else {
                "Bend point"
            }
        }
        ShapeKind::ArrowRight
        | ShapeKind::ArrowLeft
        | ShapeKind::ArrowUp
        | ShapeKind::ArrowDown
        | ShapeKind::ArrowLeftRight
        | ShapeKind::ArrowUpDown => {
            if index == 0 {
                "Head length"
            } else {
                "Shaft thickness"
            }
        }
        ShapeKind::ArrowPentagon | ShapeKind::ArrowChevron => "Point depth",
        ShapeKind::Parallelogram | ShapeKind::Trapezoid => "Slant",
        ShapeKind::Hexagon | ShapeKind::Octagon | ShapeKind::Plus => "Corner cut",
        ShapeKind::Star4
        | ShapeKind::Star5
        | ShapeKind::Star6
        | ShapeKind::Star7
        | ShapeKind::Star8
        | ShapeKind::Star10
        | ShapeKind::Star12
        | ShapeKind::Star16 => "Inner radius",
        ShapeKind::Donut => "Thickness",
        ShapeKind::BlockArc => match index {
            0 => "Thickness",
            1 => "Start angle",
            _ => "End angle",
        },
        ShapeKind::Pie | ShapeKind::Chord | ShapeKind::Arc => {
            if index == 0 {
                "Start angle"
            } else {
                "End angle"
            }
        }
        ShapeKind::Frame | ShapeKind::Can | ShapeKind::Bevel => "Band",
        ShapeKind::FoldedCorner | ShapeKind::Cube => "Depth",
        ShapeKind::LeftBracket
        | ShapeKind::RightBracket
        | ShapeKind::LeftBrace
        | ShapeKind::RightBrace => "Curve size",
        _ => "Adjust",
    }
}

/// Where each adjustment handle sits, in frame units.
///
/// A callout tail handle can lie outside the frame. Kinds without adjustments,
/// invalid sizes and hostile values return an empty list.
#[must_use]
pub fn adjust_handles(kind: ShapeKind, w: f32, h: f32, adjust: &[f32]) -> Vec<Pt> {
    if !finite(w) || !finite(h) || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let a = effective_adjust(kind, adjust);
    let d = detail(w, h);
    let mid = (w * 0.5, h * 0.5);
    match kind {
        ShapeKind::RoundedRectangle => vec![(d * value(&a, 0, 0.1), 0.0)],
        ShapeKind::SnipSingleCorner | ShapeKind::RoundSingleCorner => {
            vec![(w - d * value(&a, 0, 0.1), 0.0)]
        }
        ShapeKind::SnipSameSide | ShapeKind::RoundSameSide => {
            vec![(w - d * value(&a, 0, 0.1), 0.0)]
        }
        ShapeKind::SnipDiagonal | ShapeKind::RoundDiagonal => {
            vec![(d * value(&a, 0, 0.1), 0.0)]
        }
        ShapeKind::SnipRound => vec![
            (d * value(&a, 0, 0.1), 0.0),
            (w - d * value(&a, 1, 0.1), 0.0),
        ],
        ShapeKind::CalloutRectangle | ShapeKind::CalloutRounded | ShapeKind::CalloutOval => {
            vec![(value(&a, 0, 0.14) * w, value(&a, 1, 1.0) * h)]
        }
        ShapeKind::CalloutLine => vec![(value(&a, 0, 0.18) * w, value(&a, 1, 0.97) * h)],
        ShapeKind::CalloutLineBent => vec![
            (value(&a, 0, 0.17) * w, value(&a, 1, 0.7) * h),
            (value(&a, 2, 0.17) * w, value(&a, 3, 0.97) * h),
        ],
        ShapeKind::CalloutLineDoubleBent => vec![
            (value(&a, 0, 0.13) * w, value(&a, 1, 0.8) * h),
            (value(&a, 2, 0.24) * w, value(&a, 3, 0.8) * h),
            (value(&a, 4, 0.24) * w, value(&a, 5, 0.97) * h),
        ],
        ShapeKind::ArrowRight => vec![
            (w - d * value(&a, 0, 0.5), h * 0.5),
            (w * 0.5, h * 0.5 - d * value(&a, 1, 0.25)),
        ],
        ShapeKind::ArrowLeft => vec![
            (d * value(&a, 0, 0.5), h * 0.5),
            (w * 0.5, h * 0.5 - d * value(&a, 1, 0.25)),
        ],
        ShapeKind::ArrowUp => vec![
            (w * 0.5, d * value(&a, 0, 0.5)),
            (w * 0.5 + d * value(&a, 1, 0.25), h * 0.5),
        ],
        ShapeKind::ArrowDown => vec![
            (w * 0.5, h - d * value(&a, 0, 0.5)),
            (w * 0.5 + d * value(&a, 1, 0.25), h * 0.5),
        ],
        ShapeKind::ArrowLeftRight => vec![
            (w - d * value(&a, 0, 0.5), h * 0.5),
            (w * 0.5, h * 0.5 - d * value(&a, 1, 0.25)),
        ],
        ShapeKind::ArrowUpDown => vec![
            (w * 0.5, d * value(&a, 0, 0.5)),
            (w * 0.5 + d * value(&a, 1, 0.25), h * 0.5),
        ],
        ShapeKind::ArrowPentagon | ShapeKind::ArrowChevron => {
            vec![(w - d * value(&a, 0, 0.5), h * 0.5)]
        }
        ShapeKind::Parallelogram | ShapeKind::Trapezoid => {
            vec![(d * value(&a, 0, 0.25), 0.0)]
        }
        ShapeKind::Hexagon | ShapeKind::Octagon | ShapeKind::Plus => {
            vec![(w * 0.5 + d * value(&a, 0, 0.0), 0.0)]
        }
        ShapeKind::Star4 => vec![star_handle(4, value(&a, 0, 0.3), w, h)],
        ShapeKind::Star5 => vec![star_handle(5, value(&a, 0, 0.38), w, h)],
        ShapeKind::Star6 => vec![star_handle(6, value(&a, 0, 0.42), w, h)],
        ShapeKind::Star7 => vec![star_handle(7, value(&a, 0, 0.45), w, h)],
        ShapeKind::Star8 => vec![star_handle(8, value(&a, 0, 0.42), w, h)],
        ShapeKind::Star10 => vec![star_handle(10, value(&a, 0, 0.48), w, h)],
        ShapeKind::Star12 => vec![star_handle(12, value(&a, 0, 0.5), w, h)],
        ShapeKind::Star16 => vec![star_handle(16, value(&a, 0, 0.52), w, h)],
        ShapeKind::Donut => {
            let thickness = value(&a, 0, 0.45);
            vec![(w * 0.5 + (1.0 - thickness) * w * 0.5, h * 0.5)]
        }
        ShapeKind::BlockArc => {
            let thickness = value(&a, 0, 0.2);
            let start = value(&a, 1, std::f32::consts::PI);
            let end = value(&a, 2, TAU);
            let (cx, cy) = (w * 0.5, h * 0.55);
            let rx = w * 0.5;
            let ry = cy.max(h * 0.01);
            let mid_angle = (start + end) * 0.5;
            let t = thickness * d;
            let mid_scale = (1.0 - t / (2.0 * ry.max(1e-6))).clamp(0.0, 1.0);
            vec![
                (
                    cx + mid_angle.cos() * mid_scale * rx,
                    cy + mid_angle.sin() * mid_scale * ry,
                ),
                (cx + start.cos() * rx, cy + start.sin() * ry),
                (cx + end.cos() * rx, cy + end.sin() * ry),
            ]
        }
        ShapeKind::Pie | ShapeKind::Chord | ShapeKind::Arc => {
            let start = value(&a, 0, 0.0);
            let end = value(&a, 1, TAU * 0.75);
            vec![
                (mid.0 + start.cos() * w * 0.5, mid.1 + start.sin() * h * 0.5),
                (mid.0 + end.cos() * w * 0.5, mid.1 + end.sin() * h * 0.5),
            ]
        }
        ShapeKind::Frame => vec![(d * value(&a, 0, 0.125), h * 0.5)],
        ShapeKind::Bevel => vec![(d * value(&a, 0, 0.16), h * 0.5)],
        ShapeKind::FoldedCorner => vec![(w - d * value(&a, 0, 0.28), h)],
        ShapeKind::Can => vec![(w * 0.5, (h * value(&a, 0, 0.12)).min(w * 0.4).max(0.0))],
        ShapeKind::Cube => vec![(w, h * value(&a, 0, 0.25))],
        ShapeKind::LeftBracket => vec![(w * value(&a, 0, 0.3), h * 0.5)],
        ShapeKind::LeftBrace => vec![(w * value(&a, 0, 0.85), h * 0.5)],
        ShapeKind::RightBracket => vec![(w - w * value(&a, 0, 0.3), h * 0.5)],
        ShapeKind::RightBrace => vec![(w - w * value(&a, 0, 0.85), h * 0.5)],
        _ => Vec::new(),
    }
}

/// New values for the handle at `index` dragged to `point` in frame units.
///
/// The result is clamped exactly like a stored value. Out-of-range indices and
/// hostile points leave the values unchanged.
#[must_use]
#[allow(clippy::cognitive_complexity)]
pub fn adjust_from_point(
    kind: ShapeKind,
    w: f32,
    h: f32,
    adjust: &[f32],
    index: usize,
    point: Pt,
) -> Vec<f32> {
    let mut values = effective_adjust(kind, adjust);
    if values.is_empty()
        || !finite(w)
        || !finite(h)
        || w <= 0.0
        || h <= 0.0
        || !finite(point.0)
        || !finite(point.1)
    {
        return values;
    }
    let d = detail(w, h);
    let (x, y) = point;
    match kind {
        ShapeKind::RoundedRectangle | ShapeKind::SnipDiagonal | ShapeKind::RoundDiagonal => {
            if index == 0 && d > 1e-6 {
                values[0] = x / d;
            }
        }
        ShapeKind::SnipSingleCorner
        | ShapeKind::SnipSameSide
        | ShapeKind::RoundSingleCorner
        | ShapeKind::RoundSameSide => {
            if index == 0 && d > 1e-6 {
                values[0] = (w - x) / d;
            }
        }
        ShapeKind::SnipRound => {
            if d > 1e-6 {
                match index {
                    0 => values[0] = x / d,
                    1 => values[1] = (w - x) / d,
                    _ => {}
                }
            }
        }
        ShapeKind::CalloutRectangle | ShapeKind::CalloutRounded | ShapeKind::CalloutOval => {
            if index == 0 {
                values[0] = x / w;
                values[1] = y / h;
            }
        }
        ShapeKind::CalloutLine => {
            if index == 0 {
                values[0] = x / w;
                values[1] = y / h;
            }
        }
        ShapeKind::CalloutLineBent | ShapeKind::CalloutLineDoubleBent => {
            if let Some(pair) = index.checked_mul(2) {
                if let Some(px) = values.get_mut(pair) {
                    *px = x / w;
                }
                if let Some(py) = values.get_mut(pair + 1) {
                    *py = y / h;
                }
            }
        }
        ShapeKind::ArrowRight => match index {
            0 if d > 1e-6 => values[0] = (w - x) / d,
            1 if d > 1e-6 => values[1] = (h * 0.5 - y) / d,
            _ => {}
        },
        ShapeKind::ArrowLeft => match index {
            0 if d > 1e-6 => values[0] = x / d,
            1 if d > 1e-6 => values[1] = (h * 0.5 - y) / d,
            _ => {}
        },
        ShapeKind::ArrowUp => match index {
            0 if d > 1e-6 => values[0] = y / d,
            1 if d > 1e-6 => values[1] = (x - w * 0.5) / d,
            _ => {}
        },
        ShapeKind::ArrowDown => match index {
            0 if d > 1e-6 => values[0] = (h - y) / d,
            1 if d > 1e-6 => values[1] = (x - w * 0.5) / d,
            _ => {}
        },
        ShapeKind::ArrowLeftRight => match index {
            0 if d > 1e-6 => values[0] = (w - x) / d,
            1 if d > 1e-6 => values[1] = (h * 0.5 - y) / d,
            _ => {}
        },
        ShapeKind::ArrowUpDown => match index {
            0 if d > 1e-6 => values[0] = y / d,
            1 if d > 1e-6 => values[1] = (x - w * 0.5) / d,
            _ => {}
        },
        ShapeKind::ArrowPentagon | ShapeKind::ArrowChevron => {
            if index == 0 && d > 1e-6 {
                values[0] = (w - x) / d;
            }
        }
        ShapeKind::Parallelogram | ShapeKind::Trapezoid => {
            if index == 0 && d > 1e-6 {
                values[0] = x / d;
            }
        }
        ShapeKind::Hexagon | ShapeKind::Octagon | ShapeKind::Plus => {
            if index == 0 && d > 1e-6 {
                values[0] = (x - w * 0.5) / d;
            }
        }
        ShapeKind::Star4
        | ShapeKind::Star5
        | ShapeKind::Star6
        | ShapeKind::Star7
        | ShapeKind::Star8
        | ShapeKind::Star10
        | ShapeKind::Star12
        | ShapeKind::Star16 => {
            if index == 0 {
                let nx = (x - w * 0.5) / (w * 0.5);
                let ny = (y - h * 0.5) / (h * 0.5);
                let radius = (nx * nx + ny * ny).sqrt();
                if finite(radius) {
                    values[0] = radius;
                }
            }
        }
        ShapeKind::Donut => {
            if index == 0 && w > 1e-6 {
                values[0] = 1.0 - 2.0 * (x - w * 0.5) / w;
            }
        }
        ShapeKind::BlockArc => {
            let (cx, cy) = (w * 0.5, h * 0.55);
            let rx = (w * 0.5).max(1e-6);
            let ry = cy.max(h * 0.01);
            match index {
                0 => {
                    let nx = (x - cx) / rx;
                    let ny = (y - cy) / ry;
                    let radius = (nx * nx + ny * ny).sqrt();
                    if finite(radius) && d > 1e-6 {
                        values[0] = 2.0 * ry * (1.0 - radius) / d;
                    }
                }
                1 => {
                    values[1] = nearest_angle(values[1], ((y - cy) / ry).atan2((x - cx) / rx));
                }
                2 => {
                    values[2] = nearest_angle(values[2], ((y - cy) / ry).atan2((x - cx) / rx));
                }
                _ => {}
            }
        }
        ShapeKind::Pie | ShapeKind::Chord | ShapeKind::Arc => {
            let (cx, cy) = (w * 0.5, h * 0.5);
            let rx = (w * 0.5).max(1e-6);
            let ry = (h * 0.5).max(1e-6);
            let raw = ((y - cy) / ry).atan2((x - cx) / rx);
            match index {
                0 => values[0] = nearest_angle(values[0], raw),
                1 => values[1] = nearest_angle(values[1], raw),
                _ => {}
            }
        }
        ShapeKind::Frame => {
            if index == 0 && d > 1e-6 {
                values[0] = x / d;
            }
        }
        ShapeKind::Bevel => {
            if index == 0 && d > 1e-6 {
                values[0] = x / d;
            }
        }
        ShapeKind::FoldedCorner => {
            if index == 0 && d > 1e-6 {
                values[0] = (w - x) / d;
            }
        }
        ShapeKind::Can => {
            if index == 0 && h > 1e-6 {
                values[0] = y / h;
            }
        }
        ShapeKind::Cube => {
            if index == 0 && h > 1e-6 {
                values[0] = y / h;
            }
        }
        ShapeKind::LeftBracket | ShapeKind::LeftBrace => {
            if index == 0 && w > 1e-6 {
                values[0] = x / w;
            }
        }
        ShapeKind::RightBracket | ShapeKind::RightBrace if index == 0 && w > 1e-6 => {
            values[0] = (w - x) / w;
        }
        ShapeKind::RightBracket | ShapeKind::RightBrace => {}
        _ => {}
    }
    clamp_adjust(kind, &values)
}

/// Closed contours for `kind` in a `w` by `h` frame, in frame units.
///
/// The contours are filled with the even-odd rule. Non-finite or non-positive
/// sizes return an empty list. Every returned point lies inside the frame except
/// a callout tail, which may reach outside it by design.
#[must_use]
pub fn shape_contours(kind: ShapeKind, w: f32, h: f32) -> Vec<Vec<Pt>> {
    shape_contours_adjusted(kind, w, h, &[])
}

/// Closed contours with per-kind adjustment values. Empty values use the
/// kind's defaults, so they draw exactly like [`shape_contours`].
#[must_use]
pub fn shape_contours_adjusted(kind: ShapeKind, w: f32, h: f32, adjust: &[f32]) -> Vec<Vec<Pt>> {
    if !finite(w) || !finite(h) || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let d = detail(w, h);
    let a = effective_adjust(kind, adjust);
    let raw: Vec<Vec<Pt>> = match kind {
        ShapeKind::LineHorizontal => {
            vec![line_band(w, h, &[(0.0, h * 0.5), (w, h * 0.5)])]
        }
        ShapeKind::LineVertical => {
            vec![line_band(w, h, &[(w * 0.5, 0.0), (w * 0.5, h)])]
        }
        ShapeKind::LineDiagonalDown => vec![line_band(w, h, &[(0.0, 0.0), (w, h)])],
        ShapeKind::LineDiagonalUp => vec![line_band(w, h, &[(0.0, h), (w, 0.0)])],
        ShapeKind::LineArrowRight => vec![line_arrow_horizontal(w, h, false)],
        ShapeKind::LineArrowLeft => {
            let source = line_arrow_horizontal(w, h, false);
            vec![map_points(&source, |(x, y)| (w - x, y))]
        }
        ShapeKind::LineArrowUp => vec![line_arrow_vertical(w, h, false)],
        ShapeKind::LineArrowDown => {
            let source = line_arrow_horizontal(h, w, false);
            vec![map_points(&source, |(x, y)| (w - y, x))]
        }
        ShapeKind::LineArrowBothHorizontal => vec![line_arrow_horizontal(w, h, true)],
        ShapeKind::LineArrowBothVertical => vec![line_arrow_vertical(w, h, true)],
        ShapeKind::Rectangle => vec![rectangle_points(w, h)],
        ShapeKind::RoundedRectangle => {
            let radius = d * value(&a, 0, 0.1);
            vec![rounded_polygon(&rectangle_points(w, h), radius)]
        }
        ShapeKind::SnipSingleCorner => {
            let cut = d * value(&a, 0, 0.1);
            vec![snip_polygon(&rectangle_points(w, h), &[1], cut)]
        }
        ShapeKind::SnipSameSide => {
            let cut = d * value(&a, 0, 0.1);
            vec![snip_polygon(&rectangle_points(w, h), &[0, 1], cut)]
        }
        ShapeKind::SnipDiagonal => {
            let cut = d * value(&a, 0, 0.1);
            vec![snip_polygon(&rectangle_points(w, h), &[0, 2], cut)]
        }
        ShapeKind::SnipRound => {
            let snip = d * value(&a, 0, 0.1);
            let radius = d * value(&a, 1, 0.1);
            let points = snip_polygon(&rectangle_points(w, h), &[0], snip);
            // After the snip the old bottom-right and bottom-left vertices moved
            // by one: round the three remaining corners.
            let n = points.len();
            let round = [1, 2, n.saturating_sub(1)];
            vec![rounded_polygon_masked(&points, &round, radius)]
        }
        ShapeKind::RoundSingleCorner => {
            let radius = d * value(&a, 0, 0.1);
            vec![rounded_polygon_masked(
                &rectangle_points(w, h),
                &[1],
                radius,
            )]
        }
        ShapeKind::RoundSameSide => {
            let radius = d * value(&a, 0, 0.1);
            vec![rounded_polygon_masked(
                &rectangle_points(w, h),
                &[0, 1],
                radius,
            )]
        }
        ShapeKind::RoundDiagonal => {
            let radius = d * value(&a, 0, 0.1);
            vec![rounded_polygon_masked(
                &rectangle_points(w, h),
                &[0, 2],
                radius,
            )]
        }
        ShapeKind::Ellipse => vec![ellipse_in_frame(w, h)],
        ShapeKind::Triangle => vec![vec![(w * 0.5, 0.0), (w, h), (0.0, h)]],
        ShapeKind::RightTriangle => vec![vec![(0.0, 0.0), (0.0, h), (w, h)]],
        ShapeKind::Parallelogram => {
            let slant = d * value(&a, 0, 0.25);
            vec![vec![(slant, 0.0), (w, 0.0), (w - slant, h), (0.0, h)]]
        }
        ShapeKind::Trapezoid => {
            let slant = d * value(&a, 0, 0.25);
            vec![vec![(slant, 0.0), (w - slant, 0.0), (w, h), (0.0, h)]]
        }
        ShapeKind::Diamond => vec![vec![
            (w * 0.5, 0.0),
            (w, h * 0.5),
            (w * 0.5, h),
            (0.0, h * 0.5),
        ]],
        ShapeKind::Pentagon => vec![regular_in_frame(5, w, h)],
        ShapeKind::Hexagon => {
            let points = regular_in_frame(6, w, h);
            let all: Vec<usize> = (0..points.len()).collect();
            vec![snip_polygon(&points, &all, d * value(&a, 0, 0.0))]
        }
        ShapeKind::Heptagon => vec![regular_in_frame(7, w, h)],
        ShapeKind::Octagon => {
            let points = regular_in_frame(8, w, h);
            let all: Vec<usize> = (0..points.len()).collect();
            vec![snip_polygon(&points, &all, d * value(&a, 0, 0.0))]
        }
        ShapeKind::Decagon => vec![regular_in_frame(10, w, h)],
        ShapeKind::Dodecagon => vec![regular_in_frame(12, w, h)],
        ShapeKind::Pie => vec![pie(w, h, &a)],
        ShapeKind::Chord => vec![chord(w, h, &a)],
        ShapeKind::Teardrop => vec![teardrop(w, h)],
        ShapeKind::Frame => frame_contours(w, h, &a),
        ShapeKind::HalfFrame => vec![half_frame(w, h)],
        ShapeKind::Corner => vec![corner_shape(w, h)],
        ShapeKind::DiagonalStripe => {
            vec![stroke_polyline(&[(0.0, h), (w, 0.0)], detail(w, h) * 0.18)]
        }
        ShapeKind::Plus => {
            let points = plus_polygon(w * 0.5, h * 0.5, d * 0.38, d * 0.16, 0.0);
            let all: Vec<usize> = (0..points.len()).collect();
            vec![snip_polygon(&points, &all, d * value(&a, 0, 0.0))]
        }
        ShapeKind::Plaque => vec![plaque(w, h)],
        ShapeKind::Can => can(w, h, &a),
        ShapeKind::Cube => cube(w, h, &a),
        ShapeKind::Bevel => {
            // A raised button: four mitred edges around a centre panel, split by gaps.
            let t = d * value(&a, 0, 0.16);
            let g = t * (0.05 / 0.16);
            let e = t - g;
            vec![
                rect_at(t, t, w - t, h - t),
                vec![(g, 0.0), (w - g, 0.0), (w - t, e), (t, e)],
                vec![(t, h - e), (w - t, h - e), (w - g, h), (g, h)],
                vec![(0.0, g), (e, t), (e, h - t), (0.0, h - g)],
                vec![(w, g), (w, h - g), (w - e, h - t), (w - e, t)],
            ]
        }
        ShapeKind::Donut => {
            ring_contours(w * 0.5, h * 0.5, w * 0.5, h * 0.5, 1.0 - value(&a, 0, 0.45))
        }
        ShapeKind::NoSymbol => vec![
            ellipse_in_frame(w, h),
            plus_polygon(
                w * 0.5,
                h * 0.5,
                detail(w, h) * 0.42,
                (detail(w, h) * 0.045).max(0.01),
                -TAU * 0.125,
            ),
        ],
        ShapeKind::BlockArc => vec![block_arc(w, h, &a)],
        ShapeKind::FoldedCorner => folded_corner(w, h, &a),
        ShapeKind::Smiley => smiley(w, h),
        ShapeKind::Heart => vec![heart(w, h)],
        ShapeKind::LightningBolt => vec![lightning_bolt(w, h)],
        ShapeKind::Sun => sun(w, h),
        ShapeKind::Moon => vec![moon(w, h)],
        ShapeKind::Cloud => vec![cloud_body(w, h)],
        ShapeKind::Arc => vec![arc_shape(w, h, &a)],
        ShapeKind::BracketPair => {
            vec![bracket(w, h, true, 0.3), bracket(w, h, false, 0.3)]
        }
        ShapeKind::BracePair => {
            let mut contours = brace(w, h, true, w * 0.40);
            contours.extend(brace(w, h, false, w * 0.40));
            contours
        }
        ShapeKind::LeftBracket => vec![bracket(w, h, true, value(&a, 0, 0.3))],
        ShapeKind::RightBracket => vec![bracket(w, h, false, value(&a, 0, 0.3))],
        ShapeKind::LeftBrace => brace(w, h, true, w * value(&a, 0, 0.85)),
        ShapeKind::RightBrace => brace(w, h, false, w * value(&a, 0, 0.85)),
        ShapeKind::ArrowRight => vec![arrow_right_adjusted(w, h, &a)],
        ShapeKind::ArrowLeft => vec![arrow_points(ArrowDirection::Left, w, h, &a)],
        ShapeKind::ArrowUp => vec![arrow_points(ArrowDirection::Up, w, h, &a)],
        ShapeKind::ArrowDown => vec![arrow_points(ArrowDirection::Down, w, h, &a)],
        ShapeKind::ArrowLeftRight => vec![arrow_left_right_adjusted(w, h, &a)],
        ShapeKind::ArrowUpDown => vec![arrow_up_down_adjusted(w, h, &a)],
        ShapeKind::ArrowQuad => vec![arrow_quad(w, h)],
        ShapeKind::ArrowLeftRightUp => vec![arrow_left_right_up(w, h)],
        ShapeKind::ArrowBent => vec![arrow_bent(w, h)],
        ShapeKind::ArrowUTurn => vec![arrow_u_turn(w, h)],
        ShapeKind::ArrowLeftUp => vec![arrow_left_up(w, h)],
        ShapeKind::ArrowBentUp => vec![arrow_bent_up(w, h)],
        ShapeKind::ArrowCurvedRight
        | ShapeKind::ArrowCurvedLeft
        | ShapeKind::ArrowCurvedUp
        | ShapeKind::ArrowCurvedDown => vec![curved_arrow(w, h, kind)],
        ShapeKind::ArrowStripedRight => striped_right_arrow(w, h),
        ShapeKind::ArrowNotchedRight => vec![notched_right_arrow(w, h)],
        ShapeKind::ArrowPentagon => vec![pentagon_arrow(w, h, &a)],
        ShapeKind::ArrowChevron => vec![chevron(w, h, &a)],
        ShapeKind::CalloutRightArrow
        | ShapeKind::CalloutDownArrow
        | ShapeKind::CalloutLeftArrow
        | ShapeKind::CalloutUpArrow => arrow_callout(w, h, kind),
        ShapeKind::CalloutLeftRightArrow => arrow_callout_left_right(w, h),
        ShapeKind::CalloutQuadArrow => arrow_callout_quad(w, h),
        ShapeKind::ArrowCircular => vec![circular_arrow(w, h)],
        ShapeKind::MathPlus => vec![plus_polygon(
            w * 0.5,
            h * 0.5,
            detail(w, h) * 0.38,
            detail(w, h) * 0.16,
            0.0,
        )],
        ShapeKind::MathMinus => vec![rect_at(w * 0.12, h * 0.44, w * 0.88, h * 0.56)],
        ShapeKind::MathMultiply => vec![plus_polygon(
            w * 0.5,
            h * 0.5,
            detail(w, h) * 0.36,
            detail(w, h) * 0.14,
            TAU * 0.125,
        )],
        ShapeKind::MathDivide => {
            let d = detail(w, h);
            vec![
                rect_at(w * 0.14, h * 0.44, w * 0.86, h * 0.56),
                ellipse(w * 0.5, h * 0.2, d * 0.09, d * 0.09),
                ellipse(w * 0.5, h * 0.8, d * 0.09, d * 0.09),
            ]
        }
        ShapeKind::MathEqual => math_bars(w, h, 2),
        ShapeKind::MathNotEqual => {
            let mut bars = math_bars(w, h, 2);
            bars.push(line_band(
                w,
                h,
                &[(w * 0.22, h * 0.86), (w * 0.78, h * 0.14)],
            ));
            bars
        }
        ShapeKind::FlowProcess => vec![rectangle_points(w, h)],
        ShapeKind::FlowAlternateProcess => {
            vec![rounded_polygon(
                &rectangle_points(w, h),
                detail(w, h) * 0.12,
            )]
        }
        ShapeKind::FlowDecision => vec![vec![
            (w * 0.5, 0.0),
            (w, h * 0.5),
            (w * 0.5, h),
            (0.0, h * 0.5),
        ]],
        ShapeKind::FlowInputOutput => {
            let slant = detail(w, h) * 0.22;
            vec![vec![(slant, 0.0), (w, 0.0), (w - slant, h), (0.0, h)]]
        }
        ShapeKind::FlowPredefinedProcess => flow_predefined(w, h),
        ShapeKind::FlowInternalStorage => flow_internal_storage(w, h),
        ShapeKind::FlowDocument => vec![wavy_document(w, h)],
        ShapeKind::FlowMultidocument => flow_multidocument(w, h),
        ShapeKind::FlowTerminator => vec![flow_terminator(w, h)],
        ShapeKind::FlowPreparation => vec![vec![
            (w * 0.2, 0.0),
            (w * 0.8, 0.0),
            (w, h * 0.5),
            (w * 0.8, h),
            (w * 0.2, h),
            (0.0, h * 0.5),
        ]],
        ShapeKind::FlowManualInput => vec![vec![(0.0, h * 0.22), (w, 0.0), (w, h), (0.0, h)]],
        ShapeKind::FlowManualOperation => {
            vec![vec![(0.0, 0.0), (w, 0.0), (w * 0.8, h), (w * 0.2, h)]]
        }
        ShapeKind::FlowConnector => vec![ellipse_in_frame(w, h)],
        ShapeKind::FlowOffpageConnector => vec![vec![
            (0.0, 0.0),
            (w, 0.0),
            (w, h * 0.65),
            (w * 0.5, h),
            (0.0, h * 0.65),
        ]],
        ShapeKind::FlowCard => {
            let f = detail(w, h) * 0.2;
            vec![vec![(f, 0.0), (w, 0.0), (w, h), (0.0, h), (0.0, f)]]
        }
        ShapeKind::FlowPunchedTape => vec![punched_tape(w, h)],
        ShapeKind::FlowSummingJunction => vec![
            ellipse_in_frame(w, h),
            plus_polygon(
                w * 0.5,
                h * 0.5,
                detail(w, h) * 0.32,
                (detail(w, h) * 0.035).max(0.01),
                TAU * 0.125,
            ),
        ],
        ShapeKind::FlowOr => vec![
            ellipse_in_frame(w, h),
            plus_polygon(
                w * 0.5,
                h * 0.5,
                detail(w, h) * 0.32,
                (detail(w, h) * 0.035).max(0.01),
                0.0,
            ),
        ],
        ShapeKind::FlowCollate => flow_collate(w, h),
        ShapeKind::FlowSort => flow_sort(w, h),
        ShapeKind::FlowExtract => vec![vec![(w * 0.5, 0.0), (w, h), (0.0, h)]],
        ShapeKind::FlowMerge => vec![vec![(0.0, 0.0), (w, 0.0), (w * 0.5, h)]],
        ShapeKind::FlowStoredData => vec![stored_data(w, h)],
        ShapeKind::FlowDelay => vec![flow_delay(w, h)],
        ShapeKind::FlowSequentialAccess => flow_sequential_access(w, h),
        ShapeKind::FlowMagneticDisk => flow_magnetic_disk(w, h),
        ShapeKind::FlowDirectAccess => flow_direct_access(w, h),
        ShapeKind::FlowDisplay => vec![flow_display(w, h)],
        ShapeKind::Explosion1 => vec![burst(w, h, &BURST_1)],
        ShapeKind::Explosion2 => vec![burst(w, h, &BURST_2)],
        ShapeKind::Star4 => vec![star_in_frame(4, value(&a, 0, 0.3), w, h)],
        ShapeKind::Star5 => vec![star_in_frame(5, value(&a, 0, 0.38), w, h)],
        ShapeKind::Star6 => vec![star_in_frame(6, value(&a, 0, 0.42), w, h)],
        ShapeKind::Star7 => vec![star_in_frame(7, value(&a, 0, 0.45), w, h)],
        ShapeKind::Star8 => vec![star_in_frame(8, value(&a, 0, 0.42), w, h)],
        ShapeKind::Star10 => vec![star_in_frame(10, value(&a, 0, 0.48), w, h)],
        ShapeKind::Star12 => vec![star_in_frame(12, value(&a, 0, 0.5), w, h)],
        ShapeKind::Star16 => vec![star_in_frame(16, value(&a, 0, 0.52), w, h)],
        ShapeKind::Star24 => vec![star_in_frame(24, 0.55, w, h)],
        ShapeKind::Star32 => vec![star_in_frame(32, 0.58, w, h)],
        ShapeKind::RibbonUp => ribbon(w, h, true),
        ShapeKind::RibbonDown => ribbon(w, h, false),
        ShapeKind::CurvedRibbonUp => curved_ribbon(w, h, true),
        ShapeKind::CurvedRibbonDown => curved_ribbon(w, h, false),
        ShapeKind::ScrollVertical => scroll_vertical(w, h),
        ShapeKind::ScrollHorizontal => scroll_horizontal(w, h),
        ShapeKind::Wave => vec![solid_wave(w, h, 1.0)],
        ShapeKind::DoubleWave => vec![solid_wave(w, h, 2.0)],
        ShapeKind::CalloutRectangle
        | ShapeKind::CalloutRounded
        | ShapeKind::CalloutOval
        | ShapeKind::CalloutCloud
        | ShapeKind::CalloutLine
        | ShapeKind::CalloutLineBent
        | ShapeKind::CalloutLineDoubleBent => callout_body(w, h, kind, &a),
        ShapeKind::ActionBlank
        | ShapeKind::ActionHome
        | ShapeKind::ActionHelp
        | ShapeKind::ActionInformation
        | ShapeKind::ActionBack
        | ShapeKind::ActionForward
        | ShapeKind::ActionBeginning
        | ShapeKind::ActionEnd
        | ShapeKind::ActionReturn
        | ShapeKind::ActionDocument
        | ShapeKind::ActionSound
        | ShapeKind::ActionMovie => action_button(w, h, kind),
        ShapeKind::CheckMark => vec![stroke_polyline(
            &[
                (w * 0.15, h * 0.55),
                (w * 0.4, h * 0.82),
                (w * 0.86, h * 0.22),
            ],
            detail(w, h) * 0.14,
        )],
        ShapeKind::CrossMark => vec![plus_polygon(
            w * 0.5,
            h * 0.5,
            detail(w, h) * 0.38,
            detail(w, h) * 0.1,
            TAU * 0.125,
        )],
        ShapeKind::CheckCircle => vec![ellipse_in_frame(w, h), check_hole(w, h)],
        ShapeKind::CrossCircle => vec![ellipse_in_frame(w, h), cross_hole(w, h)],
        ShapeKind::InfoCircle => {
            let mut contours = vec![ellipse_in_frame(w, h)];
            contours.extend(info_holes_frame(w, h));
            contours
        }
        ShapeKind::QuestionCircle => {
            let mut contours = vec![ellipse_in_frame(w, h)];
            contours.extend(question_mark_hole(w, h));
            contours
        }
        ShapeKind::WarningTriangle => warning_triangle(w, h),
        ShapeKind::Lightbulb => lightbulb(w, h),
        ShapeKind::Target => target(w, h),
        ShapeKind::Trophy => trophy(w, h),
        ShapeKind::Medal => medal(w, h),
        ShapeKind::Flag => flag(w, h),
        ShapeKind::LocationPin => location_pin(w, h),
        ShapeKind::Clock => clock(w, h),
        ShapeKind::Lock => lock(w, h),
        ShapeKind::Key => key(w, h),
        ShapeKind::Shield => vec![shield(w, h)],
        ShapeKind::Book => book(w, h),
        ShapeKind::GraduationCap => graduation_cap(w, h),
        ShapeKind::Person => person(w, h),
        ShapeKind::People => people(w, h),
        ShapeKind::ThumbsUp => thumbs_up(w, h),
        ShapeKind::SpeechBubble => speech_bubble(w, h),
        ShapeKind::ThoughtBubble => thought_bubble(w, h),
        ShapeKind::PlayCircle => play_circle(w, h),
        ShapeKind::PauseCircle => pause_circle(w, h),
        ShapeKind::Gear => gear(w, h),
        ShapeKind::Magnifier => magnifier(w, h),
        ShapeKind::StarBadge => star_badge(w, h),
    };
    let tail_may_escape = matches!(
        kind,
        ShapeKind::CalloutRectangle
            | ShapeKind::CalloutRounded
            | ShapeKind::CalloutOval
            | ShapeKind::CalloutLine
            | ShapeKind::CalloutLineBent
            | ShapeKind::CalloutLineDoubleBent
    );
    raw.into_iter()
        .map(|contour| clean_contour(contour, w, h, tail_may_escape))
        .filter(|contour| contour.len() >= 3)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_sizes_return_nothing() {
        for &kind in ShapeKind::ALL {
            assert!(shape_contours(kind, 0.0, 10.0).is_empty());
            assert!(shape_contours(kind, 10.0, 0.0).is_empty());
            assert!(shape_contours(kind, f32::NAN, 10.0).is_empty());
            assert!(shape_contours(kind, 10.0, f32::INFINITY).is_empty());
        }
    }

    #[test]
    fn helpers_are_sane() {
        assert_eq!(arc(10.0, 10.0, 5.0, 5.0, 0.0, TAU).len(), CIRCLE_SEGMENTS);
        assert_eq!(regular_polygon(5).len(), 5);
        assert_eq!(star(5, 0.5).len(), 10);
        let band = stroke_polyline(&[(0.0, 0.0), (10.0, 0.0)], 2.0);
        assert_eq!(band.len(), 4);
        assert!(stroke_polyline(&[(0.0, 0.0)], 2.0).is_empty());
        assert!(star(2, 0.5).is_empty());
    }
}
