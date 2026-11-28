//! Slide painting using [`egui::Painter`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use coursecraft_course::{
    Align, Color, DashStyle, LineJoin, MarkerIcon, MeterStyle, Paragraph, ScrollBar, ShapeKind,
    Theme, TimerFormat, VAlign,
};
use egui::epaint::PathShape;
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Stroke, StrokeKind,
    pos2, vec2,
};

use crate::compose::{
    DrawItem, Hit, ResolvedFill, ResolvedOutline, ResolvedPath, ResolvedTextRun, ReviewMark, Visual,
};
use crate::icons::Icon;
use crate::layout::SlideTransform;
use crate::media::{LottieView, MediaCache, MediaSource, MediaView};
use crate::path_geometry::{PathMeshSpec, path_mesh};
use crate::shape_fill::{
    StrokeJoin, fill_contours, fill_contours_shaded, fill_contours_uv, stroke_contours_with_join,
};
use crate::shape_geometry::{shape_contours_adjusted, shape_text_rect};

/// Default maximum number of cached shape/gradient/outline meshes per [`egui::Context`].
pub const DEFAULT_SHAPE_MESH_CACHE_MAX: usize = 256;

const SHAPE_MESH_CACHE_ID: &str = "coursecraft-shape-mesh-cache";

fn f32_bits(v: f32) -> u32 {
    if v.is_nan() {
        f32::NAN.to_bits()
    } else if v == 0.0 {
        0.0_f32.to_bits()
    } else {
        v.to_bits()
    }
}

fn dash_tag(dash: DashStyle) -> u8 {
    match dash {
        DashStyle::Solid => 0,
        DashStyle::Dash => 1,
        DashStyle::Dot => 2,
        DashStyle::DashDot => 3,
        DashStyle::Custom => 4,
    }
}

fn join_tag(join: LineJoin) -> u8 {
    match join {
        LineJoin::Miter => 0,
        LineJoin::Round => 1,
        LineJoin::Bevel => 2,
    }
}

struct MeshHash {
    h1: u64,
    h2: u64,
}

impl MeshHash {
    fn new() -> Self {
        Self {
            h1: 0xcbf2_9ce4_8422_2325,
            h2: 0x8422_2325_cbf2_9ce4,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        const P1: u64 = 0x0000_0100_0000_01b3;
        const P2: u64 = 0x9e37_79b9_7f4a_7c15;
        for &b in bytes {
            self.h1 ^= u64::from(b);
            self.h1 = self.h1.wrapping_mul(P1);
            self.h2 = self.h2.wrapping_mul(P2) ^ u64::from(b).wrapping_add(0x51);
        }
    }

    fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }

    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        self.write_bytes(s.as_bytes());
    }

    fn finish(self) -> (u64, u64) {
        (self.h1, self.h2)
    }
}

#[derive(Clone, PartialEq, Eq)]
enum ShapeMeshStyleKey {
    Solid([u8; 4]),
    Linear {
        angle_bits: u32,
        stops: Vec<([u8; 4], u32)>,
        dimmed: bool,
        opacity_bits: u32,
    },
    Radial {
        center_bits: [u32; 2],
        stops: Vec<([u8; 4], u32)>,
        dimmed: bool,
        opacity_bits: u32,
    },
    Pattern {
        preset: String,
        fg: [u8; 4],
        bg: [u8; 4],
    },
    Outline {
        color: [u8; 4],
        width_bits: u32,
        dash: u8,
        dash_pattern: Option<Vec<u32>>,
        join: u8,
        miter_limit_bits: Option<u32>,
    },
}

enum BorrowedMeshStyle<'a> {
    Solid([u8; 4]),
    Linear {
        angle_bits: u32,
        stops: &'a [crate::compose::ResolvedGradientStop],
        dimmed: bool,
        opacity_bits: u32,
    },
    Radial {
        center_bits: [u32; 2],
        stops: &'a [crate::compose::ResolvedGradientStop],
        dimmed: bool,
        opacity_bits: u32,
    },
    Pattern {
        preset: &'a str,
        fg: [u8; 4],
        bg: [u8; 4],
    },
    Outline {
        color: [u8; 4],
        width_bits: u32,
        dash: u8,
        dash_pattern: Option<&'a [f32]>,
        join: u8,
        miter_limit_bits: Option<u32>,
    },
}

impl BorrowedMeshStyle<'_> {
    fn to_owned(&self) -> ShapeMeshStyleKey {
        match self {
            Self::Solid(color) => ShapeMeshStyleKey::Solid(*color),
            Self::Linear {
                angle_bits,
                stops,
                dimmed,
                opacity_bits,
            } => ShapeMeshStyleKey::Linear {
                angle_bits: *angle_bits,
                stops: stops
                    .iter()
                    .map(|s| (s.color.to_array(), f32_bits(s.position)))
                    .collect(),
                dimmed: *dimmed,
                opacity_bits: *opacity_bits,
            },
            Self::Radial {
                center_bits,
                stops,
                dimmed,
                opacity_bits,
            } => ShapeMeshStyleKey::Radial {
                center_bits: *center_bits,
                stops: stops
                    .iter()
                    .map(|s| (s.color.to_array(), f32_bits(s.position)))
                    .collect(),
                dimmed: *dimmed,
                opacity_bits: *opacity_bits,
            },
            Self::Pattern { preset, fg, bg } => ShapeMeshStyleKey::Pattern {
                preset: (*preset).to_owned(),
                fg: *fg,
                bg: *bg,
            },
            Self::Outline {
                color,
                width_bits,
                dash,
                dash_pattern,
                join,
                miter_limit_bits,
            } => ShapeMeshStyleKey::Outline {
                color: *color,
                width_bits: *width_bits,
                dash: *dash,
                dash_pattern: dash_pattern.map(|p| p.iter().copied().map(f32_bits).collect()),
                join: *join,
                miter_limit_bits: *miter_limit_bits,
            },
        }
    }

    fn matches(&self, owned: &ShapeMeshStyleKey) -> bool {
        match (self, owned) {
            (Self::Solid(a), ShapeMeshStyleKey::Solid(b)) => a == b,
            (
                Self::Linear {
                    angle_bits: a_ang,
                    stops: a_stops,
                    dimmed: a_dim,
                    opacity_bits: a_op,
                },
                ShapeMeshStyleKey::Linear {
                    angle_bits: b_ang,
                    stops: b_stops,
                    dimmed: b_dim,
                    opacity_bits: b_op,
                },
            ) => {
                a_ang == b_ang
                    && a_dim == b_dim
                    && a_op == b_op
                    && a_stops.len() == b_stops.len()
                    && a_stops
                        .iter()
                        .zip(b_stops.iter())
                        .all(|(s, (c, p))| s.color.to_array() == *c && f32_bits(s.position) == *p)
            }
            (
                Self::Radial {
                    center_bits: a_c,
                    stops: a_stops,
                    dimmed: a_dim,
                    opacity_bits: a_op,
                },
                ShapeMeshStyleKey::Radial {
                    center_bits: b_c,
                    stops: b_stops,
                    dimmed: b_dim,
                    opacity_bits: b_op,
                },
            ) => {
                a_c == b_c
                    && a_dim == b_dim
                    && a_op == b_op
                    && a_stops.len() == b_stops.len()
                    && a_stops
                        .iter()
                        .zip(b_stops.iter())
                        .all(|(s, (c, p))| s.color.to_array() == *c && f32_bits(s.position) == *p)
            }
            (
                Self::Pattern {
                    preset: a_p,
                    fg: a_fg,
                    bg: a_bg,
                },
                ShapeMeshStyleKey::Pattern {
                    preset: b_p,
                    fg: b_fg,
                    bg: b_bg,
                },
            ) => a_fg == b_fg && a_bg == b_bg && *a_p == b_p.as_str(),
            (
                Self::Outline {
                    color: a_col,
                    width_bits: a_w,
                    dash: a_dash,
                    dash_pattern: a_pat,
                    join: a_join,
                    miter_limit_bits: a_miter,
                },
                ShapeMeshStyleKey::Outline {
                    color: b_col,
                    width_bits: b_w,
                    dash: b_dash,
                    dash_pattern: b_pat,
                    join: b_join,
                    miter_limit_bits: b_miter,
                },
            ) => {
                a_col == b_col
                    && a_w == b_w
                    && a_dash == b_dash
                    && a_join == b_join
                    && a_miter == b_miter
                    && match (a_pat, b_pat) {
                        (None, None) => true,
                        (Some(ap), Some(bp)) => {
                            ap.len() == bp.len()
                                && ap.iter().zip(bp.iter()).all(|(&a, &b)| f32_bits(a) == b)
                        }
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    fn write_hash(&self, h: &mut MeshHash) {
        match self {
            Self::Solid(color) => {
                h.write_u8(1);
                h.write_bytes(color);
            }
            Self::Linear {
                angle_bits,
                stops,
                dimmed,
                opacity_bits,
            } => {
                h.write_u8(2);
                h.write_u32(*angle_bits);
                h.write_u8(u8::from(*dimmed));
                h.write_u32(*opacity_bits);
                h.write_u64(stops.len() as u64);
                for s in *stops {
                    h.write_bytes(&s.color.to_array());
                    h.write_u32(f32_bits(s.position));
                }
            }
            Self::Radial {
                center_bits,
                stops,
                dimmed,
                opacity_bits,
            } => {
                h.write_u8(3);
                h.write_u32(center_bits[0]);
                h.write_u32(center_bits[1]);
                h.write_u8(u8::from(*dimmed));
                h.write_u32(*opacity_bits);
                h.write_u64(stops.len() as u64);
                for s in *stops {
                    h.write_bytes(&s.color.to_array());
                    h.write_u32(f32_bits(s.position));
                }
            }
            Self::Pattern { preset, fg, bg } => {
                h.write_u8(4);
                h.write_str(preset);
                h.write_bytes(fg);
                h.write_bytes(bg);
            }
            Self::Outline {
                color,
                width_bits,
                dash,
                dash_pattern,
                join,
                miter_limit_bits,
            } => {
                h.write_u8(5);
                h.write_bytes(color);
                h.write_u32(*width_bits);
                h.write_u8(*dash);
                h.write_u8(*join);
                if let Some(m) = miter_limit_bits {
                    h.write_u8(1);
                    h.write_u32(*m);
                } else {
                    h.write_u8(0);
                }
                if let Some(pat) = dash_pattern {
                    h.write_u8(1);
                    h.write_u64(pat.len() as u64);
                    for &v in *pat {
                        h.write_u32(f32_bits(v));
                    }
                } else {
                    h.write_u8(0);
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ShapeMeshKey {
    kind: ShapeKind,
    width_bits: u32,
    height_bits: u32,
    ppp_bits: u32,
    adjust_bits: Vec<u32>,
    style: ShapeMeshStyleKey,
}

struct BorrowedShapeMeshKey<'a> {
    kind: ShapeKind,
    width_bits: u32,
    height_bits: u32,
    ppp_bits: u32,
    adjust: &'a [f32],
    style: BorrowedMeshStyle<'a>,
}

impl BorrowedShapeMeshKey<'_> {
    fn hash128(&self) -> (u64, u64) {
        let mut h = MeshHash::new();
        // ShapeKind derives Hash; hash its discriminant stably via Debug or std::hash.
        let mut kind_hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.kind, &mut kind_hasher);
        h.write_u64(std::hash::Hasher::finish(&kind_hasher));
        h.write_u32(self.width_bits);
        h.write_u32(self.height_bits);
        h.write_u32(self.ppp_bits);
        h.write_u64(self.adjust.len() as u64);
        for &v in self.adjust {
            h.write_u32(f32_bits(v));
        }
        self.style.write_hash(&mut h);
        h.finish()
    }

    fn to_owned(&self) -> ShapeMeshKey {
        ShapeMeshKey {
            kind: self.kind,
            width_bits: self.width_bits,
            height_bits: self.height_bits,
            ppp_bits: self.ppp_bits,
            adjust_bits: self.adjust.iter().copied().map(f32_bits).collect(),
            style: self.style.to_owned(),
        }
    }

    fn matches(&self, owned: &ShapeMeshKey) -> bool {
        self.kind == owned.kind
            && self.width_bits == owned.width_bits
            && self.height_bits == owned.height_bits
            && self.ppp_bits == owned.ppp_bits
            && self.adjust.len() == owned.adjust_bits.len()
            && self
                .adjust
                .iter()
                .zip(owned.adjust_bits.iter())
                .all(|(&a, &b)| f32_bits(a) == b)
            && self.style.matches(&owned.style)
    }
}

struct ShapeMeshEntry {
    key: ShapeMeshKey,
    mesh: Arc<egui::Mesh>,
    last_used: u64,
}

struct ShapeMeshCache {
    buckets: HashMap<(u64, u64), Vec<ShapeMeshEntry>>,
    total_entries: usize,
    max_entries: usize,
    tick: u64,
}

impl ShapeMeshCache {
    fn new(max_entries: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            total_entries: 0,
            max_entries,
            tick: 0,
        }
    }

    fn get(&mut self, hash: (u64, u64), key: &BorrowedShapeMeshKey<'_>) -> Option<Arc<egui::Mesh>> {
        if self.max_entries == 0 {
            return None;
        }
        let bucket = self.buckets.get_mut(&hash)?;
        for entry in bucket.iter_mut() {
            if key.matches(&entry.key) {
                self.tick = self.tick.saturating_add(1);
                entry.last_used = self.tick;
                return Some(Arc::clone(&entry.mesh));
            }
        }
        None
    }

    fn insert(&mut self, hash: (u64, u64), key: ShapeMeshKey, mesh: Arc<egui::Mesh>) {
        if self.max_entries == 0 {
            return;
        }
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        let bucket = self.buckets.entry(hash).or_default();
        if let Some(existing) = bucket.iter_mut().find(|e| e.key == key) {
            existing.mesh = mesh;
            existing.last_used = tick;
            return;
        }
        bucket.push(ShapeMeshEntry {
            key,
            mesh,
            last_used: tick,
        });
        self.total_entries = self.total_entries.saturating_add(1);
        self.evict_excess();
    }

    fn set_limit(&mut self, max_entries: usize) {
        self.max_entries = max_entries;
        if max_entries == 0 {
            self.clear();
        } else {
            self.evict_excess();
        }
    }

    fn clear(&mut self) {
        self.buckets.clear();
        self.total_entries = 0;
    }

    fn evict_excess(&mut self) {
        while self.total_entries > self.max_entries {
            let mut oldest: Option<(u64, (u64, u64), usize)> = None;
            for (&hash, bucket) in &self.buckets {
                for (idx, entry) in bucket.iter().enumerate() {
                    if oldest.is_none_or(|(best_tick, _, _)| entry.last_used < best_tick) {
                        oldest = Some((entry.last_used, hash, idx));
                    }
                }
            }
            let Some((_, hash, idx)) = oldest else {
                break;
            };
            let mut remove_bucket = false;
            if let Some(bucket) = self.buckets.get_mut(&hash) {
                if idx < bucket.len() {
                    bucket.swap_remove(idx);
                    self.total_entries = self.total_entries.saturating_sub(1);
                }
                remove_bucket = bucket.is_empty();
            }
            if remove_bucket {
                self.buckets.remove(&hash);
            }
        }
    }
}

fn shape_mesh_cache(ctx: &egui::Context) -> Arc<Mutex<ShapeMeshCache>> {
    let id = egui::Id::unique(SHAPE_MESH_CACHE_ID);
    if let Some(existing) = ctx.data(|d| d.get_temp::<Arc<Mutex<ShapeMeshCache>>>(id)) {
        return existing;
    }
    let created = Arc::new(Mutex::new(ShapeMeshCache::new(
        DEFAULT_SHAPE_MESH_CACHE_MAX,
    )));
    ctx.data_mut(|d| {
        if let Some(existing) = d.get_temp::<Arc<Mutex<ShapeMeshCache>>>(id) {
            return existing;
        }
        d.insert_temp(id, Arc::clone(&created));
        created
    })
}

/// Set the maximum number of entries retained in the per-context shape mesh cache.
///
/// Passing `0` disables caching and clears existing entries.
pub fn set_shape_mesh_cache_limit(ctx: &egui::Context, max_entries: usize) {
    let cache = shape_mesh_cache(ctx);
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.set_limit(max_entries);
}

/// Clear all entries from the per-context shape mesh cache.
pub fn clear_shape_mesh_cache(ctx: &egui::Context) {
    let cache = shape_mesh_cache(ctx);
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.clear();
}

#[cfg(test)]
pub(crate) fn shape_mesh_cache_limit(ctx: &egui::Context) -> usize {
    let cache = shape_mesh_cache(ctx);
    let guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.max_entries
}

fn cached_shape_mesh(
    ctx: &egui::Context,
    key: &BorrowedShapeMeshKey<'_>,
    build: impl FnOnce() -> egui::Mesh,
) -> Arc<egui::Mesh> {
    let hash = key.hash128();
    let cache = shape_mesh_cache(ctx);
    {
        let mut guard = match cache.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(hit) = guard.get(hash, key) {
            return hit;
        }
    }

    #[cfg(debug_assertions)]
    crate::media::test_counts::note_shape_mesh_miss();

    let mesh = Arc::new(build());
    let owned_key = key.to_owned();
    {
        let mut guard = match cache.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.insert(hash, owned_key, Arc::clone(&mesh));
    }
    mesh
}

fn paint_translated_mesh(painter: &Painter, base: &egui::Mesh, offset: egui::Vec2) {
    if base.is_empty() {
        return;
    }
    let mut mesh = base.clone();
    if offset != egui::Vec2::ZERO {
        for vertex in &mut mesh.vertices {
            vertex.pos += offset;
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

const DIM_FACTOR: f32 = 0.35;
pub const DEFAULT_BACKGROUND_COLOR: egui::Color32 = egui::Color32::WHITE;
/// Default longest edge, in screen points, used when shading a gradient fill.
const GRADIENT_MAX_EDGE: f32 = 12.0;

fn active_gradient_max_edge(ctx: &egui::Context) -> f32 {
    let edge = crate::design::paint_quality(ctx).gradient_max_edge;
    if edge.is_finite() && edge > 0.0 {
        edge
    } else {
        GRADIENT_MAX_EDGE
    }
}

fn active_max_effect_layers(ctx: &egui::Context) -> usize {
    crate::design::paint_quality(ctx).max_effect_layers.max(1)
}

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
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

fn sample_gradient(stops: &[crate::compose::ResolvedGradientStop], t: f32) -> Color32 {
    sample_gradient_side(stops, t, t)
}

fn sample_gradient_side(
    stops: &[crate::compose::ResolvedGradientStop],
    t: f32,
    tri_mid_t: f32,
) -> Color32 {
    let Some(first) = stops.first() else {
        return Color32::TRANSPARENT;
    };
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if t < first.position
        || (t <= first.position && (first.position <= 1e-6 || tri_mid_t <= first.position))
    {
        return first.color;
    }
    if let Some(last) = stops.last()
        && (t > last.position
            || (t >= last.position && (last.position >= 1.0 - 1e-6 || tri_mid_t >= last.position)))
    {
        return last.color;
    }
    let prefer_right = tri_mid_t > t + 1e-6;
    for pair in stops.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let in_span = if prefer_right {
            t < b.position
        } else {
            t <= b.position
        };
        if in_span {
            let span = b.position - a.position;
            if span.abs() <= 1e-6 {
                return if prefer_right { b.color } else { a.color };
            }
            return lerp_color(a.color, b.color, (t - a.position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

/// Paint a linear, radial, or pattern fill inside a shape's outline using [`ShapeMeshCache`].
fn paint_gradient_fill(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    style: &ResolvedFill,
    dimmed: bool,
    opacity: f32,
) {
    if !rect.is_positive()
        || ![rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .iter()
            .all(|value| value.is_finite())
    {
        return;
    }
    let style_key = match style {
        ResolvedFill::Linear { angle, stops } => BorrowedMeshStyle::Linear {
            angle_bits: f32_bits(*angle),
            stops,
            dimmed,
            opacity_bits: f32_bits(opacity),
        },
        ResolvedFill::Radial { center, stops } => BorrowedMeshStyle::Radial {
            center_bits: [f32_bits(center[0]), f32_bits(center[1])],
            stops,
            dimmed,
            opacity_bits: f32_bits(opacity),
        },
        ResolvedFill::Pattern { preset, fg, bg } => BorrowedMeshStyle::Pattern {
            preset,
            fg: apply_dim(*fg, dimmed, opacity).to_array(),
            bg: apply_dim(*bg, dimmed, opacity).to_array(),
        },
        ResolvedFill::Picture { .. } => return,
    };
    let ppp = painter.pixels_per_point();
    let key = BorrowedShapeMeshKey {
        kind,
        width_bits: f32_bits(rect.width()),
        height_bits: f32_bits(rect.height()),
        ppp_bits: f32_bits(ppp),
        adjust,
        style: style_key,
    };
    let ctx = painter.ctx();
    let mesh = cached_shape_mesh(ctx, &key, || {
        let local_rect = Rect::from_min_size(pos2(0.0, 0.0), rect.size());
        let contours =
            shape_contours_adjusted(kind, local_rect.width(), local_rect.height(), adjust);
        if contours.is_empty() {
            return egui::Mesh::default();
        }
        build_gradient_mesh(ctx, ppp, local_rect, &contours, style, dimmed, opacity)
            .unwrap_or_default()
    });
    paint_translated_mesh(painter, &mesh, rect.min.to_vec2());
}

fn pattern_pixel(preset: &str, x: usize, y: usize, n: usize) -> bool {
    match preset {
        "ltUpDiag" => (x + y).is_multiple_of(4),
        "wdUpDiag" => (x + y) % 8 < 3,
        "ltDnDiag" => (x + n - (y % n)).is_multiple_of(4),
        "wdDnDiag" => (x + n - (y % n)) % 8 < 3,
        "smCheck" => ((x / 4) + (y / 4)).is_multiple_of(2),
        "lgCheck" => ((x / 8) + (y / 8)).is_multiple_of(2),
        "dotGrid" => (x % 6) < 2 && (y % 6) < 2,
        "horz" | "ltHorz" => y.is_multiple_of(4),
        "wdHorz" => y % 8 < 3,
        "vert" | "ltVert" => x.is_multiple_of(4),
        "wdVert" => x % 8 < 3,
        "cross" => x.is_multiple_of(8) || y.is_multiple_of(8),
        "diagCross" => (x + y).is_multiple_of(8) || (x + n - (y % n)).is_multiple_of(8),
        "pct50" => (x + y).is_multiple_of(2),
        "pct10" => x.is_multiple_of(4) && y.is_multiple_of(4),
        "pct20" | "pct25" => x.is_multiple_of(2) && y.is_multiple_of(2),
        "pct75" | "pct80" => !(x.is_multiple_of(2) && y.is_multiple_of(2)),
        "pct90" => !(x.is_multiple_of(4) && y.is_multiple_of(4)),
        "dashVert" => x.is_multiple_of(4) && y % 8 < 4,
        "dashHorz" => y.is_multiple_of(4) && x % 8 < 4,
        "trellis" => (x + y) % 8 < 2 || (x + n - (y % n)) % 8 < 2,
        _ => (x + y).is_multiple_of(4),
    }
}

fn pattern_tile_texture(
    ctx: &egui::Context,
    preset: &str,
    fg: Color32,
    bg: Color32,
) -> (egui::TextureHandle, f32) {
    let n = if preset == "dotGrid" {
        12usize
    } else {
        16usize
    };
    let key = egui::Id::unique(("cc_patt_tile", preset, fg.to_array(), bg.to_array()));
    if let Some(handle) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return (handle, n as f32);
    }
    let mut pixels = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            pixels.push(if pattern_pixel(preset, x, y, n) {
                fg
            } else {
                bg
            });
        }
    }
    let image = egui::ColorImage {
        size: [n, n],
        source_size: egui::vec2(n as f32, n as f32),
        pixels,
    };
    let opts = egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        wrap_mode: egui::TextureWrapMode::Repeat,
        ..Default::default()
    };
    let handle = ctx.load_texture(format!("patt_{preset}"), image, opts);
    ctx.data_mut(|d| d.insert_temp(key, handle.clone()));
    (handle, n as f32)
}

fn build_gradient_mesh(
    ctx: &egui::Context,
    ppp: f32,
    rect: Rect,
    contours: &[Vec<crate::shape_geometry::Pt>],
    style: &ResolvedFill,
    dimmed: bool,
    opacity: f32,
) -> Option<egui::Mesh> {
    let centre = rect.center();
    let max_edge = active_gradient_max_edge(ctx);
    let mesh = match style {
        ResolvedFill::Linear { angle, stops } => {
            let radians = if angle.is_finite() {
                angle.to_radians()
            } else {
                0.0
            };
            let (sin, cos) = radians.sin_cos();
            let extent = (rect.width() * cos).abs() + (rect.height() * sin).abs();
            let mut split_ts = Vec::new();
            for pair in stops.windows(2) {
                if (pair[1].position - pair[0].position).abs() <= 1e-5
                    && split_ts
                        .last()
                        .is_none_or(|&prev: &f32| (prev - pair[0].position).abs() > 1e-5)
                {
                    split_ts.push(pair[0].position);
                }
            }
            let t_at = |point: crate::shape_geometry::Pt| {
                let distance = (point.0 - centre.x) * cos + (point.1 - centre.y) * sin;
                if extent > 1e-6 {
                    0.5 + distance / extent
                } else {
                    0.5
                }
            };
            crate::shape_fill::fill_contours_linear_shaded(
                contours,
                ppp,
                max_edge,
                &split_ts,
                t_at,
                |point, tri_mid_t| {
                    let t = t_at(point);
                    apply_dim(sample_gradient_side(stops, t, tri_mid_t), dimmed, opacity)
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
            fill_contours_shaded(contours, ppp, max_edge, |point| {
                let dx = point.0 - origin.x;
                let dy = point.1 - origin.y;
                let t = if max_distance > 1e-6 {
                    (dx * dx + dy * dy).sqrt() / max_distance
                } else {
                    0.0
                };
                apply_dim(sample_gradient(stops, t), dimmed, opacity)
            })
        }
        ResolvedFill::Picture { .. } => return None,
        ResolvedFill::Pattern { preset, fg, bg } => {
            let fg = apply_dim(*fg, dimmed, opacity);
            let bg = apply_dim(*bg, dimmed, opacity);
            let (handle, tile_px) = pattern_tile_texture(ctx, preset, fg, bg);
            let mut mesh = crate::shape_fill::fill_contours_uv(contours, ppp, |point| {
                (
                    (point.0 - rect.min.x) / tile_px,
                    (point.1 - rect.min.y) / tile_px,
                )
            });
            mesh.texture_id = handle.id();
            mesh
        }
    };
    Some(mesh)
}

fn paint_gradient_contours(
    painter: &Painter,
    rect: Rect,
    contours: &[Vec<crate::shape_geometry::Pt>],
    style: &ResolvedFill,
    dimmed: bool,
    opacity: f32,
) {
    if let Some(mesh) = build_gradient_mesh(
        painter.ctx(),
        painter.pixels_per_point(),
        rect,
        contours,
        style,
        dimmed,
        opacity,
    ) && !mesh.is_empty()
    {
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
    cover: bool,
    window: Rect,
    tile_scale: [f32; 2],
    scale: f32,
    dimmed: bool,
    opacity: f32,
    media: &mut Option<(&mut MediaCache, &dyn MediaSource)>,
) -> bool {
    let Some(contours) = screen_contours(rect, kind, adjust) else {
        return false;
    };
    paint_picture_contours(
        painter, rect, &contours, src, tile, cover, window, tile_scale, scale, dimmed, opacity,
        media,
    )
}

/// The part of a picture a shape shows. `window` is the kept part of the source as UV
/// fractions (the whole source, or a crop). With `cover` the window is cut in the middle
/// so its proportions match the shape's; without it the window is stretched.
///
/// Returns the whole source window when a size or the window cannot be used.
#[must_use]
pub fn picture_uv_window(
    window: Rect,
    tex_size: (f32, f32),
    shape_size: (f32, f32),
    cover: bool,
) -> Rect {
    let finite = |v: f32| v.is_finite() && v > 0.0;
    let window_ok = [window.min.x, window.min.y, window.max.x, window.max.y]
        .iter()
        .all(|v| v.is_finite())
        && window.width() > 0.0
        && window.height() > 0.0;
    if !window_ok {
        return FULL_UV;
    }
    if !cover
        || !(finite(tex_size.0)
            && finite(tex_size.1)
            && finite(shape_size.0)
            && finite(shape_size.1))
    {
        return window;
    }
    let source_aspect = (tex_size.0 * window.width()) / (tex_size.1 * window.height());
    let shape_aspect = shape_size.0 / shape_size.1;
    if source_aspect > shape_aspect {
        let kept = window.width() * shape_aspect / source_aspect;
        let x0 = window.center().x - kept * 0.5;
        Rect::from_min_max(pos2(x0, window.min.y), pos2(x0 + kept, window.max.y))
    } else {
        let kept = window.height() * source_aspect / shape_aspect;
        let y0 = window.center().y - kept * 0.5;
        Rect::from_min_max(pos2(window.min.x, y0), pos2(window.max.x, y0 + kept))
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_picture_contours(
    painter: &Painter,
    rect: Rect,
    contours: &[Vec<crate::shape_geometry::Pt>],
    src: &str,
    tile: bool,
    cover: bool,
    window: Rect,
    tile_scale: [f32; 2],
    scale: f32,
    dimmed: bool,
    opacity: f32,
    media: &mut Option<(&mut MediaCache, &dyn MediaSource)>,
) -> bool {
    let Some((cache, source)) = media.as_mut() else {
        return false;
    };
    let view = if tile {
        cache.texture_tiled(painter.ctx(), src, *source)
    } else {
        let target_px = image_target_px(rect, painter.ctx().pixels_per_point());
        cache.texture_for(painter.ctx(), src, *source, target_px)
    };
    let MediaView::Ready { texture, size } = view else {
        return false;
    };
    let (tex_w, tex_h) = (size[0] as f32, size[1] as f32);
    if !(tex_w.is_finite() && tex_h.is_finite() && tex_w > 0.0 && tex_h > 0.0) {
        return false;
    }
    let window = picture_uv_window(window, (tex_w, tex_h), (rect.width(), rect.height()), cover);
    let sx = if tile_scale[0].is_finite() && tile_scale[0] > 0.0 {
        tile_scale[0]
    } else {
        1.0
    };
    let sy = if tile_scale[1].is_finite() && tile_scale[1] > 0.0 {
        tile_scale[1]
    } else {
        1.0
    };
    let tile_size = (
        (tex_w * scale * sx).max(1e-3),
        (tex_h * scale * sy).max(1e-3),
    );
    let mut mesh = fill_contours_uv(contours, painter.pixels_per_point(), |point| {
        let local_x = point.0 - rect.min.x;
        let local_y = point.1 - rect.min.y;
        if tile {
            (local_x / tile_size.0, local_y / tile_size.1)
        } else if rect.width() > 0.0 && rect.height() > 0.0 {
            (
                window.min.x + local_x / rect.width() * window.width(),
                window.min.y + local_y / rect.height() * window.height(),
            )
        } else {
            (window.min.x, window.min.y)
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
fn effect_copies(
    offset: (f32, f32),
    radius: f32,
    glow: bool,
    max_effect_layers: usize,
) -> Vec<(f32, f32, f32, f32)> {
    if !radius.is_finite() || radius <= 0.1 {
        return if glow {
            Vec::new()
        } else {
            vec![(offset.0, offset.1, 0.0, 1.0)]
        };
    }
    let layers = max_effect_layers.max(1).saturating_add(1);
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
    let max_layers = active_max_effect_layers(painter.ctx());
    for (dx, dy, expansion, alpha) in effect_copies(offset, radius, glow, max_layers) {
        if alpha <= 0.0 {
            continue;
        }
        let layer_color = base.gamma_multiply(alpha.clamp(0.0, 1.0));
        let layer_rect = expand_clamped(rect.translate(vec2(dx, dy)), expansion);
        paint_shape(painter, layer_rect, kind, adjust, layer_color);
    }
}

/// Paint an outline-only shadow or glow behind a shape when its fill is off.
#[allow(clippy::too_many_arguments)]
fn paint_shape_outline_effect(
    painter: &Painter,
    rect: Rect,
    kind: ShapeKind,
    adjust: &[f32],
    outline: &ResolvedOutline,
    scale: f32,
    color: Color32,
    offset: (f32, f32),
    radius: f32,
    glow: bool,
    dimmed: bool,
    opacity: f32,
) {
    if !scale.is_finite() || scale <= 0.0 || !outline.width.is_finite() || outline.width <= 0.0 {
        return;
    }
    let base = apply_dim(color, dimmed, opacity);
    if base.a() == 0 {
        return;
    }
    let max_layers = active_max_effect_layers(painter.ctx());
    for (dx, dy, expansion, alpha) in effect_copies(offset, radius, glow, max_layers) {
        if alpha <= 0.0 {
            continue;
        }
        let layer_color = base.gamma_multiply(alpha.clamp(0.0, 1.0));
        if layer_color.a() == 0 {
            continue;
        }
        let layer_rect = rect.translate(vec2(dx, dy));
        let layer_width = (outline.width + (expansion * 2.0) / scale).max(0.5 / scale);
        if !layer_width.is_finite() {
            continue;
        }
        let layer_outline = ResolvedOutline {
            color: layer_color,
            width: layer_width,
            dash: outline.dash,
            dash_pattern: outline.dash_pattern.clone(),
            join: outline.join,
            miter_limit: outline.miter_limit,
        };
        paint_outline(painter, layer_rect, kind, adjust, &layer_outline, scale);
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
    if !rect.is_positive()
        || ![rect.min.x, rect.min.y, rect.max.x, rect.max.y]
            .iter()
            .all(|value| value.is_finite())
    {
        return;
    }
    if kind == ShapeKind::Rectangle {
        painter.rect_filled(rect, CornerRadius::ZERO, color);
        return;
    }
    let ppp = painter.pixels_per_point();
    let key = BorrowedShapeMeshKey {
        kind,
        width_bits: f32_bits(rect.width()),
        height_bits: f32_bits(rect.height()),
        ppp_bits: f32_bits(ppp),
        adjust,
        style: BorrowedMeshStyle::Solid(color.to_array()),
    };
    let mesh = cached_shape_mesh(painter.ctx(), &key, || {
        let contours = shape_contours_adjusted(kind, rect.width(), rect.height(), adjust);
        if contours.is_empty() {
            return egui::Mesh::default();
        }
        fill_contours(&contours, color, ppp)
    });
    paint_translated_mesh(painter, &mesh, rect.min.to_vec2());
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
    let stroke_width = outline.width * scale;
    let ppp = painter.pixels_per_point();
    let key = BorrowedShapeMeshKey {
        kind,
        width_bits: f32_bits(rect.width()),
        height_bits: f32_bits(rect.height()),
        ppp_bits: f32_bits(ppp),
        adjust,
        style: BorrowedMeshStyle::Outline {
            color: outline.color.to_array(),
            width_bits: f32_bits(stroke_width),
            dash: dash_tag(outline.dash),
            dash_pattern: outline.dash_pattern.as_deref(),
            join: join_tag(outline.join),
            miter_limit_bits: outline.miter_limit.map(f32_bits),
        },
    };
    let mesh = cached_shape_mesh(painter.ctx(), &key, || {
        let contours = shape_contours_adjusted(kind, rect.width(), rect.height(), adjust);
        if contours.is_empty() {
            return egui::Mesh::default();
        }
        stroke_contours_with_join(
            &contours,
            outline.color,
            stroke_width,
            outline.dash,
            outline.dash_pattern.as_deref(),
            StrokeJoin::from_outline(outline.join, outline.miter_limit),
            ppp,
        )
    });
    paint_translated_mesh(painter, &mesh, rect.min.to_vec2());
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

/// Resolves `(effective_fill, solid_override)` for interactive button/shape fills.
///
/// While pressed, `down` wins if it defines a fill of its own (`down_fill` or
/// `down_has_fill_style`); otherwise a solid `hover_fill` keeps showing while hovered.
#[allow(clippy::too_many_arguments)]
fn resolve_pointer_fill(
    base_fill: Color32,
    hover_fill: Option<Color32>,
    down_fill: Option<Color32>,
    down_has_fill_style: bool,
    disabled: bool,
    is_hovered: bool,
    is_pressed: bool,
    auto_hover_darken: bool,
) -> (Color32, bool) {
    if disabled {
        return (base_fill, false);
    }
    if is_pressed {
        let solid_override =
            down_fill.is_some() || (!down_has_fill_style && is_hovered && hover_fill.is_some());
        let effective = if down_has_fill_style {
            base_fill
        } else {
            down_fill
                .or(if is_hovered { hover_fill } else { None })
                .unwrap_or_else(|| {
                    if auto_hover_darken {
                        darken_color(base_fill, 0.15)
                    } else {
                        base_fill
                    }
                })
        };
        return (effective, solid_override);
    }
    if is_hovered {
        let effective = hover_fill.unwrap_or_else(|| {
            if auto_hover_darken {
                lighten_color(base_fill, 0.12)
            } else {
                base_fill
            }
        });
        return (effective, hover_fill.is_some());
    }
    (base_fill, false)
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
fn paint_text_with_options(
    slide_painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    align: &Align,
    valign: &VAlign,
    autofit: coursecraft_course::Autofit,
    wrap: bool,
    insets: Option<coursecraft_course::TextInsets>,
    direction: coursecraft_course::TextDirection,
    dimmed: bool,
    opacity: f32,
    theme_text_color: Color32,
) {
    let Some((text_rect, layout_rect, rot_deg)) =
        crate::text_layout::text_box_layout(screen_rect, scale, insets, direction)
    else {
        return;
    };
    let dir_marker = (rot_deg != 0.0).then(|| slide_painter.add(egui::Shape::Noop));
    let dimmed_runs: Option<Vec<ResolvedTextRun>> = if dimmed || opacity < 1.0 {
        Some(
            runs.iter()
                .map(|run| ResolvedTextRun {
                    color: apply_dim(run.color, dimmed, opacity),
                    highlight: run.highlight.map(|c| apply_dim(c, dimmed, opacity)),
                    ..run.clone()
                })
                .collect(),
        )
    } else {
        None
    };
    let effective_runs = dimmed_runs.as_deref().unwrap_or(runs);
    let laid = crate::text_layout::layout_text_with_wrap(
        slide_painter.ctx(),
        layout_rect,
        scale,
        effective_runs,
        paragraphs,
        *align,
        *valign,
        autofit,
        wrap,
    );
    let fallback_color = apply_dim(theme_text_color, dimmed, opacity);
    for paragraph in &laid.paragraphs {
        crate::text_layout::paint_paragraph_backdrop(slide_painter, paragraph);
        slide_painter.galley(paragraph.pos, paragraph.galley.clone(), fallback_color);
        for (points, stroke) in crate::text_layout::underline_segments(paragraph, paragraph.pos) {
            slide_painter.line_segment(points, stroke);
        }
        if let Some(marker) = &paragraph.marker {
            crate::text_layout::paint_marker(slide_painter, marker, fallback_color);
        }
    }
    if let Some(marker) = dir_marker {
        transform_painted_shapes(
            slide_painter,
            marker,
            &crate::mesh_post::MeshPost::turn(text_rect.center(), rot_deg, false, false),
        );
    }
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
    paint_text_with_options(
        slide_painter,
        screen_rect,
        scale,
        runs,
        paragraphs,
        align,
        valign,
        autofit,
        true,
        None,
        coursecraft_course::TextDirection::Horizontal,
        dimmed,
        opacity,
        theme_text_color,
    );
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
            let dimmed_runs: Option<Vec<ResolvedTextRun>> = if dimmed || opacity < 1.0 {
                Some(
                    cell.text
                        .runs
                        .iter()
                        .map(|run| ResolvedTextRun {
                            color: apply_dim(run.color, dimmed, opacity),
                            highlight: run.highlight.map(|c| apply_dim(c, dimmed, opacity)),
                            ..run.clone()
                        })
                        .collect(),
                )
            } else {
                None
            };
            let effective_runs = dimmed_runs.as_deref().unwrap_or(&cell.text.runs);
            let laid = crate::text_layout::layout_text(
                painter.ctx(),
                inner,
                scale,
                effective_runs,
                &cell.text.paragraphs,
                cell.text.align,
                cell.text.valign,
                coursecraft_course::Autofit::None,
            );
            let cell_painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
            let fallback = apply_dim(theme_text_color, dimmed, opacity);
            for paragraph in &laid.paragraphs {
                crate::text_layout::paint_paragraph_backdrop(&cell_painter, paragraph);
                cell_painter.galley(paragraph.pos, paragraph.galley.clone(), fallback);
                for (points, stroke) in
                    crate::text_layout::underline_segments(paragraph, paragraph.pos)
                {
                    cell_painter.line_segment(points, stroke);
                }
                if let Some(marker) = &paragraph.marker {
                    crate::text_layout::paint_marker(&cell_painter, marker, fallback);
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
    // A border with a dash style or pattern draws as a dashed line, like a shape outline.
    let draw_line = |a: Pos2, b: Pos2, outline: &ResolvedOutline, stroke: Stroke| {
        if outline.dash == DashStyle::Solid && outline.dash_pattern.is_none() {
            painter.line_segment([a, b], stroke);
        } else {
            paint_dashed_polyline(painter, &[a, b], false, outline, stroke);
        }
    };
    if let Some(outer) = &table.outer {
        let stroke = line(outer, outer.color);
        if outer.dash == DashStyle::Solid && outer.dash_pattern.is_none() {
            painter.rect_stroke(screen_rect, CornerRadius::ZERO, stroke, StrokeKind::Inside);
        } else {
            let edge = screen_rect.shrink(stroke.width * 0.5);
            paint_dashed_polyline(
                painter,
                &[
                    edge.left_top(),
                    edge.right_top(),
                    edge.right_bottom(),
                    edge.left_bottom(),
                ],
                true,
                outer,
                stroke,
            );
        }
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
                    draw_line(
                        pos2(x, screen_rect.top() + top * screen_rect.height()),
                        pos2(x, screen_rect.top() + bottom * screen_rect.height()),
                        inner,
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
                draw_line(
                    pos2(screen_rect.left() + left * screen_rect.width(), y),
                    pos2(screen_rect.left() + right * screen_rect.width(), y),
                    inner,
                    stroke,
                );
            }
        }
    }
}

/// Stroke a polyline with the outline's dash style or pattern.
fn paint_dashed_polyline(
    painter: &Painter,
    points: &[Pos2],
    closed: bool,
    outline: &ResolvedOutline,
    stroke: Stroke,
) {
    let points: Vec<crate::path_geometry::Pt> = points.iter().map(|p| (p.x, p.y)).collect();
    let mesh = path_mesh(
        &points,
        coursecraft_course::PathStyle::Straight,
        closed,
        None,
        coursecraft_course::LineEnd::None,
        coursecraft_course::LineEnd::None,
        &PathMeshSpec {
            color: stroke.color,
            width: stroke.width,
            dash: outline.dash,
            dash_pattern: outline.dash_pattern.clone(),
            join: StrokeJoin::from_outline(outline.join, outline.miter_limit),
            fill: None,
            pixels_per_point: painter.pixels_per_point(),
        },
    );
    if !mesh.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
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
    (wrap, insets, direction): (
        bool,
        Option<coursecraft_course::TextInsets>,
        coursecraft_course::TextDirection,
    ),
    dimmed: bool,
    opacity: f32,
) {
    if runs.is_empty() {
        return;
    }
    // The same box as the text itself: its insets, its wrap and its direction.
    let Some((text_rect, layout_rect, rot_deg)) =
        crate::text_layout::text_box_layout(screen_rect, scale, insets, direction)
    else {
        return;
    };
    let base = apply_dim(color, dimmed, opacity);
    let flat: Vec<ResolvedTextRun> = runs
        .iter()
        .map(|run| ResolvedTextRun {
            color: base,
            underline: false,
            link: None,
            highlight: None,
            shadow: false,
            ..run.clone()
        })
        .collect();
    let laid = crate::text_layout::layout_text_with_wrap(
        painter.ctx(),
        layout_rect,
        scale,
        &flat,
        paragraphs,
        *align,
        *valign,
        autofit,
        wrap,
    );
    let dir_marker = (rot_deg != 0.0).then(|| painter.add(egui::Shape::Noop));
    let max_layers = active_max_effect_layers(painter.ctx());
    for (dx, dy, _expansion, alpha) in effect_copies(offset, radius, glow, max_layers) {
        if alpha <= 0.0 {
            continue;
        }
        // The offset is on screen; the text is laid out before its quarter turn, so
        // turn the offset back into the layout's axes.
        let shift = crate::compose::rotate_point(pos2(dx, dy), pos2(0.0, 0.0), -rot_deg).to_vec2();
        let tint = base.gamma_multiply(alpha.clamp(0.0, 1.0));
        for paragraph in &laid.paragraphs {
            painter.galley(paragraph.pos + shift, paragraph.galley.clone(), tint);
            if let Some(marker) = &paragraph.marker {
                painter.galley(marker.pos + shift, marker.galley.clone(), tint);
            }
        }
    }
    if let Some(marker) = dir_marker {
        transform_painted_shapes(
            painter,
            marker,
            &crate::mesh_post::MeshPost::turn(text_rect.center(), rot_deg, false, false),
        );
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
#[allow(clippy::too_many_arguments)]
fn paint_path(
    painter: &Painter,
    screen_rect: Rect,
    scale: f32,
    path: &ResolvedPath,
    dimmed: bool,
    opacity: f32,
    media: &mut Option<(&mut MediaCache, &dyn MediaSource)>,
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
    let to_screen = |pts: &[coursecraft_course::PathPoint]| -> Vec<crate::shape_geometry::Pt> {
        pts.iter()
            .map(|point| {
                (
                    screen_rect.min.x + point.x * screen_rect.width(),
                    screen_rect.min.y + point.y * screen_rect.height(),
                )
            })
            .collect()
    };
    let points = to_screen(&path.points);
    let fill = path.fill.map(|color| apply_dim(color, dimmed, opacity));
    let (outline_color, outline_width, dash, dash_pattern, join) = match &path.outline {
        Some(outline) => (
            apply_dim(outline.color, dimmed, opacity),
            outline.width * scale,
            outline.dash,
            outline.dash_pattern.clone(),
            StrokeJoin::from_outline(outline.join, outline.miter_limit),
        ),
        None => (
            Color32::TRANSPARENT,
            0.0,
            coursecraft_course::DashStyle::Solid,
            None,
            StrokeJoin::default(),
        ),
    };
    if path.subpaths.is_empty() && path.fill_style.is_none() {
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
                dash_pattern,
                join,
                fill,
                pixels_per_point: painter.pixels_per_point(),
            },
        );
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
        return;
    }

    // Multi-subpath or styled fill path: fill all closed contours together with
    // even-odd winding so holes cut cleanly, then stroke each contour.
    let mut fill_contours_list: Vec<Vec<crate::shape_geometry::Pt>> = Vec::new();
    // Fill the curve that is stroked, not its control points.
    if path.closed && points.len() >= 3 {
        fill_contours_list.push(crate::path_geometry::flatten(
            &points, path.style, None, true,
        ));
    }
    for sub in &path.subpaths {
        let sub_pts = to_screen(&sub.points);
        if sub.closed && sub_pts.len() >= 3 {
            fill_contours_list.push(crate::path_geometry::flatten(
                &sub_pts, path.style, None, true,
            ));
        }
    }
    if !fill_contours_list.is_empty() {
        let mut drawn_style = false;
        match &path.fill_style {
            Some(ResolvedFill::Picture {
                src,
                tile,
                cover,
                tile_scale,
            }) => {
                drawn_style = paint_picture_contours(
                    painter,
                    screen_rect,
                    &fill_contours_list,
                    src,
                    *tile,
                    *cover,
                    FULL_UV,
                    *tile_scale,
                    scale,
                    dimmed,
                    opacity,
                    media,
                );
            }
            Some(style) => {
                paint_gradient_contours(
                    painter,
                    screen_rect,
                    &fill_contours_list,
                    style,
                    dimmed,
                    opacity,
                );
                drawn_style = true;
            }
            None => {}
        }
        if !drawn_style
            && let Some(fill_color) = fill
            && fill_color.a() > 0
        {
            let filled = fill_contours(&fill_contours_list, fill_color, painter.pixels_per_point());
            if !filled.is_empty() {
                painter.add(egui::Shape::mesh(filled));
            }
        }
    }

    if outline_width > 0.0 && outline_color.a() > 0 {
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
                dash_pattern: dash_pattern.clone(),
                join,
                fill: None,
                pixels_per_point: painter.pixels_per_point(),
            },
        );
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
        for sub in &path.subpaths {
            let sub_pts = to_screen(&sub.points);
            let sub_mesh = path_mesh(
                &sub_pts,
                path.style,
                sub.closed,
                None,
                coursecraft_course::LineEnd::None,
                coursecraft_course::LineEnd::None,
                &PathMeshSpec {
                    color: outline_color,
                    width: outline_width,
                    dash,
                    dash_pattern: dash_pattern.clone(),
                    join,
                    fill: None,
                    pixels_per_point: painter.pixels_per_point(),
                },
            );
            if !sub_mesh.is_empty() {
                painter.add(egui::Shape::mesh(sub_mesh));
            }
        }
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
    fill_interior: bool,
) {
    if !screen_rect.is_positive() || !scale.is_finite() || scale <= 0.0 || color.a() == 0 {
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
    let base_width = path.outline.as_ref().map_or(2.0, |outline| outline.width);
    let width = (base_width * scale + extra_width * 2.0).max(0.5);
    let fill = fill_interior.then_some(color);
    let (dash, dash_pattern, join) = if !fill_interior && let Some(outline) = &path.outline {
        (
            outline.dash,
            outline.dash_pattern.clone(),
            StrokeJoin::from_outline(outline.join, outline.miter_limit),
        )
    } else {
        (
            coursecraft_course::DashStyle::Solid,
            None,
            StrokeJoin::default(),
        )
    };
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
            dash,
            dash_pattern,
            join,
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

/// Tessellate every shape painted after `marker`, run the meshes through `post` (scale,
/// rotate, mirror, mask, tint), and replace the originals with the results.
///
/// Text and images become textured meshes here too, so they change exactly like shapes.
pub(crate) fn transform_painted_shapes(
    painter: &Painter,
    marker: egui::layers::ShapeIdx,
    post: &crate::mesh_post::MeshPost<'_>,
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

    let ppp = painter.pixels_per_point();
    for (_, mut clipped) in painted {
        if post.scales() {
            // Content is laid out larger or smaller than it shows; don't let tessellation
            // cull parts that the scale brings back on screen. The painter clips the result.
            clipped.clip_rect = Rect::EVERYTHING;
        }
        for primitive in painter.ctx().tessellate(vec![clipped], ppp).into_iter() {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                continue;
            };
            let mesh = post.apply(mesh);
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
#[allow(clippy::cognitive_complexity, clippy::too_many_arguments)]
pub fn paint_items(
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    highlight: Highlight,
    mut media: Option<(&mut MediaCache, &dyn MediaSource)>,
    theme: Option<&Theme>,
    slide_bg: Option<&Color>,
    offset: (f32, f32),
) {
    let slide_rect = t.slide_rect();
    let slide_painter = painter.with_clip_rect(slide_rect);
    let scale = t.scale();

    let slide_bg_rgba = slide_bg
        .and_then(coursecraft_course::parse_color)
        .or_else(|| theme.and_then(|th| coursecraft_course::parse_color(&th.background)))
        .map(|bg| {
            bg.over(coursecraft_course::Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            })
        });

    let theme_text_color = theme
        .and_then(|th| crate::color::parse_color(&th.text))
        .unwrap_or_else(|| Color32::from_rgb(0x1F, 0x1F, 0x1F));
    let theme_accent_color = theme
        .and_then(|th| crate::color::parse_color(&th.accent))
        .unwrap_or_else(|| Color32::from_rgb(0x2F, 0x6F, 0xEB));

    let mut used_fade_cache = false;
    let mut i = 0;
    while i < items.len() {
        let Some(ga) = items[i].group_anim.as_ref() else {
            paint_single_item(
                painter,
                &slide_painter,
                slide_rect,
                t,
                scale,
                &items[i],
                &highlight,
                media.as_mut().map(|(c, s)| (&mut **c, *s)),
                slide_bg_rgba,
                theme_text_color,
                theme_accent_color,
                offset,
            );
            i += 1;
            continue;
        };
        let mut end = i + 1;
        while end < items.len()
            && items[end]
                .group_anim
                .as_ref()
                .is_some_and(|other| other.id == ga.id)
        {
            end += 1;
        }
        let run = &items[i..end];
        if paint_group_run(
            painter,
            &slide_painter,
            slide_rect,
            t,
            scale,
            ga,
            run,
            &highlight,
            &mut media,
            theme,
            slide_bg,
            slide_bg_rgba,
            theme_text_color,
            theme_accent_color,
            offset,
        ) {
            used_fade_cache = true;
        }
        i = end;
    }

    evict_stale_group_textures(painter.ctx(), used_fade_cache);
}

const GROUP_FADE_CACHE_ID: &str = "coursecraft-group-fade-cache";
const GROUP_FADE_FLAG_ID: &str = "coursecraft-group-fade-active";

#[derive(Clone, Hash, PartialEq, Eq)]
struct GroupTextureKey {
    id: String,
    pixel_w: u32,
    pixel_h: u32,
    fingerprint: u64,
}

type GroupFadeCache = std::collections::HashMap<GroupTextureKey, (u64, egui::TextureHandle)>;

fn evict_stale_group_textures(ctx: &egui::Context, used_fade_cache: bool) {
    let flag_id = egui::Id::unique(GROUP_FADE_FLAG_ID);
    let was_active = ctx.data(|d| d.get_temp::<bool>(flag_id).unwrap_or(false));
    if !used_fade_cache && !was_active {
        return;
    }
    let current_pass = ctx.cumulative_pass_nr();
    let cache_id = egui::Id::unique(GROUP_FADE_CACHE_ID);
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<GroupFadeCache>(cache_id);
        cache.retain(|_, (pass, _)| *pass == current_pass);
        let still_active = !cache.is_empty();
        d.insert_temp(flag_id, still_active);
    });
}

fn fnv1a_u64(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn group_run_fingerprint(
    ga: &crate::compose::GroupAnim,
    run: &[DrawItem],
    highlight: &Highlight,
    scale: f32,
    offset: (f32, f32),
) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    h = fnv1a_u64(h, ga.id.as_bytes());
    h = fnv1a_u64(h, &scale.to_bits().to_le_bytes());
    h = fnv1a_u64(h, &offset.0.to_bits().to_le_bytes());
    h = fnv1a_u64(h, &offset.1.to_bits().to_le_bytes());
    for item in run {
        h = fnv1a_u64(h, &item.rect.min.x.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.rect.min.y.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.rect.max.x.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.rect.max.y.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.opacity.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.rotation.to_bits().to_le_bytes());
        h = fnv1a_u64(
            h,
            &[
                u8::from(item.flip_h),
                u8::from(item.flip_v),
                u8::from(item.dimmed),
            ],
        );
        h = fnv1a_u64(h, &item.anim.scale.0.to_bits().to_le_bytes());
        h = fnv1a_u64(h, &item.anim.scale.1.to_bits().to_le_bytes());
        if let Some(c) = item.clip {
            h = fnv1a_u64(h, &c.min_x.to_bits().to_le_bytes());
            h = fnv1a_u64(h, &c.min_y.to_bits().to_le_bytes());
            h = fnv1a_u64(h, &c.max_x.to_bits().to_le_bytes());
            h = fnv1a_u64(h, &c.max_y.to_bits().to_le_bytes());
        }
        if let Some(tint) = item.anim.tint {
            h = fnv1a_u64(h, &tint.rgb);
            h = fnv1a_u64(h, &tint.amount.to_bits().to_le_bytes());
        }
        let is_pressed = item.hit != Hit::None && highlight.pressed.as_ref() == Some(&item.hit);
        let is_hovered = item.hit != Hit::None && highlight.hovered.as_ref() == Some(&item.hit);
        h = fnv1a_u64(h, &[u8::from(is_pressed), u8::from(is_hovered)]);
    }
    h
}

fn group_run_bounds(
    t: &SlideTransform,
    slide_rect: Rect,
    scale: f32,
    run: &[DrawItem],
    offset: (f32, f32),
) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for item in run {
        if item.opacity <= 0.0 {
            continue;
        }
        let shifted = Rect::from_min_size(
            pos2(item.rect.min.x + offset.0, item.rect.min.y + offset.1),
            item.rect.size(),
        );
        let visual = t.to_screen_rect(shifted);
        let margin = effect_margin(item, scale);
        let rot = if item.rotation.is_finite() {
            item.rotation
        } else {
            0.0
        };
        let item_bounds = transformed_bounds(visual.expand(margin), rot, item.flip_h, item.flip_v);
        bounds = Some(match bounds {
            None => item_bounds,
            Some(b) => b.union(item_bounds),
        });
    }
    let sane_limit = slide_rect.expand(slide_rect.width().max(slide_rect.height()));
    let clipped = bounds?.expand(2.0).intersect(sane_limit);
    clipped.is_positive().then_some(clipped)
}

#[allow(clippy::too_many_arguments)]
fn paint_group_run(
    painter: &Painter,
    slide_painter: &Painter,
    slide_rect: Rect,
    t: &SlideTransform,
    scale: f32,
    ga: &crate::compose::GroupAnim,
    run: &[DrawItem],
    highlight: &Highlight,
    media: &mut Option<(&mut MediaCache, &dyn MediaSource)>,
    theme: Option<&Theme>,
    slide_bg: Option<&Color>,
    slide_bg_rgba: Option<coursecraft_course::Rgba>,
    theme_text_color: Color32,
    theme_accent_color: Color32,
    offset: (f32, f32),
) -> bool {
    let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
    let sx = fin(ga.sample.scale.0, 1.0).max(0.0);
    let sy = fin(ga.sample.scale.1, 1.0).max(0.0);
    if sx < 1e-4 || sy < 1e-4 {
        return false;
    }
    let opacity = fin(ga.sample.opacity, 1.0).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return false;
    }
    let rot = fin(ga.sample.rotation_deg, 0.0);
    let slide_off = vec2(fin(ga.sample.offset.0, 0.0), fin(ga.sample.offset.1, 0.0));
    let screen_off = slide_off * scale;

    let shifted_group_frame = ga.frame.translate(vec2(offset.0, offset.1));
    let group_screen_frame = t.to_screen_rect(shifted_group_frame);

    let translated_frame = shifted_group_frame.translate(slide_off);
    let anim_slide_frame = Rect::from_center_size(
        translated_frame.center(),
        vec2(
            translated_frame.width() * sx,
            translated_frame.height() * sy,
        ),
    );
    let anim_screen_frame = t.to_screen_rect(anim_slide_frame);

    let mut mask_polys: Option<Vec<Vec<Pos2>>> = ga.sample.mask.as_ref().map(|m| {
        let placed = crate::compose::PlacedMask {
            mask: m.clone(),
            frame: anim_slide_frame,
            rotation: rot,
        };
        placed
            .slide_polys(egui::Vec2::ZERO)
            .into_iter()
            .map(|poly| poly.into_iter().map(|p| t.to_screen_pos(p)).collect())
            .collect()
    });
    if let Some(frac) = ga.sample.clip {
        let sub = Rect::from_min_max(
            pos2(
                anim_screen_frame.min.x + frac.min_x * anim_screen_frame.width(),
                anim_screen_frame.min.y + frac.min_y * anim_screen_frame.height(),
            ),
            pos2(
                anim_screen_frame.min.x + frac.max_x * anim_screen_frame.width(),
                anim_screen_frame.min.y + frac.max_y * anim_screen_frame.height(),
            ),
        );
        let c = anim_screen_frame.center();
        let quad: Vec<Pos2> = [
            sub.left_top(),
            sub.right_top(),
            sub.right_bottom(),
            sub.left_bottom(),
        ]
        .into_iter()
        .map(|corner| crate::compose::rotate_point(corner, c, rot))
        .collect();
        mask_polys = Some(match mask_polys {
            Some(polys) => polys
                .iter()
                .map(|poly| crate::mesh_post::intersect_convex(poly, &quad))
                .filter(|poly| poly.len() >= 3)
                .collect(),
            None => vec![quad],
        });
    }

    let visible_count = run.iter().filter(|it| it.opacity > 0.0).count();
    let use_offscreen = opacity < 0.999 && visible_count >= 2;

    let group_post = crate::mesh_post::MeshPost {
        centre: group_screen_frame.center(),
        scale: (sx, sy),
        rotation: rot,
        flip_h: false,
        flip_v: false,
        offset: screen_off,
        mask: mask_polys.as_deref(),
        tint: ga.sample.tint,
        opacity: if use_offscreen { 1.0 } else { opacity },
    };

    if !use_offscreen {
        let group_marker = slide_painter.add(egui::Shape::Noop);
        for item in run {
            paint_single_item(
                painter,
                slide_painter,
                slide_rect,
                t,
                scale,
                item,
                highlight,
                media.as_mut().map(|(c, s)| (&mut **c, *s)),
                slide_bg_rgba,
                theme_text_color,
                theme_accent_color,
                offset,
            );
        }
        transform_painted_shapes(slide_painter, group_marker, &group_post);
        return false;
    }

    let Some(bounds) = group_run_bounds(t, slide_rect, scale, run, offset) else {
        return false;
    };
    let ppp = {
        let p = painter.pixels_per_point();
        if p.is_finite() && p > 0.0 { p } else { 1.0 }
    };
    let raw_w = (bounds.width() * ppp).ceil().max(1.0);
    let raw_h = (bounds.height() * ppp).ceil().max(1.0);
    let max_dim = raw_w.max(raw_h);
    let eff_ppp = if max_dim > 2048.0 {
        ppp * (2048.0 / max_dim)
    } else {
        ppp
    };
    let pixel_w = (bounds.width() * eff_ppp).round().clamp(1.0, 2048.0) as u32;
    let pixel_h = (bounds.height() * eff_ppp).round().clamp(1.0, 2048.0) as u32;

    let fingerprint = group_run_fingerprint(ga, run, highlight, scale, offset);
    let key = GroupTextureKey {
        id: ga.id.clone(),
        pixel_w,
        pixel_h,
        fingerprint,
    };

    let ctx = painter.ctx();
    let current_pass = ctx.cumulative_pass_nr();
    let cache_id = egui::Id::unique(GROUP_FADE_CACHE_ID);
    let flag_id = egui::Id::unique(GROUP_FADE_FLAG_ID);

    let cached_tex = ctx.data_mut(|d| {
        let found = {
            let cache = d.get_temp_mut_or_default::<GroupFadeCache>(cache_id);
            if let Some((pass, tex)) = cache.get_mut(&key) {
                *pass = current_pass;
                Some(tex.clone())
            } else {
                None
            }
        };
        if found.is_some() {
            d.insert_temp(flag_id, true);
        }
        found
    });

    let tex = if let Some(tex) = cached_tex {
        tex
    } else {
        let media_source = media.as_ref().map(|(_, src)| *src);
        let Some(img) = crate::offscreen::render_group_to_rgba(
            ctx,
            t,
            bounds,
            pixel_w,
            pixel_h,
            run,
            highlight.clone(),
            media_source,
            theme,
            slide_bg,
            offset,
        ) else {
            return false;
        };
        let pixels: Vec<Color32> = img
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]))
            .collect();
        let color_image = egui::ColorImage {
            size: [img.width as usize, img.height as usize],
            source_size: vec2(img.width as f32, img.height as f32),
            pixels,
        };
        let handle = ctx.load_texture(
            format!("coursecraft-group-{}", ga.id),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|d| {
            {
                let cache = d.get_temp_mut_or_default::<GroupFadeCache>(cache_id);
                cache.insert(key, (current_pass, handle.clone()));
            }
            d.insert_temp(flag_id, true);
        });
        handle
    };

    let group_marker = slide_painter.add(egui::Shape::Noop);
    let tint = Color32::WHITE.linear_multiply(opacity);
    slide_painter.image(tex.id(), bounds, FULL_UV, tint);
    transform_painted_shapes(slide_painter, group_marker, &group_post);
    true
}

#[allow(clippy::cognitive_complexity, clippy::too_many_arguments)]
fn paint_single_item(
    painter: &Painter,
    slide_painter: &Painter,
    slide_rect: Rect,
    t: &SlideTransform,
    scale: f32,
    item: &DrawItem,
    highlight: &Highlight,
    mut media: Option<(&mut MediaCache, &dyn MediaSource)>,
    slide_bg_rgba: Option<coursecraft_course::Rgba>,
    theme_text_color: Color32,
    theme_accent_color: Color32,
    offset: (f32, f32),
) {
    if item.opacity <= 0.0 {
        return;
    }
    let is_pressed = item.hit != Hit::None && highlight.pressed.as_ref() == Some(&item.hit);
    let is_hovered = item.hit != Hit::None && highlight.hovered.as_ref() == Some(&item.hit);

    let shifted_item_rect = Rect::from_min_size(
        pos2(item.rect.min.x + offset.0, item.rect.min.y + offset.1),
        item.rect.size(),
    );
    // `visual_rect` is where the item shows; `screen_rect` is where its content is laid
    // out before the animation scale shrinks or grows the painted meshes onto it.
    let visual_rect = t.to_screen_rect(shifted_item_rect);
    let look = &item.anim;
    let (anim_sx, anim_sy) = look.scale;
    if !(anim_sx.is_finite() && anim_sy.is_finite()) || anim_sx < 1e-4 || anim_sy < 1e-4 {
        return;
    }
    let screen_rect = if look.scale == (1.0, 1.0) {
        visual_rect
    } else {
        Rect::from_center_size(
            visual_rect.center(),
            vec2(
                visual_rect.width() / anim_sx,
                visual_rect.height() / anim_sy,
            ),
        )
    };
    let op = item.opacity;
    let rotation = if item.rotation.is_finite() {
        item.rotation
    } else {
        0.0
    };
    let animated = !look.is_identity();
    let transformed = rotation != 0.0 || item.flip_h || item.flip_v || animated;
    let mut mask_polys: Option<Vec<Vec<egui::Pos2>>> = look.mask.as_ref().map(|m| {
        m.slide_polys(vec2(offset.0, offset.1))
            .into_iter()
            .map(|poly| poly.into_iter().map(|p| t.to_screen_pos(p)).collect())
            .collect()
    });
    // The rectangular reveal clip, in screen space, when the item is rotated or
    // flipped. A painter clip rect is axis-aligned and applies to the turned
    // meshes, so it would cut the wrong region: the clip becomes a quad that is
    // turned with the item and joins the mask instead.
    let sub_screen = item.clip.map(|frac| {
        let sub_x = shifted_item_rect.min.x + frac.min_x * shifted_item_rect.width();
        let sub_y = shifted_item_rect.min.y + frac.min_y * shifted_item_rect.height();
        let sub_w = (frac.max_x - frac.min_x) * shifted_item_rect.width();
        let sub_h = (frac.max_y - frac.min_y) * shifted_item_rect.height();
        t.to_screen_rect(Rect::from_min_size(pos2(sub_x, sub_y), vec2(sub_w, sub_h)))
    });
    let turned = rotation != 0.0 || item.flip_h || item.flip_v;
    if let Some(sub) = sub_screen
        && turned
    {
        let centre = visual_rect.center();
        let quad: Vec<Pos2> = [
            sub.left_top(),
            sub.right_top(),
            sub.right_bottom(),
            sub.left_bottom(),
        ]
        .into_iter()
        .map(|corner| {
            crate::compose::flip_point(
                crate::compose::rotate_point(corner, centre, rotation),
                centre,
                item.flip_h,
                item.flip_v,
            )
        })
        .collect();
        mask_polys = Some(match mask_polys {
            Some(polys) => polys
                .iter()
                .map(|poly| crate::mesh_post::intersect_convex(poly, &quad))
                .filter(|poly| poly.len() >= 3)
                .collect(),
            None => vec![quad],
        });
    }
    let post = crate::mesh_post::MeshPost {
        centre: screen_rect.center(),
        scale: look.scale,
        rotation,
        flip_h: item.flip_h,
        flip_v: item.flip_v,
        offset: egui::Vec2::ZERO,
        mask: mask_polys.as_deref(),
        tint: look.tint,
        opacity: 1.0,
    };

    let mut item_painter = if let Some(screen_sub) = sub_screen {
        let clip = if transformed {
            let effect_margin = effect_margin(item, scale);
            let bounds = transformed_bounds(
                visual_rect.expand(effect_margin),
                rotation,
                item.flip_h,
                item.flip_v,
            );
            let bounds = bounds.intersect(slide_rect);
            if turned {
                // The quad in the mask does the cutting.
                bounds
            } else {
                bounds.intersect(screen_sub)
            }
        } else {
            screen_sub.intersect(slide_rect)
        };
        slide_painter.with_clip_rect(clip)
    } else {
        slide_painter.clone()
    };

    if let Some(clip_rect) = item.clip_rect
        && clip_rect.is_positive()
    {
        let screen_clip = t.to_screen_rect(clip_rect.translate(vec2(offset.0, offset.1)));
        let current = item_painter.clip_rect();
        item_painter = item_painter.with_clip_rect(current.intersect(screen_clip));
    }

    let mut marker = transformed.then(|| item_painter.add(egui::Shape::Noop));

    match &item.visual {
        Visual::Text {
            runs,
            align,
            valign,
            paragraphs,
            autofit,
            outline,
            wrap,
            insets,
            direction,
        } => {
            if let Some(outline) = outline {
                let resolved = ResolvedOutline {
                    color: apply_dim(outline.color, item.dimmed, op),
                    width: outline.width,
                    dash: outline.dash,
                    dash_pattern: outline.dash_pattern.clone(),
                    join: outline.join,
                    miter_limit: outline.miter_limit,
                };
                paint_outline(
                    &item_painter,
                    screen_rect,
                    ShapeKind::Rectangle,
                    &[],
                    &resolved,
                    scale,
                );
            }
            // A horizontally flipped text box mirrors its outline but not its glyphs, so
            // the glyphs, their shadow and glow all go in the second group, which
            // is turned to match the outline instead of being mirrored.
            let text_marker = if item.flip_h
                && let Some(shape_marker) = marker.take()
            {
                transform_painted_shapes(&item_painter, shape_marker, &post);
                (rotation != 0.0 || item.flip_v || animated)
                    .then(|| item_painter.add(egui::Shape::Noop))
            } else {
                None
            };
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
                    (*wrap, *insets, *direction),
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
                    (*wrap, *insets, *direction),
                    item.dimmed,
                    op,
                );
            }
            paint_text_with_options(
                &item_painter,
                screen_rect,
                scale,
                runs,
                paragraphs,
                align,
                valign,
                *autofit,
                *wrap,
                *insets,
                *direction,
                item.dimmed,
                op,
                theme_text_color,
            );
            if let Some(tm) = text_marker {
                transform_painted_shapes(&item_painter, tm, &post.unmirrored_text());
            }
        }
        Visual::Button {
            label,
            fill,
            fill_style,
            hover_fill,
            down_fill,
            down_has_fill_style,
            disabled,
        } => {
            let (effective_fill, solid_override) = resolve_pointer_fill(
                *fill,
                *hover_fill,
                *down_fill,
                *down_has_fill_style,
                *disabled,
                is_hovered,
                is_pressed,
                true,
            );
            let (button_kind, button_adjust) = button_shape(screen_rect, scale);
            let has_fill = (!solid_override && fill_style.is_some()) || effective_fill.a() > 0;
            if has_fill {
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
            }

            let fill_color = apply_dim(effective_fill, item.dimmed, op);
            let mut drawn = false;
            if !solid_override {
                match fill_style {
                    Some(ResolvedFill::Picture {
                        src,
                        tile,
                        cover,
                        tile_scale,
                    }) => {
                        drawn = paint_picture_fill(
                            &item_painter,
                            screen_rect,
                            button_kind,
                            &button_adjust,
                            src,
                            *tile,
                            *cover,
                            FULL_UV,
                            *tile_scale,
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
            let [r, g, b, a] = effective_fill.to_srgba_unmultiplied();
            let label_rgba = coursecraft_course::button_label_color(
                coursecraft_course::Rgba { r, g, b, a },
                slide_bg_rgba,
            );
            let label_color = Color32::from_rgba_unmultiplied(
                label_rgba.r,
                label_rgba.g,
                label_rgba.b,
                label_rgba.a,
            );
            let text_color = apply_dim(label_color, item.dimmed, op);
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
            down_has_fill_style,
            disabled,
        } => {
            let (effective_fill, solid_override) = resolve_pointer_fill(
                *fill,
                *hover_fill,
                *down_fill,
                *down_has_fill_style,
                *disabled,
                is_hovered,
                is_pressed,
                false,
            );
            let has_fill = (!solid_override && fill_style.is_some()) || effective_fill.a() > 0;
            let visible_outline = outline
                .as_ref()
                .filter(|o| o.color.a() > 0 && o.width.is_finite() && o.width > 0.0);
            if let Some(shadow) = &item.shadow {
                if has_fill {
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
                } else if let Some(out) = visible_outline {
                    paint_shape_outline_effect(
                        &item_painter,
                        screen_rect,
                        *shape,
                        adjust,
                        out,
                        scale,
                        shadow.color,
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        item.dimmed,
                        op,
                    );
                }
            }
            if let Some(glow) = &item.glow {
                if has_fill {
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
                } else if let Some(out) = visible_outline {
                    paint_shape_outline_effect(
                        &item_painter,
                        screen_rect,
                        *shape,
                        adjust,
                        out,
                        scale,
                        glow.color,
                        (0.0, 0.0),
                        glow.size * scale,
                        true,
                        item.dimmed,
                        op,
                    );
                }
            }

            let fill_color = apply_dim(effective_fill, item.dimmed, op);
            let mut drawn = false;
            if !solid_override {
                match fill_style {
                    Some(ResolvedFill::Picture {
                        src,
                        tile,
                        cover,
                        tile_scale,
                    }) => {
                        drawn = paint_picture_fill(
                            &item_painter,
                            screen_rect,
                            *shape,
                            adjust,
                            src,
                            *tile,
                            *cover,
                            FULL_UV,
                            *tile_scale,
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
                    dash_pattern: outline.dash_pattern.clone(),
                    join: outline.join,
                    miter_limit: outline.miter_limit,
                };
                paint_outline(&item_painter, screen_rect, *shape, adjust, &resolved, scale);
            }
            if let Some(text) = text
                && !text.runs.is_empty()
            {
                if item.flip_h
                    && let Some(shape_marker) = marker.take()
                {
                    transform_painted_shapes(&item_painter, shape_marker, &post);
                }
                let text_marker = (item.flip_h && (rotation != 0.0 || item.flip_v || animated))
                    .then(|| item_painter.add(egui::Shape::Noop));
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
                    paint_text_with_options(
                        &item_painter,
                        text_rect,
                        scale,
                        &text.runs,
                        &text.paragraphs,
                        &text.align,
                        &text.valign,
                        text.autofit,
                        text.wrap,
                        text.insets,
                        text.direction,
                        item.dimmed,
                        op,
                        theme_text_color,
                    );
                }
                if let Some(tm) = text_marker {
                    transform_painted_shapes(&item_painter, tm, &post.unmirrored_text());
                }
            }
        }
        Visual::Path(path) => {
            let has_fill =
                path.closed && (path.fill_style.is_some() || path.fill.is_some_and(|c| c.a() > 0));
            let has_outline = path
                .outline
                .as_ref()
                .is_some_and(|o| o.color.a() > 0 && o.width.is_finite() && o.width > 0.0);
            if has_fill || has_outline {
                let max_layers = active_max_effect_layers(item_painter.ctx());
                if let Some(shadow) = &item.shadow {
                    for (dx, dy, expansion, alpha) in effect_copies(
                        (shadow.offset_x * scale, shadow.offset_y * scale),
                        shadow.blur * scale,
                        false,
                        max_layers,
                    ) {
                        let color = apply_dim(shadow.color, item.dimmed, op)
                            .gamma_multiply(alpha.clamp(0.0, 1.0));
                        let translated = screen_rect.translate(vec2(dx, dy));
                        let (rect, extra_width) = if has_fill {
                            (expand_clamped(translated, expansion), expansion.max(0.0))
                        } else {
                            (translated, expansion)
                        };
                        paint_path_silhouette(
                            &item_painter,
                            rect,
                            scale,
                            path,
                            color,
                            extra_width,
                            has_fill,
                        );
                    }
                }
                if let Some(glow) = &item.glow {
                    for (_, _, expansion, alpha) in
                        effect_copies((0.0, 0.0), glow.size * scale, true, max_layers)
                    {
                        let color = apply_dim(glow.color, item.dimmed, op)
                            .gamma_multiply(alpha.clamp(0.0, 1.0));
                        let rect = if has_fill {
                            screen_rect.expand(expansion)
                        } else {
                            screen_rect
                        };
                        paint_path_silhouette(
                            &item_painter,
                            rect,
                            scale,
                            path,
                            color,
                            expansion,
                            has_fill,
                        );
                    }
                }
            }
            paint_path(
                &item_painter,
                screen_rect,
                scale,
                path,
                item.dimmed,
                op,
                &mut media,
            );
            if let Some(text) = &path.text
                && !text.runs.is_empty()
            {
                if item.flip_h
                    && let Some(shape_marker) = marker.take()
                {
                    transform_painted_shapes(&item_painter, shape_marker, &post);
                }
                let text_marker = (item.flip_h && (rotation != 0.0 || item.flip_v || animated))
                    .then(|| item_painter.add(egui::Shape::Noop));
                paint_text_with_options(
                    &item_painter,
                    screen_rect.shrink((4.0 * scale).min(screen_rect.width() * 0.25)),
                    scale,
                    &text.runs,
                    &text.paragraphs,
                    &text.align,
                    &text.valign,
                    text.autofit,
                    text.wrap,
                    text.insets,
                    text.direction,
                    item.dimmed,
                    op,
                    theme_text_color,
                );
                if let Some(tm) = text_marker {
                    transform_painted_shapes(&item_painter, tm, &post.unmirrored_text());
                }
            }
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
        Visual::Image {
            src,
            alt,
            crop,
            outline,
            shape,
        } => {
            let shape_kind = shape.unwrap_or(ShapeKind::Rectangle);
            if let Some(shadow) = &item.shadow {
                paint_shape_effect(
                    &item_painter,
                    screen_rect,
                    shape_kind,
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
                    shape_kind,
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
            if let Some(mask_shape) = shape {
                drawn_texture = paint_picture_fill(
                    &item_painter,
                    screen_rect,
                    *mask_shape,
                    &[],
                    src,
                    false,
                    false,
                    crop.unwrap_or(FULL_UV),
                    [1.0, 1.0],
                    scale,
                    item.dimmed,
                    op,
                    &mut media,
                );
            } else if let Some((ref mut cache, source)) = media {
                let target_px = image_target_px(screen_rect, painter.pixels_per_point());
                match cache.texture_for(painter.ctx(), src, source, target_px) {
                    MediaView::Ready { texture, size } => {
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

                draw_image_placeholder(&item_painter, screen_rect, scale, &label, item.dimmed, op);
            }
            if let Some(outline) = outline {
                let resolved = ResolvedOutline {
                    color: apply_dim(outline.color, item.dimmed, op),
                    width: outline.width,
                    dash: outline.dash,
                    dash_pattern: outline.dash_pattern.clone(),
                    join: outline.join,
                    miter_limit: outline.miter_limit,
                };
                paint_outline(
                    &item_painter,
                    screen_rect,
                    shape_kind,
                    &[],
                    &resolved,
                    scale,
                );
            }
        }
        Visual::ImagePlaceholder { label } => {
            draw_image_placeholder(&item_painter, screen_rect, scale, label, item.dimmed, op);
        }
        Visual::Video {
            src: _,
            poster,
            playing,
            show_placeholder,
        } => {
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
                if !drawn_frame && let Some(poster_src) = poster {
                    let target_px = image_target_px(screen_rect, painter.pixels_per_point());
                    if let MediaView::Ready { texture, size } =
                        cache.texture_for(painter.ctx(), poster_src, *source, target_px)
                    {
                        let tex_size = (size[0] as f32, size[1] as f32);
                        if let Some(dest_rect) = image_dest_rect(screen_rect, tex_size, None) {
                            let tint = apply_dim(Color32::WHITE, item.dimmed, op);
                            item_painter.image(texture.id(), dest_rect, FULL_UV, tint);
                            drawn_frame = true;
                        }
                    }
                }
            }
            if !drawn_frame {
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
            }

            // Poster text (on editor canvas) or plain error message if preview decode failed.
            let error_text = if drawn_frame {
                None
            } else {
                media.as_ref().and_then(|(_, source)| {
                    if let Hit::Object(id) = &item.hit {
                        source.video_error(id)
                    } else {
                        None
                    }
                })
            };
            let label = if let Some(err) = error_text {
                Some(err.to_owned())
            } else if !drawn_frame && *show_placeholder {
                Some("Video plays in the published course".to_owned())
            } else {
                None
            };

            let center = screen_rect.center();
            let play_radius = (24.0 * scale).min(screen_rect.height() * 0.25).max(4.0);
            let icon_offset_y = if label.is_some() { 12.0 * scale } else { 0.0 };
            if !*playing {
                let icon_center = pos2(center.x, center.y - icon_offset_y);
                let circle_color = apply_dim(
                    Color32::from_rgba_unmultiplied(0, 0, 0, 140),
                    item.dimmed,
                    op,
                );
                item_painter.circle_filled(icon_center, play_radius, circle_color);

                let tri_size = play_radius * 0.5;
                let tri_points = vec![
                    pos2(center.x - tri_size * 0.5, icon_center.y - tri_size * 0.7),
                    pos2(center.x + tri_size * 0.8, icon_center.y),
                    pos2(center.x - tri_size * 0.5, icon_center.y + tri_size * 0.7),
                ];
                let tri_color = apply_dim(Color32::WHITE, item.dimmed, op);
                item_painter.add(PathShape::convex_polygon(
                    tri_points,
                    tri_color,
                    Stroke::NONE,
                ));
            }

            if let Some(label) = label {
                let font_size = (14.0 * scale).max(1.0);
                let font_id = crate::font::slide_font(item_painter.ctx(), false, false, font_size);
                let text_color = apply_dim(Color32::from_rgb(0xAA, 0xAA, 0xAA), item.dimmed, op);
                let galley =
                    item_painter.layout(label, font_id, text_color, screen_rect.width().max(0.0));
                let text_pos = pos2(
                    center.x - galley.size().x * 0.5,
                    center.y + play_radius + 4.0 * scale - icon_offset_y,
                );
                item_painter.galley(text_pos, galley, text_color);
            }
        }
        Visual::Web { label } => {
            draw_web_placeholder(&item_painter, screen_rect, scale, label, item.dimmed, op);
        }
        Visual::DrawCard { text } => {
            draw_draw_card(&item_painter, screen_rect, scale, text, item.dimmed, op);
        }
        Visual::Audio { src: _ } => {
            if item.rect.width() <= 0.0 || item.rect.height() <= 0.0 {
                return;
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
                draw_image_placeholder(&item_painter, screen_rect, scale, &label, item.dimmed, op);
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
                    item_painter.rect_filled(marker_rect, CornerRadius::same(radius), active_color);
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
            let galley = item_painter.layout(text.clone(), font_id, text_color, available_text_w);
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
                // The panel item takes the wheel and drag; its scrollbar copy (no hit)
                // draws the thumb, so the thumb is painted once and above its children.
                thumb
                    .filter(|_| item.hit == Hit::None)
                    .map(|thumb| t.to_screen_rect(thumb.translate(vec2(offset.0, offset.1)))),
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
        transform_painted_shapes(&item_painter, marker, &post);
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
            if !side.is_finite() || side <= 0.0 {
                return;
            }
            let thickness = coursecraft_course::safe_clamp(side * 0.18, 2.0 * scale, side * 0.5);
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
    if pulse && crate::design::paint_quality(painter.ctx()).marker_pulse {
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
        painter
            .ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
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
            highlight: run.highlight.map(|c| apply_dim(c, dimmed, opacity)),
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
        highlight: None,
        shadow: false,
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
    let card_width = coursecraft_course::safe_clamp(content_width + pad * 2.0, 0.0, max_width);
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
            card_x = coursecraft_course::safe_clamp(
                screen_rect.center().x - card_width * 0.5,
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
    card_y = coursecraft_course::safe_clamp(
        card_y,
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
            crate::text_layout::paint_paragraph_backdrop(painter, paragraph);
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
    paint_items(
        painter,
        t,
        items,
        highlight,
        media,
        theme,
        slide_bg,
        (0.0, 0.0),
    );
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

    #[test]
    fn ring_progress_meter_in_tiny_and_zero_rect_does_not_panic() {
        let ctx = egui::Context::default();
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            for size in [vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(-2.0, 4.0)] {
                paint_progress_meter(
                    painter,
                    Rect::from_min_size(pos2(0.0, 0.0), size),
                    1.0,
                    0.5,
                    MeterStyle::Ring,
                    true,
                    50,
                    false,
                    1.0,
                    Color32::BLUE,
                    Color32::BLACK,
                );
            }
        });
        out.drop_without_applying_deltas();
    }
}
