//! Changing the slide size, with or without scaling the content.

use coursecraft_course::{Course, Object, ObjectKind, Outline, Size, Slide, TextRun, TextStyle};

/// Smallest and largest slide sides the command accepts, in pixels.
pub(crate) const MIN_SIDE: u32 = 100;
pub(crate) const MAX_W: u32 = 7680;
pub(crate) const MAX_H: u32 = 4320;

/// How a content-aware resize maps old coordinates to new ones.
#[derive(Clone, Copy)]
struct Map {
    factor: f32,
    dx: f32,
    dy: f32,
}

impl Map {
    fn len(self, v: f32) -> f32 {
        if v.is_finite() { v * self.factor } else { v }
    }

    fn x(self, v: f32) -> f32 {
        if v.is_finite() {
            v * self.factor + self.dx
        } else {
            v
        }
    }

    fn y(self, v: f32) -> f32 {
        if v.is_finite() {
            v * self.factor + self.dy
        } else {
            v
        }
    }
}

fn round_len(v: f32, min: f32) -> f32 {
    if v.is_finite() {
        v.round().max(min)
    } else {
        min
    }
}

fn scale_size(style: &mut TextStyle, m: Map) {
    style.size = m.len(style.size).clamp(1.0, 400.0);
}

fn scale_runs(runs: &mut [TextRun], m: Map) {
    for run in runs {
        if let Some(size) = run.size {
            run.size = Some(m.len(size).clamp(1.0, 400.0));
        }
    }
}

fn scale_outline(outline: &mut Outline, m: Map) {
    outline.width = m.len(outline.width).clamp(0.0, 50.0);
}

fn scale_object(o: &mut Object, m: Map) {
    let f = &mut o.frame;
    f.x = m.x(f.x).round();
    f.y = m.y(f.y).round();
    f.w = round_len(m.len(f.w), 1.0);
    f.h = round_len(m.len(f.h), 1.0);
    let frame_h = f.h;
    if let Some(shadow) = o.shadow.as_mut() {
        shadow.offset_x = m.len(shadow.offset_x).clamp(-200.0, 200.0);
        shadow.offset_y = m.len(shadow.offset_y).clamp(-200.0, 200.0);
        shadow.blur = m.len(shadow.blur).clamp(0.0, 100.0);
    }
    if let Some(glow) = o.glow.as_mut() {
        glow.size = m.len(glow.size).clamp(0.0, 100.0);
    }
    for path in &mut o.motion_paths {
        for point in &mut path.points {
            if path.relative {
                point.x = m.len(point.x);
                point.y = m.len(point.y);
            } else {
                point.x = m.x(point.x);
                point.y = m.y(point.y);
            }
        }
    }
    match &mut o.kind {
        ObjectKind::Text { style, runs, .. } => {
            scale_size(style, m);
            scale_runs(runs, m);
        }
        ObjectKind::Shape { outline, text, .. } => {
            if let Some(outline) = outline {
                scale_outline(outline, m);
            }
            if let Some(text) = text {
                scale_size(&mut text.style, m);
                scale_runs(&mut text.runs, m);
            }
        }
        ObjectKind::Path { path } => scale_outline(&mut path.outline, m),
        ObjectKind::Table { rows, borders, .. } => {
            for row in rows {
                for cell in &mut row.cells {
                    scale_size(&mut cell.style, m);
                    scale_runs(&mut cell.runs, m);
                }
            }
            if let Some(outline) = borders.outer.as_mut() {
                scale_outline(outline, m);
            }
            if let Some(outline) = borders.inner.as_mut() {
                scale_outline(outline, m);
            }
        }
        ObjectKind::ScrollPanel { content_height, .. } => {
            *content_height = m
                .len(*content_height)
                .max(frame_h)
                .clamp(0.0, coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT);
        }
        _ => {}
    }
}

fn scale_slide(slide: &mut Slide, m: Map) {
    for object in &mut slide.objects {
        scale_object(object, m);
    }
    for layer in &mut slide.layers {
        for object in &mut layer.objects {
            scale_object(object, m);
        }
    }
    if let Some(question) = slide.question.as_mut() {
        for region in &mut question.regions {
            region.x = m.x(region.x).round();
            region.y = m.y(region.y).round();
            region.w = round_len(m.len(region.w), 1.0);
            region.h = round_len(m.len(region.h), 1.0);
        }
    }
}

/// Check the requested size against the bounds.
pub(crate) fn size_in_bounds(w: u32, h: u32) -> bool {
    (MIN_SIDE..=MAX_W).contains(&w) && (MIN_SIDE..=MAX_H).contains(&h)
}

/// Change the slide size. With `scale`, content follows by one uniform factor
/// and is centred. Returns false when nothing changed.
pub(crate) fn set_slide_size(course: &mut Course, w: u32, h: u32, scale: bool) -> bool {
    let old = course.slide_size;
    if old.w == w && old.h == h {
        return false;
    }
    if scale && old.w > 0 && old.h > 0 {
        let factor = (w as f32 / old.w as f32).min(h as f32 / old.h as f32);
        if factor.is_finite() && factor > 0.0 {
            let m = Map {
                factor,
                dx: (w as f32 - old.w as f32 * factor) / 2.0,
                dy: (h as f32 - old.h as f32 * factor) / 2.0,
            };
            for scene in &mut course.scenes {
                for slide in &mut scene.slides {
                    scale_slide(slide, m);
                }
            }
            for slide in course.masters.iter_mut().chain(course.templates.iter_mut()) {
                scale_slide(slide, m);
            }
            for bank in &mut course.banks {
                for slide in &mut bank.slides {
                    scale_slide(slide, m);
                }
            }
        }
    }
    course.slide_size = Size { w, h };
    true
}
