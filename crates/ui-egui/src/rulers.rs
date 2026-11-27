//! Rulers, grid and user guides for the authoring canvas. Settings and guides
//! are saved on `Course.editor`; the player and publish ignore them.

use coursecraft_course::EditorSettings;
pub use coursecraft_course::Guide;
use coursecraft_render::{SlideTransform, palette};
use egui::{Align2, FontId, Painter, Pos2, Rect, Stroke, Ui, pos2};

use crate::Editor;
use crate::snap::LineSnaps;

/// Thickness of a ruler strip in screen points.
pub const RULER_SIZE: f32 = 18.0;
/// The grid is hidden when its zoomed spacing is below this many screen points.
pub const MIN_GRID_SCREEN: f32 = 6.0;
/// How close a press must be to a guide to grab it, in screen points.
const GRAB: f32 = 4.0;

/// A guide being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideDrag {
    /// Index of the existing guide being moved, or `None` when pulling a new guide off a ruler.
    pub existing: Option<usize>,
    pub vertical: bool,
    pub at: f32,
    /// The pointer has left the guide's own ruler, so dropping it back deletes it.
    pub left_ruler: bool,
}

/// Transient pointer state for dragging guides. Persistent settings live on `Course.editor`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RulerState {
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
pub fn captures(
    settings: &EditorSettings,
    state: &RulerState,
    canvas: Rect,
    t: &SlideTransform,
    pos: Pos2,
) -> bool {
    if state.drag.is_some() {
        return true;
    }
    if settings.show_rulers && (top_strip(canvas).contains(pos) || left_strip(canvas).contains(pos))
    {
        return true;
    }
    settings.show_guides && guide_at(settings, canvas, t, pos).is_some()
}

fn guide_at(
    settings: &EditorSettings,
    canvas: Rect,
    t: &SlideTransform,
    pos: Pos2,
) -> Option<usize> {
    if !canvas.contains(pos) {
        return None;
    }
    settings.guides.iter().position(|guide| {
        let at = guide_screen(t, *guide);
        let pointer = if guide.vertical { pos.x } else { pos.y };
        at.is_finite() && (at - pointer).abs() <= GRAB
    })
}

/// Snap lines for moves and resizes: user guides and the grid.
#[must_use]
pub fn snap_lines(settings: &EditorSettings) -> LineSnaps {
    let mut lines = LineSnaps::default();
    if settings.show_guides {
        for guide in &settings.guides {
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
    if settings.snap_to_grid {
        lines.grid = Some(settings.grid_step());
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
    if !editor.session.course.editor.show_grid {
        return;
    }
    let step = editor.session.course.editor.grid_step();
    let scale = t.scale();
    if !scale.is_finite() || scale <= 0.0 || step * scale < MIN_GRID_SCREEN {
        return;
    }
    let p = palette(painter.ctx());
    let size = editor.session.course.slide_size;
    let (w, h) = (size.w as f32, size.h as f32);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let cols = (w / step).floor() as usize;
    let rows = (h / step).floor() as usize;
    if cols.saturating_mul(rows) > 40_000 {
        return;
    }
    let clip = painter.clip_rect();
    for row in 0..=rows {
        for col in 0..=cols {
            let pt = t.to_screen_pos(pos2(col as f32 * step, row as f32 * step));
            if clip.contains(pt) {
                painter.circle_filled(pt, 0.9, p.border_strong);
            }
        }
    }
}

/// Handle the pointer for rulers and guides, then paint them.
pub fn show(editor: &mut Editor, ui: &Ui, painter: &Painter, t: &SlideTransform, canvas: Rect) {
    if !editor.session.course.editor.show_rulers
        && editor.session.course.editor.guides.is_empty()
        && editor.ui.rulers.drag.is_none()
    {
        return;
    }
    let pos = ui.input(|i| i.pointer.interact_pos());
    // A press on a menu or popup drawn over a ruler or guide is not meant for it.
    let pressed = ui.input(|i| i.pointer.primary_pressed())
        && pos.is_some_and(|pos| crate::widgets::pointer_reaches(ui, pos));
    let down = ui.input(|i| i.pointer.primary_down());

    if editor.ui.rulers.drag.is_none()
        && pressed
        && let Some(pos) = pos
    {
        let settings = &editor.session.course.editor;
        let existing = if settings.show_guides {
            guide_at(settings, canvas, t, pos)
        } else {
            None
        };
        if let Some(index) = existing
            && let Some(guide) = settings.guides.get(index).copied()
        {
            editor.ui.rulers.drag = Some(GuideDrag {
                existing: Some(index),
                vertical: guide.vertical,
                at: guide.at,
                left_ruler: true,
            });
        } else if settings.show_rulers {
            let vertical = left_strip(canvas).contains(pos);
            if vertical || top_strip(canvas).contains(pos) {
                let slide = t.to_slide_pos(pos);
                let at = if vertical { slide.x } else { slide.y };
                if at.is_finite() {
                    editor.ui.rulers.drag = Some(GuideDrag {
                        existing: None,
                        vertical,
                        at: at.round(),
                        left_ruler: false,
                    });
                }
            }
        }
    }

    if let Some(mut drag) = editor.ui.rulers.drag {
        let on_ruler = pos.is_some_and(|p| own_strip(canvas, drag.vertical).contains(p));
        if down {
            if let Some(pos) = pos {
                let slide = t.to_slide_pos(pos);
                let at = if drag.vertical { slide.x } else { slide.y };
                if at.is_finite() {
                    drag.at = at.round();
                }
            }
            if !on_ruler {
                drag.left_ruler = true;
            }
            editor.ui.rulers.drag = Some(drag);
            ui.ctx().request_repaint();
        } else {
            editor.ui.rulers.drag = None;
            if on_ruler && drag.left_ruler {
                if let Some(index) = drag.existing {
                    editor.run("view.guides.remove", serde_json::json!({ "index": index }));
                }
            } else if let Some(index) = drag.existing {
                editor.run(
                    "view.guides.move",
                    serde_json::json!({ "index": index, "at": drag.at.round() }),
                );
            } else if new_guide_wanted(editor, &drag) {
                editor.run(
                    "view.guides.add",
                    serde_json::json!({ "vertical": drag.vertical, "at": drag.at.round() }),
                );
            }
        }
    }

    paint(editor, painter, t, canvas);
}

/// A new guide is added only when it was dragged off the ruler and ends on the slide: a plain
/// click on the ruler adds nothing (`N107`).
fn new_guide_wanted(editor: &Editor, drag: &GuideDrag) -> bool {
    let size = editor.session.course.slide_size;
    let limit = if drag.vertical { size.w } else { size.h };
    drag.left_ruler && drag.at >= 0.0 && drag.at <= limit as f32
}

fn paint(editor: &Editor, painter: &Painter, t: &SlideTransform, canvas: Rect) {
    let p = palette(painter.ctx());
    let settings = &editor.session.course.editor;
    let drag = editor.ui.rulers.drag;
    let clip = painter.with_clip_rect(painter.clip_rect().intersect(canvas));
    let draw_guide = |guide: Guide| {
        let at = guide_screen(t, guide);
        if !at.is_finite() {
            return;
        }
        let (a, b) = if guide.vertical {
            (pos2(at, canvas.min.y), pos2(at, canvas.max.y))
        } else {
            (pos2(canvas.min.x, at), pos2(canvas.max.x, at))
        };
        clip.line_segment([a, b], Stroke::new(1.0, p.accent));
    };
    if settings.show_guides {
        for (i, guide) in settings.guides.iter().enumerate() {
            if drag.and_then(|d| d.existing) == Some(i) {
                continue;
            }
            draw_guide(*guide);
        }
    }
    if let Some(drag) = drag {
        draw_guide(Guide {
            vertical: drag.vertical,
            at: drag.at,
        });
    }
    if !settings.show_rulers {
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
            let text = format!("{value:.0}");
            let est_len = text.len() as f32 * 6.0 + 4.0;
            if screen + est_len <= hi {
                clip.text(
                    label_pos,
                    Align2::LEFT_TOP,
                    text,
                    font.clone(),
                    p.text_tertiary,
                );
            }
        }
    }
}

#[cfg(test)]
mod neon_tests {
    use super::*;
    use crate::test_harness::TestHarness;

    /// `N107`: a click on the ruler, or a drop off the slide, adds no guide.
    #[test]
    fn n107_a_plain_ruler_click_adds_no_guide() {
        let json = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/hello-course/course.json"
        ))
        .expect("hello course");
        let h = TestHarness::new(coursecraft_course::Course::from_json(&json).expect("course"));
        let drag = |left_ruler: bool, at: f32| GuideDrag {
            existing: None,
            vertical: true,
            at,
            left_ruler,
        };
        assert!(!new_guide_wanted(&h.editor, &drag(false, 100.0)), "a click");
        assert!(
            !new_guide_wanted(&h.editor, &drag(true, -20.0)),
            "off the slide"
        );
        assert!(
            new_guide_wanted(&h.editor, &drag(true, 100.0)),
            "a real drag"
        );
    }
}
