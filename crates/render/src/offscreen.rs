//! Offscreen CPU slide rasteriser for CourseCraft (L3).
//!
//! Produces RGBA images from slides without opening a window or needing a GPU.
//! Works in both authoring (static/editor) and player (runtime simulation) modes.

use std::collections::HashMap;

use coursecraft_course::{Course, Slide};
use coursecraft_logic::{Input, Player};
use coursecraft_media::MediaError;
use egui::epaint::{ClippedPrimitive, Primitive};
use egui::{ImageData, Pos2, Rect, TextureId, vec2};

use crate::compose::{compose, compose_authoring, compose_object_with_theme};
use crate::font::{install_course_fonts, install_fonts};
use crate::layout::SlideTransform;
use crate::media::{MediaCache, MediaSource};
use crate::paint::{Highlight, paint};

/// Minimum allowed rendering width in pixels.
pub const MIN_RENDER_WIDTH: u32 = 16;
/// Maximum allowed rendering width in pixels.
pub const MAX_RENDER_WIDTH: u32 = 4096;
/// Default rendering width in pixels.
pub const DEFAULT_RENDER_WIDTH: u32 = 1280;

/// Tolerance, in pixel-area units, for a pixel centre lying on a triangle edge.
const EDGE_EPSILON: f32 = 1e-3;

/// Rendering mode for offscreen rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Authoring view: static slide composition matching the editor canvas.
    #[default]
    Authoring,
    /// Player view: simulated runtime state after ticking to a timestamp.
    Player,
}

/// Options controlling offscreen slide rasterisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Target image width in pixels (clamped to 16..=4096, default 1280).
    pub width: u32,
    /// Rendering mode (authoring or player).
    pub mode: RenderMode,
    /// Additional layer IDs to compose on top.
    pub layers: Vec<String>,
    /// Milliseconds to tick to in player mode (default: slide duration).
    pub at_ms: Option<u32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: DEFAULT_RENDER_WIDTH,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        }
    }
}

/// In-memory RGBA8 image result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw unmultiplied/premultiplied RGBA pixel bytes (length is `width * height * 4`).
    pub pixels: Vec<u8>,
}

/// Errors produced during offscreen slide rendering.
#[derive(Debug)]
pub enum RenderError {
    /// The specified slide was not found in the course.
    SlideNotFound(String),
    /// The course is not valid or playable in player mode.
    NotPlayable(Vec<String>),
    /// Slide or image dimensions are invalid or hostile.
    InvalidDimensions(String),
    /// Image encoding or media decode error.
    Media(MediaError),
    /// General rendering failure.
    Other(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SlideNotFound(id) => write!(f, "slide not found: {id}"),
            Self::NotPlayable(issues) => write!(f, "course is not playable: {issues:?}"),
            Self::InvalidDimensions(msg) => write!(f, "invalid dimensions: {msg}"),
            Self::Media(err) => write!(f, "media error: {err}"),
            Self::Other(msg) => write!(f, "render error: {msg}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Media(err) => Some(err),
            _ => None,
        }
    }
}

impl From<MediaError> for RenderError {
    fn from(err: MediaError) -> Self {
        Self::Media(err)
    }
}

struct StoredTexture {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 4]>,
    /// True when the texture was uploaded with repeat wrapping.
    repeat: bool,
}

fn apply_image_delta(
    textures: &mut HashMap<TextureId, StoredTexture>,
    id: TextureId,
    delta: &egui::epaint::ImageDelta,
) {
    let ImageData::Color(ref image) = delta.image;
    let img_w = image.size[0];
    let img_h = image.size[1];

    match delta.pos {
        None => {
            let mut pixels = Vec::with_capacity(img_w.saturating_mul(img_h));
            for c in &image.pixels {
                pixels.push([c.r(), c.g(), c.b(), c.a()]);
            }
            textures.insert(
                id,
                StoredTexture {
                    width: img_w,
                    height: img_h,
                    pixels,
                    repeat: false,
                },
            );
        }
        Some([x, y]) => {
            if let Some(tex) = textures.get_mut(&id) {
                for py in 0..img_h {
                    let dst_y = y.saturating_add(py);
                    if dst_y >= tex.height {
                        continue;
                    }
                    for px in 0..img_w {
                        let dst_x = x.saturating_add(px);
                        if dst_x >= tex.width {
                            continue;
                        }
                        if let Some(src_c) = image.pixels.get(py * img_w + px) {
                            let dst_idx = dst_y * tex.width + dst_x;
                            if let Some(dst_p) = tex.pixels.get_mut(dst_idx) {
                                *dst_p = [src_c.r(), src_c.g(), src_c.b(), src_c.a()];
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sample_texture(tex: &StoredTexture, u: f32, v: f32) -> [f32; 4] {
    if tex.width == 0 || tex.height == 0 {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let (u, v) = if tex.repeat {
        (u.rem_euclid(1.0), v.rem_euclid(1.0))
    } else {
        (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
    };
    let fx = (u * tex.width as f32 - 0.5).max(0.0);
    let fy = (v * tex.height as f32 - 0.5).max(0.0);
    let x0 = (fx.floor() as usize).min(tex.width.saturating_sub(1));
    let x1 = (x0 + 1).min(tex.width.saturating_sub(1));
    let y0 = (fy.floor() as usize).min(tex.height.saturating_sub(1));
    let y1 = (y0 + 1).min(tex.height.saturating_sub(1));
    let wx = fx - fx.floor();
    let wy = fy - fy.floor();

    let get_texel = |x: usize, y: usize| -> [f32; 4] {
        let idx = y * tex.width + x;
        if let Some(p) = tex.pixels.get(idx) {
            [
                p[0] as f32 * (1.0 / 255.0),
                p[1] as f32 * (1.0 / 255.0),
                p[2] as f32 * (1.0 / 255.0),
                p[3] as f32 * (1.0 / 255.0),
            ]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        }
    };

    let c00 = get_texel(x0, y0);
    let c10 = get_texel(x1, y0);
    let c01 = get_texel(x0, y1);
    let c11 = get_texel(x1, y1);

    let mut out = [0.0; 4];
    for i in 0..4 {
        let top = (1.0 - wx) * c00[i] + wx * c10[i];
        let bot = (1.0 - wx) * c01[i] + wx * c11[i];
        out[i] = (1.0 - wy) * top + wy * bot;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn rasterise_triangle(
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    clip_rect: Rect,
    opt_tex: Option<&StoredTexture>,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    let p0 = v0.pos;
    let p1 = v1.pos;
    let p2 = v2.pos;

    if !p0.x.is_finite()
        || !p0.y.is_finite()
        || !p1.x.is_finite()
        || !p1.y.is_finite()
        || !p2.x.is_finite()
        || !p2.y.is_finite()
    {
        return;
    }

    let mut area2 = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
    if area2.abs() < 1e-6 {
        return;
    }

    let (v1, v2) = if area2 < 0.0 {
        area2 = -area2;
        (v2, v1)
    } else {
        (v1, v2)
    };

    let (p1, p2) = (v1.pos, v2.pos);
    let min_x = p0.x.min(p1.x).min(p2.x);
    let max_x = p0.x.max(p1.x).max(p2.x);
    let min_y = p0.y.min(p1.y).min(p2.y);
    let max_y = p0.y.max(p1.y).max(p2.y);

    let clip_min_x = clip_rect.min.x.max(0.0);
    let clip_max_x = clip_rect.max.x.min(width as f32);
    let clip_min_y = clip_rect.min.y.max(0.0);
    let clip_max_y = clip_rect.max.y.min(height as f32);

    let bbox_min_x = min_x.floor().max(clip_min_x);
    let bbox_max_x = max_x.ceil().min(clip_max_x);
    let bbox_min_y = min_y.floor().max(clip_min_y);
    let bbox_max_y = max_y.ceil().min(clip_max_y);

    if bbox_min_x >= bbox_max_x || bbox_min_y >= bbox_max_y {
        return;
    }

    let x_start = bbox_min_x as usize;
    let x_end = (bbox_max_x as usize).min(width as usize);
    let y_start = bbox_min_y as usize;
    let y_end = (bbox_max_y as usize).min(height as usize);

    let inv_area2 = 1.0 / area2;
    let dw0_dx = -(p2.y - p1.y) * inv_area2;
    let dw1_dx = -(p0.y - p2.y) * inv_area2;
    let dw2_dx = -(p1.y - p0.y) * inv_area2;

    let width_usize = width as usize;

    // Inclusive edge test: a pixel centre exactly on an edge belongs to the
    // triangle on *both* sides. That keeps fills watertight along shared edges
    // and also keeps boundary pixels from falling between the fill and its
    // feather fringe (which used to leave white nicks on long diagonals).
    // Edge values are tested unnormalised (pixel-area units) so both triangles
    // see the same magnitude.
    let covers = |w: f32| w * area2 >= -EDGE_EPSILON;

    for y in y_start..y_end {
        let py = y as f32 + 0.5;
        let px_start = x_start as f32 + 0.5;

        let mut w0 = ((p2.x - p1.x) * (py - p1.y) - (p2.y - p1.y) * (px_start - p1.x)) * inv_area2;
        let mut w1 = ((p0.x - p2.x) * (py - p2.y) - (p0.y - p2.y) * (px_start - p2.x)) * inv_area2;
        let mut w2 = ((p1.x - p0.x) * (py - p0.y) - (p1.y - p0.y) * (px_start - p0.x)) * inv_area2;

        let row_offset = (y * width_usize) * 4;

        for x in x_start..x_end {
            if covers(w0) && covers(w1) && covers(w2) {
                let mut b0 = w0.max(0.0);
                let mut b1 = w1.max(0.0);
                let mut b2 = w2.max(0.0);
                let sum = b0 + b1 + b2;
                if sum > 0.0 {
                    let inv_sum = 1.0 / sum;
                    b0 *= inv_sum;
                    b1 *= inv_sum;
                    b2 *= inv_sum;
                }

                let c0 = v0.color;
                let c1 = v1.color;
                let c2 = v2.color;
                let vr =
                    (b0 * c0.r() as f32 + b1 * c1.r() as f32 + b2 * c2.r() as f32) * (1.0 / 255.0);
                let vg =
                    (b0 * c0.g() as f32 + b1 * c1.g() as f32 + b2 * c2.g() as f32) * (1.0 / 255.0);
                let vb =
                    (b0 * c0.b() as f32 + b1 * c1.b() as f32 + b2 * c2.b() as f32) * (1.0 / 255.0);
                let va =
                    (b0 * c0.a() as f32 + b1 * c1.a() as f32 + b2 * c2.a() as f32) * (1.0 / 255.0);

                if va > 0.0 {
                    let u = b0 * v0.uv.x + b1 * v1.uv.x + b2 * v2.uv.x;
                    let v = b0 * v0.uv.y + b1 * v1.uv.y + b2 * v2.uv.y;

                    let tex_sample = if let Some(tex) = opt_tex {
                        sample_texture(tex, u, v)
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    };

                    let src_r = vr * tex_sample[0];
                    let src_g = vg * tex_sample[1];
                    let src_b = vb * tex_sample[2];
                    let src_a = va * tex_sample[3];

                    if src_a > 0.0 {
                        let inv_a = 1.0 - src_a;
                        let idx = row_offset + x * 4;
                        if let Some(slice) = pixels.get_mut(idx..idx + 4) {
                            let dst_r = slice[0] as f32 * (1.0 / 255.0);
                            let dst_g = slice[1] as f32 * (1.0 / 255.0);
                            let dst_b = slice[2] as f32 * (1.0 / 255.0);
                            let dst_a = slice[3] as f32 * (1.0 / 255.0);

                            let out_r = (src_r + dst_r * inv_a).clamp(0.0, 1.0);
                            let out_g = (src_g + dst_g * inv_a).clamp(0.0, 1.0);
                            let out_b = (src_b + dst_b * inv_a).clamp(0.0, 1.0);
                            let out_a = (src_a + dst_a * inv_a).clamp(0.0, 1.0);

                            slice[0] = (out_r * 255.0).round() as u8;
                            slice[1] = (out_g * 255.0).round() as u8;
                            slice[2] = (out_b * 255.0).round() as u8;
                            slice[3] = (out_a * 255.0).round() as u8;
                        }
                    }
                }
            }
            w0 += dw0_dx;
            w1 += dw1_dx;
            w2 += dw2_dx;
        }
    }
}

/// Render an arbitrary egui UI to an in-memory RGBA pixel buffer using the CPU rasteriser.
///
/// Runs up to 6 passes until font and media textures stabilize, tessellates shapes,
/// and rasterises triangles into an RGBA pixel buffer.
///
/// `pixels_per_point` controls rasterisation scale (e.g. 1.0 or 2.0). The output
/// image dimensions are `(width * ppp) x (height * ppp)`.
///
/// If a palette has been installed via [`crate::design::apply_style`] in `setup`,
/// or if a colour is registered under `"coursecraft-clear-color"`, the background
/// buffer is initialised with that colour; otherwise it defaults to white.
///
/// # Errors
/// Returns [`RenderError::Other`] if the egui context fails to produce a valid frame.
pub fn render_ui(
    width: u32,
    height: u32,
    pixels_per_point: f32,
    setup: impl FnOnce(&egui::Context),
    ui: impl FnMut(&egui::Context),
) -> Result<RgbaImage, RenderError> {
    render_ui_at(width, height, pixels_per_point, None, setup, ui)
}

/// [`render_ui`] with an optional fixed input time, in seconds.
///
/// Passing a time keeps egui at that time on every internal pass so entrance
/// animations and transitions draw at the requested moment. `None` restores
/// the frame-stepped time used for still renders.
fn render_ui_at(
    width: u32,
    height: u32,
    pixels_per_point: f32,
    time: Option<f64>,
    setup: impl FnOnce(&egui::Context),
    mut ui: impl FnMut(&egui::Context),
) -> Result<RgbaImage, RenderError> {
    let width = width.clamp(MIN_RENDER_WIDTH, MAX_RENDER_WIDTH);
    let height = height.clamp(MIN_RENDER_WIDTH, MAX_RENDER_WIDTH);
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };

    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(ppp);
    install_fonts(&ctx);

    setup(&ctx);

    let screen_rect = Rect::from_min_size(Pos2::ZERO, vec2(width as f32, height as f32));
    let mut raw_input = egui::RawInput {
        screen_rect: Some(screen_rect),
        time: Some(0.0),
        predicted_dt: 1.0 / 60.0,
        ..Default::default()
    };

    let mut textures: HashMap<TextureId, StoredTexture> = HashMap::new();
    let mut last_output = None;

    for frame in 0..6 {
        raw_input.time = Some(match time {
            Some(seconds) if seconds.is_finite() => seconds,
            Some(_) => 0.0,
            None => frame as f64 * 0.1,
        });
        let output = ctx.run_pass(raw_input.clone(), |ctx| {
            ui(ctx);
        });

        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                apply_image_delta(&mut textures, *id, delta);
            }
            if let Some(meta) = ctx.tex_manager().read().meta(*id)
                && let Some(texture) = textures.get_mut(id)
            {
                texture.repeat = meta.options.wrap_mode != egui::TextureWrapMode::ClampToEdge;
            }
        }
        for id in &output.textures_delta.free {
            textures.remove(id);
        }

        let is_stable = frame > 0 && output.textures_delta.set.is_empty();
        let mut handled_output = output;
        handled_output.textures_delta.clear();
        last_output = Some(handled_output);
        if is_stable {
            break;
        }
    }

    let full_output = last_output.ok_or_else(|| {
        RenderError::Other("egui context failed to produce frame output".to_string())
    })?;

    let primitives = ctx.tessellate(full_output.shapes, ppp);

    let target_w = (width as f32 * ppp).round() as u32;
    let target_h = (height as f32 * ppp).round() as u32;
    let pixel_count = (target_w as usize).saturating_mul(target_h as usize);

    let start_color = ctx
        .data(|d| d.get_temp::<egui::Color32>(egui::Id::unique("coursecraft-clear-color")))
        .or_else(|| {
            ctx.data(|d| {
                d.get_temp::<crate::design::Palette>(egui::Id::unique("coursecraft-palette"))
                    .map(|p| p.app)
            })
        })
        .unwrap_or(egui::Color32::WHITE);

    let mut pixels = [
        start_color.r(),
        start_color.g(),
        start_color.b(),
        start_color.a(),
    ]
    .repeat(pixel_count);

    let scale_needed = (ppp - 1.0).abs() >= 1e-4;

    for ClippedPrimitive {
        clip_rect,
        primitive,
    } in primitives
    {
        let Primitive::Mesh(mesh) = primitive else {
            continue;
        };

        if !clip_rect.is_positive() {
            continue;
        }

        let opt_tex = textures.get(&mesh.texture_id);

        let final_clip_rect = if scale_needed {
            Rect::from_min_max(clip_rect.min * ppp, clip_rect.max * ppp)
        } else {
            clip_rect
        };

        for chunk in mesh.indices.as_chunks::<3>().0 {
            let Some(v0) = mesh.vertices.get(chunk[0] as usize) else {
                continue;
            };
            let Some(v1) = mesh.vertices.get(chunk[1] as usize) else {
                continue;
            };
            let Some(v2) = mesh.vertices.get(chunk[2] as usize) else {
                continue;
            };

            if scale_needed {
                let mut sv0 = *v0;
                sv0.pos = v0.pos * ppp;
                let mut sv1 = *v1;
                sv1.pos = v1.pos * ppp;
                let mut sv2 = *v2;
                sv2.pos = v2.pos * ppp;
                rasterise_triangle(
                    &sv0,
                    &sv1,
                    &sv2,
                    final_clip_rect,
                    opt_tex,
                    &mut pixels,
                    target_w,
                    target_h,
                );
            } else {
                rasterise_triangle(
                    v0,
                    v1,
                    v2,
                    final_clip_rect,
                    opt_tex,
                    &mut pixels,
                    target_w,
                    target_h,
                );
            }
        }
    }

    Ok(RgbaImage {
        width: target_w,
        height: target_h,
        pixels,
    })
}

/// Render a slide to an in-memory RGBA pixel buffer using the CPU rasteriser.
///
/// In authoring mode, static items and question previews are composed directly,
/// with requested `opts.layers` composed on top (non-dimmed).
/// In player mode, interactive simulation runs up to `opts.at_ms` (clamped to slide
/// duration), and requested layers that are not already active are composed on top.
///
/// The provided `media` source should return assets synchronously for offscreen use.
///
/// # Errors
/// Returns [`RenderError::SlideNotFound`] if `slide_id` is not present,
/// [`RenderError::NotPlayable`] if player mode is requested on an unplayable course,
/// or [`RenderError::InvalidDimensions`] if slide or target dimensions are non-positive.
pub fn render_slide_rgba(
    course: &Course,
    slide_id: &str,
    opts: &RenderOptions,
    media: &dyn MediaSource,
) -> Result<RgbaImage, RenderError> {
    let slide = course
        .slide(slide_id)
        .ok_or_else(|| RenderError::SlideNotFound(slide_id.to_string()))?;

    let slide_w = course.slide_size.w as f32;
    let slide_h = course.slide_size.h as f32;
    if slide_w <= 0.0 || slide_h <= 0.0 || !slide_w.is_finite() || !slide_h.is_finite() {
        return Err(RenderError::InvalidDimensions(format!(
            "invalid course slide dimensions: {slide_w}x{slide_h}"
        )));
    }

    if opts.mode == RenderMode::Player {
        let mut player = Player::new(course).map_err(|issues| {
            RenderError::NotPlayable(
                issues
                    .into_iter()
                    .map(|i| format!("{}: {}", i.path, i.message))
                    .collect(),
            )
        })?;
        player.handle(&Input::Start);
        player.handle(&Input::GoTo {
            slide: slide_id.to_string(),
        });

        let duration = slide.duration_ms;
        let target_ms = opts.at_ms.unwrap_or(duration).min(duration);
        if target_ms > 0 {
            player.handle(&Input::Tick { ms: target_ms });
        }

        let mut extra_items = Vec::new();
        for layer_id in &opts.layers {
            if !player.state().visible_layers.contains(layer_id)
                && let Some(layer) = slide.layer(layer_id)
            {
                for obj in &layer.objects {
                    let current_state = player.object_state(&obj.id).unwrap_or(&obj.initial_state);
                    if current_state == "hidden" {
                        continue;
                    }
                    if let Some(item) =
                        compose_object_with_theme(obj, current_state, Some(&course.theme))
                    {
                        extra_items.push(item);
                    }
                }
            }
        }
        return render_player_rgba_with(&player, opts.width, &[], extra_items, media, None);
    }

    let mut items = compose_authoring(course, slide, None);
    for layer_id in &opts.layers {
        if let Some(layer) = slide.layer(layer_id) {
            for obj in &layer.objects {
                if let Some(mut item) =
                    compose_object_with_theme(obj, &obj.initial_state, Some(&course.theme))
                {
                    item.dimmed = obj.initial_state == "hidden";
                    items.push(item);
                }
            }
        }
    }
    render_items_rgba(course, slide, opts.width, items, media, None)
}

/// Render a live player's current slide to an in-memory RGBA pixel buffer.
///
/// Variables are substituted and object states applied exactly as the player
/// shows them. Items whose hit is an object id in `exclude` are left out, so a
/// certificate's Print button can print the slide without itself.
///
/// `time` is the input time in seconds egui sees, so entrance animations and
/// transitions draw at that moment. `None` keeps the still-render behaviour.
///
/// # Errors
/// Returns [`RenderError::SlideNotFound`] if the player has no current slide,
/// or [`RenderError::InvalidDimensions`] if the slide or target dimensions are
/// non-positive.
pub fn render_player_rgba(
    player: &Player<'_>,
    width: u32,
    exclude: &[String],
    media: &dyn MediaSource,
    time: Option<f64>,
) -> Result<RgbaImage, RenderError> {
    render_player_rgba_with(player, width, exclude, Vec::new(), media, time)
}

fn render_player_rgba_with(
    player: &Player<'_>,
    width: u32,
    exclude: &[String],
    extra_items: Vec<crate::compose::DrawItem>,
    media: &dyn MediaSource,
    time: Option<f64>,
) -> Result<RgbaImage, RenderError> {
    let course = player.course();
    let slide = player.current_slide().ok_or_else(|| {
        RenderError::SlideNotFound(format!("slide index {}", player.state().slide))
    })?;

    let mut items = compose(player);
    if !exclude.is_empty() {
        items.retain(|item| match &item.hit {
            crate::compose::Hit::Object(id) => !exclude.iter().any(|excluded| excluded == id),
            _ => true,
        });
    }
    items.extend(extra_items);
    render_items_rgba(course, slide, width, items, media, time)
}

/// Apply the still-frame marker and timer fixes, then rasterise `items`.
fn render_items_rgba(
    course: &Course,
    slide: &Slide,
    width: u32,
    mut items: Vec<crate::compose::DrawItem>,
    media: &dyn MediaSource,
    time: Option<f64>,
) -> Result<RgbaImage, RenderError> {
    let slide_w = course.slide_size.w as f32;
    let slide_h = course.slide_size.h as f32;
    if slide_w <= 0.0 || slide_h <= 0.0 || !slide_w.is_finite() || !slide_h.is_finite() {
        return Err(RenderError::InvalidDimensions(format!(
            "invalid course slide dimensions: {slide_w}x{slide_h}"
        )));
    }

    let width = width.clamp(MIN_RENDER_WIDTH, MAX_RENDER_WIDTH);
    let height = ((width as f32 * slide_h / slide_w).round() as u32)
        .clamp(MIN_RENDER_WIDTH, MAX_RENDER_WIDTH);

    let screen_rect = Rect::from_min_size(Pos2::ZERO, vec2(width as f32, height as f32));
    let transform = SlideTransform::fit(screen_rect, course.slide_size).ok_or_else(|| {
        RenderError::InvalidDimensions(format!("cannot fit slide in {width}x{height}"))
    })?;

    let mut media_cache = MediaCache::new();
    for item in &mut items {
        if let crate::compose::Visual::Marker { pulse, .. } = &mut item.visual {
            // Offscreen renders are a single still frame: no pulse ring.
            *pulse = false;
        }
        // Offscreen renders show a timer at its full duration, even in
        // player mode where the runtime may have counted down.
        if let crate::compose::Visual::Timer { runs, .. } = &mut item.visual
            && let crate::compose::Hit::Object(id) = &item.hit
            && let Some((object, _)) = slide.object(id)
            && let coursecraft_course::ObjectKind::Timer {
                duration_ms,
                format,
                ..
            } = object.kind
            && let Some(first) = runs.first_mut()
        {
            first.text = coursecraft_course::format_timer_text(duration_ms, format);
        }
    }
    render_ui_at(
        width,
        height,
        1.0,
        time,
        |ctx| install_course_fonts(ctx, course, media),
        |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            paint(
                &painter,
                &transform,
                &items,
                Highlight::default(),
                Some((&mut media_cache, media)),
                Some(&course.theme),
                Some(course.slide_background(slide)),
            );
        },
    )
}

/// Encode an [`RgbaImage`] to PNG bytes.
///
/// Delegates to `coursecraft-media`'s safe PNG encoder.
///
/// # Errors
/// Returns [`RenderError::Media`] if PNG encoding fails.
pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, RenderError> {
    coursecraft_media::encode_png_rgba(img.width, img.height, &img.pixels)
        .map_err(RenderError::Media)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape_fill::fill_contours;
    use crate::shape_geometry::{Pt, shape_contours};
    use coursecraft_course::ShapeKind;

    fn point_in_contours(point: Pt, contours: &[Vec<Pt>]) -> bool {
        let mut inside = false;
        for contour in contours {
            let mut in_this = false;
            for (i, &a) in contour.iter().enumerate() {
                let Some(&b) = contour.get((i + 1) % contour.len()) else {
                    continue;
                };
                if (a.1 > point.1) != (b.1 > point.1) {
                    let dy = b.1 - a.1;
                    if dy.abs() <= f32::EPSILON {
                        continue;
                    }
                    let x = a.0 + (point.1 - a.1) / dy * (b.0 - a.0);
                    if point.0 < x {
                        in_this = !in_this;
                    }
                }
            }
            if in_this {
                inside = !inside;
            }
        }
        inside
    }

    fn render_mesh(mesh: &egui::Mesh, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![255u8; (width as usize * height as usize * 4).max(1)];
        let clip = Rect::from_min_max(Pos2::ZERO, egui::pos2(width as f32, height as f32));
        for [a, b, c] in mesh.triangles() {
            let (Some(v0), Some(v1), Some(v2)) = (
                mesh.vertices.get(a as usize),
                mesh.vertices.get(b as usize),
                mesh.vertices.get(c as usize),
            ) else {
                continue;
            };
            rasterise_triangle(v0, v1, v2, clip, None, &mut pixels, width, height);
        }
        pixels
    }

    fn assert_interior_is_exact(kind: ShapeKind, w: f32, h: f32, ppp: f32) {
        let contours = shape_contours(kind, w, h);
        assert!(!contours.is_empty(), "{kind:?} has no contours");
        let fill = egui::Color32::from_rgb(10, 20, 30);
        let mesh = fill_contours(&contours, fill, ppp);
        let width = w as u32;
        let height = h as u32;
        let pixels = render_mesh(&mesh, width, height);
        let expected = [fill.r(), fill.g(), fill.b(), fill.a()];
        let mut sampled = 0usize;
        for y in 2..height.saturating_sub(2) {
            for x in 2..width.saturating_sub(2) {
                let point = (x as f32 + 0.5, y as f32 + 0.5);
                if !point_in_contours(point, &contours) {
                    continue;
                }
                sampled += 1;
                let idx = (y as usize * width as usize + x as usize) * 4;
                let Some(pixel) = pixels.get(idx..idx + 4) else {
                    continue;
                };
                assert_eq!(
                    pixel, expected,
                    "{kind:?} at {w}x{h}: interior pixel ({x}, {y}) is {pixel:?}, not {expected:?}"
                );
            }
        }
        assert!(sampled > 0, "{kind:?} had no interior pixels to sample");
    }

    #[test]
    fn interior_pixels_are_exactly_the_fill_without_fringe_seams() {
        assert_interior_is_exact(ShapeKind::SnipDiagonal, 200.0, 120.0, 0.0);
        assert_interior_is_exact(ShapeKind::SnipDiagonal, 200.0, 120.0, 1.0);
        assert_interior_is_exact(ShapeKind::Star5, 200.0, 120.0, 1.0);
        assert_interior_is_exact(ShapeKind::CrossMark, 200.0, 120.0, 1.0);
        assert_interior_is_exact(ShapeKind::ArrowRight, 200.0, 120.0, 1.0);
        assert_interior_is_exact(ShapeKind::ArrowUp, 120.0, 200.0, 1.0);
    }
}
