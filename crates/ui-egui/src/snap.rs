//! Smart-guide snapping for canvas moves and resizes.
//!
//! The maths is pure: callers pass slide-space rectangles and the snap
//! threshold already converted through the slide transform. The returned
//! offset is added to the moving rectangle, and the guides are drawn in screen
//! space by the canvas.

use egui::{Rect, Vec2};

/// A guide line to draw while the moving rectangle is snapped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    /// True for a vertical line (`at` is an x coordinate), false for horizontal.
    pub vertical: bool,
    /// The slide-space coordinate of the line.
    pub at: f32,
    /// Start of the relevant span along the other axis, in slide space.
    pub start: f32,
    /// End of the relevant span along the other axis, in slide space.
    pub end: f32,
}

/// The offset to apply and the guides to draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapResult {
    pub offset: Vec2,
    pub guides: Vec<SnapGuide>,
}

fn finite_rect(rect: Rect) -> bool {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
        .iter()
        .all(|value| value.is_finite())
}

fn better(
    current: Option<(f32, f32, f32, f32, f32)>,
    candidate: (f32, f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32, f32)> {
    match current {
        Some(existing) if existing.0 <= candidate.0 => Some(existing),
        _ => Some(candidate),
    }
}

/// Snap `moving` to the nearest of `targets` plus `slide`, independently on
/// each axis. Each rectangle contributes its left/centre/right and
/// top/middle/bottom lines. A candidate closer than `threshold` (slide units)
/// wins; ties keep the earlier target. Non-finite input is ignored.
#[must_use]
pub fn snap_rect(moving: Rect, targets: &[Rect], slide: Rect, threshold: f32) -> SnapResult {
    let mut result = SnapResult::default();
    if !finite_rect(moving) || !threshold.is_finite() || threshold <= 0.0 {
        return result;
    }

    let moving_x = [moving.left(), moving.center().x, moving.right()];
    let moving_y = [moving.top(), moving.center().y, moving.bottom()];

    let mut best_x: Option<(f32, f32, f32, f32, f32)> = None;
    let mut best_y: Option<(f32, f32, f32, f32, f32)> = None;

    for target in targets.iter().chain(std::iter::once(&slide)) {
        if !finite_rect(*target) {
            continue;
        }
        let target_x = [target.left(), target.center().x, target.right()];
        let target_y = [target.top(), target.center().y, target.bottom()];

        let span_top = moving.top().min(target.top());
        let span_bottom = moving.bottom().max(target.bottom());
        let span_left = moving.left().min(target.left());
        let span_right = moving.right().max(target.right());

        for line in target_x {
            for edge in moving_x {
                let delta = line - edge;
                let distance = delta.abs();
                if distance <= threshold {
                    best_x = better(best_x, (distance, delta, line, span_top, span_bottom));
                }
            }
        }
        for line in target_y {
            for edge in moving_y {
                let delta = line - edge;
                let distance = delta.abs();
                if distance <= threshold {
                    best_y = better(best_y, (distance, delta, line, span_left, span_right));
                }
            }
        }
    }

    if let Some((_, delta, line, start, end)) = best_x {
        result.offset.x = delta;
        result.guides.push(SnapGuide {
            vertical: true,
            at: line,
            start,
            end,
        });
    }
    if let Some((_, delta, line, start, end)) = best_y {
        result.offset.y = delta;
        result.guides.push(SnapGuide {
            vertical: false,
            at: line,
            start,
            end,
        });
    }
    result
}

/// Extra snap lines: user guides and a grid, in slide pixels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineSnaps {
    /// x coordinates of vertical lines.
    pub vertical: Vec<f32>,
    /// y coordinates of horizontal lines.
    pub horizontal: Vec<f32>,
    /// Grid spacing, when snapping to the grid.
    pub grid: Option<f32>,
}

impl LineSnaps {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertical.is_empty() && self.horizontal.is_empty() && self.grid.is_none()
    }
}

/// Snapping is off while Cmd/Ctrl is held.
#[must_use]
pub fn snapping_active(enabled: bool, command_held: bool) -> bool {
    enabled && !command_held
}

/// The closest line for one axis: `(distance, delta, line)`.
fn nearest_line(
    edges: [f32; 3],
    lines: &[f32],
    grid: Option<f32>,
    threshold: f32,
) -> Option<(f32, f32, f32)> {
    let mut best: Option<(f32, f32, f32)> = None;
    let mut consider = |line: f32, edge: f32| {
        let delta = line - edge;
        let distance = delta.abs();
        if line.is_finite()
            && distance <= threshold
            && best.is_none_or(|(current, _, _)| distance < current)
        {
            best = Some((distance, delta, line));
        }
    };
    for line in lines {
        for edge in edges {
            consider(*line, edge);
        }
    }
    if let Some(step) = grid
        && step.is_finite()
        && step > 0.0
    {
        // The leading and trailing edges follow the grid; the centre does not.
        for edge in [edges[0], edges[2]] {
            consider((edge / step).round() * step, edge);
        }
    }
    best
}

/// Like [`snap_rect`] but also snapping to guides and the grid. `smart` turns
/// the object-to-object guides on. A line closer than the smart candidate wins.
#[must_use]
pub fn snap_rect_lines(
    moving: Rect,
    targets: &[Rect],
    slide: Rect,
    threshold: f32,
    smart: bool,
    lines: &LineSnaps,
) -> SnapResult {
    let mut result = if smart {
        snap_rect(moving, targets, slide, threshold)
    } else {
        SnapResult::default()
    };
    if !finite_rect(moving) || !threshold.is_finite() || threshold <= 0.0 || lines.is_empty() {
        return result;
    }
    let xs = [moving.left(), moving.center().x, moving.right()];
    let ys = [moving.top(), moving.center().y, moving.bottom()];
    let smart_x = result.guides.iter().any(|g| g.vertical);
    let smart_y = result.guides.iter().any(|g| !g.vertical);
    if let Some((distance, delta, line)) = nearest_line(xs, &lines.vertical, lines.grid, threshold)
        && (!smart_x || distance < result.offset.x.abs())
    {
        result.guides.retain(|g| !g.vertical);
        result.offset.x = delta;
        result.guides.push(SnapGuide {
            vertical: true,
            at: line,
            start: moving.top(),
            end: moving.bottom(),
        });
    }
    if let Some((distance, delta, line)) =
        nearest_line(ys, &lines.horizontal, lines.grid, threshold)
        && (!smart_y || distance < result.offset.y.abs())
    {
        result.guides.retain(|g| g.vertical);
        result.offset.y = delta;
        result.guides.push(SnapGuide {
            vertical: false,
            at: line,
            start: moving.left(),
            end: moving.right(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    const SLIDE: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(1000.0, 1000.0),
    };

    #[test]
    fn snaps_each_horizontal_edge() {
        let target = rect(100.0, 100.0, 100.0, 100.0);
        let moving = rect(103.0, 300.0, 50.0, 50.0);
        // Left edge of moving is 3 px from target's left.
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.x, -3.0);
        assert_eq!(result.guides.len(), 1);
        assert!(result.guides[0].vertical);

        // Centre to centre.
        let moving = rect(123.0, 300.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.x, 2.0);

        // Right to right.
        let moving = rect(153.0, 300.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.x, -3.0);
    }

    #[test]
    fn snaps_each_vertical_edge() {
        let target = rect(100.0, 100.0, 100.0, 100.0);
        let moving = rect(300.0, 103.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.y, -3.0);
        assert!(!result.guides[0].vertical);

        let moving = rect(300.0, 123.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.y, 2.0);

        let moving = rect(300.0, 153.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.y, -3.0);
    }

    #[test]
    fn threshold_boundary_is_inclusive_and_just_outside_is_ignored() {
        let target = rect(100.0, 100.0, 100.0, 100.0);
        let at_threshold = rect(106.0, 300.0, 10.0, 10.0);
        assert_eq!(
            snap_rect(at_threshold, &[target], SLIDE, 6.0).offset.x,
            -6.0
        );

        let outside = rect(106.1, 300.0, 10.0, 10.0);
        let result = snap_rect(outside, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.x, 0.0);
        assert!(result.guides.is_empty());
    }

    #[test]
    fn nearest_target_wins_per_axis() {
        let near = rect(102.0, 100.0, 100.0, 10.0);
        let far = rect(105.0, 200.0, 100.0, 10.0);
        let moving = rect(100.0, 300.0, 50.0, 10.0);
        let result = snap_rect(moving, &[far, near], SLIDE, 6.0);
        assert_eq!(result.offset.x, 2.0);
    }

    #[test]
    fn snaps_both_axes_independently() {
        let target = rect(100.0, 100.0, 100.0, 100.0);
        let moving = rect(103.0, 197.0, 50.0, 50.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result.offset.x, -3.0);
        assert_eq!(result.offset.y, 3.0);
        assert_eq!(result.guides.len(), 2);
    }

    #[test]
    fn returns_no_offset_when_nothing_is_close() {
        let target = rect(100.0, 100.0, 10.0, 10.0);
        let moving = rect(300.0, 300.0, 10.0, 10.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result, SnapResult::default());
    }

    #[test]
    fn ignores_non_finite_input() {
        let target = rect(f32::NAN, 100.0, 10.0, 10.0);
        let moving = rect(300.0, 300.0, 10.0, 10.0);
        let result = snap_rect(moving, &[target], SLIDE, 6.0);
        assert_eq!(result, SnapResult::default());

        let moving = rect(f32::INFINITY, 300.0, 10.0, 10.0);
        let result = snap_rect(moving, &[], SLIDE, 6.0);
        assert_eq!(result, SnapResult::default());

        let result = snap_rect(rect(300.0, 300.0, 10.0, 10.0), &[], SLIDE, f32::NAN);
        assert_eq!(result, SnapResult::default());
    }

    #[test]
    fn snaps_to_the_slide_edges() {
        let moving = rect(4.0, 4.0, 50.0, 50.0);
        let result = snap_rect(moving, &[], SLIDE, 6.0);
        assert_eq!(result.offset.x, -4.0);
        assert_eq!(result.offset.y, -4.0);
        assert_eq!(result.guides.len(), 2);
    }

    #[test]
    fn snaps_to_a_guide_and_the_grid_and_prefers_the_nearer_line() {
        let lines = LineSnaps {
            vertical: vec![400.0],
            horizontal: vec![],
            grid: None,
        };
        let moving = rect(297.0, 300.0, 100.0, 50.0);
        let result = snap_rect_lines(moving, &[], SLIDE, 6.0, false, &lines);
        assert_eq!(result.offset.x, 3.0, "right edge to the guide at 400");
        assert_eq!(result.offset.y, 0.0);

        let grid = LineSnaps {
            vertical: vec![],
            horizontal: vec![],
            grid: Some(20.0),
        };
        let moving = rect(123.0, 258.0, 50.0, 50.0);
        let result = snap_rect_lines(moving, &[], SLIDE, 6.0, false, &grid);
        assert_eq!(result.offset.x, -3.0);
        assert_eq!(result.offset.y, 2.0);

        // A guide 1px away beats a smart guide 3px away.
        let target = rect(103.0, 600.0, 10.0, 10.0);
        let lines = LineSnaps {
            vertical: vec![101.0],
            horizontal: vec![],
            grid: None,
        };
        let moving = rect(100.0, 300.0, 50.0, 50.0);
        let result = snap_rect_lines(moving, &[target], SLIDE, 6.0, true, &lines);
        assert_eq!(result.offset.x, 1.0);
        assert_eq!(result.guides.iter().filter(|g| g.vertical).count(), 1);
    }

    #[test]
    fn line_snapping_is_off_with_the_modifier_and_ignores_bad_input() {
        assert!(snapping_active(true, false));
        assert!(!snapping_active(true, true));
        assert!(!snapping_active(false, false));
        let lines = LineSnaps {
            vertical: vec![f32::NAN, 100.0],
            horizontal: vec![],
            grid: Some(f32::NAN),
        };
        let result = snap_rect_lines(rect(98.0, 0.0, 10.0, 10.0), &[], SLIDE, 6.0, false, &lines);
        assert_eq!(result.offset.x, 2.0);
        let bad = snap_rect_lines(
            rect(f32::NAN, 0.0, 1.0, 1.0),
            &[],
            SLIDE,
            6.0,
            false,
            &lines,
        );
        assert_eq!(bad, SnapResult::default());
    }
}
