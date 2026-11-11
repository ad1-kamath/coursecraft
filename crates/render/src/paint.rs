//! Slide painting using [`egui::Painter`].

use coursecraft_course::{
    Align, Color, MarkerIcon, MeterStyle, Paragraph, ScrollBar, ShapeKind, Theme, TimerFormat,
    VAlign,
};
use egui::emath::Rot2;
use egui::epaint::PathShape;
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Rect, Stroke, StrokeKind, pos2,
    vec2,
};

use crate::compose::{
    DrawItem, Hit, ResolvedFill, ResolvedOutline, ResolvedPath, ResolvedTextRun, ReviewMark, Visual,
};
use crate::icons::Icon;
use crate::layout::SlideTransform;
use crate::media::{LottieView, MediaCache, MediaSource, MediaView};
use crate::path_geometry::{PathMeshSpec, path_mesh};
use crate::shape_fill::{fill_contours, fill_contours_shaded, fill_contours_uv, stroke_contours};
use crate::shape_geometry::{shape_contours_adjusted, shape_text_rect};

const DIM_FACTOR: f32 = 0.35;
pub const DEFAULT_BACKGROUND_COLOR: egui::Color32 = egui::Color32::WHITE;
/// Longest edge, in screen points, used when shading a gradient fill.
const GRADIENT_MAX_EDGE: f32 = 12.0;
/// Layers used to approximate a blur or glow. Keeps per-object cost bounded.
const MAX_EFFECT_LAYERS: usize = 6;

/// UV window of the whole texture: no crop.
pub const FULL_UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

/// The rectangle an image's texture is drawn into inside its object frame.
///
/// With no crop the whole picture is aspect-fit and centred in `frame`, so a
/// frame whose proportions differ from the picture's is letterboxed, exactly as
/// before crops existed. With a crop the kept window fills `frame`.
///
/// Returns `None` when the frame, the texture size or the crop window cannot be
/// drawn safely (non-finite or empty).
#[must_use]
pub fn image_dest_rect(frame: Rect, tex_size: (f32, f32), crop: Option<Rect>) -> Option<Rect> {
    let frame_ok = [frame.min.x, frame.min.y, frame.max.x, frame.max.y]
        .iter()
        .all(|v| v.is_finite())
        && frame.width() > 0.0
        && frame.height() > 0.0;
    if !frame_ok {
        return None;
    }
    if let Some(crop) = crop {
        let crop_ok = [crop.min.x, crop.min.y, crop.max.x, crop.max.y]
            .iter()
            .all(|v| v.is_finite())
            && crop.width() > 0.0
            && crop.height() > 0.0;
        return crop_ok.then_some(frame);
    }
    let (tex_w, tex_h) = tex_size;
    if !(tex_w.is_finite() && tex_h.is_finite() && tex_w > 0.0 && tex_h > 0.0) {
        return None;
    }
    let aspect = tex_w / tex_h;
    let frame_aspect = frame.width() / frame.height();
    let (drawn_w, drawn_h) = if aspect > frame_aspect {
        (frame.width(), frame.width() / aspect)
    } else {
        (frame.height() * aspect, frame.height())
    };
    if !(drawn_w.is_finite() && drawn_h.is_finite() && drawn_w > 0.0 && drawn_h > 0.0) {
        return None;
    }
    Some(Rect::from_center_size(
        frame.center(),
        vec2(drawn_w, drawn_h),
    ))
}

/// Pixel size a picture occupies on screen, for vector rasterisation.
fn image_target_px(rect: Rect, pixels_per_point: f32) -> (u32, u32) {
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let convert = |value: f32| {
        if !value.is_finite() || value <= 0.0 {
            1
        } else {
            value.round().clamp(1.0, u32::MAX as f32) as u32
        }
    };
    (convert(rect.width() * ppp), convert(rect.height() * ppp))
}

/// Presentational highlight state for hover and press.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Highlight {
    pub hovered: Option<Hit>,
    pub pressed: Option<Hit>,
}

/// Number of polygon segments used to draw an ellipse with on-screen radius `radius_px`.
///
/// Large ellipses need more segments than small ones to look smooth. The count is
/// driven by the painted pixel radius and clamped so tiny shapes stay cheap and
/// huge shapes stay bounded.
#[must_use]
pub fn ellipse_segment_count(radius_px: f32) -> usize {
    if !radius_px.is_finite() || radius_px <= 0.0 {
        return 32;
    }
    ((radius_px * 0.5) as usize).clamp(32, 256)
}

fn apply_dim(color: Color32, dimmed: bool, opacity: f32) -> Color32 {
    let mut c = if dimmed {
        color.gamma_multiply(DIM_FACTOR)
    } else {
        color
    };
    if opacity < 1.0 {
        c = c.gamma_multiply(opacity.clamp(0.0, 1.0));
    }
    c
}

/// Screen-space contours for a shape, or `None` when it cannot draw.
fn screen_contours(
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
) -> Option<Vec<Vec<crate::shape_fill::Pt>>> {
    if !rect.is_positive()
        || ![rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .iter()
            .all(|value| value.is_finite())
    {
        return None;
    }
    let contours = shape_contours_adjusted(kind, rect.width(), rect.height(), adjust);
    if contours.is_empty() {
        return None;
    }
    Some(
        contours
            .into_iter()
            .map(|contour| {
                contour
                    .into_iter()
                    .map(|(x, y)| (rect.min.x + x, rect.min.y + y))
                    .collect()
            })
            .collect(),
    )
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(ar, br), mix(ag, bg), mix(ab, bb), mix(aa, ba))
}

fn sample_gradient(stops: &[crate::compose::ResolvedGradientStop], t: f32) -> Color32 {
    let Some(first) = stops.first() else {
        return Color32::TRANSPARENT;
    };
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if t <= first.position {
        return first.color;
    }
    for pair in stops.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if t <= b.position {
            let span = b.position - a.position;
            if span.abs() <= 1e-6 {
                return b.color;
            }
            return lerp_color(a.color, b.color, (t - a.position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

/// Paint a linear or radial gradient inside a shape's outline.
fn paint_gradient_fill(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    style: &ResolvedFill,
    dimmed: bool,
    opacity: f32,
) {
    let Some(contours) = screen_contours(rect, kind, adjust) else {
        return;
    };
    let centre = rect.center();
    let mesh = match style {
        ResolvedFill::Linear { angle, stops } => {
            let radians = if angle.is_finite() {
                angle.to_radians()
            } else {
                0.0
            };
            let (sin, cos) = radians.sin_cos();
            let extent = (rect.width() * cos).abs() + (rect.height() * sin).abs();
            fill_contours_shaded(
                &contours,
                painter.pixels_per_point(),
                GRADIENT_MAX_EDGE,
                |point| {
                    let distance = (point.0 - centre.x) * cos + (point.1 - centre.y) * sin;
                    let t = if extent > 1e-6 {
                        0.5 + distance / extent
                    } else {
                        0.5
                    };
                    apply_dim(sample_gradient(stops, t), dimmed, opacity)
                },
            )
        }
        ResolvedFill::Radial { center, stops } => {
            let origin = pos2(
                rect.min.x + center[0] * rect.width(),
                rect.min.y + center[1] * rect.height(),
            );
            let max_distance = [
                rect.left_top(),
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
            ]
            .iter()
            .map(|corner| {
                let dx = corner.x - origin.x;
                let dy = corner.y - origin.y;
                (dx * dx + dy * dy).sqrt()
            })
            .fold(0.0_f32, f32::max);
            fill_contours_shaded(
                &contours,
                painter.pixels_per_point(),
                GRADIENT_MAX_EDGE,
                |point| {
                    let dx = point.0 - origin.x;
                    let dy = point.1 - origin.y;
                    let t = if max_distance > 1e-6 {
                        (dx * dx + dy * dy).sqrt() / max_distance
                    } else {
                        0.0
                    };
                    apply_dim(sample_gradient(stops, t), dimmed, opacity)
                },
            )
        }
        ResolvedFill::Picture { .. } => return,
    };
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Paint a picture clipped to a shape's outline. Returns false when the picture
/// cannot be drawn, so the caller can fall back to the solid fill.
#[allow(clippy::too_many_arguments)]
fn paint_picture_fill(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    src: &str,
    tile: bool,
    scale: f32,
    dimmed: bool,
    opacity: f32,
    media: &mut Option<(&mut MediaCache, &dyn MediaSource)>,
) -> bool {
    let Some(contours) = screen_contours(rect, kind, adjust) else {
        return false;
    };
    let Some((cache, source)) = media.as_mut() else {
        return false;
    };
    let view = if tile {
        cache.texture_tiled(painter.ctx(), src, *source)
    } else {
        cache.texture(painter.ctx(), src, *source)
    };
    let MediaView::Ready { texture, size } = view else {
        return false;
    };
    let (tex_w, tex_h) = (size[0] as f32, size[1] as f32);
    if !(tex_w.is_finite() && tex_h.is_finite() && tex_w > 0.0 && tex_h > 0.0) {
        return false;
    }
    let tile_size = ((tex_w * scale).max(1e-3), (tex_h * scale).max(1e-3));
    let mut mesh = fill_contours_uv(&contours, painter.pixels_per_point(), |point| {
        let local_x = point.0 - rect.min.x;
        let local_y = point.1 - rect.min.y;
        if tile {
            (local_x / tile_size.0, local_y / tile_size.1)
        } else if rect.width() > 0.0 && rect.height() > 0.0 {
            (local_x / rect.width(), local_y / rect.height())
        } else {
            (0.0, 0.0)
        }
    });
    if mesh.is_empty() {
        return false;
    }
    mesh.texture_id = texture.id();
    let tint = apply_dim(Color32::WHITE, dimmed, opacity);
    for vertex in &mut mesh.vertices {
        let coverage = vertex.color.a();
        let alpha = (u16::from(tint.a()) * u16::from(coverage) / 255) as u8;
        let [r, g, b, _] = tint.to_srgba_unmultiplied();
        vertex.color = Color32::from_rgba_unmultiplied(r, g, b, alpha);
    }
    painter.add(egui::Shape::mesh(mesh));
    true
}

/// Button fill geometry: a rounded rectangle matching the old 6 px radius.
fn button_shape(rect: Rect, scale: f32) -> (ShapeKind, Vec<f32>) {
    let radius = (6.0 * scale).clamp(0.0, 255.0);
    let shortest = rect.width().min(rect.height()).max(1e-3);
    (
        ShapeKind::RoundedRectangle,
        vec![(radius / shortest).clamp(0.0, 0.5)],
    )
}

/// The offset/expansion/alpha copies used to fake a blur.
///
/// A shadow's copies run from `radius / 2` inside the silhouette to `radius / 2`
/// outside it, and a glow's from the edge to `radius` outside. Each copy has the
/// same alpha, chosen so the copies stacked over the middle reach about 90 % of
/// the colour. Where fewer copies overlap the colour fades, so the edge is a soft
/// ramp rather than a hard copy of the shape.
fn effect_copies(offset: (f32, f32), radius: f32, glow: bool) -> Vec<(f32, f32, f32, f32)> {
    if !radius.is_finite() || radius <= 0.1 {
        return if glow {
            Vec::new()
        } else {
            vec![(offset.0, offset.1, 0.0, 1.0)]
        };
    }
    let layers = MAX_EFFECT_LAYERS + 1;
    let alpha = 1.0 - 0.1_f32.powf(1.0 / layers as f32);
    let (from, to) = if glow {
        (0.0, radius)
    } else {
        (-radius * 0.5, radius * 0.5)
    };
    (0..layers)
        .map(|i| {
            let frac = i as f32 / (layers - 1) as f32;
            (offset.0, offset.1, from + (to - from) * frac, alpha)
        })
        .collect()
}

/// Grow `rect` by `amount` on every side; a negative amount shrinks it, never
/// past a 1-point-wide rect around its centre.
fn expand_clamped(rect: Rect, amount: f32) -> Rect {
    let floor = -((rect.width().min(rect.height()) - 1.0) * 0.5).max(0.0);
    rect.expand(amount.max(floor))
}

/// Paint a shape or rectangle silhouette behind an object for a shadow or glow.
#[allow(clippy::too_many_arguments)]
fn paint_shape_effect(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    color: Color32,
    offset: (f32, f32),
    radius: f32,
    glow: bool,
    dimmed: bool,
    opacity: f32,
) {
    let base = apply_dim(color, dimmed, opacity);
    for (dx, dy, expansion, alpha) in effect_copies(offset, radius, glow) {
        if alpha <= 0.0 {
            continue;
        }
        let layer_color = base.gamma_multiply(alpha.clamp(0.0, 1.0));
        let layer_rect = expand_clamped(rect.translate(vec2(dx, dy)), expansion);
        paint_shape(painter, layer_rect, kind, adjust, layer_color);
    }
}

/// Paint one shape into `rect`.
///
/// Geometry is computed for the rect's size, so details keep their proportions
/// when the frame is stretched. The mesh is clipped by the painter's existing
/// clip rect, and works the same offscreen and in the wasm player.
pub fn paint_shape(painter: &Painter, rect: Rect, kind: ShapeKind, adjust: &[f32], color: Color32) {
    if color.a() == 0 {
        return;
    }
    let Some(translated) = screen_contours(rect, kind, adjust) else {
        return;
    };
    let mesh = fill_contours(&translated, color, painter.pixels_per_point());
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Paint a shape's outline into `rect`.
///
/// The band is built as a mesh with the same feather rule as the fill, so the
/// offscreen rasteriser and the wasm player draw it identically. `scale` turns
/// the slide-pixel width into screen points.
pub fn paint_outline(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    outline: &ResolvedOutline,
    scale: f32,
) {
    if !rect.is_positive()
        || !scale.is_finite()
        || scale <= 0.0
        || ![rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .iter()
            .all(|value| value.is_finite())
    {
        return;
    }
    let contours = shape_contours_adjusted(kind, rect.width(), rect.height(), adjust);
    if contours.is_empty() {
        return;
    }
    let translated: Vec<Vec<(f32, f32)>> = contours
        .into_iter()
        .map(|contour| {
            contour
                .into_iter()
                .map(|(x, y)| (rect.min.x + x, rect.min.y + y))
                .collect()
        })
        .collect();
    let mesh = stroke_contours(
        &translated,
        outline.color,
        outline.width * scale,
        outline.dash,
        painter.pixels_per_point(),
    );
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

fn lighten_color(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let r = (r as f32 + (255.0 - r as f32) * amount)
        .clamp(0.0, 255.0)
        .round() as u8;
    let g = (g as f32 + (255.0 - g as f32) * amount)
        .clamp(0.0, 255.0)
        .round() as u8;
    let b = (b as f32 + (255.0 - b as f32) * amount)
        .clamp(0.0, 255.0)
        .round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn darken_color(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let r = (r as f32 * (1.0 - amount)).clamp(0.0, 255.0).round() as u8;
    let g = (g as f32 * (1.0 - amount)).clamp(0.0, 255.0).round() as u8;
    let b = (b as f32 * (1.0 - amount)).clamp(0.0, 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn draw_image_placeholder(
    slide_painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    label: &str,
    dimmed: bool,
    opacity: f32,
) {
    let bg = apply_dim(Color32::from_rgb(0xE0, 0xE0, 0xE0), dimmed, opacity);
    slide_painter.rect_filled(screen_rect, CornerRadius::ZERO, bg);
    let stroke_w = (1.0 * scale).max(1.0);
    let border_color = apply_dim(Color32::from_rgb(0xB0, 0xB0, 0xB0), dimmed, opacity);
    slide_painter.rect_stroke(
        screen_rect,
        CornerRadius::ZERO,
        Stroke::new(stroke_w, border_color),
        StrokeKind::Inside,
    );
    let font_size = (16.0 * scale).max(1.0);
    let font_id = crate::font::slide_font(slide_painter.ctx(), false, false, font_size);
    let text_color = apply_dim(Color32::from_rgb(0x60, 0x60, 0x60), dimmed, opacity);
    let galley = slide_painter.layout(
        label.to_string(),
        font_id,
        text_color,
        screen_rect.width().max(0.0),
    );
    let text_pos = pos2(
        screen_rect.center().x - galley.size().x * 0.5,
        screen_rect.center().y - galley.size().y * 0.5,
    );
    slide_painter.galley(text_pos, galley, text_color);
}

/// A web object on a host that cannot show pages: a card with a globe, the
/// source label and a line saying where the page does run.
fn draw_web_placeholder(
    slide_painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    label: &str,
    dimmed: bool,
    opacity: f32,
) {
    let palette = crate::design::palette(slide_painter.ctx());
    let bg = apply_dim(palette.panel, dimmed, opacity);
    let border = apply_dim(palette.border_strong, dimmed, opacity);
    let radius = CornerRadius::same((crate::design::RADIUS_MD * scale).clamp(0.0, 255.0) as u8);
    slide_painter.rect_filled(screen_rect, radius, bg);
    slide_painter.rect_stroke(
        screen_rect,
        radius,
        Stroke::new((1.0 * scale).max(1.0), border),
        StrokeKind::Inside,
    );

    let icon_color = apply_dim(palette.accent_text, dimmed, opacity);
    let icon_radius = (22.0 * scale).min(screen_rect.height() * 0.18).max(3.0);
    let icon_center = pos2(
        screen_rect.center().x,
        screen_rect.center().y - icon_radius - 12.0 * scale,
    );
    let stroke = Stroke::new((1.5 * scale).max(1.0), icon_color);
    slide_painter.circle_stroke(icon_center, icon_radius, stroke);
    slide_painter.line_segment(
        [
            pos2(icon_center.x - icon_radius, icon_center.y),
            pos2(icon_center.x + icon_radius, icon_center.y),
        ],
        stroke,
    );
    slide_painter.line_segment(
        [
            pos2(icon_center.x, icon_center.y - icon_radius),
            pos2(icon_center.x, icon_center.y + icon_radius),
        ],
        stroke,
    );
    let latitude = icon_radius * 0.45;
    slide_painter.line_segment(
        [
            pos2(icon_center.x - icon_radius * 0.95, icon_center.y - latitude),
            pos2(icon_center.x + icon_radius * 0.95, icon_center.y - latitude),
        ],
        stroke,
    );
    slide_painter.line_segment(
        [
            pos2(icon_center.x - icon_radius * 0.95, icon_center.y + latitude),
            pos2(icon_center.x + icon_radius * 0.95, icon_center.y + latitude),
        ],
        stroke,
    );

    let label_size = (15.0 * scale).max(1.0);
    let label_color = apply_dim(palette.text, dimmed, opacity);
    let label_font = crate::font::slide_font(slide_painter.ctx(), true, false, label_size);
    let label_galley = slide_painter.layout(
        label.to_owned(),
        label_font,
        label_color,
        screen_rect.width().max(0.0),
    );
    let label_pos = pos2(
        screen_rect.center().x - label_galley.size().x * 0.5,
        icon_center.y + icon_radius + 10.0 * scale,
    );
    slide_painter.galley(label_pos, label_galley.clone(), label_color);

    let note_size = (12.0 * scale).max(1.0);
    let note_color = apply_dim(palette.text_secondary, dimmed, opacity);
    let note_font = crate::font::slide_font(slide_painter.ctx(), false, false, note_size);
    let note_galley = slide_painter.layout(
        "Plays in the published course".to_owned(),
        note_font,
        note_color,
        screen_rect.width().max(0.0),
    );
    let note_pos = pos2(
        screen_rect.center().x - note_galley.size().x * 0.5,
        label_pos.y + label_galley.size().y + 4.0 * scale,
    );
    slide_painter.galley(note_pos, note_galley, note_color);
}

/// A question-bank draw in the authoring view: a centred card naming how
/// many questions the slide draws from which bank.
fn draw_draw_card(
    slide_painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    text: &str,
    dimmed: bool,
    opacity: f32,
) {
    let palette = crate::design::palette(slide_painter.ctx());
    let bg = apply_dim(palette.raised, dimmed, opacity);
    let border = apply_dim(palette.border_strong, dimmed, opacity);
    let radius = CornerRadius::same((crate::design::RADIUS_LG * scale).clamp(0.0, 255.0) as u8);
    slide_painter.rect_filled(screen_rect, radius, bg);
    slide_painter.rect_stroke(
        screen_rect,
        radius,
        Stroke::new((1.5 * scale).max(1.0), border),
        StrokeKind::Inside,
    );

    let text_size = (18.0 * scale).max(1.0);
    let text_color = apply_dim(palette.text, dimmed, opacity);
    let text_font = crate::font::slide_font(slide_painter.ctx(), true, false, text_size);
    let galley = slide_painter.layout(
        text.to_owned(),
        text_font,
        text_color,
        screen_rect.width().max(0.0),
    );
    let text_pos = pos2(
        screen_rect.center().x - galley.size().x * 0.5,
        screen_rect.center().y - galley.size().y * 0.5,
    );
    slide_painter.galley(text_pos, galley, text_color);
}

#[allow(clippy::too_many_arguments)]
fn paint_text(
    slide_painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    align: &Align,
    valign: &VAlign,
    autofit: coursecraft_course::Autofit,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
) {
    let dimmed_runs: Vec<ResolvedTextRun> = runs
        .iter()
        .map(|run| ResolvedTextRun {
            color: apply_dim(run.color, dimmed, opacity),
            ..run.clone()
        })
        .collect();
    let laid = crate::text_layout::layout_text(
        slide_painter.ctx(),
        screen_rect,
        scale,
        &dimmed_runs,
        paragraphs,
        *align,
        *valign,
        autofit,
    );
    let fallback_color = apply_dim(theme_text_color, dimmed, opacity);
    for paragraph in &laid.paragraphs {
        slide_painter.galley(paragraph.pos, paragraph.galley.clone(), fallback_color);
        for (points, stroke) in crate::text_layout::underline_segments(paragraph, paragraph.pos) {
            slide_painter.line_segment(points, stroke);
        }
        if let Some(marker) = &paragraph.marker {
            slide_painter.galley(marker.pos, marker.galley.clone(), fallback_color);
        }
    }
}

/// Which anchor cell owns each slot of a resolved table grid.
fn table_owner_grid(table: &crate::compose::ResolvedTable) -> Vec<Vec<Option<(usize, usize)>>> {
    let columns = table.columns.len();
    let mut owners = vec![vec![None; columns]; table.rows.len()];
    for row in 0..table.rows.len() {
        for column in 0..columns {
            if owners
                .get(row)
                .and_then(|line| line.get(column))
                .is_some_and(Option::is_some)
            {
                continue;
            }
            let (row_span, col_span) = table
                .rows
                .get(row)
                .and_then(|line| line.cells.get(column))
                .map_or((1, 1), |cell| {
                    (
                        usize::from(cell.row_span.max(1)),
                        usize::from(cell.col_span.max(1)),
                    )
                });
            for covered_row in row..table.rows.len().min(row.saturating_add(row_span)) {
                for covered_column in column..columns.min(column.saturating_add(col_span)) {
                    if let Some(slot) = owners
                        .get_mut(covered_row)
                        .and_then(|line| line.get_mut(covered_column))
                        && slot.is_none()
                    {
                        *slot = Some((row, column));
                    }
                }
            }
        }
    }
    owners
}

fn fraction_rect(
    frame: Rect,
    col_offsets: &[(f32, f32)],
    row_offsets: &[(f32, f32)],
    row: usize,
    column: usize,
    row_span: usize,
    col_span: usize,
) -> Option<Rect> {
    let (left, _) = *col_offsets.get(column)?;
    let (_, right) = *col_offsets.get(column.saturating_add(col_span).saturating_sub(1))?;
    let (top, _) = *row_offsets.get(row)?;
    let (_, bottom) = *row_offsets.get(row.saturating_add(row_span).saturating_sub(1))?;
    let rect = Rect::from_min_max(
        pos2(
            frame.left() + left * frame.width(),
            frame.top() + top * frame.height(),
        ),
        pos2(
            frame.left() + right * frame.width(),
            frame.top() + bottom * frame.height(),
        ),
    );
    rect.is_positive().then_some(rect)
}

/// Paint one table: fills, then clipped cell text, then grid lines.
#[allow(clippy::too_many_arguments)]
fn paint_table(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    table: &crate::compose::ResolvedTable,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
    show_overflow: bool,
) {
    if !screen_rect.is_positive() || table.columns.is_empty() || table.rows.is_empty() {
        return;
    }
    let col_offsets = coursecraft_course::table_fraction_offsets(&table.columns);
    let heights: Vec<f32> = table.rows.iter().map(|row| row.height).collect();
    let row_offsets = coursecraft_course::table_fraction_offsets(&heights);
    let owners = table_owner_grid(table);
    let pad = 6.0 * scale;

    for (row_index, row) in table.rows.iter().enumerate() {
        for (column, cell) in row.cells.iter().enumerate() {
            let owner = owners
                .get(row_index)
                .and_then(|line| line.get(column))
                .copied()
                .flatten();
            if owner != Some((row_index, column)) {
                continue;
            }
            let Some(rect) = fraction_rect(
                screen_rect,
                &col_offsets,
                &row_offsets,
                row_index,
                column,
                usize::from(cell.row_span.max(1)),
                usize::from(cell.col_span.max(1)),
            ) else {
                continue;
            };
            if let Some(fill) = cell.fill {
                painter.rect_filled(rect, CornerRadius::ZERO, apply_dim(fill, dimmed, opacity));
            }
            let inner = rect.shrink(pad.min(rect.width() * 0.5).min(rect.height() * 0.5));
            if !inner.is_positive() {
                continue;
            }
            let dimmed_runs: Vec<ResolvedTextRun> = cell
                .text
                .runs
                .iter()
                .map(|run| ResolvedTextRun {
                    color: apply_dim(run.color, dimmed, opacity),
                    ..run.clone()
                })
                .collect();
            let laid = crate::text_layout::layout_text(
                painter.ctx(),
                inner,
                scale,
                &dimmed_runs,
                &cell.text.paragraphs,
                cell.text.align,
                cell.text.valign,
                coursecraft_course::Autofit::None,
            );
            let cell_painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
            let fallback = apply_dim(theme_text_color, dimmed, opacity);
            for paragraph in &laid.paragraphs {
                cell_painter.galley(paragraph.pos, paragraph.galley.clone(), fallback);
                for (points, stroke) in
                    crate::text_layout::underline_segments(paragraph, paragraph.pos)
                {
                    cell_painter.line_segment(points, stroke);
                }
                if let Some(marker) = &paragraph.marker {
                    cell_painter.galley(marker.pos, marker.galley.clone(), fallback);
                }
            }
            if show_overflow
                && (laid.height > inner.height() + 0.5 || laid.width > inner.width() + 0.5)
            {
                let mark = (2.5 * scale).clamp(1.0, 8.0);
                let bottom = rect.right_bottom() - vec2(mark * 1.4, mark * 1.4);
                if bottom.x.is_finite() && bottom.y.is_finite() {
                    cell_painter.add(egui::Shape::convex_polygon(
                        vec![bottom, bottom + vec2(mark, 0.0), bottom + vec2(mark, -mark)],
                        fallback,
                        Stroke::NONE,
                    ));
                }
            }
        }
    }

    let line = |outline: &ResolvedOutline, color: Color32| {
        Stroke::new(
            (outline.width * scale).max(1.0),
            apply_dim(color, dimmed, opacity),
        )
    };
    if let Some(outer) = &table.outer {
        painter.rect_stroke(
            screen_rect,
            CornerRadius::ZERO,
            line(outer, outer.color),
            StrokeKind::Inside,
        );
    }
    if let Some(inner) = &table.inner {
        let stroke = line(inner, inner.color);
        if !table.columns.is_empty() {
            for boundary in 1..table.columns.len() {
                let Some((_, x)) = col_offsets.get(boundary.saturating_sub(1)) else {
                    continue;
                };
                let x = screen_rect.left() + x * screen_rect.width();
                for (row_index, row) in table.rows.iter().enumerate() {
                    let left = owners
                        .get(row_index)
                        .and_then(|line| line.get(boundary.saturating_sub(1)))
                        .copied()
                        .flatten();
                    let right = owners
                        .get(row_index)
                        .and_then(|line| line.get(boundary))
                        .copied()
                        .flatten();
                    if left == right {
                        continue;
                    }
                    let Some((top, bottom)) = row_offsets.get(row_index) else {
                        continue;
                    };
                    let _ = row;
                    painter.line_segment(
                        [
                            pos2(x, screen_rect.top() + top * screen_rect.height()),
                            pos2(x, screen_rect.top() + bottom * screen_rect.height()),
                        ],
                        stroke,
                    );
                }
            }
        }
        for boundary in 1..table.rows.len() {
            let Some((_, y)) = row_offsets.get(boundary.saturating_sub(1)) else {
                continue;
            };
            let y = screen_rect.top() + y * screen_rect.height();
            for column in 0..table.columns.len() {
                let above = owners
                    .get(boundary.saturating_sub(1))
                    .and_then(|line| line.get(column))
                    .copied()
                    .flatten();
                let below = owners
                    .get(boundary)
                    .and_then(|line| line.get(column))
                    .copied()
                    .flatten();
                if above == below {
                    continue;
                }
                let Some((left, right)) = col_offsets.get(column) else {
                    continue;
                };
                painter.line_segment(
                    [
                        pos2(screen_rect.left() + left * screen_rect.width(), y),
                        pos2(screen_rect.left() + right * screen_rect.width(), y),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// Paint glyph silhouettes behind text for a shadow or glow.
///
/// The same layout, wrapping, bullets and alignment are drawn again in one flat
/// colour, then drawn at every blur copy. Text shadows therefore rotate and
/// mirror with the object exactly like its glyphs.
#[allow(clippy::too_many_arguments)]
fn paint_text_effect(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    align: &Align,
    valign: &VAlign,
    autofit: coursecraft_course::Autofit,
    color: Color32,
    offset: (f32, f32),
    radius: f32,
    glow: bool,
    dimmed: bool,
    opacity: f32,
) {
    if runs.is_empty() {
        return;
    }
    let base = apply_dim(color, dimmed, opacity);
    let flat: Vec<ResolvedTextRun> = runs
        .iter()
        .map(|run| ResolvedTextRun {
            color: base,
            underline: false,
            link: None,
            ..run.clone()
        })
        .collect();
    let laid = crate::text_layout::layout_text(
        painter.ctx(),
        screen_rect,
        scale,
        &flat,
        paragraphs,
        *align,
        *valign,
        autofit,
    );
    for (dx, dy, _expansion, alpha) in effect_copies(offset, radius, glow) {
        if alpha <= 0.0 {
            continue;
        }
        let tint = base.gamma_multiply(alpha.clamp(0.0, 1.0));
        for paragraph in &laid.paragraphs {
            painter.galley(paragraph.pos + vec2(dx, dy), paragraph.galley.clone(), tint);
            if let Some(marker) = &paragraph.marker {
                painter.galley(marker.pos + vec2(dx, dy), marker.galley.clone(), tint);
            }
        }
    }
}

/// Paint the slide background through a [`SlideTransform`], optionally shifted by `offset`.
pub fn paint_background(
    painter: &Painter,
    t: &SlideTransform,
    theme: Option<&Theme>,
    slide_bg: Option<&Color>,
    offset: (f32, f32),
) {
    let bg_color = slide_bg
        .and_then(crate::color::parse_color)
        .or_else(|| theme.and_then(|th| crate::color::parse_color(&th.background)))
        .unwrap_or(DEFAULT_BACKGROUND_COLOR);

    let slide_rect = t.slide_rect();
    if offset == (0.0, 0.0) {
        painter.rect_filled(slide_rect, CornerRadius::ZERO, bg_color);
    } else {
        let scale = t.scale();
        let shifted = Rect::from_min_size(
            pos2(
                slide_rect.min.x + offset.0 * scale,
                slide_rect.min.y + offset.1 * scale,
            ),
            slide_rect.size(),
        );
        let clipped = shifted.intersect(slide_rect);
        if clipped.is_positive() {
            painter.rect_filled(clipped, CornerRadius::ZERO, bg_color);
        }
    }
}

/// Paint one drawn path into `screen_rect`.
fn paint_path(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    path: &ResolvedPath,
    dimmed: bool,
    opacity: f32,
) {
    if !screen_rect.is_positive()
        || !scale.is_finite()
        || scale <= 0.0
        || ![
            screen_rect.min.x,
            screen_rect.min.y,
            screen_rect.max.x,
            screen_rect.max.y,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return;
    }
    let points: Vec<crate::shape_geometry::Pt> = path
        .points
        .iter()
        .map(|point| {
            (
                screen_rect.min.x + point.x * screen_rect.width(),
                screen_rect.min.y + point.y * screen_rect.height(),
            )
        })
        .collect();
    let fill = path.fill.map(|color| apply_dim(color, dimmed, opacity));
    let (outline_color, outline_width, dash) = match &path.outline {
        Some(outline) => (
            apply_dim(outline.color, dimmed, opacity),
            outline.width * scale,
            outline.dash,
        ),
        None => (
            Color32::TRANSPARENT,
            0.0,
            coursecraft_course::DashStyle::Solid,
        ),
    };
    let mesh = path_mesh(
        &points,
        path.style,
        path.closed,
        path.connector,
        path.start_end,
        path.end_end,
        &PathMeshSpec {
            color: outline_color,
            width: outline_width,
            dash,
            fill,
            pixels_per_point: painter.pixels_per_point(),
        },
    );
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Paint a flat-colour copy of a path, used for its shadow or glow silhouette.
fn paint_path_silhouette(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    path: &ResolvedPath,
    color: Color32,
    extra_width: f32,
) {
    if !screen_rect.is_positive() || scale <= 0.0 {
        return;
    }
    let points: Vec<crate::shape_geometry::Pt> = path
        .points
        .iter()
        .map(|point| {
            (
                screen_rect.min.x + point.x * screen_rect.width(),
                screen_rect.min.y + point.y * screen_rect.height(),
            )
        })
        .collect();
    let width =
        path.outline.as_ref().map_or(2.0, |outline| outline.width) * scale + extra_width * 2.0;
    let fill = path.closed.then_some(color);
    let mesh = path_mesh(
        &points,
        path.style,
        path.closed,
        path.connector,
        path.start_end,
        path.end_end,
        &PathMeshSpec {
            color,
            width,
            dash: coursecraft_course::DashStyle::Solid,
            fill,
            pixels_per_point: painter.pixels_per_point(),
        },
    );
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Axis-aligned screen bounds of a rotated and flipped rectangle.
fn transformed_bounds(rect: Rect, rotation: f32, flip_h: bool, flip_v: bool) -> Rect {
    let centre = rect.center();
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let mut min = pos2(f32::INFINITY, f32::INFINITY);
    let mut max = pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for corner in corners {
        let corner = crate::compose::flip_point(
            crate::compose::rotate_point(corner, centre, rotation),
            centre,
            flip_h,
            flip_v,
        );
        min.x = min.x.min(corner.x);
        min.y = min.y.min(corner.y);
        max.x = max.x.max(corner.x);
        max.y = max.y.max(corner.y);
    }
    Rect::from_min_max(min, max)
}

/// Tessellate every shape painted after `marker`, rotate and mirror the meshes
/// about `centre`, and replace the originals with the transformed meshes.
///
/// Text and images become textured meshes here too, so they rotate and mirror
/// exactly like shapes. A single flip reverses the triangle winding.
fn transform_painted_shapes(
    painter: &Painter,
    marker: egui::layers::ShapeIdx,
    centre: egui::Pos2,
    rotation: f32,
    flip_h: bool,
    flip_v: bool,
) {
    let mut painted: Vec<(egui::layers::ShapeIdx, egui::epaint::ClippedShape)> = Vec::new();
    // `for_each_shape` does not expose the index, but entries are in order.
    let mut index = 0usize;
    painter.for_each_shape(|shape| {
        if index > marker.0 {
            painted.push((egui::layers::ShapeIdx(index), shape.clone()));
        }
        index = index.saturating_add(1);
    });
    if painted.is_empty() {
        return;
    }
    for (idx, _) in &painted {
        painter.set(*idx, egui::Shape::Noop);
    }
    painter.set(marker, egui::Shape::Noop);

    let radians = if rotation.is_finite() {
        rotation.to_radians()
    } else {
        0.0
    };
    let rot = Rot2::from_angle(radians);
    let mirror = usize::from(flip_h) + usize::from(flip_v);
    let ppp = painter.pixels_per_point();
    for (_, clipped) in painted {
        for primitive in painter.ctx().tessellate(vec![clipped], ppp).into_iter() {
            let egui::epaint::Primitive::Mesh(mut mesh) = primitive.primitive else {
                continue;
            };
            if radians != 0.0 {
                mesh.rotate(rot, centre);
            }
            if flip_h || flip_v {
                for vertex in &mut mesh.vertices {
                    if flip_h {
                        vertex.pos.x = 2.0 * centre.x - vertex.pos.x;
                    }
                    if flip_v {
                        vertex.pos.y = 2.0 * centre.y - vertex.pos.y;
                    }
                }
                if mirror == 1 {
                    for triangle in mesh.indices.as_chunks_mut::<3>().0 {
                        let [a, _, c] = triangle;
                        std::mem::swap(a, c);
                    }
                }
            }
            if !mesh.is_empty() {
                painter.add(egui::Shape::mesh(mesh));
            }
        }
    }
}

/// Screen-space margin a shadow or glow can paint beyond the item frame.
fn effect_margin(item: &DrawItem, scale: f32) -> f32 {
    let shadow = item.shadow.as_ref().map_or(0.0, |shadow| {
        let reach = shadow.offset_x.abs().max(shadow.offset_y.abs()) + shadow.blur;
        reach * scale
    });
    let glow = item
        .glow
        .as_ref()
        .map_or(0.0, |glow| glow.size.max(0.0) * scale);
    shadow.max(glow).max(0.0)
}

/// Paint slide draw items through a [`SlideTransform`], optionally shifted by `offset`.
#[allow(clippy::cognitive_complexity)]
pub fn paint_items(
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    highlight: Highlight,
    mut media: Option<(&mut MediaCache, &dyn MediaSource)>,
    theme: Option<&Theme>,
    offset: (f32, f32),
) {
    let slide_rect = t.slide_rect();
    let slide_painter = painter.with_clip_rect(slide_rect);
    let scale = t.scale();

    let theme_text_color = theme
        .and_then(|th| crate::color::parse_color(&th.text))
        .unwrap_or_else(|| Color32::from_rgb(0x1F, 0x1F, 0x1F));
    let theme_accent_color = theme
        .and_then(|th| crate::color::parse_color(&th.accent))
        .unwrap_or_else(|| Color32::from_rgb(0x2F, 0x6F, 0xEB));

    for item in items {
        if item.opacity <= 0.0 {
            continue;
        }
        let is_pressed = item.hit != Hit::None && highlight.pressed.as_ref() == Some(&item.hit);
        let is_hovered = item.hit != Hit::None && highlight.hovered.as_ref() == Some(&item.hit);

        let shifted_item_rect = Rect::from_min_size(
            pos2(item.rect.min.x + offset.0, item.rect.min.y + offset.1),
            item.rect.size(),
        );
        let screen_rect = t.to_screen_rect(shifted_item_rect);
        let op = item.opacity;
        let rotation = if item.rotation.is_finite() {
            item.rotation
        } else {
            0.0
        };
        let transformed = rotation != 0.0 || item.flip_h || item.flip_v;
        let effect_margin = effect_margin(item, scale);

        let mut item_painter = if transformed {
            // The rotated frame's bounding box must be visible even when the
            // unrotated frame is partly outside the slide. A wipe/fade clip
            // still applies on top.
            let bounds = transformed_bounds(
                screen_rect.expand(effect_margin),
                rotation,
                item.flip_h,
                item.flip_v,
            );
            let mut clip = bounds.intersect(slide_rect);
            if let Some(frac) = item.clip {
                let sub_x = shifted_item_rect.min.x + frac.min_x * shifted_item_rect.width();
                let sub_y = shifted_item_rect.min.y + frac.min_y * shifted_item_rect.height();
                let sub_w = (frac.max_x - frac.min_x) * shifted_item_rect.width();
                let sub_h = (frac.max_y - frac.min_y) * shifted_item_rect.height();
                let sub_rect = Rect::from_min_size(pos2(sub_x, sub_y), vec2(sub_w, sub_h));
                clip = clip.intersect(t.to_screen_rect(sub_rect));
            }
            slide_painter.with_clip_rect(clip)
        } else if let Some(clip) = item.clip {
            let sub_x = shifted_item_rect.min.x + clip.min_x * shifted_item_rect.width();
            let sub_y = shifted_item_rect.min.y + clip.min_y * shifted_item_rect.height();
            let sub_w = (clip.max_x - clip.min_x) * shifted_item_rect.width();
            let sub_h = (clip.max_y - clip.min_y) * shifted_item_rect.height();
            let sub_rect = Rect::from_min_size(pos2(sub_x, sub_y), vec2(sub_w, sub_h));
            let screen_sub = t.to_screen_rect(sub_rect);
            slide_painter.with_clip_rect(screen_sub.intersect(slide_rect))
        } else {
            slide_painter.clone()
        };

        if let Some(clip_rect) = item.clip_rect
            && clip_rect.is_positive()
        {
            let screen_clip = t.to_screen_rect(clip_rect);
            let current = item_painter.clip_rect();
            item_painter = item_painter.with_clip_rect(current.intersect(screen_clip));
        }

        let marker = transformed.then(|| item_painter.add(egui::Shape::Noop));

        match &item.visual {
            Visual::Text {
                runs,
                align,
                valign,
                paragraphs,
                autofit,
            } => {
                if let Some(shadow) = &item.shadow {
                    paint_text_effect(
                        &item_painter,
                        screen_rect,
                        scale,
                        runs,
                        paragraphs,
                        align,
                        valign,
                        *autofit,
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
                if let Some(glow) = &item.glow {
                    paint_text_effect(
                        &item_painter,
                        screen_rect,
                        scale,
                        runs,
                        paragraphs,
                        align,
                        valign,
                        *autofit,
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }
                paint_text(
                    &item_painter,
                    screen_rect,
                    scale,
                    runs,
                    paragraphs,
                    align,
                    valign,
                    *autofit,
                    item.dimmed,
                    op,
                    theme_text_color,
                );
            }
            Visual::Button {
                label,
                fill,
                fill_style,
                hover_fill,
                down_fill,
                disabled,
            } => {
                let effective_fill = if *disabled {
                    *fill
                } else if is_pressed {
                    down_fill.unwrap_or_else(|| darken_color(*fill, 0.15))
                } else if is_hovered {
                    hover_fill.unwrap_or_else(|| lighten_color(*fill, 0.12))
                } else {
                    *fill
                };
                let solid_override = !disabled
                    && ((is_pressed && down_fill.is_some())
                        || (is_hovered && hover_fill.is_some()));
                let (button_kind, button_adjust) = button_shape(screen_rect, scale);
                if let Some(shadow) = &item.shadow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        button_kind,
                        &button_adjust,
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
                if let Some(glow) = &item.glow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        button_kind,
                        &button_adjust,
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }

                let fill_color = apply_dim(effective_fill, item.dimmed, op);
                let mut drawn = false;
                if !solid_override {
                    match fill_style {
                        Some(ResolvedFill::Picture { src, tile }) => {
                            drawn = paint_picture_fill(
                                &item_painter,
                                screen_rect,
                                button_kind,
                                &button_adjust,
                                src,
                                *tile,
                                scale,
                                item.dimmed,
                                op,
                                &mut media,
                            );
                        }
                        Some(style) => {
                            paint_gradient_fill(
                                &item_painter,
                                screen_rect,
                                button_kind,
                                &button_adjust,
                                style,
                                item.dimmed,
                                op,
                            );
                            drawn = true;
                        }
                        None => {}
                    }
                }
                if !drawn {
                    let radius = (6.0 * scale).clamp(0.0, 255.0).round() as u8;
                    item_painter.rect_filled(screen_rect, CornerRadius::same(radius), fill_color);
                }

                let font_size = (20.0 * scale).max(1.0);
                let font_id = crate::font::slide_font(item_painter.ctx(), false, false, font_size);
                let text_color = apply_dim(Color32::WHITE, item.dimmed, op);
                let galley = item_painter.layout(
                    label.clone(),
                    font_id,
                    text_color,
                    screen_rect.width().max(0.0),
                );
                let text_pos = pos2(
                    screen_rect.center().x - galley.size().x * 0.5,
                    screen_rect.center().y - galley.size().y * 0.5,
                );
                item_painter.galley(text_pos, galley, text_color);
            }
            Visual::Shape {
                shape,
                adjust,
                fill,
                fill_style,
                outline,
                text,
                hover_fill,
                down_fill,
                disabled,
            } => {
                let effective_fill = if *disabled {
                    *fill
                } else if is_pressed {
                    down_fill.unwrap_or(*fill)
                } else if is_hovered {
                    hover_fill.unwrap_or(*fill)
                } else {
                    *fill
                };
                let solid_override = !disabled
                    && ((is_pressed && down_fill.is_some())
                        || (is_hovered && hover_fill.is_some()));
                if let Some(shadow) = &item.shadow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        *shape,
                        adjust,
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
                if let Some(glow) = &item.glow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        *shape,
                        adjust,
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }

                let fill_color = apply_dim(effective_fill, item.dimmed, op);
                let mut drawn = false;
                if !solid_override {
                    match fill_style {
                        Some(ResolvedFill::Picture { src, tile }) => {
                            drawn = paint_picture_fill(
                                &item_painter,
                                screen_rect,
                                *shape,
                                adjust,
                                src,
                                *tile,
                                scale,
                                item.dimmed,
                                op,
                                &mut media,
                            );
                        }
                        Some(style) => {
                            paint_gradient_fill(
                                &item_painter,
                                screen_rect,
                                *shape,
                                adjust,
                                style,
                                item.dimmed,
                                op,
                            );
                            drawn = true;
                        }
                        None => {}
                    }
                }
                if !drawn {
                    paint_shape(&item_painter, screen_rect, *shape, adjust, fill_color);
                }
                if let Some(outline) = outline {
                    let resolved = ResolvedOutline {
                        color: apply_dim(outline.color, item.dimmed, op),
                        width: outline.width,
                        dash: outline.dash,
                    };
                    paint_outline(&item_painter, screen_rect, *shape, adjust, &resolved, scale);
                }
                if let Some(text) = text
                    && !text.runs.is_empty()
                {
                    let (tx, ty, tw, th) = shape_text_rect(
                        *shape,
                        screen_rect.width() / scale,
                        screen_rect.height() / scale,
                        adjust,
                    );
                    let text_rect = Rect::from_min_size(
                        pos2(
                            screen_rect.min.x + tx * scale,
                            screen_rect.min.y + ty * scale,
                        ),
                        vec2(tw * scale, th * scale),
                    );
                    if text_rect.width() > 0.0 && text_rect.height() > 0.0 {
                        // Text may overflow its shape, as it does in PowerPoint.
                        paint_text(
                            &item_painter,
                            text_rect,
                            scale,
                            &text.runs,
                            &text.paragraphs,
                            &text.align,
                            &text.valign,
                            text.autofit,
                            item.dimmed,
                            op,
                            theme_text_color,
                        );
                    }
                }
            }
            Visual::Path(path) => {
                if let Some(shadow) = &item.shadow {
                    for (dx, dy, expansion, alpha) in effect_copies(
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                    ) {
                        let color = apply_dim(shadow.color, item.dimmed, op)
                            .gamma_multiply(alpha.clamp(0.0, 1.0));
                        let rect = expand_clamped(screen_rect.translate(vec2(dx, dy)), expansion);
                        paint_path_silhouette(
                            &item_painter,
                            rect,
                            scale,
                            path,
                            color,
                            expansion.max(0.0),
                        );
                    }
                }
                if let Some(glow) = &item.glow {
                    for (_, _, expansion, alpha) in
                        effect_copies((0.0, 0.0), glow.size * scale, true)
                    {
                        let color = apply_dim(glow.color, item.dimmed, op)
                            .gamma_multiply(alpha.clamp(0.0, 1.0));
                        let rect = screen_rect.expand(expansion);
                        paint_path_silhouette(&item_painter, rect, scale, path, color, expansion);
                    }
                }
                paint_path(&item_painter, screen_rect, scale, path, item.dimmed, op);
            }
            Visual::Table(table) => {
                if let Some(shadow) = &item.shadow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        ShapeKind::Rectangle,
                        &[],
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
                if let Some(glow) = &item.glow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        ShapeKind::Rectangle,
                        &[],
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }
                paint_table(
                    &item_painter,
                    screen_rect,
                    scale,
                    table,
                    item.dimmed,
                    op,
                    theme_text_color,
                    item.show_table_overflow,
                );
            }
            Visual::Image { src, alt, crop } => {
                if let Some(shadow) = &item.shadow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        ShapeKind::Rectangle,
                        &[],
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
                if let Some(glow) = &item.glow {
                    paint_shape_effect(
                        &item_painter,
                        screen_rect,
                        ShapeKind::Rectangle,
                        &[],
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }
                let mut drawn_texture = false;
                let mut failure_msg = None;
                if let Some((ref mut cache, source)) = media {
                    let target_px = image_target_px(screen_rect, painter.pixels_per_point());
                    match cache.texture_for(painter.ctx(), src, source, target_px) {
                        MediaView::Ready { texture, size } => {
                            // `None` (no crop, or an untrusted crop the composer
                            // dropped) keeps the old aspect-fit drawing. A crop's
                            // kept window fills the frame, so its pixels keep the
                            // scale the crop editor works in, per axis.
                            let tex_size = (size[0] as f32, size[1] as f32);
                            if let Some(dest_rect) = image_dest_rect(screen_rect, tex_size, *crop) {
                                let window = crop.unwrap_or(FULL_UV);
                                let tint = apply_dim(Color32::WHITE, item.dimmed, op);
                                item_painter.image(texture.id(), dest_rect, window, tint);
                                drawn_texture = true;
                            }
                        }
                        MediaView::Pending => {}
                        MediaView::Failed(msg) => {
                            failure_msg = Some(msg);
                        }
                    }
                }

                if !drawn_texture {
                    let label = if failure_msg.is_some() {
                        if !alt.is_empty() {
                            format!("{alt}\nimage failed to load")
                        } else {
                            "image failed to load".to_string()
                        }
                    } else if !alt.is_empty() {
                        alt.clone()
                    } else {
                        src.clone()
                    };

                    draw_image_placeholder(
                        &item_painter,
                        screen_rect,
                        scale,
                        &label,
                        item.dimmed,
                        op,
                    );
                }
            }
            Visual::ImagePlaceholder { label } => {
                draw_image_placeholder(&item_painter, screen_rect, scale, label, item.dimmed, op);
            }
            Visual::Video { src: _, poster: _ } => {
                // Desktop playback: draw the newest decoded frame as a texture
                // when one exists, with the poster as the fallback.
                let mut drawn_frame = false;
                if let Some((cache, source)) = media.as_mut() {
                    let object = match &item.hit {
                        Hit::Object(id) => Some(id.clone()),
                        _ => None,
                    };
                    if let Some(object) = object
                        && let Some(frame) = source.video_frame(&object)
                        && let Some(view) = cache.video_texture(painter.ctx(), &object, &frame)
                        && let MediaView::Ready { texture, size } = view
                    {
                        let tex_size = (size[0] as f32, size[1] as f32);
                        if let Some(dest_rect) = image_dest_rect(screen_rect, tex_size, None) {
                            let tint = apply_dim(Color32::WHITE, item.dimmed, op);
                            item_painter.image(texture.id(), dest_rect, FULL_UV, tint);
                            drawn_frame = true;
                        }
                    }
                }
                if drawn_frame {
                    continue;
                }
                let bg = apply_dim(Color32::from_rgb(0x1A, 0x1A, 0x1A), item.dimmed, op);
                item_painter.rect_filled(screen_rect, CornerRadius::ZERO, bg);
                let border = apply_dim(Color32::from_rgb(0x33, 0x33, 0x33), item.dimmed, op);
                let stroke_w = (1.0 * scale).max(1.0);
                item_painter.rect_stroke(
                    screen_rect,
                    CornerRadius::ZERO,
                    Stroke::new(stroke_w, border),
                    StrokeKind::Inside,
                );

                // Play icon in center
                let center = screen_rect.center();
                let play_radius = (24.0 * scale).min(screen_rect.height() * 0.25).max(4.0);
                let circle_color = apply_dim(
                    Color32::from_rgba_unmultiplied(255, 255, 255, 48),
                    item.dimmed,
                    op,
                );
                item_painter.circle_filled(
                    pos2(center.x, center.y - 12.0 * scale),
                    play_radius,
                    circle_color,
                );

                let tri_size = play_radius * 0.5;
                let tri_center_y = center.y - 12.0 * scale;
                let tri_points = vec![
                    pos2(center.x - tri_size * 0.5, tri_center_y - tri_size * 0.7),
                    pos2(center.x + tri_size * 0.8, tri_center_y),
                    pos2(center.x - tri_size * 0.5, tri_center_y + tri_size * 0.7),
                ];
                let tri_color = apply_dim(Color32::WHITE, item.dimmed, op);
                item_painter.add(PathShape::convex_polygon(
                    tri_points,
                    tri_color,
                    Stroke::NONE,
                ));

                // Poster text
                let font_size = (14.0 * scale).max(1.0);
                let font_id = crate::font::slide_font(item_painter.ctx(), false, false, font_size);
                let text_color = apply_dim(Color32::from_rgb(0xAA, 0xAA, 0xAA), item.dimmed, op);
                let galley = item_painter.layout(
                    "Video plays in the published course".to_string(),
                    font_id,
                    text_color,
                    screen_rect.width().max(0.0),
                );
                let text_pos = pos2(
                    center.x - galley.size().x * 0.5,
                    center.y + play_radius + 4.0 * scale - 12.0 * scale,
                );
                item_painter.galley(text_pos, galley, text_color);
            }
            Visual::Web { label } => {
                draw_web_placeholder(&item_painter, screen_rect, scale, label, item.dimmed, op);
            }
            Visual::DrawCard { text } => {
                draw_draw_card(&item_painter, screen_rect, scale, text, item.dimmed, op);
            }
            Visual::Audio { src: _ } => {
                if item.rect.width() <= 0.0 || item.rect.height() <= 0.0 {
                    continue;
                }
                let bg = apply_dim(Color32::from_rgb(0xE8, 0xEA, 0xED), item.dimmed, op);
                let border = apply_dim(Color32::from_rgb(0xBD, 0xC1, 0xC6), item.dimmed, op);
                let stroke_w = (1.5 * scale).max(1.0);
                let radius = CornerRadius::same((4.0 * scale).clamp(0.0, 255.0).round() as u8);
                item_painter.rect_filled(screen_rect, radius, bg);
                item_painter.rect_stroke(
                    screen_rect,
                    radius,
                    Stroke::new(stroke_w, border),
                    StrokeKind::Inside,
                );

                let center = screen_rect.center();
                let icon_color = apply_dim(Color32::from_rgb(0x3C, 0x40, 0x43), item.dimmed, op);
                let sz = (16.0 * scale).max(2.0);

                let body_w = (4.0 * scale).max(1.0);
                let body_h = (6.0 * scale).max(1.0);
                let body_rect = Rect::from_min_size(
                    pos2(center.x - sz * 0.4, center.y - body_h * 0.5),
                    vec2(body_w, body_h),
                );
                item_painter.rect_filled(body_rect, CornerRadius::ZERO, icon_color);

                let cone_left = center.x - sz * 0.4 + body_w;
                let cone_right = center.x + sz * 0.1;
                let cone_top = center.y - sz * 0.4;
                let cone_bottom = center.y + sz * 0.4;
                let cone_points = vec![
                    pos2(cone_left, center.y - body_h * 0.5),
                    pos2(cone_right, cone_top),
                    pos2(cone_right, cone_bottom),
                    pos2(cone_left, center.y + body_h * 0.5),
                ];
                item_painter.add(PathShape::convex_polygon(
                    cone_points,
                    icon_color,
                    Stroke::NONE,
                ));

                let wave_stroke = Stroke::new((1.5 * scale).max(1.0), icon_color);
                let w1_x = center.x + sz * 0.25;
                let w2_x = center.x + sz * 0.45;
                let wave1 = vec![
                    pos2(w1_x, center.y - sz * 0.2),
                    pos2(w1_x + sz * 0.08, center.y),
                    pos2(w1_x, center.y + sz * 0.2),
                ];
                let wave2 = vec![
                    pos2(w2_x, center.y - sz * 0.35),
                    pos2(w2_x + sz * 0.12, center.y),
                    pos2(w2_x, center.y + sz * 0.35),
                ];
                item_painter.add(PathShape::line(wave1, wave_stroke));
                item_painter.add(PathShape::line(wave2, wave_stroke));
            }
            Visual::Lottie {
                src,
                loop_playback,
                speed,
            } => {
                let mut failure: Option<String> = None;
                let mut drawn = false;
                if let Some((ref mut cache, source)) = media {
                    match cache.lottie_view(src, source) {
                        LottieView::Ready { composition, info } => {
                            let frame = item.media_time_ms.map_or(info.start_frame, |ms| {
                                crate::lottie::frame_at(&info, ms, *speed, *loop_playback)
                            });
                            let alpha = if item.dimmed { DIM_FACTOR } else { 1.0 } * op;
                            crate::lottie::paint_composition(
                                &item_painter,
                                screen_rect,
                                composition,
                                frame,
                                alpha,
                                painter.pixels_per_point(),
                            );
                            drawn = true;
                        }
                        LottieView::Pending => {}
                        LottieView::Failed(message) => failure = Some(message.to_owned()),
                    }
                }
                if !drawn {
                    let label = match failure {
                        Some(message) => format!("Lottie failed to load\n{message}"),
                        None => format!("Lottie\n{src}"),
                    };
                    draw_image_placeholder(
                        &item_painter,
                        screen_rect,
                        scale,
                        &label,
                        item.dimmed,
                        op,
                    );
                }
            }
            Visual::Prompt { text } => {
                let font_size = (28.0 * scale).max(1.0);
                let font_id = crate::font::slide_font(item_painter.ctx(), true, false, font_size);
                let text_color = apply_dim(theme_text_color, item.dimmed, op);
                let galley = item_painter.layout(
                    text.clone(),
                    font_id,
                    text_color,
                    screen_rect.width().max(0.0),
                );
                let pos = screen_rect.min;
                item_painter.galley(pos, galley, text_color);
            }
            Visual::ChoiceRow {
                text,
                multiple,
                selected,
                mark,
            } => {
                let marker_size = 20.0 * scale;
                let marker_x = screen_rect.min.x + 12.0 * scale;
                let marker_center = pos2(marker_x + marker_size * 0.5, screen_rect.center().y);
                let active_color = apply_dim(theme_accent_color, item.dimmed, op);
                let border_color = apply_dim(Color32::from_rgb(0x5A, 0x5A, 0x5A), item.dimmed, op);
                let unselected_bg = apply_dim(Color32::WHITE, item.dimmed, op);
                let stroke_w = (2.0 * scale).max(1.0);

                if *multiple {
                    let marker_rect =
                        Rect::from_center_size(marker_center, vec2(marker_size, marker_size));
                    let radius = (2.0 * scale).clamp(0.0, 255.0).round() as u8;
                    if *selected {
                        item_painter.rect_filled(
                            marker_rect,
                            CornerRadius::same(radius),
                            active_color,
                        );
                    } else {
                        item_painter.rect_filled(
                            marker_rect,
                            CornerRadius::same(radius),
                            unselected_bg,
                        );
                        item_painter.rect_stroke(
                            marker_rect,
                            CornerRadius::same(radius),
                            Stroke::new(stroke_w, border_color),
                            StrokeKind::Inside,
                        );
                    }
                } else {
                    let radius = marker_size * 0.5;
                    if *selected {
                        item_painter.circle_filled(marker_center, radius, active_color);
                    } else {
                        item_painter.circle_filled(marker_center, radius, unselected_bg);
                        item_painter.circle_stroke(
                            marker_center,
                            radius,
                            Stroke::new(stroke_w, border_color),
                        );
                    }
                }

                let text_start_x = screen_rect.min.x + 44.0 * scale;
                let available_text_w = (screen_rect.max.x - text_start_x).max(0.0);
                let font_size = (22.0 * scale).max(1.0);
                let text_color = apply_dim(theme_text_color, item.dimmed, op);
                let font_id = crate::font::slide_font(item_painter.ctx(), false, false, font_size);
                let galley =
                    item_painter.layout(text.clone(), font_id, text_color, available_text_w);
                let text_pos = pos2(text_start_x, screen_rect.center().y - galley.size().y * 0.5);
                item_painter.galley(text_pos, galley, text_color);
                paint_review_outline(&item_painter, screen_rect, scale, *mark, item.dimmed, op);
                paint_review_mark(&item_painter, screen_rect, scale, *mark, item.dimmed, op);
            }
            Visual::RegionOutline { ellipse } => {
                let color = apply_dim(
                    crate::design::palette(item_painter.ctx()).success,
                    item.dimmed,
                    op,
                );
                let stroke = Stroke::new((3.0 * scale).max(1.0), color);
                if *ellipse {
                    item_painter.add(egui::Shape::ellipse_stroke(
                        screen_rect.center(),
                        screen_rect.size() * 0.5,
                        stroke,
                    ));
                } else {
                    item_painter.rect_stroke(
                        screen_rect,
                        CornerRadius::ZERO,
                        stroke,
                        StrokeKind::Inside,
                    );
                }
            }
            Visual::HotspotMarker => {
                let center = t.to_screen_pos(shifted_item_rect.center());
                let radius = (12.0 * scale).max(1.0);
                let ring = (2.0 * scale).max(1.0);
                let fill = apply_dim(theme_accent_color, item.dimmed, op);
                let ring_color = apply_dim(Color32::WHITE, item.dimmed, op);
                item_painter.circle_filled(center, radius, fill);
                item_painter.circle_stroke(center, radius, Stroke::new(ring, ring_color));
            }
            visual @ (Visual::FillIn { .. }
            | Visual::SequenceRow { .. }
            | Visual::SequenceButton { .. }
            | Visual::MatchRow { .. }
            | Visual::CorrectAnswer { .. }) => {
                paint_answer_visual(
                    &item_painter,
                    visual,
                    screen_rect,
                    scale,
                    item.dimmed,
                    op,
                    (theme_text_color, theme_accent_color),
                );
            }
            visual @ (Visual::TextEntry { .. }
            | Visual::Slider { .. }
            | Visual::Dial { .. }
            | Visual::Checkbox { .. }
            | Visual::RadioGroup { .. }) => {
                paint_input_visual(
                    &item_painter,
                    visual,
                    screen_rect,
                    scale,
                    item.dimmed,
                    op,
                    (theme_text_color, theme_accent_color),
                );
            }
            Visual::Marker {
                title,
                body,
                paragraphs,
                icon,
                color,
                open,
                pulse,
            } => {
                paint_marker(
                    &item_painter,
                    screen_rect,
                    scale,
                    title,
                    body,
                    paragraphs,
                    *icon,
                    *color,
                    *open,
                    *pulse,
                    item.dimmed,
                    op,
                    theme_text_color,
                    slide_rect,
                );
            }
            Visual::ScrollPanel {
                content_height,
                bar,
                authoring,
                thumb,
                ..
            } => {
                paint_scroll_panel(
                    &item_painter,
                    screen_rect,
                    scale,
                    *content_height,
                    *bar,
                    *authoring,
                    thumb.map(|thumb| t.to_screen_rect(thumb)),
                    item.dimmed,
                    op,
                    theme_text_color,
                );
            }
            Visual::Timer {
                runs,
                align,
                valign,
                format,
                authoring,
            } => {
                paint_timer(
                    &item_painter,
                    screen_rect,
                    scale,
                    runs,
                    align,
                    valign,
                    *format,
                    *authoring,
                    item.dimmed,
                    op,
                    theme_text_color,
                );
            }
            Visual::ProgressMeter {
                fraction,
                style,
                show_value,
                percent,
            } => {
                paint_progress_meter(
                    &item_painter,
                    screen_rect,
                    scale,
                    *fraction,
                    *style,
                    *show_value,
                    *percent,
                    item.dimmed,
                    op,
                    theme_accent_color,
                    theme_text_color,
                );
            }
        }

        if let Some(mark) = item.mark {
            paint_drag_mark(&item_painter, screen_rect, scale, mark, item.dimmed, op);
        }

        if let Some(marker) = marker {
            transform_painted_shapes(
                &item_painter,
                marker,
                screen_rect.center(),
                rotation,
                item.flip_h,
                item.flip_v,
            );
        }
    }
}

fn paint_answer_visual(
    painter: &Painter,
    visual: &Visual,
    screen_rect: Rect,
    scale: f32,
    dimmed: bool,
    opacity: f32,
    colors: (Color32, Color32),
) {
    let (text_color, accent) = colors;
    let mark = match visual {
        Visual::FillIn { mark, .. }
        | Visual::SequenceRow { mark, .. }
        | Visual::MatchRow { mark, .. } => *mark,
        _ => None,
    };
    let outline =
        mark.map(|mark| apply_dim(review_mark_color(painter.ctx(), mark), dimmed, opacity));
    match visual {
        Visual::FillIn {
            text,
            placeholder,
            multiline,
            ..
        } => {
            paint_question_box(painter, screen_rect, scale, dimmed, opacity, outline);
            let shown = if text.is_empty() { placeholder } else { text };
            let color = if text.is_empty() {
                apply_dim(Color32::from_rgb(0x8A, 0x8A, 0x8A), dimmed, opacity)
            } else {
                apply_dim(text_color, dimmed, opacity)
            };
            if *multiline {
                let inset = 8.0 * scale;
                let x = screen_rect.min.x + inset;
                let width = (screen_rect.max.x - inset - x).max(0.0);
                let font_size = (22.0 * scale).max(1.0);
                let font_id = crate::font::slide_font(painter.ctx(), false, false, font_size);
                let galley = painter.layout(shown.clone(), font_id, color, width);
                let clip = painter.with_clip_rect(screen_rect);
                clip.galley(pos2(x, screen_rect.min.y + inset), galley, color);
            } else {
                paint_inset_label(painter, screen_rect, scale, 8.0, shown, color);
            }
        }
        Visual::SequenceRow { text, .. } => {
            paint_question_box(painter, screen_rect, scale, dimmed, opacity, outline);
            let color = apply_dim(text_color, dimmed, opacity);
            let inset = screen_rect.shrink2(vec2(12.0 * scale, 0.0));
            let text_rect = Rect::from_min_max(
                inset.min,
                pos2((inset.max.x - 72.0 * scale).max(inset.min.x), inset.max.y),
            );
            paint_inset_label(painter, text_rect, scale, 0.0, text, color);
        }
        Visual::SequenceButton { up, enabled } => {
            let color = if *enabled {
                apply_dim(accent, dimmed, opacity)
            } else {
                apply_dim(Color32::from_rgb(0xC8, 0xC8, 0xC8), dimmed, opacity)
            };
            paint_triangle(painter, screen_rect, *up, color);
        }
        Visual::MatchRow { left, value, .. } => {
            paint_question_box(painter, screen_rect, scale, dimmed, opacity, outline);
            let color = apply_dim(text_color, dimmed, opacity);
            let combo_w = screen_rect.width() * 0.46;
            let left_rect = Rect::from_min_max(
                screen_rect.min,
                pos2(
                    (screen_rect.max.x - combo_w - 12.0 * scale).max(screen_rect.min.x),
                    screen_rect.max.y,
                ),
            );
            paint_inset_label(painter, left_rect, scale, 12.0, left, color);
            let combo = Rect::from_min_size(
                pos2(
                    screen_rect.max.x - combo_w - 8.0 * scale,
                    screen_rect.min.y + 6.0 * scale,
                ),
                vec2(
                    combo_w.max(0.0),
                    (screen_rect.height() - 12.0 * scale).max(0.0),
                ),
            );
            painter.rect_filled(
                combo,
                CornerRadius::same((4.0 * scale).clamp(0.0, 255.0).round() as u8),
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
            painter.rect_stroke(
                combo,
                CornerRadius::same((4.0 * scale).clamp(0.0, 255.0).round() as u8),
                Stroke::new(
                    (1.0 * scale).max(1.0),
                    apply_dim(Color32::from_rgb(0x5A, 0x5A, 0x5A), dimmed, opacity),
                ),
                StrokeKind::Inside,
            );
            paint_inset_label(painter, combo, scale, 8.0, value, color);
        }
        Visual::CorrectAnswer { text } => {
            let color = apply_dim(
                crate::design::palette(painter.ctx()).text_secondary,
                dimmed,
                opacity,
            );
            let font_size = (22.0 * 0.85 * scale).max(1.0);
            let font_id = crate::font::slide_font(painter.ctx(), false, false, font_size);
            let x = screen_rect.min.x + 12.0 * scale;
            let width = (screen_rect.max.x - x).max(0.0);
            let galley = painter.layout(text.clone(), font_id, color, width);
            let y = screen_rect.center().y - galley.size().y * 0.5;
            painter.galley(pos2(x, y), galley, color);
        }
        _ => {}
    }
    paint_review_mark(painter, screen_rect, scale, mark, dimmed, opacity);
}

fn review_mark_color(ctx: &egui::Context, mark: ReviewMark) -> Color32 {
    let palette = crate::design::palette(ctx);
    match mark {
        ReviewMark::Correct | ReviewMark::MissedCorrect => palette.success,
        ReviewMark::Incorrect => palette.danger,
    }
}

/// The filled badge at a row's right edge: a tick, a cross, or an outlined
/// tick for a missed correct answer.
fn paint_review_mark(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    mark: Option<ReviewMark>,
    dimmed: bool,
    opacity: f32,
) {
    let Some(mark) = mark else {
        return;
    };
    let radius = (rect.height() * 0.3).max(0.0);
    if radius <= 0.0 {
        return;
    }
    let inset = 8.0 * scale;
    let center = pos2(
        (rect.max.x - inset - radius).max(rect.min.x),
        rect.center().y,
    );
    let color = apply_dim(review_mark_color(painter.ctx(), mark), dimmed, opacity);
    let icon_size = (radius * 1.2).max(1.0);
    let icon_font = FontId::new(icon_size, FontFamily::Name("phosphor".into()));
    match mark {
        ReviewMark::Correct => {
            painter.circle_filled(center, radius, color);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::Check.codepoint().to_string(),
                icon_font,
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
        }
        ReviewMark::Incorrect => {
            painter.circle_filled(center, radius, color);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::X.codepoint().to_string(),
                icon_font,
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
        }
        ReviewMark::MissedCorrect => {
            painter.circle_stroke(center, radius, Stroke::new((2.0 * scale).max(1.0), color));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::Check.codepoint().to_string(),
                icon_font,
                color,
            );
        }
    }
}

/// A drag item's badge at its top-right corner.
fn paint_drag_mark(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    mark: ReviewMark,
    dimmed: bool,
    opacity: f32,
) {
    let radius = (rect.height().min(rect.width()) * 0.3).max(0.0);
    if radius <= 0.0 {
        return;
    }
    let inset = 8.0 * scale;
    let center = pos2(
        (rect.max.x - inset - radius).max(rect.min.x),
        (rect.min.y + inset + radius).min(rect.max.y),
    );
    let color = apply_dim(review_mark_color(painter.ctx(), mark), dimmed, opacity);
    let icon_font = FontId::new((radius * 1.2).max(1.0), FontFamily::Name("phosphor".into()));
    match mark {
        ReviewMark::Correct => {
            painter.circle_filled(center, radius, color);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::Check.codepoint().to_string(),
                icon_font,
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
        }
        ReviewMark::Incorrect => {
            painter.circle_filled(center, radius, color);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::X.codepoint().to_string(),
                icon_font,
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
        }
        ReviewMark::MissedCorrect => {
            painter.circle_stroke(center, radius, Stroke::new((2.0 * scale).max(1.0), color));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                Icon::Check.codepoint().to_string(),
                icon_font,
                color,
            );
        }
    }
}

/// A row's outline in its mark colour.
fn paint_review_outline(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    mark: Option<ReviewMark>,
    dimmed: bool,
    opacity: f32,
) {
    let Some(mark) = mark else {
        return;
    };
    let color = apply_dim(review_mark_color(painter.ctx(), mark), dimmed, opacity);
    painter.rect_stroke(
        rect,
        CornerRadius::same((6.0 * scale).clamp(0.0, 255.0).round() as u8),
        Stroke::new((2.0 * scale).max(1.0), color),
        StrokeKind::Inside,
    );
}

/// How far a slider's track is inset so its thumb stays inside the frame.
fn slider_inset(rect: Rect, orientation: coursecraft_course::Orientation) -> f32 {
    match orientation {
        coursecraft_course::Orientation::Horizontal => {
            (rect.height() * 0.5).min(rect.width() * 0.5)
        }
        coursecraft_course::Orientation::Vertical => (rect.width() * 0.5).min(rect.height() * 0.5),
    }
    .max(0.0)
}

/// The track line of a slider, inset from the frame for the thumb.
#[must_use]
pub fn slider_track_rect(rect: Rect, orientation: coursecraft_course::Orientation) -> Rect {
    let inset = slider_inset(rect, orientation);
    match orientation {
        coursecraft_course::Orientation::Horizontal => Rect::from_min_max(
            pos2(rect.left() + inset, rect.center().y),
            pos2(rect.right() - inset, rect.center().y),
        ),
        coursecraft_course::Orientation::Vertical => Rect::from_min_max(
            pos2(rect.center().x, rect.top() + inset),
            pos2(rect.center().x, rect.bottom() - inset),
        ),
    }
}

/// The filled part of a slider's track for a `0..=1` fraction.
#[must_use]
pub fn slider_fill_rect(
    rect: Rect,
    orientation: coursecraft_course::Orientation,
    fraction: f32,
) -> Rect {
    let track = slider_track_rect(rect, orientation);
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match orientation {
        coursecraft_course::Orientation::Horizontal => Rect::from_min_max(
            track.min,
            pos2(track.left() + track.width() * fraction, track.bottom()),
        ),
        coursecraft_course::Orientation::Vertical => Rect::from_min_max(
            pos2(track.left(), track.bottom() - track.height() * fraction),
            track.max,
        ),
    }
}

/// Centre of a slider's thumb for a `0..=1` fraction. Vertical sliders put
/// min at the bottom.
#[must_use]
pub fn slider_thumb_center(
    rect: Rect,
    orientation: coursecraft_course::Orientation,
    fraction: f32,
) -> egui::Pos2 {
    let track = slider_track_rect(rect, orientation);
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match orientation {
        coursecraft_course::Orientation::Horizontal => {
            pos2(track.left() + track.width() * fraction, track.center().y)
        }
        coursecraft_course::Orientation::Vertical => {
            pos2(track.center().x, track.bottom() - track.height() * fraction)
        }
    }
}

/// Distance from the centre for an angle in degrees clockwise from 12 o'clock.
#[must_use]
pub fn dial_point(center: egui::Pos2, radius: f32, degrees: f32) -> egui::Pos2 {
    let radians = degrees.to_radians();
    pos2(
        center.x + radius * radians.sin(),
        center.y - radius * radians.cos(),
    )
}

/// The filled part of a progress bar for a `0..=1` fraction, left to right.
#[must_use]
pub fn progress_bar_fill_rect(rect: Rect, fraction: f32) -> Rect {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    Rect::from_min_max(
        rect.min,
        pos2(rect.left() + rect.width() * fraction, rect.bottom()),
    )
}

/// Point on a progress ring at `fraction` of a full clockwise turn from 12.
#[must_use]
pub fn progress_ring_point(center: egui::Pos2, radius: f32, fraction: f32) -> egui::Pos2 {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    dial_point(center, radius, fraction * 360.0)
}

#[allow(clippy::too_many_arguments)]
fn paint_timer(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    align: &Align,
    valign: &VAlign,
    format: TimerFormat,
    authoring: bool,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
) {
    if format == TimerFormat::Hidden {
        if !authoring {
            return;
        }
        // The canvas shows a small labelled placeholder for a hidden timer.
        let border = apply_dim(
            crate::design::palette(painter.ctx()).border_strong,
            dimmed,
            opacity,
        );
        let radius = (6.0 * scale).clamp(0.0, 255.0).round() as u8;
        painter.rect_stroke(
            screen_rect,
            CornerRadius::same(radius),
            Stroke::new((1.0 * scale).max(1.0), border),
            StrokeKind::Inside,
        );
        let color = apply_dim(theme_text_color, dimmed, opacity);
        let font_size = (14.0 * scale).max(1.0);
        let font = crate::font::slide_font(painter.ctx(), false, false, font_size);
        let label = "Timer (hidden)";
        let galley = painter.layout(label.to_owned(), font, color, screen_rect.width());
        let pos = pos2(
            screen_rect.center().x - galley.size().x * 0.5,
            screen_rect.center().y - galley.size().y * 0.5,
        );
        painter.galley(pos, galley, color);
        return;
    }
    paint_text(
        painter,
        screen_rect,
        scale,
        runs,
        &[],
        align,
        valign,
        coursecraft_course::Autofit::None,
        dimmed,
        opacity,
        theme_text_color,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_progress_meter(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    fraction: f32,
    style: MeterStyle,
    show_value: bool,
    percent: u8,
    dimmed: bool,
    opacity: f32,
    accent: Color32,
    text_color: Color32,
) {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let accent = apply_dim(accent, dimmed, opacity);
    // A meter belongs to the slide, so its track doesn't follow the editor theme.
    let track = apply_dim(Color32::from_rgb(0xC9, 0xCD, 0xD4), dimmed, opacity);
    match style {
        MeterStyle::Bar => {
            let thickness =
                (rect.height() * 0.4).clamp(4.0 * scale, rect.height().max(4.0 * scale));
            let band = Rect::from_center_size(rect.center(), vec2(rect.width(), thickness));
            let radius = (thickness * 0.5).clamp(0.0, 255.0).round() as u8;
            painter.rect_filled(band, CornerRadius::same(radius), track);
            let fill = progress_bar_fill_rect(band, fraction);
            if fill.width() > 0.0 {
                let fill_band =
                    Rect::from_center_size(fill.center(), vec2(fill.width(), thickness));
                painter.rect_filled(fill_band, CornerRadius::same(radius), accent);
            }
        }
        MeterStyle::Ring => {
            let side = rect.width().min(rect.height());
            let thickness = (side * 0.18).clamp(2.0 * scale, side * 0.5);
            let center = rect.center();
            let radius = (side * 0.5 - thickness * 0.5).max(1.0);
            let stroke = Stroke::new(thickness, track);
            painter.circle_stroke(center, radius, stroke);
            if fraction > 0.0 {
                let steps = (64.0 * fraction).ceil().max(1.0) as usize;
                let points: Vec<_> = (0..=steps)
                    .map(|i| {
                        let t = fraction * i as f32 / steps as f32;
                        progress_ring_point(center, radius, t)
                    })
                    .collect();
                // One continuous stroke: separate segments show as bands where they meet.
                painter.add(egui::Shape::line(points, Stroke::new(thickness, accent)));
            }
        }
    }
    if show_value {
        let color = apply_dim(text_color, dimmed, opacity);
        let font_size = (16.0 * scale).max(1.0);
        let font = crate::font::slide_font(painter.ctx(), false, false, font_size);
        let label = format!("{percent}%");
        let galley = painter.layout(label, font, color, rect.width());
        let pos = pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        painter.galley(pos, galley, color);
    }
}

/// Value fraction of a dial or slider, clamped to `0..=1`.
#[must_use]
pub fn value_fraction(value: f64, min: f64, max: f64) -> f32 {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || max <= min {
        return 0.0;
    }
    let fraction = (value - min) / (max - min);
    if fraction.is_finite() {
        fraction.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_input_visual(
    painter: &Painter,
    visual: &Visual,
    rect: Rect,
    scale: f32,
    dimmed: bool,
    opacity: f32,
    colors: (Color32, Color32),
) {
    let (text_color, accent) = colors;
    let accent = apply_dim(accent, dimmed, opacity);
    let text_color = apply_dim(text_color, dimmed, opacity);
    let muted = apply_dim(Color32::from_rgb(0x8A, 0x8A, 0x8A), dimmed, opacity);
    let border = apply_dim(Color32::from_rgb(0xD0, 0xD4, 0xDC), dimmed, opacity);
    let track = apply_dim(Color32::from_rgb(0xC9, 0xCD, 0xD4), dimmed, opacity);
    let stroke_w = (1.0 * scale).max(1.0);
    match visual {
        Visual::TextEntry {
            text,
            placeholder,
            multiline,
        } => {
            let radius = (6.0 * scale).clamp(0.0, 255.0).round() as u8;
            painter.rect_filled(
                rect,
                CornerRadius::same(radius),
                apply_dim(Color32::WHITE, dimmed, opacity),
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(radius),
                Stroke::new(stroke_w, border),
                StrokeKind::Inside,
            );
            let pad = 8.0 * scale;
            let inner = Rect::from_min_max(
                pos2(rect.left() + pad, rect.top() + pad * 0.5),
                pos2(rect.right() - pad, rect.bottom() - pad * 0.5),
            );
            if inner.width() <= 0.0 || inner.height() <= 0.0 {
                return;
            }
            let shown = if text.is_empty() { placeholder } else { text };
            let color = if text.is_empty() { muted } else { text_color };
            let font_size = (22.0 * scale).max(1.0);
            let font = crate::font::slide_font(painter.ctx(), false, false, font_size);
            let wrap = if *multiline {
                inner.width()
            } else {
                f32::INFINITY
            };
            let galley = painter.layout(shown.clone(), font, color, wrap);
            let clip = painter.with_clip_rect(rect.intersect(inner));
            let origin = if *multiline {
                inner.min
            } else {
                pos2(inner.left(), rect.center().y - galley.size().y * 0.5)
            };
            clip.galley(origin, galley, color);
        }
        Visual::Slider {
            value,
            min,
            max,
            orientation,
            ..
        } => {
            let fraction = value_fraction(*value, *min, *max);
            let track_rect = slider_track_rect(rect, *orientation);
            if track_rect.width() >= 0.0 && track_rect.height() >= 0.0 {
                let thickness = (6.0 * scale).max(2.0);
                let band = match orientation {
                    coursecraft_course::Orientation::Horizontal => Rect::from_center_size(
                        track_rect.center(),
                        vec2(track_rect.width(), thickness),
                    ),
                    coursecraft_course::Orientation::Vertical => Rect::from_center_size(
                        track_rect.center(),
                        vec2(thickness, track_rect.height()),
                    ),
                };
                let radius = (thickness * 0.5).clamp(0.0, 255.0).round() as u8;
                painter.rect_filled(band, CornerRadius::same(radius), track);
            }
            let fill = slider_fill_rect(rect, *orientation, fraction);
            let thickness = (6.0 * scale).max(2.0);
            let fill_band = match orientation {
                coursecraft_course::Orientation::Horizontal => {
                    Rect::from_center_size(fill.center(), vec2(fill.width(), thickness))
                }
                coursecraft_course::Orientation::Vertical => {
                    Rect::from_center_size(fill.center(), vec2(thickness, fill.height()))
                }
            };
            let radius = (thickness * 0.5).clamp(0.0, 255.0).round() as u8;
            painter.rect_filled(fill_band, CornerRadius::same(radius), accent);
            let radius = (10.0 * scale).max(2.0);
            painter.circle_filled(
                slider_thumb_center(rect, *orientation, fraction),
                radius,
                accent,
            );
        }
        Visual::Dial {
            value,
            min,
            max,
            start_angle,
            end_angle,
            ..
        } => {
            let fraction = value_fraction(*value, *min, *max);
            let center = rect.center();
            let radius = (rect.width().min(rect.height()) * 0.5 - 10.0 * scale).max(1.0);
            let start = f64::from(*start_angle);
            let span = (f64::from(*end_angle) - start).max(0.0);
            let steps = 48usize;
            let mut track_points = Vec::with_capacity(steps + 1);
            let mut fill_points = Vec::with_capacity(steps + 1);
            let fill_steps = ((steps as f32) * fraction).round() as usize;
            for i in 0..=steps {
                let t = i as f64 / steps as f64;
                let angle = start + span * t;
                #[allow(clippy::cast_possible_truncation)]
                let point = dial_point(center, radius, angle as f32);
                track_points.push(point);
                if i <= fill_steps {
                    fill_points.push(point);
                }
            }
            painter.add(PathShape::line(
                track_points,
                Stroke::new((6.0 * scale).max(2.0), track),
            ));
            if fill_points.len() >= 2 {
                painter.add(PathShape::line(
                    fill_points,
                    Stroke::new((6.0 * scale).max(2.0), accent),
                ));
            }
            #[allow(clippy::cast_possible_truncation)]
            let pointer_angle = (start + span * f64::from(fraction)) as f32;
            let tip = dial_point(center, radius * 0.72, pointer_angle);
            painter.add(PathShape::line(
                vec![center, tip],
                Stroke::new((3.0 * scale).max(1.0), accent),
            ));
            painter.circle_filled(center, (4.0 * scale).max(1.0), accent);
        }
        Visual::Checkbox { checked, label } => {
            let size = (20.0 * scale).min(rect.height()).max(2.0);
            let marker = Rect::from_min_size(
                pos2(rect.left() + 6.0 * scale, rect.center().y - size * 0.5),
                vec2(size, size),
            );
            let radius = (3.0 * scale).clamp(0.0, 255.0).round() as u8;
            if *checked {
                painter.rect_filled(marker, CornerRadius::same(radius), accent);
                let check = vec![
                    pos2(marker.left() + size * 0.22, marker.center().y),
                    pos2(
                        marker.center().x - size * 0.05,
                        marker.bottom() - size * 0.28,
                    ),
                    pos2(marker.right() - size * 0.20, marker.top() + size * 0.26),
                ];
                painter.add(PathShape::line(
                    check,
                    Stroke::new((2.5 * scale).max(1.0), Color32::WHITE),
                ));
            } else {
                painter.rect_filled(
                    marker,
                    CornerRadius::same(radius),
                    apply_dim(Color32::WHITE, dimmed, opacity),
                );
                painter.rect_stroke(
                    marker,
                    CornerRadius::same(radius),
                    Stroke::new((2.0 * scale).max(1.0), border),
                    StrokeKind::Inside,
                );
            }
            if !label.is_empty() {
                let font_size = (22.0 * scale).max(1.0);
                let font = crate::font::slide_font(painter.ctx(), false, false, font_size);
                let x = marker.right() + 8.0 * scale;
                let width = (rect.right() - x).max(0.0);
                let galley = painter.layout(label.clone(), font, text_color, width);
                painter.galley(
                    pos2(x, rect.center().y - galley.size().y * 0.5),
                    galley,
                    text_color,
                );
            }
        }
        Visual::RadioGroup { options } => {
            let size = (20.0 * scale).max(2.0);
            let row_h = rect.height() / (options.len().max(1) as f32);
            for (i, (label, _value, selected)) in options.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let center_y = rect.top() + row_h * (i as f32 + 0.5);
                let center = pos2(rect.left() + 6.0 * scale + size * 0.5, center_y);
                let ring = Stroke::new((2.0 * scale).max(1.0), border);
                if *selected {
                    painter.circle_filled(center, size * 0.5, accent);
                    painter.circle_filled(center, size * 0.22, Color32::WHITE);
                } else {
                    painter.circle_filled(
                        center,
                        size * 0.5,
                        apply_dim(Color32::WHITE, dimmed, opacity),
                    );
                    painter.circle_stroke(center, size * 0.5, ring);
                }
                if !label.is_empty() {
                    let font_size = (22.0 * scale).max(1.0);
                    let font = crate::font::slide_font(painter.ctx(), false, false, font_size);
                    let x = center.x + size;
                    let width = (rect.right() - x).max(0.0);
                    let galley = painter.layout(label.clone(), font, text_color, width);
                    painter.galley(
                        pos2(x, center_y - galley.size().y * 0.5),
                        galley,
                        text_color,
                    );
                }
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_marker(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    title: &str,
    body: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    icon: MarkerIcon,
    color: Color32,
    open: bool,
    pulse: bool,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
    slide_rect: Rect,
) {
    if !screen_rect.is_positive() {
        return;
    }
    let dot_color = apply_dim(color, dimmed, opacity);
    let radius = (screen_rect.width().min(screen_rect.height()) * 0.5).max(1.0);
    let center = screen_rect.center();
    if pulse {
        let phase = (painter.ctx().input(|i| i.time) as f32 * 1.2).fract();
        let ring = radius + phase * (radius * 0.9);
        let alpha = ((1.0 - phase) * 0.35).clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = (alpha * 255.0) as u8;
        let ring_color =
            Color32::from_rgba_unmultiplied(dot_color.r(), dot_color.g(), dot_color.b(), alpha);
        painter.circle_stroke(
            center,
            ring,
            Stroke::new((1.5 * scale).max(1.0), ring_color),
        );
        painter.ctx().request_repaint();
    }
    painter.circle_filled(center, radius, dot_color);
    paint_marker_icon(painter, center, radius, icon, scale);

    if !open {
        return;
    }

    let body: Vec<ResolvedTextRun> = body
        .iter()
        .map(|run| ResolvedTextRun {
            color: apply_dim(run.color, dimmed, opacity),
            ..run.clone()
        })
        .collect();
    let body = body.as_slice();

    let pad = (10.0 * scale).max(2.0);
    let gap = (8.0 * scale).max(2.0);
    let max_width = (360.0 * scale).max(pad * 2.0 + 16.0);
    let text_width = (max_width - pad * 2.0).max(1.0);
    let title_color = apply_dim(theme_text_color, dimmed, opacity);
    let title_run = ResolvedTextRun {
        text: title.to_owned(),
        size: 16.0,
        color: title_color,
        bold: true,
        italic: false,
        underline: false,
        font: None,
        link: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    };
    let measure = Rect::from_min_size(pos2(0.0, 0.0), vec2(text_width, 100_000.0));
    let title_laid = crate::text_layout::layout_text(
        painter.ctx(),
        measure,
        scale,
        std::slice::from_ref(&title_run),
        &[],
        Align::Left,
        VAlign::Top,
        coursecraft_course::Autofit::None,
    );
    let body_laid = crate::text_layout::layout_text(
        painter.ctx(),
        measure,
        scale,
        body,
        paragraphs,
        Align::Left,
        VAlign::Top,
        coursecraft_course::Autofit::None,
    );
    let title_height = if title.trim().is_empty() {
        0.0
    } else {
        title_laid.height
    };
    let body_height = if body.iter().all(|run| run.text.trim().is_empty()) {
        0.0
    } else {
        body_laid.height
    };
    let content_width = title_laid.width.max(body_laid.width).min(text_width);
    let separator = if title_height > 0.0 && body_height > 0.0 {
        4.0 * scale
    } else {
        0.0
    };
    let card_width = (content_width + pad * 2.0).clamp(0.0, max_width);
    let card_height = title_height + separator + body_height + pad * 2.0;
    if !card_width.is_finite() || !card_height.is_finite() {
        return;
    }

    let mut card_x = screen_rect.right() + gap;
    let mut card_y = screen_rect.top();
    if card_x + card_width > slide_rect.right() {
        let left = screen_rect.left() - gap - card_width;
        if left >= slide_rect.left() {
            card_x = left;
        } else {
            card_x = (screen_rect.center().x - card_width * 0.5).clamp(
                slide_rect.left(),
                (slide_rect.right() - card_width).max(slide_rect.left()),
            );
            let below = screen_rect.bottom() + gap;
            card_y = if below + card_height <= slide_rect.bottom() {
                below
            } else {
                (screen_rect.top() - gap - card_height).max(slide_rect.top())
            };
        }
    }
    card_y = card_y.clamp(
        slide_rect.top(),
        (slide_rect.bottom() - card_height).max(slide_rect.top()),
    );
    let card = Rect::from_min_size(pos2(card_x, card_y), vec2(card_width, card_height));

    let (card_fill, card_border) = marker_card_colors(theme_text_color);
    let radius = (8.0 * scale).clamp(0.0, 255.0).round() as u8;
    painter.rect_filled(
        card,
        CornerRadius::same(radius),
        apply_dim(card_fill, dimmed, opacity),
    );
    painter.rect_stroke(
        card,
        CornerRadius::same(radius),
        Stroke::new(
            (1.0 * scale).max(1.0),
            apply_dim(card_border, dimmed, opacity),
        ),
        StrokeKind::Inside,
    );

    let mut cursor = card.top() + pad;
    if title_height > 0.0 {
        let rect = Rect::from_min_size(
            pos2(card.left() + pad, cursor),
            vec2(text_width, title_height + 1.0),
        );
        let laid = crate::text_layout::layout_text(
            painter.ctx(),
            rect,
            scale,
            std::slice::from_ref(&title_run),
            &[],
            Align::Left,
            VAlign::Top,
            coursecraft_course::Autofit::None,
        );
        for paragraph in &laid.paragraphs {
            painter.galley(paragraph.pos, paragraph.galley.clone(), title_color);
        }
        cursor += title_height + separator;
    }
    if body_height > 0.0 {
        let rect = Rect::from_min_size(
            pos2(card.left() + pad, cursor),
            vec2(text_width, body_height + 1.0),
        );
        let laid = crate::text_layout::layout_text(
            painter.ctx(),
            rect,
            scale,
            body,
            paragraphs,
            Align::Left,
            VAlign::Top,
            coursecraft_course::Autofit::None,
        );
        let fallback = apply_dim(theme_text_color, dimmed, opacity);
        for paragraph in &laid.paragraphs {
            painter.galley(paragraph.pos, paragraph.galley.clone(), fallback);
            for (points, stroke) in crate::text_layout::underline_segments(paragraph, paragraph.pos)
            {
                painter.line_segment(points, stroke);
            }
            if let Some(marker) = &paragraph.marker {
                painter.galley(marker.pos, marker.galley.clone(), fallback);
            }
        }
    }
}

fn paint_marker_icon(
    painter: &Painter,
    center: egui::Pos2,
    radius: f32,
    icon: MarkerIcon,
    scale: f32,
) {
    let color = Color32::WHITE;
    let unit = (radius * 0.42).max(1.0);
    let stroke = Stroke::new((2.0 * scale).max(1.0), color);
    match icon {
        MarkerIcon::Info => {
            painter.circle_filled(center + vec2(0.0, -unit * 0.8), unit * 0.16, color);
            painter.line_segment(
                [
                    center + vec2(0.0, -unit * 0.3),
                    center + vec2(0.0, unit * 0.8),
                ],
                stroke,
            );
        }
        MarkerIcon::Plus => {
            painter.line_segment(
                [
                    center + vec2(-unit * 0.8, 0.0),
                    center + vec2(unit * 0.8, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + vec2(0.0, -unit * 0.8),
                    center + vec2(0.0, unit * 0.8),
                ],
                stroke,
            );
        }
        MarkerIcon::Question => {
            let font = crate::font::slide_font(painter.ctx(), true, false, unit * 1.8);
            painter.text(center, egui::Align2::CENTER_CENTER, "?", font, color);
        }
        MarkerIcon::Star => {
            let mut points = Vec::with_capacity(11);
            let outer = unit * 1.05;
            let inner = outer * 0.45;
            for index in 0..10 {
                let angle =
                    -std::f32::consts::FRAC_PI_2 + (index as f32) * std::f32::consts::PI / 5.0;
                let length = if index % 2 == 0 { outer } else { inner };
                points.push(center + vec2(angle.cos() * length, angle.sin() * length));
            }
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            painter.add(PathShape::line(points, stroke));
        }
        MarkerIcon::Pin => {
            painter.circle_filled(center + vec2(0.0, -unit * 0.35), unit * 0.55, color);
            painter.line_segment(
                [
                    center + vec2(0.0, unit * 0.2),
                    center + vec2(0.0, unit * 1.05),
                ],
                stroke,
            );
        }
    }
}

/// A marker card belongs to the slide, not the editor, so it contrasts with the
/// course's text colour whichever appearance the app is in.
fn marker_card_colors(text: Color32) -> (Color32, Color32) {
    let luminance =
        0.2126 * f32::from(text.r()) + 0.7152 * f32::from(text.g()) + 0.0722 * f32::from(text.b());
    if luminance < 128.0 {
        (Color32::WHITE, Color32::from_rgb(0xD5, 0xD9, 0xE0))
    } else {
        (
            Color32::from_rgb(0x24, 0x26, 0x2B),
            Color32::from_rgb(0x4A, 0x4D, 0x55),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_scroll_panel(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    content_height: f32,
    bar: ScrollBar,
    authoring: bool,
    thumb: Option<Rect>,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
) {
    if !screen_rect.is_positive() {
        return;
    }
    let palette = crate::design::palette(painter.ctx());
    if authoring {
        let stroke = Stroke::new(
            (1.0 * scale).max(1.0),
            apply_dim(palette.border_strong, dimmed, opacity),
        );
        painter.rect_stroke(screen_rect, CornerRadius::ZERO, stroke, StrokeKind::Inside);
        if content_height.is_finite() && content_height > screen_rect.height() / scale.max(1e-6) {
            let bottom = screen_rect.top()
                + (content_height * scale).min(screen_rect.height().max(0.0) + 100_000.0);
            let content_rect = Rect::from_min_max(
                screen_rect.min,
                pos2(screen_rect.right(), bottom.max(screen_rect.top())),
            );
            painter.rect_stroke(
                content_rect,
                CornerRadius::ZERO,
                Stroke::new(
                    (1.0 * scale).max(1.0),
                    apply_dim(palette.text_tertiary, dimmed, opacity * 0.6),
                ),
                StrokeKind::Inside,
            );
        }
        return;
    }
    if bar == ScrollBar::Visible
        && let Some(thumb) = thumb
        && thumb.is_positive()
    {
        let radius = (thumb.width() * 0.5).clamp(0.0, 255.0).round() as u8;
        painter.rect_filled(
            thumb,
            CornerRadius::same(radius),
            apply_dim(
                Color32::from_rgba_unmultiplied(
                    theme_text_color.r(),
                    theme_text_color.g(),
                    theme_text_color.b(),
                    110,
                ),
                dimmed,
                opacity,
            ),
        );
    }
}

fn paint_question_box(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    dimmed: bool,
    opacity: f32,
    outline: Option<Color32>,
) {
    let radius = (6.0 * scale).clamp(0.0, 255.0).round() as u8;
    painter.rect_filled(
        rect,
        CornerRadius::same(radius),
        apply_dim(Color32::from_rgb(0xF7, 0xF8, 0xFA), dimmed, opacity),
    );
    let border =
        outline.unwrap_or_else(|| apply_dim(Color32::from_rgb(0xD0, 0xD4, 0xDC), dimmed, opacity));
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(
            (if outline.is_some() { 2.0 } else { 1.0 } * scale).max(1.0),
            border,
        ),
        StrokeKind::Inside,
    );
}

fn paint_inset_label(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    inset: f32,
    text: &str,
    color: Color32,
) {
    let font_size = (22.0 * scale).max(1.0);
    let x = rect.min.x + inset * scale;
    let width = (rect.max.x - x).max(0.0);
    let font_id = crate::font::slide_font(painter.ctx(), false, false, font_size);
    let galley = painter.layout(text.to_owned(), font_id, color, width);
    let y = rect.center().y - galley.size().y * 0.5;
    painter.galley(pos2(x, y), galley, color);
}

fn paint_triangle(painter: &Painter, rect: Rect, up: bool, color: Color32) {
    let center = rect.center();
    let w = rect.width() * 0.28;
    let h = rect.height() * 0.22;
    let points = if up {
        vec![
            pos2(center.x, center.y - h),
            pos2(center.x - w, center.y + h),
            pos2(center.x + w, center.y + h),
        ]
    } else {
        vec![
            pos2(center.x, center.y + h),
            pos2(center.x - w, center.y - h),
            pos2(center.x + w, center.y - h),
        ]
    };
    painter.add(PathShape::convex_polygon(points, color, Stroke::NONE));
}

/// Paint slide background and draw items through a [`SlideTransform`].
///
/// Fills the slide background with `slide_bg` or `theme.background` (defaulting to white),
/// clips items to the slide rect, and applies hover and down visuals to the item matching
/// `highlight`. Dimmed items are drawn at 35% opacity. Images with no crop are drawn
/// aspect-fit and centred in their frame; a cropped image fills its frame with the kept
/// part of the texture. Either uses a fallback placeholder when `media` cannot supply
/// the picture.
pub fn paint(
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    highlight: Highlight,
    mut media: Option<(&mut MediaCache, &dyn MediaSource)>,
    theme: Option<&Theme>,
    slide_bg: Option<&Color>,
) {
    // The static entry point always shows an animation's first frame. The
    // player and preview paint through `paint_items` with a clock instead.
    if let Some((cache, _)) = media.as_mut() {
        cache.set_animation_time(None);
    }
    paint_background(painter, t, theme, slide_bg, (0.0, 0.0));
    paint_items(painter, t, items, highlight, media, theme, (0.0, 0.0));
}

#[cfg(test)]
mod marker_card_tests {
    use super::*;

    #[test]
    fn marker_card_contrasts_with_the_course_text_colour() {
        let (light_fill, _) = marker_card_colors(Color32::from_rgb(0x1A, 0x1A, 0x1A));
        assert_eq!(light_fill, Color32::WHITE);
        let (dark_fill, _) = marker_card_colors(Color32::from_rgb(0xF0, 0xF0, 0xF0));
        assert!(dark_fill.r() < 80, "{dark_fill:?}");
    }
}
