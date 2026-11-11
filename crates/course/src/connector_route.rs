//! Routing a connector between two points or glue sites. Lives with the model so
//! the engine can re-route without any drawing code; the renderer re-exports it.

use crate::ConnectorStyle;

/// A point in slide coordinates.
type Pt = (f32, f32);

const EPS: f32 = 1e-4;
const MAX_FLAT_POINTS: usize = 8192;

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

/// How far a routed connector leaves a glued site before it may turn.
pub const CONNECTOR_MIN_LEAD: f32 = 20.0;
const CONNECTOR_CURVE_MAX_LEAD: f32 = 240.0;

/// One of the four connection sites on a frame. Site 0 is top centre, 1 right
/// centre, 2 bottom centre and 3 left centre; other values wrap with `% 4`.
#[must_use]
pub fn connector_site(frame: (f32, f32, f32, f32), site: u8) -> Pt {
    let (x, y, w, h) = frame;
    match site % 4 {
        0 => (x + w * 0.5, y),
        1 => (x + w, y + h * 0.5),
        2 => (x + w * 0.5, y + h),
        _ => (x, y + h * 0.5),
    }
}

/// The outward unit normal of a connection site.
#[must_use]
pub fn connector_site_direction(site: u8) -> Pt {
    match site % 4 {
        0 => (0.0, -1.0),
        1 => (1.0, 0.0),
        2 => (0.0, 1.0),
        _ => (-1.0, 0.0),
    }
}

/// Pick the site whose outward normal points most directly at the other end.
fn auto_site(from: Pt, to: Pt) -> u8 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    if !dx.is_finite() || !dy.is_finite() {
        return 1;
    }
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 { 1 } else { 3 }
    } else if dy >= 0.0 {
        2
    } else {
        0
    }
}

fn same_axis_direction(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() <= EPS && (a.1 - b.1).abs() <= EPS
}

fn route_length(points: &[Pt]) -> f32 {
    points
        .windows(2)
        .map(|pair| match (pair.first(), pair.get(1)) {
            (Some(&a), Some(&b)) => distance(a, b),
            _ => 0.0,
        })
        .sum()
}

fn push_route_candidate(out: &mut Vec<Vec<Pt>>, start: Pt, a: Pt, middle: &[Pt], b: Pt, end: Pt) {
    let mut points = Vec::with_capacity(middle.len().saturating_add(4));
    points.push(start);
    points.push(a);
    points.extend_from_slice(middle);
    points.push(b);
    points.push(end);
    let points = clean(&points, false);
    if points.len() >= 2 {
        out.push(points);
    }
}

/// Direction changes along a route; collinear points don't count.
fn route_bends(points: &[Pt]) -> usize {
    let mut bends = 0;
    let mut previous: Option<Pt> = None;
    for pair in points.windows(2) {
        if let [a, b] = pair
            && let Some(current) = direction(*a, *b)
        {
            if previous.is_some_and(|before| !same_axis_direction(before, current)) {
                bends += 1;
            }
            previous = Some(current);
        }
    }
    bends
}

fn elbow_route(start: Pt, a: Pt, b: Pt, end: Pt, d0: Pt, d1: Pt) -> Vec<Pt> {
    let mut candidates: Vec<Vec<Pt>> = Vec::new();
    if (a.0 - b.0).abs() <= EPS || (a.1 - b.1).abs() <= EPS {
        push_route_candidate(&mut candidates, start, a, &[], b, end);
    }
    // Midpoint routes go first: on a tie the bend sits halfway, not against a target.
    let mid_x = (a.0 + b.0) * 0.5;
    push_route_candidate(
        &mut candidates,
        start,
        a,
        &[(mid_x, a.1), (mid_x, b.1)],
        b,
        end,
    );
    let mid_y = (a.1 + b.1) * 0.5;
    push_route_candidate(
        &mut candidates,
        start,
        a,
        &[(a.0, mid_y), (b.0, mid_y)],
        b,
        end,
    );
    push_route_candidate(&mut candidates, start, a, &[(b.0, a.1)], b, end);
    push_route_candidate(&mut candidates, start, a, &[(a.0, b.1)], b, end);
    if d0.0 * d1.0 > 0.0 {
        let x = if d0.0 > 0.0 {
            a.0.max(b.0)
        } else {
            a.0.min(b.0)
        };
        push_route_candidate(&mut candidates, start, a, &[(x, a.1), (x, b.1)], b, end);
    }
    if d0.1 * d1.1 > 0.0 {
        let y = if d0.1 > 0.0 {
            a.1.max(b.1)
        } else {
            a.1.min(b.1)
        };
        push_route_candidate(&mut candidates, start, a, &[(a.0, y), (b.0, y)], b, end);
    }

    let last_direction = mul(d1, -1.0);
    let mut best: Option<(i32, usize, f32, Vec<Pt>)> = None;
    for candidate in candidates {
        if candidate.iter().any(|point| !finite(*point)) {
            continue;
        }
        let mut score = 0;
        if let (Some(&first), Some(&second)) = (candidate.first(), candidate.get(1))
            && let Some(direction) = direction(first, second)
            && same_axis_direction(direction, d0)
        {
            score += 2;
        }
        let last = candidate.len().saturating_sub(1);
        if last >= 1
            && let (Some(&previous), Some(&last_point)) =
                (candidate.get(last.saturating_sub(1)), candidate.get(last))
            && let Some(direction) = direction(previous, last_point)
            && same_axis_direction(direction, last_direction)
        {
            score += 2;
        }
        let length = route_length(&candidate);
        let bends = route_bends(&candidate);
        let better = best
            .as_ref()
            .is_none_or(|(best_score, best_points, best_len, _)| {
                score > *best_score
                    || (score == *best_score
                        && (bends < *best_points
                            || (bends == *best_points && length < *best_len - 0.01)))
            });
        if better {
            best = Some((score, bends, length, candidate));
        }
    }
    best.map(|(_, _, _, points)| points)
        .unwrap_or_else(|| vec![start, end])
}

/// Route a connector between two slide-space ends.
///
/// Glued ends carry their site so the route leaves along that side; a free end
/// picks the side facing the other end. `Straight` is a direct line, `Elbow`
/// uses axis-aligned segments with at least [`CONNECTOR_MIN_LEAD`] of lead at
/// each glued end, and `Curved` is a cubic whose controls follow the site
/// directions. The result is a polyline in slide coordinates, ready to store as
/// the path's points.
#[must_use]
pub fn route_connector(
    start: Pt,
    start_site: Option<u8>,
    end: Pt,
    end_site: Option<u8>,
    style: ConnectorStyle,
) -> Vec<Pt> {
    if !finite(start) || !finite(end) {
        return Vec::new();
    }
    let start_side = start_site.unwrap_or_else(|| auto_site(start, end));
    let end_side = end_site.unwrap_or_else(|| auto_site(end, start));
    let d0 = connector_site_direction(start_side);
    let d1 = connector_site_direction(end_side);
    match style {
        ConnectorStyle::Straight => vec![start, end],
        ConnectorStyle::Elbow => {
            let a = add(start, mul(d0, CONNECTOR_MIN_LEAD));
            let b = add(end, mul(d1, CONNECTOR_MIN_LEAD));
            elbow_route(start, a, b, end, d0, d1)
        }
        ConnectorStyle::Curved => {
            let dist = distance(start, end);
            let lead = (dist * 0.5).clamp(CONNECTOR_MIN_LEAD, CONNECTOR_CURVE_MAX_LEAD);
            let c1 = add(start, mul(d0, lead));
            let c2 = add(end, mul(d1, lead));
            let steps = 24usize;
            let mut out = Vec::with_capacity(steps.saturating_add(1));
            out.push(start);
            for i in 1..=steps {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / steps as f32;
                let u = 1.0 - t;
                out.push((
                    u * u * u * start.0
                        + 3.0 * u * u * t * c1.0
                        + 3.0 * u * t * t * c2.0
                        + t * t * t * end.0,
                    u * u * u * start.1
                        + 3.0 * u * u * t * c1.1
                        + 3.0 * u * t * t * c2.1
                        + t * t * t * end.1,
                ));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_sites_bend_halfway_between_them() {
        let route = route_connector(
            (100.0, 100.0),
            Some(1),
            (500.0, 300.0),
            Some(3),
            ConnectorStyle::Elbow,
        );
        assert!(
            route.iter().any(|point| (point.0 - 300.0).abs() < 0.01),
            "{route:?}"
        );
    }
}
