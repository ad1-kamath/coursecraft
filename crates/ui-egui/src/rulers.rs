//! Rulers, grid and user guides for the authoring canvas. Editor only: nothing
//! here reaches the course, the preview or a published package.

use coursecraft_render::{SlideTransform, palette};
use egui::{Align2, FontId, Painter, Pos2, Rect, Stroke, Ui, pos2};

use crate::Editor;
use crate::snap::LineSnaps;

/// Thickness of a ruler strip in screen points.
pub const RULER_SIZE: f32 = 18.0;
/// Grid spacing in slide pixels.
pub const GRID_STEP: f32 = 20.0;
/// The grid is hidden when its zoomed spacing is below this many screen points.
pub const MIN_GRID_SCREEN: f32 = 6.0;
/// How close a press must be to a guide to grab it, in screen points.
const GRAB: f32 = 4.0;

/// One guide line across the slide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    /// True for a vertical line (`at` is an x coordinate in slide pixels).
    pub vertical: bool,
    pub at: f32,
}

/// A guide being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideDrag {
    pub index: usize,
    /// The pointer has left the guide's own ruler, so dropping it back deletes it.
    pub left_ruler: bool,
}

/// View settings and guides, kept for the session and the whole course.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RulerState {
    pub show_rulers: bool,
    pub show_grid: bool,
    pub snap_to_grid: bool,
    pub show_guides: bool,
    pub guides: Vec<Guide>,
    pub drag: Option<GuideDrag>,
}

/// The horizontal ruler along the top of the canvas.
#[must_use]
pub fn top_strip(canvas: Rect) -> Rect {
    Rect::from_min_max(canvas.min, pos2(canvas.max.x, canvas.min.y + RULER_SIZE))
}

/// The vertical ruler along the left of the canvas.
#[must_use]
pub fn left_strip(canvas: Rect) -> Rect {
    Rect::from_min_max(
        pos2(canvas.min.x, canvas.min.y + RULER_SIZE),
        pos2(canvas.min.x + RULER_SIZE, canvas.max.y),
    )
}

fn own_strip(canvas: Rect, vertical: bool) -> Rect {
    if vertical {
        left_strip(canvas)
    } else {
        top_strip(canvas)
    }
}

fn guide_screen(t: &SlideTransform, guide: Guide) -> f32 {
    if guide.vertical {
        t.to_screen_pos(pos2(guide.at, 0.0)).x
    } else {
        t.to_screen_pos(pos2(0.0, guide.at)).y
    }
}

/// Whether a press at `pos` belongs to a ruler or a guide, not the canvas.
#[must_use]
pub fn captures(state: &RulerState, canvas: Rect, t: &SlideTransform, pos: Pos2) -> bool {
    if state.drag.is_some() {
        return true;
    }
    if state.show_rulers && (top_strip(canvas).contains(pos) || left_strip(canvas).contains(pos)) {
        return true;
    }
    state.show_guides && guide_at(state, canvas, t, pos).is_some()
}

fn guide_at(state: &RulerState, canvas: Rect, t: &SlideTransform, pos: Pos2) -> Option<usize> {
    if !canvas.contains(pos) {
        return None;
    }
    state.guides.iter().position(|guide| {
        let at = guide_screen(t, *guide);
        let pointer = if guide.vertical { pos.x } else { pos.y };
        at.is_finite() && (at - pointer).abs() <= GRAB
    })
}

/// Snap lines for moves and resizes: user guides and the grid.
#[must_use]
pub fn snap_lines(state: &RulerState) -> LineSnaps {
    let mut lines = LineSnaps::default();
    if state.show_guides {
        for guide in &state.guides {
            if !guide.at.is_finite() {
                continue;
            }
            if guide.vertical {
                lines.vertical.push(guide.at);
            } else {
                lines.horizontal.push(guide.at);
            }
        }
    }
    if state.snap_to_grid {
        lines.grid = Some(GRID_STEP);
    }
    lines
}

/// Pick a tick spacing in slide pixels that is at least 50 points apart on screen.
#[must_use]
pub fn tick_step(scale: f32) -> f32 {
    if !scale.is_finite() || scale <= 0.0 {
        return 100.0;
    }
    let mut step = 10.0;
    for _ in 0..12 {
        for mult in [1.0, 2.0, 5.0] {
            if step * mult * scale >= 50.0 {
                return step * mult;
            }
        }
        step *= 10.0;
    }
    step
}

/// Draw the grid dots over the slide. Hidden when too dense to be useful.
pub fn draw_grid(editor: &Editor, painter: &Painter, t: &SlideTransform) {
    if !editor.ui.rulers.show_grid {
        return;
    }
    let scale = t.scale();
    if !scale.is_finite() || scale <= 0.0 || GRID_STEP * scale < MIN_GRID_SCREEN {
        return;
    }
    let p = palette(painter.ctx());
    let size = editor.session.course.slide_size;
    let (w, h) = (size.w as f32, size.h as f32);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let cols = (w / GRID_STEP).floor() as usize;
    let rows = (h / GRID_STEP).floor() as usize;
    if cols.saturating_mul(rows) > 40_000 {
        return;
    }
    let clip = painter.clip_rect();
    for row in 0..=rows {
        for col in 0..=cols {
            let pt = t.to_screen_pos(pos2(col as f32 * GRID_STEP, row as f32 * GRID_STEP));
            if clip.contains(pt) {
                painter.circle_filled(pt, 0.9, p.border_strong);
            }
        }
    }
}

/// Handle the pointer for rulers and guides, then paint them.
pub fn show(editor: &mut Editor, ui: &Ui, painter: &Painter, t: &SlideTransform, canvas: Rect) {
    if !editor.ui.rulers.show_rulers && editor.ui.rulers.guides.is_empty() {
        editor.ui.rulers.drag = None;
        return;
    }
    let pos = ui.input(|i| i.pointer.interact_pos());
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    let down = ui.input(|i| i.pointer.primary_down());

    if editor.ui.rulers.drag.is_none()
        && pressed
        && let Some(pos) = pos
    {
        let state = &mut editor.ui.rulers;
        let existing = if state.show_guides {
            guide_at(state, canvas, t, pos)
        } else {
            None
        };
        if let Some(index) = existing {
            state.drag = Some(GuideDrag {
                index,
                left_ruler: true,
            });
        } else if state.show_rulers {
            let vertical = left_strip(canvas).contains(pos);
            if vertical || top_strip(canvas).contains(pos) {
                let slide = t.to_slide_pos(pos);
                let at = if vertical { slide.x } else { slide.y };
                if at.is_finite() {
                    state.guides.push(Guide { vertical, at });
                    state.show_guides = true;
                    state.drag = Some(GuideDrag {
                        index: state.guides.len() - 1,
                        left_ruler: false,
                    });
                }
            }
        }
    }

    if let Some(mut drag) = editor.ui.rulers.drag {
        let state = &mut editor.ui.rulers;
        let Some(guide) = state.guides.get(drag.index).copied() else {
            state.drag = None;
            return;
        };
        let on_ruler = pos.is_some_and(|p| own_strip(canvas, guide.vertical).contains(p));
        if down {
            if let Some(pos) = pos {
                let slide = t.to_slide_pos(pos);
                let at = if guide.vertical { slide.x } else { slide.y };
                if at.is_finite()
                    && let Some(g) = state.guides.get_mut(drag.index)
                {
                    g.at = at.round();
                }
            }
            if !on_ruler {
                drag.left_ruler = true;
            }
            state.drag = Some(drag);
            ui.ctx().request_repaint();
        } else {
            if on_ruler && drag.left_ruler {
                state.guides.remove(drag.index);
            }
            state.drag = None;
        }
    }

    paint(editor, painter, t, canvas);
}

fn paint(editor: &Editor, painter: &Painter, t: &SlideTransform, canvas: Rect) {
    let p = palette(painter.ctx());
    let state = &editor.ui.rulers;
    let clip = painter.with_clip_rect(painter.clip_rect().intersect(canvas));
    if state.show_guides {
        for guide in &state.guides {
            let at = guide_screen(t, *guide);
            if !at.is_finite() {
                continue;
            }
            let (a, b) = if guide.vertical {
                (pos2(at, canvas.min.y), pos2(at, canvas.max.y))
            } else {
                (pos2(canvas.min.x, at), pos2(canvas.max.x, at))
            };
            clip.line_segment([a, b], Stroke::new(1.0, p.accent));
        }
    }
    if !state.show_rulers {
        return;
    }
    let scale = t.scale();
    let step = tick_step(scale);
    let font = FontId::proportional(9.0);
    let top = top_strip(canvas);
    let left = left_strip(canvas);
    clip.rect_filled(top, 0.0, p.panel);
    clip.rect_filled(left, 0.0, p.panel);
    clip.line_segment(
        [top.left_bottom(), top.right_bottom()],
        Stroke::new(1.0, p.border),
    );
    clip.line_segment(
        [left.right_top(), left.right_bottom()],
        Stroke::new(1.0, p.border),
    );
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let origin = t.to_screen_pos(pos2(0.0, 0.0));
    if !origin.x.is_finite() || !origin.y.is_finite() {
        return;
    }
    for (vertical, strip) in [(false, top), (true, left)] {
        let (lo, hi) = if vertical {
            (strip.min.y, strip.max.y)
        } else {
            (strip.min.x, strip.max.x)
        };
        let base = if vertical { origin.y } else { origin.x };
        let first = ((lo - base) / scale / step).floor() as i64;
        let last = ((hi - base) / scale / step).ceil() as i64;
        let count = last.saturating_sub(first);
        if !(0..=2000).contains(&count) {
            continue;
        }
        for n in first..=last {
            let value = n as f32 * step;
            let screen = base + value * scale;
            if screen < lo || screen > hi {
                continue;
            }
            let (a, b, label_pos) = if vertical {
                (
                    pos2(strip.max.x - 8.0, screen),
                    pos2(strip.max.x, screen),
                    pos2(strip.min.x + 1.0, screen + 1.0),
                )
            } else {
                (
                    pos2(screen, strip.max.y - 8.0),
                    pos2(screen, strip.max.y),
                    pos2(screen + 2.0, strip.min.y + 1.0),
                )
            };
            clip.line_segment([a, b], Stroke::new(1.0, p.text_tertiary));
            clip.text(
                label_pos,
                Align2::LEFT_TOP,
                format!("{value:.0}"),
                font.clone(),
                p.text_tertiary,
            );
        }
    }
}
