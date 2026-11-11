//! Lottie parsing and painting (L3).
//!
//! Parsing is deliberately suspicious of its input: the file size, JSON depth,
//! element count, composition size, frame rate and frame count are all capped
//! before anything reaches `velato`, and a rejected file becomes a placeholder
//! with a message. Painting goes through an [`velato::RenderSink`] that emits
//! epaint meshes, so the canvas, preview, player, offscreen renderer and wasm
//! player all draw the same thing.
//!
//! Known limits: velato does not expose the Lottie fill rule, so fills use the
//! shape code's even-odd triangulator (non-zero is emulated by dropping nested
//! contours that wind the same way). Strokes reuse the existing band builder,
//! so joins are miter/bevel and caps are its fixed square ends; Lottie dash
//! patterns are approximated as dash or dot. Layer masks are clipped to the
//! mask path's bounding box, not its exact shape.

use std::collections::HashSet;

use egui::{Color32, Mesh, Painter, Rect, pos2};
use kurbo::{Affine, PathEl, Point};
use peniko::color::{DynamicColor, Srgb};
use velato::{
    Composition, RenderSink, Renderer,
    model::{Content, Layer, Repeater, Shape as LottieShape, Value, fixed},
};

use crate::path_geometry::Pt;
use crate::shape_fill::{fill_contours, fill_contours_shaded, stroke_path};

/// Largest Lottie file accepted, before any parsing.
pub const MAX_LOTTIE_BYTES: usize = 10 * 1024 * 1024;
/// Largest JSON nesting depth accepted.
pub const MAX_JSON_DEPTH: usize = 128;
/// Largest number of JSON value tokens accepted.
pub const MAX_JSON_ELEMENTS: usize = 250_000;
/// Largest composition width or height in pixels.
pub const MAX_COMPOSITION_DIM: u32 = 4096;
/// Largest accepted frame rate.
pub const MAX_FRAME_RATE: f64 = 240.0;
/// Largest span between the first and last composition frame.
pub const MAX_COMPOSITION_FRAMES: f64 = 100_000.0;
/// Largest number of layers accepted.
pub const MAX_LAYERS: usize = 5_000;
/// Largest number of shape nodes accepted in one composition.
pub const MAX_SHAPE_NODES: usize = 100_000;
/// Largest repeater copy count accepted.
pub const MAX_REPEATER_COPIES: f64 = 256.0;
/// Largest precomposition nesting depth accepted.
const MAX_COMPOSITION_NESTING: usize = 64;
/// Triangles one animation may draw in one frame before it is skipped.
pub const MAX_TRIANGLES_PER_FRAME: usize = 120_000;

/// Longest edge, in points, used when shading a gradient fill.
const GRADIENT_MAX_EDGE: f32 = 12.0;
/// Largest number of flattened points kept from one animation path.
const MAX_FLATTEN_POINTS: usize = 4_000;
/// Largest number of subpaths kept from one animation path.
const MAX_CONTOURS: usize = 512;

/// Metadata about a parsed animation, used to size objects and select frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LottieInfo {
    /// Authored width in pixels.
    pub width: u32,
    /// Authored height in pixels.
    pub height: u32,
    /// Frames per second.
    pub frame_rate: f64,
    /// First frame.
    pub start_frame: f64,
    /// One past the last frame.
    pub end_frame: f64,
    /// Length in milliseconds at normal speed.
    pub duration_ms: u64,
}

/// Parse a `.json` or `.lottie` file into a composition under the input limits.
///
/// # Errors
/// Returns a human-readable reason when the file is too large, the archive has
/// no animation, the JSON is too deep or too large, or velato rejects it.
pub fn parse_composition(bytes: &[u8]) -> Result<Composition, String> {
    if bytes.len() > MAX_LOTTIE_BYTES {
        return Err(format!(
            "file is too large ({} bytes, max {})",
            bytes.len(),
            MAX_LOTTIE_BYTES
        ));
    }
    let json = if is_zip(bytes) {
        dotlottie::read_lottie_animation(bytes)?
    } else {
        bytes.to_vec()
    };
    check_json_limits(&json)?;
    let composition = Composition::from_slice(&json).map_err(|e| format!("invalid Lottie: {e}"))?;
    check_composition_limits(&composition)?;
    Ok(composition)
}

/// Parse only the metadata needed to size and time an animation.
///
/// # Errors
/// Same reasons as [`parse_composition`].
pub fn parse_info(bytes: &[u8]) -> Result<LottieInfo, String> {
    let composition = parse_composition(bytes)?;
    info_of(&composition).ok_or_else(|| "Lottie has no drawable frames".to_string())
}

/// Metadata of an already parsed composition.
#[must_use]
pub fn info_of(composition: &Composition) -> Option<LottieInfo> {
    let width = u32::try_from(composition.width).ok()?;
    let height = u32::try_from(composition.height).ok()?;
    let frame_rate = composition.frame_rate;
    let start = composition.frames.start;
    let end = composition.frames.end;
    if !frame_rate.is_finite()
        || frame_rate <= 0.0
        || frame_rate > MAX_FRAME_RATE
        || !start.is_finite()
        || !end.is_finite()
        || end <= start
    {
        return None;
    }
    let seconds = (end - start) / frame_rate;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let duration_ms = (seconds * 1000.0).round().clamp(1.0, u64::MAX as f64) as u64;
    Some(LottieInfo {
        width,
        height,
        frame_rate,
        start_frame: start,
        end_frame: end,
        duration_ms,
    })
}

/// Frame to draw for `elapsed_ms` at `speed`, looping or holding the last frame.
#[must_use]
pub fn frame_at(info: &LottieInfo, elapsed_ms: u64, speed: f32, loop_playback: bool) -> f64 {
    let speed = coursecraft_course::clamp_lottie_speed(speed) as f64;
    let elapsed = elapsed_ms as f64 / 1000.0;
    let start = info.start_frame;
    let span = info.end_frame - info.start_frame;
    if !span.is_finite() || span <= 0.0 {
        return start;
    }
    let raw = elapsed * speed * info.frame_rate;
    if !raw.is_finite() {
        return start;
    }
    if loop_playback {
        start + raw.rem_euclid(span)
    } else {
        // `end_frame` is exclusive in velato's layer visibility test, so hold
        // just below it rather than at the empty end frame.
        let last = (info.end_frame - 1e-3).max(start);
        (start + raw).min(last)
    }
}

/// Paint `composition` fitted inside `dest`, preserving its aspect ratio.
///
/// Returns `false` when the triangle budget was exceeded and drawing stopped.
pub fn paint_composition(
    painter: &Painter,
    dest: Rect,
    composition: &Composition,
    frame: f64,
    opacity: f32,
    pixels_per_point: f32,
) -> bool {
    let Some(info) = info_of(composition) else {
        return true;
    };
    if !dest.is_positive()
        || ![dest.min.x, dest.min.y, dest.max.x, dest.max.y]
            .iter()
            .all(|value| value.is_finite())
    {
        return true;
    }
    let aspect = info.width as f32 / info.height as f32;
    if !aspect.is_finite() || aspect <= 0.0 {
        return true;
    }
    let target = crate::paint::image_dest_rect(dest, (info.width as f32, info.height as f32), None)
        .unwrap_or(dest);
    if !target.is_positive() {
        return true;
    }
    let sx = target.width() as f64 / info.width as f64;
    let sy = target.height() as f64 / info.height as f64;
    if !(sx.is_finite() && sy.is_finite() && sx > 0.0 && sy > 0.0) {
        return true;
    }
    let base = Affine::translate((target.min.x as f64, target.min.y as f64))
        * Affine::scale_non_uniform(sx, sy);
    let alpha = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0) as f64
    } else {
        1.0
    };
    let mut sink = EpaintSink::new(painter.clone(), base, alpha, pixels_per_point);
    Renderer::new().append(composition, frame, Affine::IDENTITY, 1.0, &mut sink);
    sink.warned
}

/// Whether two bytes start a ZIP archive (a `.lottie` file).
fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

// ---------------------------------------------------------------- JSON limits

/// Reject deeply nested or very large JSON before velato's parser sees it.
fn check_json_limits(bytes: &[u8]) -> Result<(), String> {
    let mut depth = 0usize;
    let mut elements = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut expect_value = true;
    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                if expect_value {
                    elements = elements.saturating_add(1);
                    expect_value = false;
                }
            }
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > MAX_JSON_DEPTH {
                    return Err(format!("JSON is nested deeper than {MAX_JSON_DEPTH}"));
                }
                elements = elements.saturating_add(1);
                expect_value = true;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                expect_value = false;
            }
            b',' => expect_value = true,
            b':' => expect_value = true,
            b'-' | b'0'..=b'9' | b't' | b'f' | b'n' if expect_value => {
                elements = elements.saturating_add(1);
                expect_value = false;
            }
            _ => {}
        }
        if elements > MAX_JSON_ELEMENTS {
            return Err(format!("JSON has more than {MAX_JSON_ELEMENTS} elements"));
        }
    }
    if in_string {
        return Err("JSON string is not closed".to_string());
    }
    if depth != 0 {
        return Err("JSON is not balanced".to_string());
    }
    Ok(())
}

fn check_composition_limits(composition: &Composition) -> Result<(), String> {
    let width = u32::try_from(composition.width).map_err(|_| "composition is too wide")?;
    let height = u32::try_from(composition.height).map_err(|_| "composition is too tall")?;
    if width == 0 || height == 0 || width > MAX_COMPOSITION_DIM || height > MAX_COMPOSITION_DIM {
        return Err(format!(
            "composition size {width}x{height} is outside 1..={MAX_COMPOSITION_DIM}"
        ));
    }
    let rate = composition.frame_rate;
    if !rate.is_finite() || rate <= 0.0 || rate > MAX_FRAME_RATE {
        return Err(format!("frame rate {rate} is outside 0..={MAX_FRAME_RATE}"));
    }
    let start = composition.frames.start;
    let end = composition.frames.end;
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Err("composition has no valid frame range".to_string());
    }
    if end - start > MAX_COMPOSITION_FRAMES {
        return Err(format!(
            "composition has more than {MAX_COMPOSITION_FRAMES} frames"
        ));
    }
    let mut layer_count = composition.layers.len();
    for layers in composition.assets.values() {
        layer_count = layer_count.saturating_add(layers.len());
    }
    if layer_count > MAX_LAYERS {
        return Err(format!("composition has more than {MAX_LAYERS} layers"));
    }
    let mut shape_nodes = 0usize;
    for layer in composition
        .layers
        .iter()
        .chain(composition.assets.values().flatten())
    {
        check_layer_limits(layer, &mut shape_nodes)?;
    }
    check_asset_cycles(composition)?;
    Ok(())
}

fn check_layer_limits(layer: &Layer, shape_nodes: &mut usize) -> Result<(), String> {
    if let Content::Shape(shapes) = &layer.content {
        check_shape_limits(shapes, shape_nodes)?;
    }
    Ok(())
}

fn check_shape_limits(shapes: &[LottieShape], shape_nodes: &mut usize) -> Result<(), String> {
    for shape in shapes {
        *shape_nodes = shape_nodes.saturating_add(1);
        if *shape_nodes > MAX_SHAPE_NODES {
            return Err(format!(
                "composition has more than {MAX_SHAPE_NODES} shape nodes"
            ));
        }
        match shape {
            LottieShape::Group(inner, _) => check_shape_limits(inner, shape_nodes)?,
            LottieShape::Repeater(repeater) => {
                if !repeater_within_limits(repeater) {
                    return Err(format!(
                        "repeater makes more than {MAX_REPEATER_COPIES} copies"
                    ));
                }
            }
            LottieShape::Geometry(_) | LottieShape::Draw(_) | LottieShape::Trim(_) => {}
        }
    }
    Ok(())
}

fn repeater_within_limits(repeater: &Repeater) -> bool {
    let copies_ok = |value: &Value<f64>| match value {
        Value::Fixed(copies) => *copies <= MAX_REPEATER_COPIES,
        Value::Animated(animated) => animated
            .values
            .iter()
            .all(|copies| *copies <= MAX_REPEATER_COPIES),
    };
    match repeater {
        Repeater::Fixed(repeater) => (repeater.copies as f64) <= MAX_REPEATER_COPIES,
        Repeater::Animated(repeater) => copies_ok(&repeater.copies),
    }
}

fn check_asset_cycles(composition: &Composition) -> Result<(), String> {
    fn visit(
        name: &str,
        composition: &Composition,
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > MAX_COMPOSITION_NESTING {
            return Err("precompositions are nested too deeply".to_string());
        }
        if done.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_owned()) {
            return Err("precompositions reference each other in a cycle".to_string());
        }
        if let Some(layers) = composition.assets.get(name) {
            for layer in layers {
                if let Content::Instance { name: child, .. } = &layer.content {
                    visit(child, composition, visiting, done, depth.saturating_add(1))?;
                }
            }
        }
        visiting.remove(name);
        done.insert(name.to_owned());
        Ok(())
    }
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    for name in composition.assets.keys() {
        visit(name, composition, &mut visiting, &mut done, 0)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- painting

struct Contour {
    points: Vec<Pt>,
    closed: bool,
}

enum Frame {
    Layer { painter: Painter, alpha: f32 },
    Clip { painter: Painter },
}

struct EpaintSink<'a> {
    painter: Painter,
    base: Affine,
    alpha: f32,
    stack: Vec<Frame>,
    pixels_per_point: f32,
    triangles: usize,
    warned: bool,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> EpaintSink<'a> {
    fn new(painter: Painter, base: Affine, alpha: f64, pixels_per_point: f32) -> Self {
        Self {
            painter,
            base,
            alpha: alpha.clamp(0.0, 1.0) as f32,
            stack: Vec::new(),
            pixels_per_point,
            triangles: 0,
            warned: false,
            _marker: std::marker::PhantomData,
        }
    }

    fn add_mesh(&mut self, mesh: Mesh) {
        let triangles = mesh.indices.len() / 3;
        if triangles == 0 {
            return;
        }
        if self.triangles.saturating_add(triangles) > MAX_TRIANGLES_PER_FRAME {
            if !self.warned {
                log::warn!(
                    "Lottie frame exceeds {MAX_TRIANGLES_PER_FRAME} triangles; skipping the rest"
                );
                self.warned = true;
            }
            return;
        }
        self.triangles = self.triangles.saturating_add(triangles);
        self.painter.add(egui::Shape::mesh(mesh));
    }

    fn fill(&mut self, contours: &[Contour], affine: Affine, brush: &peniko::Brush) {
        if self.alpha <= 0.0 || contours.is_empty() {
            return;
        }
        match brush {
            peniko::Brush::Solid(color) => {
                let Some(color) = solid_color(*color) else {
                    return;
                };
                let color = multiply_alpha(color, self.alpha);
                if color.a() == 0 {
                    return;
                }
                let kept = non_zero_contours(contours);
                let mesh = fill_contours(&kept, color, self.pixels_per_point);
                self.add_mesh(mesh);
            }
            peniko::Brush::Gradient(gradient) => {
                let kept = non_zero_contours(contours);
                let alpha = self.alpha;
                let mesh = fill_contours_shaded(
                    &kept,
                    self.pixels_per_point,
                    GRADIENT_MAX_EDGE,
                    |point| {
                        let color = gradient_color(gradient, affine, point);
                        multiply_alpha(color, alpha)
                    },
                );
                self.add_mesh(mesh);
            }
            peniko::Brush::Image(_) => {}
        }
    }

    fn stroke(
        &mut self,
        contours: &[Contour],
        affine: Affine,
        brush: &peniko::Brush,
        stroke: &fixed::Stroke,
    ) {
        let scale = affine_scale(affine);
        let width = (stroke.width * scale) as f32;
        if !width.is_finite() || width <= 0.0 {
            return;
        }
        let color = match brush {
            peniko::Brush::Solid(color) => solid_color(*color),
            peniko::Brush::Gradient(gradient) => {
                let bbox = contour_bounds(contours);
                bbox.map(|rect| {
                    gradient_color(gradient, affine, (rect.center().x, rect.center().y))
                })
            }
            peniko::Brush::Image(_) => None,
        };
        let Some(color) = color else {
            return;
        };
        let color = multiply_alpha(color, self.alpha);
        if color.a() == 0 {
            return;
        }
        let dash = dash_style(stroke);
        for contour in contours {
            if contour.points.len() < 2 {
                continue;
            }
            let mesh = stroke_path(
                &contour.points,
                contour.closed,
                color,
                width,
                dash,
                self.pixels_per_point,
            );
            self.add_mesh(mesh);
        }
    }
}

impl RenderSink for EpaintSink<'_> {
    fn push_layer(
        &mut self,
        _blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        _transform: Affine,
        _shape: &impl kurbo::Shape,
    ) {
        self.stack.push(Frame::Layer {
            painter: self.painter.clone(),
            alpha: self.alpha,
        });
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.alpha = (self.alpha * alpha).clamp(0.0, 1.0);
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl kurbo::Shape) {
        self.stack.push(Frame::Clip {
            painter: self.painter.clone(),
        });
        let affine = self.base * transform;
        let clip = shape_bbox(shape, affine).map_or_else(
            || Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 0.0)),
            |rect| self.painter.clip_rect().intersect(rect),
        );
        self.painter = self.painter.with_clip_rect(clip);
    }

    fn pop_layer(&mut self) {
        match self.stack.pop() {
            Some(Frame::Layer { painter, alpha }) => {
                self.painter = painter;
                self.alpha = alpha;
            }
            Some(Frame::Clip { painter }) => self.painter = painter,
            None => {}
        }
    }

    fn draw(
        &mut self,
        stroke: Option<&fixed::Stroke>,
        transform: Affine,
        brush: &fixed::Brush,
        shape: &impl kurbo::Shape,
    ) {
        if self.alpha <= 0.0 {
            return;
        }
        let affine = self.base * transform;
        if !affine.is_finite() {
            return;
        }
        let tolerance = flatten_tolerance(affine, self.pixels_per_point);
        let contours = flatten_shape(shape, affine, tolerance);
        if contours.is_empty() {
            return;
        }
        match stroke {
            Some(stroke) => self.stroke(&contours, affine, brush, stroke),
            None => self.fill(&contours, affine, brush),
        }
    }

    fn draw_image(&mut self, _image: &velato::model::ImageAsset, _transform: Affine, _alpha: f64) {
        // Raster images inside a Lottie file are out of scope for this step.
    }
}

fn flatten_tolerance(affine: Affine, pixels_per_point: f32) -> f64 {
    let scale = affine_scale(affine).max(1e-6);
    let pixel = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        0.5 / f64::from(pixels_per_point)
    } else {
        0.5
    };
    (pixel / scale).clamp(1e-3, 4.0)
}

fn affine_scale(affine: Affine) -> f64 {
    let [a, b, c, d, _, _] = affine.as_coeffs();
    let sx = a.hypot(b);
    let sy = c.hypot(d);
    let scale = sx.max(sy);
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn shape_bbox(shape: &impl kurbo::Shape, affine: Affine) -> Option<Rect> {
    // Walk the path elements ourselves rather than using kurbo's
    // `bounding_box`, which documents a panic when a slice starts with
    // `ClosePath`. Control points overestimate a clip slightly, which is fine.
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut saw = false;
    let mut count = 0usize;
    let mut extend = |point: Point| {
        let p = affine * point;
        if p.x.is_finite() && p.y.is_finite() {
            min.0 = min.0.min(p.x);
            min.1 = min.1.min(p.y);
            max.0 = max.0.max(p.x);
            max.1 = max.1.max(p.y);
            saw = true;
        }
    };
    for element in shape.path_elements(0.5) {
        count = count.saturating_add(1);
        if count > MAX_FLATTEN_POINTS {
            break;
        }
        match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => extend(p),
            PathEl::QuadTo(c, p) => {
                extend(c);
                extend(p);
            }
            PathEl::CurveTo(c1, c2, p) => {
                extend(c1);
                extend(c2);
                extend(p);
            }
            PathEl::ClosePath => {}
        }
    }
    if !saw {
        return None;
    }
    Some(Rect::from_min_max(
        pos2(min.0 as f32, min.1 as f32),
        pos2(max.0 as f32, max.1 as f32),
    ))
}

fn flatten_shape(shape: &impl kurbo::Shape, affine: Affine, tolerance: f64) -> Vec<Contour> {
    let mut contours: Vec<Contour> = Vec::new();
    let mut current: Vec<Pt> = Vec::new();
    let mut total = 0usize;
    let push = |current: &mut Vec<Pt>, closed: bool, contours: &mut Vec<Contour>| {
        if current.len() >= if closed { 3 } else { 2 } {
            contours.push(Contour {
                points: std::mem::take(current),
                closed,
            });
        } else {
            current.clear();
        }
    };
    kurbo::flatten(shape.path_elements(tolerance), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            push(&mut current, false, &mut contours);
            if total < MAX_FLATTEN_POINTS {
                current.push(transform_point(affine, p));
                total = total.saturating_add(1);
            }
        }
        PathEl::LineTo(p) => {
            if current.is_empty() {
                if total < MAX_FLATTEN_POINTS {
                    current.push(transform_point(affine, p));
                    total = total.saturating_add(1);
                }
            } else if total < MAX_FLATTEN_POINTS {
                current.push(transform_point(affine, p));
                total = total.saturating_add(1);
            }
        }
        PathEl::ClosePath => {
            if let Some(&first) = current.first()
                && current.last().copied() != Some(first)
                && total < MAX_FLATTEN_POINTS
            {
                current.push(first);
                total = total.saturating_add(1);
            }
            push(&mut current, true, &mut contours);
        }
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {
            // `kurbo::flatten` converts curves to lines.
        }
    });
    push(&mut current, false, &mut contours);
    contours.truncate(MAX_CONTOURS);
    contours
}

fn transform_point(affine: Affine, point: Point) -> Pt {
    let p = affine * point;
    (p.x as f32, p.y as f32)
}

fn signed_area(points: &[Pt]) -> f32 {
    let mut sum = 0.0f32;
    for (i, &a) in points.iter().enumerate() {
        let Some(&b) = points.get((i + 1) % points.len()) else {
            continue;
        };
        sum += a.0 * b.1 - a.1 * b.0;
    }
    sum * 0.5
}

fn point_in_polygon(point: Pt, poly: &[Pt]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (i, &a) in poly.iter().enumerate() {
        let Some(&b) = poly.get((i + 1) % poly.len()) else {
            continue;
        };
        if (a.1 > point.1) != (b.1 > point.1) {
            let dy = b.1 - a.1;
            if dy.abs() <= f32::EPSILON {
                continue;
            }
            let x = a.0 + (point.1 - a.1) / dy * (b.0 - a.0);
            if point.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Drop contours that wind the same way as their nearest kept parent.
///
/// The result, filled with the even-odd triangulator, covers the same area as
/// non-zero filling. Same-direction nesting is already filled by its parent;
/// opposite-direction nesting is a hole.
fn non_zero_contours(contours: &[Contour]) -> Vec<Vec<Pt>> {
    if contours.len() <= 1 {
        return contours
            .iter()
            .map(|contour| contour.points.clone())
            .collect();
    }
    let mut order: Vec<usize> = (0..contours.len()).collect();
    order.sort_by(|&a, &b| {
        signed_area(&contours[b].points)
            .abs()
            .total_cmp(&signed_area(&contours[a].points).abs())
    });
    let mut kept: Vec<(usize, bool)> = Vec::new(); // (index, ccw)
    for index in order {
        let Some(contour) = contours.get(index) else {
            continue;
        };
        let Some(&sample) = contour.points.first() else {
            continue;
        };
        let ccw = signed_area(&contour.points) >= 0.0;
        let mut parent: Option<usize> = None;
        let mut parent_area = f32::INFINITY;
        for (kept_index, _) in &kept {
            let Some(other) = contours.get(*kept_index) else {
                continue;
            };
            let area = signed_area(&other.points).abs();
            if area < parent_area && point_in_polygon(sample, &other.points) {
                parent = Some(*kept_index);
                parent_area = area;
            }
        }
        let parent_ccw = parent
            .and_then(|parent| contours.get(parent))
            .map(|parent| signed_area(&parent.points) >= 0.0);
        if parent_ccw.is_some_and(|parent_ccw| parent_ccw == ccw) {
            continue;
        }
        kept.push((index, ccw));
    }
    kept.sort_by_key(|(index, _)| *index);
    kept.into_iter()
        .filter_map(|(index, _)| contours.get(index).map(|contour| contour.points.clone()))
        .collect()
}

fn contour_bounds(contours: &[Contour]) -> Option<Rect> {
    let mut min = (f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for contour in contours {
        for &(x, y) in &contour.points {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            min.0 = min.0.min(x);
            min.1 = min.1.min(y);
            max.0 = max.0.max(x);
            max.1 = max.1.max(y);
        }
    }
    (min.0.is_finite() && min.1.is_finite() && max.0.is_finite() && max.1.is_finite())
        .then(|| Rect::from_min_max(pos2(min.0, min.1), pos2(max.0, max.1)))
}

fn dash_style(stroke: &fixed::Stroke) -> coursecraft_course::DashStyle {
    let Some(pattern) = stroke.dash_pattern.first() else {
        return coursecraft_course::DashStyle::Solid;
    };
    if !pattern.is_finite() || *pattern <= 0.0 {
        return coursecraft_course::DashStyle::Solid;
    }
    let second = stroke.dash_pattern.get(1).copied().unwrap_or(0.0);
    if second.is_finite() && second >= 0.0 && (*pattern - second).abs() <= pattern * 0.25 {
        coursecraft_course::DashStyle::Dot
    } else {
        coursecraft_course::DashStyle::Dash
    }
}

fn solid_color(color: peniko::Color) -> Option<Color32> {
    let [r, g, b, a] = color.components;
    if ![r, g, b, a].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(Color32::from_rgba_unmultiplied(
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    ))
}

fn dynamic_color(color: DynamicColor) -> Color32 {
    solid_color(color.to_alpha_color::<Srgb>()).unwrap_or(Color32::TRANSPARENT)
}

fn multiply_alpha(color: Color32, alpha: f32) -> Color32 {
    if !alpha.is_finite() {
        return color;
    }
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let a = (f32::from(a) * alpha.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn sample_stops(stops: &peniko::ColorStops, t: f64) -> Color32 {
    let Some(first) = stops.first() else {
        return Color32::TRANSPARENT;
    };
    let t = if t.is_finite() { t } else { 0.0 };
    let first_offset = f64::from(first.offset);
    if t <= first_offset {
        return dynamic_color(first.color);
    }
    for pair in stops.as_slice().windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let (a_off, b_off) = (f64::from(a.offset), f64::from(b.offset));
        if t <= b_off {
            let span = b_off - a_off;
            let f = if span.abs() <= 1e-9 {
                1.0
            } else {
                ((t - a_off) / span).clamp(0.0, 1.0)
            };
            return mix_color(dynamic_color(a.color), dynamic_color(b.color), f as f32);
        }
    }
    stops
        .last()
        .map_or(dynamic_color(first.color), |stop| dynamic_color(stop.color))
}

fn mix_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(ar, br), mix(ag, bg), mix(ab, bb), mix(aa, ba))
}

fn gradient_color(gradient: &peniko::Gradient, affine: Affine, point: Pt) -> Color32 {
    let p = Point::new(point.0 as f64, point.1 as f64);
    let raw = match gradient.kind {
        peniko::GradientKind::Linear(position) => {
            let start = affine * position.start;
            let end = affine * position.end;
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let denom = dx * dx + dy * dy;
            if denom <= 1e-12 {
                0.0
            } else {
                ((p.x - start.x) * dx + (p.y - start.y) * dy) / denom
            }
        }
        peniko::GradientKind::Radial(position) => {
            let center = affine * position.end_center;
            let radius = f64::from(position.end_radius) * affine_scale(affine);
            if !radius.is_finite() || radius <= 1e-9 {
                0.0
            } else {
                let dx = p.x - center.x;
                let dy = p.y - center.y;
                (dx * dx + dy * dy).sqrt() / radius
            }
        }
        peniko::GradientKind::Sweep(position) => {
            let center = affine * position.center;
            let angle = (p.y - center.y).atan2(p.x - center.x);
            let start = f64::from(position.start_angle);
            let end = f64::from(position.end_angle);
            let span = end - start;
            if span.abs() <= 1e-9 {
                0.0
            } else {
                (angle - start) / span
            }
        }
    };
    let t = match gradient.extend {
        peniko::Extend::Pad => raw.clamp(0.0, 1.0),
        peniko::Extend::Repeat => raw.rem_euclid(1.0),
        peniko::Extend::Reflect => {
            let v = raw.rem_euclid(2.0);
            if v <= 1.0 { v } else { 2.0 - v }
        }
    };
    sample_stops(&gradient.stops, t)
}

// ---------------------------------------------------------------- .lottie archives

mod dotlottie {
    use std::io::{Cursor, Read};

    use super::MAX_LOTTIE_BYTES;

    /// Most entries a `.lottie` archive may list before it is refused.
    const MAX_ENTRIES: usize = 512;

    /// The animation JSON inside a `.lottie` archive: the manifest's
    /// `activeAnimationId` (or its first animation) when it names one that
    /// exists, otherwise the first `animations/*.json` entry. Sizes are checked
    /// against [`MAX_LOTTIE_BYTES`] before and while reading, so a zip bomb stops
    /// at the cap.
    pub fn read_lottie_animation(bytes: &[u8]) -> Result<Vec<u8>, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| "not a readable .lottie archive".to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err(format!(
                "the .lottie archive lists too many files (max {MAX_ENTRIES})"
            ));
        }
        let names: Vec<String> = archive
            .file_names()
            .filter_map(Result::ok)
            .map(|name| name.into_owned())
            .collect();
        let animations: Vec<&String> = names
            .iter()
            .filter(|name| {
                name.starts_with("animations/")
                    && name.to_ascii_lowercase().ends_with(".json")
                    && !name.ends_with('/')
            })
            .collect();
        let preferred = manifest_choice(&mut archive)
            .map(|id| format!("animations/{id}.json"))
            .filter(|path| animations.contains(&path));
        let chosen = preferred
            .or_else(|| animations.first().map(|name| (*name).clone()))
            .ok_or_else(|| "the .lottie archive has no animations/*.json entry".to_string())?;
        read_entry(&mut archive, &chosen)
    }

    fn manifest_choice(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Option<String> {
        let text = read_entry(archive, "manifest.json").ok()?;
        let manifest: serde_json::Value = serde_json::from_slice(&text).ok()?;
        let id = manifest
            .get("activeAnimationId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                manifest
                    .get("animations")?
                    .as_array()?
                    .first()?
                    .get("id")?
                    .as_str()
                    .map(str::to_owned)
            })?;
        let safe = !id.is_empty()
            && id.len() <= 128
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !id.contains("..");
        safe.then_some(id)
    }

    fn read_entry(
        archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
        name: &str,
    ) -> Result<Vec<u8>, String> {
        let entry = archive
            .by_name(name)
            .map_err(|_| format!("the .lottie archive can't read {name}"))?;
        if entry.size() > MAX_LOTTIE_BYTES as u64 {
            return Err(format!(
                "animation is too large ({} bytes, max {MAX_LOTTIE_BYTES})",
                entry.size()
            ));
        }
        let mut out = Vec::new();
        entry
            .take(MAX_LOTTIE_BYTES as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| format!("the .lottie archive can't read {name}"))?;
        if out.len() > MAX_LOTTIE_BYTES {
            return Err(format!(
                "animation is too large (max {MAX_LOTTIE_BYTES} bytes)"
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 30-frame, 100x100 animation: a red 20x20 rectangle moving from
    /// x=20 to x=80 across the composition.
    fn tiny_json() -> String {
        r#"{
          "v":"5.5.7","fr":30,"ip":0,"op":30,"w":100,"h":100,"nm":"tiny","ddd":0,
          "assets":[],
          "layers":[{
            "ddd":0,"ind":1,"ty":4,"nm":"Rect","sr":1,
            "ks":{
              "a":{"a":0,"k":[0,0]},
              "p":{"a":1,"k":[
                {"t":0,"s":[20,50],"e":[80,50],
                 "i":{"x":[0.5],"y":[0.5]},"o":{"x":[0.5],"y":[0.5]}},
                {"t":30,"s":[80,50]}
              ]},
              "s":{"a":0,"k":[100,100]},
              "r":{"a":0,"k":0},
              "o":{"a":0,"k":100}
            },
            "ao":0,
            "shapes":[{"ty":"gr","it":[
              {"ty":"rc","d":1,"s":{"a":0,"k":[20,20]},
               "p":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},
              {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1},
              {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},
               "s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
            ]}],
            "ip":0,"op":30,"st":0,"bm":0
          }]
        }"#
        .to_string()
    }

    fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, data) in entries {
                writer.start_file(*name, options).expect("entry");
                writer.write_all(data).expect("write entry");
            }
            writer.finish().expect("finish");
        }
        buffer.into_inner()
    }

    #[test]
    fn parses_a_tiny_animation() {
        let composition = parse_composition(tiny_json().as_bytes()).expect("valid tiny Lottie");
        let info = info_of(&composition).expect("info");
        assert_eq!((info.width, info.height), (100, 100));
        assert_eq!(info.frame_rate, 30.0);
        assert_eq!(info.duration_ms, 1000);
        assert_eq!(info.start_frame, 0.0);
        assert_eq!(info.end_frame, 30.0);
    }

    #[test]
    fn reads_a_lottie_zip_with_a_manifest() {
        let json = tiny_json();
        let manifest = br#"{"version":"1.0","activeAnimationId":"tiny",
            "animations":[{"id":"tiny","path":"animations/tiny.json"}]}"#;
        let archive = stored_zip(&[
            ("manifest.json", manifest.as_slice()),
            ("animations/tiny.json", json.as_bytes()),
        ]);
        let composition = parse_composition(&archive).expect("valid .lottie");
        assert_eq!(info_of(&composition).map(|i| i.duration_ms), Some(1000));
    }

    #[test]
    fn reads_a_compressed_lottie_and_follows_the_active_animation() {
        use std::io::Write;
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer
                .start_file("manifest.json", options)
                .expect("manifest");
            writer
                .write_all(br#"{"activeAnimationId":"second","animations":[{"id":"first"},{"id":"second"}]}"#)
                .expect("write manifest");
            writer
                .start_file("animations/first.json", options)
                .expect("first");
            writer.write_all(b"not json").expect("write first");
            writer
                .start_file("animations/second.json", options)
                .expect("second");
            writer
                .write_all(tiny_json().as_bytes())
                .expect("write second");
            writer.finish().expect("finish");
        }
        let composition = parse_composition(buffer.get_ref()).expect("valid compressed .lottie");
        assert_eq!(info_of(&composition).map(|i| i.duration_ms), Some(1000));
    }

    #[test]
    fn limits_reject_hostile_input_without_panicking() {
        assert!(parse_composition(&vec![0u8; MAX_LOTTIE_BYTES + 1]).is_err());
        assert!(parse_composition(b"not a zip and not json").is_err());
        assert!(parse_composition(b"PK\x03\x04garbage").is_err());
        let deep = format!(
            "{}{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        assert!(parse_composition(deep.as_bytes()).is_err());
        let empty_archive = stored_zip(&[("manifest.json", b"{}")]);
        assert!(parse_composition(&empty_archive).is_err());
    }

    #[test]
    fn frame_selection_loops_or_holds_the_last_frame() {
        let composition = parse_composition(tiny_json().as_bytes()).expect("valid Lottie");
        let info = info_of(&composition).expect("info");
        assert!((frame_at(&info, 0, 1.0, true) - 0.0).abs() < 1e-6);
        assert!((frame_at(&info, 500, 1.0, true) - 15.0).abs() < 1e-6);
        // Twice the speed covers the whole animation in half a second, so the
        // loop wraps back to frame 0.
        assert!(frame_at(&info, 500, 2.0, true) < 1e-3);
        // At exactly one second a loop is back at frame 0, while a non-looping
        // animation holds the last drawable frame.
        assert!(frame_at(&info, 1000, 1.0, true) < 1e-3);
        let held = frame_at(&info, 1000, 1.0, false);
        assert!(held > 29.9 && held < 30.0);
    }
}
