//! In-memory media cache and abstractions for the slide compositor (L3).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Duration;

use coursecraft_course::ObjectKind;
use coursecraft_logic::{Input, Player};
use coursecraft_media::{
    Animation, Limits, decode_animation, decode_image_fitted, decode_svg, svg_natural_size,
};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use velato::Composition;

use crate::lottie::{self, LottieInfo};

/// How much the drawn size may change before an SVG is rasterised again.
pub const SVG_RERASTERISE_THRESHOLD: f32 = 0.25;

/// Minimum time between two SVG rasters of the same source, in seconds.
pub const SVG_RERASTERISE_DEBOUNCE_SECONDS: f64 = 0.15;

/// Whether `cached` must be rasterised again for a new `requested` size.
#[must_use]
pub fn svg_needs_rerasterise(cached: (u32, u32), requested: (u32, u32)) -> bool {
    let changed = |old: u32, new: u32| {
        let old = old.max(1) as f32;
        let new = new.max(1) as f32;
        ((old - new).abs() / old.max(1.0)) > SVG_RERASTERISE_THRESHOLD
    };
    changed(cached.0, requested.0) || changed(cached.1, requested.1)
}

fn ends_with_ascii_ci(src: &str, suffix: &[u8]) -> bool {
    src.len()
        .checked_sub(suffix.len())
        .and_then(|start| src.as_bytes().get(start..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

fn is_svg_src(src: &str) -> bool {
    ends_with_ascii_ci(src, b".svg") || ends_with_ascii_ci(src, b".svgz")
}

fn is_animated_src(src: &str) -> bool {
    ends_with_ascii_ci(src, b".gif") || ends_with_ascii_ci(src, b".webp")
}

struct SvgEntry {
    texture: TextureHandle,
    target: (u32, u32),
    uploaded_bytes: u64,
    last_used: u64,
    last_frame: u64,
    global_use: u64,
}

struct SvgAsset {
    bytes: Arc<[u8]>,
    natural: (f32, f32),
    entries: Vec<SvgEntry>,
    last_raster: f64,
    use_counter: u64,
}

struct AnimationEntry {
    animation: Option<Animation>,
    frames: Vec<Option<TextureHandle>>,
    warning: Option<String>,
    cpu_bytes: u64,
    frame_bytes: u64,
    uploaded_frames: u64,
    last_frame: u64,
    global_use: u64,
}

enum LottieEntry {
    Ready {
        composition: Composition,
        info: LottieInfo,
    },
    Failed(String),
}

/// Resolved view of a requested Lottie animation.
pub enum LottieView<'a> {
    /// Parsed animation and its metadata.
    Ready {
        composition: &'a Composition,
        info: LottieInfo,
    },
    /// Bytes are still loading.
    Pending,
    /// The file is missing or rejected, with the reason.
    Failed(&'a str),
}

/// Result of requesting a media asset from a [`MediaSource`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaFetch {
    /// Asset is available in memory.
    Ready(Arc<[u8]>),
    /// Asset is currently loading or in flight.
    Pending,
    /// Asset could not be found or failed to fetch.
    Missing,
}

/// Abstract provider for media files.
///
/// Implemented by host applications so that the renderer stays free of direct I/O.
pub trait MediaSource {
    /// Retrieve the media asset identified by `src`.
    fn get(&self, src: &str) -> MediaFetch;
    /// Newest decoded video frame for `object`, if the host has one (desktop
    /// playback). Default is `None`, which draws the poster.
    fn video_frame(&self, _object: &str) -> Option<Arc<coursecraft_playback::VideoFrame>> {
        None
    }
    /// Plain-language error message if video preview decoding failed or timed out.
    fn video_error(&self, _object: &str) -> Option<&str> {
        None
    }
}

/// Resolved presentation view of a requested media texture.
#[derive(Clone, Copy)]
pub enum MediaView<'a> {
    /// Successfully loaded GPU texture handle and natural dimensions `[width, height]`.
    Ready {
        texture: &'a TextureHandle,
        size: [u32; 2],
    },
    /// Asset is currently loading or queued.
    Pending,
    /// Asset failed to load or decode with an error description.
    Failed(&'a str),
}

impl std::fmt::Debug for MediaView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready { texture, size } => f
                .debug_struct("Ready")
                .field("texture_id", &texture.id())
                .field("size", size)
                .finish(),
            Self::Pending => write!(f, "Pending"),
            Self::Failed(msg) => f.debug_tuple("Failed").field(msg).finish(),
        }
    }
}

/// Cached state of an individual media asset in [`MediaCache`].
pub enum MediaEntry {
    /// Asset is in flight or pending arrival.
    Loading,
    /// Successfully decoded image uploaded as an egui texture.
    Texture(TextureHandle, [u32; 2]),
    /// Asset decode or retrieval failed with the given reason.
    Failed(String),
}

impl std::fmt::Debug for MediaEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Texture(handle, size) => f
                .debug_struct("Texture")
                .field("texture_id", &handle.id())
                .field("size", size)
                .finish(),
            Self::Failed(msg) => f.debug_tuple("Failed").field(msg).finish(),
        }
    }
}

/// Test-only renderer work counters. Gated on `debug_assertions` rather than
/// `test` because the guard tests live in `coursecraft-ui-egui`, and a
/// dependency is not built with `cfg(test)` for its caller's tests. Debug
/// builds (which include the normal `cargo test` run) have the counters; release
/// builds compile them away, so a shipped frame never pays for them.
#[cfg(debug_assertions)]
pub mod test_counts {
    use std::cell::Cell;

    thread_local! {
        static PICTURE_DECODES: Cell<u64> = const { Cell::new(0) };
        static FONT_DEF_CLONES: Cell<u64> = const { Cell::new(0) };
        static TEXT_LAYOUT_MISSES: Cell<u64> = const { Cell::new(0) };
        static SHAPE_MESH_MISSES: Cell<u64> = const { Cell::new(0) };
        static FULL_RES_THUMBNAIL_DECODES: Cell<u64> = const { Cell::new(0) };
    }

    /// Called only after `decode_image` actually produced a texture.
    pub fn note_picture_decode() {
        PICTURE_DECODES.set(PICTURE_DECODES.get().saturating_add(1));
    }

    /// Number of picture decodes on this thread since the last reset.
    #[must_use]
    pub fn picture_decode_count() -> u64 {
        PICTURE_DECODES.get()
    }

    /// Zero the picture decode count before the measured part of a guard test.
    pub fn reset_picture_decode_count() {
        PICTURE_DECODES.set(0);
    }

    /// Record a `FontDefinitions` clone from pending-pass lookup.
    pub fn note_font_def_clone() {
        FONT_DEF_CLONES.set(FONT_DEF_CLONES.get().saturating_add(1));
    }

    /// Number of `FontDefinitions` clones on this thread since the last reset.
    #[must_use]
    pub fn font_def_clone_count() -> u64 {
        FONT_DEF_CLONES.get()
    }

    /// Record a text layout cache miss.
    pub fn note_text_layout_miss() {
        TEXT_LAYOUT_MISSES.set(TEXT_LAYOUT_MISSES.get().saturating_add(1));
    }

    /// Number of text layout cache misses on this thread since the last reset.
    #[must_use]
    pub fn text_layout_miss_count() -> u64 {
        TEXT_LAYOUT_MISSES.get()
    }

    /// Record a shape mesh cache miss.
    pub fn note_shape_mesh_miss() {
        SHAPE_MESH_MISSES.set(SHAPE_MESH_MISSES.get().saturating_add(1));
    }

    /// Number of shape mesh cache misses on this thread since the last reset.
    #[must_use]
    pub fn shape_mesh_miss_count() -> u64 {
        SHAPE_MESH_MISSES.get()
    }

    /// Record a full-resolution decode triggered while drawing a thumbnail.
    pub fn note_full_res_thumbnail_decode() {
        FULL_RES_THUMBNAIL_DECODES.set(FULL_RES_THUMBNAIL_DECODES.get().saturating_add(1));
    }

    /// Number of full-resolution thumbnail decodes on this thread since the last reset.
    #[must_use]
    pub fn full_res_thumbnail_decode_count() -> u64 {
        FULL_RES_THUMBNAIL_DECODES.get()
    }

    /// Reset all renderer test counters on this thread to zero.
    pub fn reset_all() {
        PICTURE_DECODES.set(0);
        FONT_DEF_CLONES.set(0);
        TEXT_LAYOUT_MISSES.set(0);
        SHAPE_MESH_MISSES.set(0);
        FULL_RES_THUMBNAIL_DECODES.set(0);
    }
}

/// Ceiling for decoded media cache (still textures, uploaded animation frames,
/// SVG rasters, and decoded animation CPU frames): 256 MiB on the desktop,
/// 96 MiB on wasm where the whole app shares the browser's memory.
#[cfg(target_arch = "wasm32")]
pub const DEFAULT_PICTURE_BUDGET: u64 = 96 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_PICTURE_BUDGET: u64 = 256 * 1024 * 1024;

/// Default maximum pixel dimension for thumbnail-tier raster decoding.
pub const DEFAULT_MAX_THUMB_DIM: u32 = 384;

/// Default maximum pixel dimension for canvas-tier raster decoding.
pub const DEFAULT_MAX_CANVAS_DIM: u32 = 2048;

/// Frames an animation can remain unused before its decoded CPU frames become
/// eligible for eviction under the unified memory budget.
const COLD_ANIMATION_FRAMES: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum DecodeTier {
    Thumbnail,
    Medium,
    Canvas,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ContentKey {
    len: usize,
    hash: u64,
    tier: DecodeTier,
    tiled: bool,
}

/// Fast, deterministic 64-bit word-at-a-time content hash for image byte deduplication.
///
/// Processes 32-byte blocks across four independent accumulators to avoid serial
/// byte-by-byte multiply chains on multi-megabyte images.
fn content_hash_64(bytes: &[u8]) -> u64 {
    const P1: u64 = 0x9e37_79b1_85eb_ca87;
    const P2: u64 = 0xc2b2_ae3d_27d4_eb4f;
    const P3: u64 = 0x1656_67b1_9e37_79f9;
    const P4: u64 = 0x85eb_ca77_c2b2_ae63;
    const P5: u64 = 0x27d4_eb2f_1656_67c5;

    let mut s1 = P1.wrapping_add(P2);
    let mut s2 = P2;
    let mut s3 = 0u64;
    let mut s4 = 0u64.wrapping_sub(P1);

    let (blocks, remainder) = bytes.as_chunks::<32>();
    for block in blocks {
        let w1 = u64::from_le_bytes([
            block[0], block[1], block[2], block[3], block[4], block[5], block[6], block[7],
        ]);
        let w2 = u64::from_le_bytes([
            block[8], block[9], block[10], block[11], block[12], block[13], block[14], block[15],
        ]);
        let w3 = u64::from_le_bytes([
            block[16], block[17], block[18], block[19], block[20], block[21], block[22], block[23],
        ]);
        let w4 = u64::from_le_bytes([
            block[24], block[25], block[26], block[27], block[28], block[29], block[30], block[31],
        ]);
        s1 = s1
            .wrapping_add(w1.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1);
        s2 = s2
            .wrapping_add(w2.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1);
        s3 = s3
            .wrapping_add(w3.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1);
        s4 = s4
            .wrapping_add(w4.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1);
    }

    let mut hash = s1
        .rotate_left(1)
        .wrapping_add(s2.rotate_left(7))
        .wrapping_add(s3.rotate_left(12))
        .wrapping_add(s4.rotate_left(18))
        .wrapping_add(bytes.len() as u64);

    let (tail_words, tail_bytes) = remainder.as_chunks::<8>();
    for word in tail_words {
        let v = u64::from_le_bytes(*word);
        let k = v.wrapping_mul(P2).rotate_left(31).wrapping_mul(P1);
        hash ^= k;
        hash = hash.rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
    }

    for &byte in tail_bytes {
        hash ^= u64::from(byte).wrapping_mul(P5);
        hash = hash.rotate_left(11).wrapping_mul(P1);
    }

    hash ^= hash >> 33;
    hash = hash.wrapping_mul(P2);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(P3);
    hash ^= hash >> 32;
    hash
}

/// Maximum number of `Arc<[u8]>` identity hashes cached before pruning dead entries.
const MAX_ARC_HASH_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ArcPtrKey {
    ptr: usize,
    len: usize,
}

struct ArcHashEntry {
    weak: Weak<[u8]>,
    hash: u64,
}

struct SharedTexture {
    handle: TextureHandle,
    natural_size: [u32; 2],
    uploaded_bytes: u64,
    refcount: usize,
}

struct PictureMeta {
    tier: DecodeTier,
    content_key: Option<ContentKey>,
}

/// Texture cache for slide media assets keyed by `src`.
///
/// Ensures each asset is decoded at most once per display tier and deduplicates
/// identical image bytes across distinct keys. Failed decodes and missing assets
/// are retained so they are not re-queried or re-decoded on every frame. Still
/// pictures, uploaded animation frames, SVG rasters, and decoded animation CPU
/// buffers are tracked in O(1) under a unified byte ceiling.
pub struct MediaCache {
    entries: HashMap<String, MediaEntry>,
    picture_meta: HashMap<String, PictureMeta>,
    shared_textures: HashMap<ContentKey, SharedTexture>,
    arc_hashes: HashMap<ArcPtrKey, ArcHashEntry>,
    svg: HashMap<String, SvgAsset>,
    animations: HashMap<String, AnimationEntry>,
    lottie: HashMap<String, LottieEntry>,
    video: HashMap<String, VideoTextureEntry>,
    limits: Limits,
    animation_time_ms: Option<u64>,
    picture_budget: u64,
    max_canvas_dim: u32,
    max_thumb_dim: u32,
    still_bytes: u64,
    anim_gpu_bytes: u64,
    anim_cpu_bytes: u64,
    svg_bytes: u64,
    frame: u64,
    use_counter: u64,
    picture_uses: HashMap<String, PictureUse>,
}

impl std::fmt::Debug for MediaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaCache")
            .field("entries_count", &self.entries.len())
            .field("shared_textures_count", &self.shared_textures.len())
            .field("svg_count", &self.svg.len())
            .field("animation_count", &self.animations.len())
            .field("lottie_count", &self.lottie.len())
            .field("video_count", &self.video.len())
            .field("picture_budget", &self.picture_budget)
            .field("picture_bytes", &self.picture_bytes())
            .field("total_bytes", &self.total_bytes())
            .field("limits", &self.limits)
            .finish()
    }
}

/// When a decoded picture was last handed out to the painter.
struct PictureUse {
    /// Monotonic use counter; higher means more recently used.
    counter: u64,
    /// Frame on which it was last handed out.
    frame: u64,
}

struct VideoTextureEntry {
    handle: TextureHandle,
    size: [u32; 2],
    /// Identity of the last uploaded frame (same allocation = same frame). A `Weak`
    /// keeps the allocation reserved, so a later frame can never reuse its address
    /// and be mistaken for it. The pixel data is freed with the last `Arc`.
    last_frame: Weak<coursecraft_playback::VideoFrame>,
}

impl Default for MediaCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            picture_meta: HashMap::new(),
            shared_textures: HashMap::new(),
            arc_hashes: HashMap::new(),
            svg: HashMap::new(),
            animations: HashMap::new(),
            lottie: HashMap::new(),
            video: HashMap::new(),
            limits: Limits::default(),
            animation_time_ms: None,
            picture_budget: DEFAULT_PICTURE_BUDGET,
            max_canvas_dim: DEFAULT_MAX_CANVAS_DIM,
            max_thumb_dim: DEFAULT_MAX_THUMB_DIM,
            still_bytes: 0,
            anim_gpu_bytes: 0,
            anim_cpu_bytes: 0,
            svg_bytes: 0,
            frame: 0,
            use_counter: 0,
            picture_uses: HashMap::new(),
        }
    }
}

impl MediaCache {
    /// Create a new, empty media cache with default decoding limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new media cache with custom resource limits.
    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// Bump the frame counter. Call once per editor or player frame, before any
    /// picture is requested. Pictures handed out in the current frame are never
    /// evicted, even if the cache is over budget for that frame.
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.saturating_add(1);
        if !self.arc_hashes.is_empty() {
            self.arc_hashes
                .retain(|_, entry| entry.weak.strong_count() > 0);
        }
    }

    /// Set the unified memory ceiling in bytes across decoded pictures, uploaded
    /// animation frames, SVG rasters, and decoded animation CPU frames.
    ///
    /// Entries over the new ceiling are dropped at once, except those used in the
    /// current frame (and recently active animation CPU buffers).
    pub fn set_picture_budget(&mut self, bytes: u64) {
        self.picture_budget = bytes;
        self.evict_over_budget();
    }

    /// Current unified memory ceiling, in bytes.
    #[must_use]
    pub fn picture_budget(&self) -> u64 {
        self.picture_budget
    }

    /// Set the maximum longest-side pixel dimension for canvas-tier raster decoding.
    pub fn set_max_canvas_dim(&mut self, max_dim: u32) {
        self.max_canvas_dim = max_dim.max(1);
    }

    /// Current maximum longest-side pixel dimension for canvas-tier raster decoding.
    #[must_use]
    pub fn max_canvas_dim(&self) -> u32 {
        self.max_canvas_dim
    }

    /// Set the maximum longest-side pixel dimension for thumbnail-tier raster decoding.
    pub fn set_max_thumb_dim(&mut self, max_dim: u32) {
        self.max_thumb_dim = max_dim.max(1);
    }

    /// Current maximum longest-side pixel dimension for thumbnail-tier raster decoding.
    #[must_use]
    pub fn max_thumb_dim(&self) -> u32 {
        self.max_thumb_dim
    }

    /// Apply the memory ceiling and raster decode caps from `profile`.
    pub fn apply_perf_profile(&mut self, profile: &crate::design::PerfProfile) {
        self.set_max_canvas_dim(profile.max_canvas_decode_px);
        self.set_max_thumb_dim(profile.max_thumb_decode_px);
        self.set_picture_budget(profile.media_budget_bytes);
    }

    /// Number of unique decoded picture textures resident in GPU memory
    /// (deduplicated across keys sharing identical image bytes).
    #[must_use]
    pub fn decoded_picture_count(&self) -> usize {
        self.shared_textures.len()
    }

    /// Bytes held by decoded picture textures, counted as RGBA (4 bytes per pixel).
    /// Includes uploaded animated GIF/WebP frames.
    fn picture_bytes(&self) -> u64 {
        self.still_bytes.saturating_add(self.anim_gpu_bytes)
    }

    /// Total bytes held across picture textures, uploaded animation frames,
    /// SVG raster textures, and decoded animation CPU frame buffers.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.still_bytes
            .saturating_add(self.anim_gpu_bytes)
            .saturating_add(self.svg_bytes)
            .saturating_add(self.anim_cpu_bytes)
    }

    fn tier_for_target(&self, target_px: (u32, u32)) -> DecodeTier {
        let longest = target_px.0.max(target_px.1);
        if longest <= 384 {
            DecodeTier::Thumbnail
        } else if longest <= 1024 {
            DecodeTier::Medium
        } else if longest <= 2048 {
            DecodeTier::Canvas
        } else {
            DecodeTier::Full
        }
    }

    fn tier_max_dim(&self, tier: DecodeTier) -> Option<(u32, u32)> {
        match tier {
            DecodeTier::Thumbnail => Some((self.max_thumb_dim, self.max_thumb_dim)),
            DecodeTier::Medium => {
                let cap = 1024.min(self.max_canvas_dim);
                Some((cap, cap))
            }
            DecodeTier::Canvas => Some((self.max_canvas_dim, self.max_canvas_dim)),
            DecodeTier::Full => None,
        }
    }

    /// Decrement shared texture refcount and release GPU byte accounting when
    /// `key` is replaced, removed, or evicted.
    fn release_entry_meta(&mut self, key: &str) {
        if let Some(meta) = self.picture_meta.remove(key)
            && let Some(content_key) = meta.content_key
        {
            let mut remove_shared = false;
            if let Some(shared) = self.shared_textures.get_mut(&content_key) {
                shared.refcount = shared.refcount.saturating_sub(1);
                if shared.refcount == 0 {
                    self.still_bytes = self.still_bytes.saturating_sub(shared.uploaded_bytes);
                    remove_shared = true;
                }
            }
            if remove_shared {
                self.shared_textures.remove(&content_key);
            }
        }
    }

    /// Remember that a decoded picture was handed out on the current frame.
    fn note_picture_use(&mut self, key: &str) {
        self.use_counter = self.use_counter.saturating_add(1);
        let counter = self.use_counter;
        let frame = self.frame;
        if let Some(used) = self.picture_uses.get_mut(key) {
            used.counter = counter;
            used.frame = frame;
            return;
        }
        self.picture_uses
            .insert(key.to_owned(), PictureUse { counter, frame });
    }

    /// Drop the least recently used still textures, uploaded animation frames,
    /// cold SVG rasters, or cold animation CPU buffers until `total_bytes()`
    /// fits within `picture_budget`. Assets used in the current frame stay.
    fn evict_over_budget(&mut self) {
        enum EvictTarget {
            Entry(String),
            AnimFrame(String, usize, String),
            SvgRaster(String, usize),
            ColdAnimation(String),
        }

        fn pick_older(
            best: Option<(u64, EvictTarget)>,
            candidate: Option<(u64, EvictTarget)>,
        ) -> Option<(u64, EvictTarget)> {
            match (best, candidate) {
                (Some((c1, t1)), Some((c2, t2))) => {
                    if c1 <= c2 {
                        Some((c1, t1))
                    } else {
                        Some((c2, t2))
                    }
                }
                (Some(item), None) | (None, Some(item)) => Some(item),
                (None, None) => None,
            }
        }

        while self.total_bytes() > self.picture_budget {
            let oldest_entry = self
                .entries
                .iter()
                .filter(|(key, entry)| {
                    matches!(entry, MediaEntry::Texture(..))
                        && self
                            .picture_uses
                            .get(key.as_str())
                            .is_none_or(|used| used.frame < self.frame)
                })
                .map(|(key, _)| {
                    let counter = self
                        .picture_uses
                        .get(key.as_str())
                        .map_or(0, |used| used.counter);
                    (counter, EvictTarget::Entry(key.clone()))
                })
                .min_by_key(|(counter, _)| *counter);

            let picture_uses = &self.picture_uses;
            let frame = self.frame;
            let oldest_anim = self
                .animations
                .iter()
                .flat_map(|(src, entry)| {
                    entry
                        .frames
                        .iter()
                        .enumerate()
                        .filter_map(move |(idx, slot)| {
                            slot.as_ref()?;
                            let key = format!("{src}\u{1}anim\u{1}{idx}");
                            let usable =
                                picture_uses.get(&key).is_none_or(|used| used.frame < frame);
                            if !usable {
                                return None;
                            }
                            let counter = picture_uses.get(&key).map_or(0, |used| used.counter);
                            Some((counter, EvictTarget::AnimFrame(src.clone(), idx, key)))
                        })
                })
                .min_by_key(|(counter, _)| *counter);

            let oldest_svg = self
                .svg
                .iter()
                .flat_map(|(src, asset)| {
                    asset
                        .entries
                        .iter()
                        .enumerate()
                        .filter_map(move |(idx, entry)| {
                            if entry.last_frame >= frame {
                                return None;
                            }
                            Some((entry.global_use, EvictTarget::SvgRaster(src.clone(), idx)))
                        })
                })
                .min_by_key(|(counter, _)| *counter);

            let oldest_cold_anim = self
                .animations
                .iter()
                .filter_map(|(src, entry)| {
                    entry.animation.as_ref()?;
                    if frame.saturating_sub(entry.last_frame) <= COLD_ANIMATION_FRAMES {
                        return None;
                    }
                    Some((entry.global_use, EvictTarget::ColdAnimation(src.clone())))
                })
                .min_by_key(|(counter, _)| *counter);

            let best = pick_older(oldest_entry, oldest_anim);
            let best = pick_older(best, oldest_svg);
            let best = pick_older(best, oldest_cold_anim);

            let Some((_, target)) = best else {
                break;
            };
            match target {
                EvictTarget::Entry(key) => {
                    self.release_entry_meta(&key);
                    self.entries.remove(&key);
                    self.picture_uses.remove(&key);
                }
                EvictTarget::AnimFrame(src, idx, key) => {
                    if let Some(entry) = self.animations.get_mut(&src)
                        && let Some(slot) = entry.frames.get_mut(idx)
                        && slot.take().is_some()
                    {
                        entry.uploaded_frames = entry.uploaded_frames.saturating_sub(1);
                        self.anim_gpu_bytes = self.anim_gpu_bytes.saturating_sub(entry.frame_bytes);
                    }
                    self.picture_uses.remove(&key);
                }
                EvictTarget::SvgRaster(src, idx) => {
                    let mut remove_asset = false;
                    if let Some(asset) = self.svg.get_mut(&src) {
                        if idx < asset.entries.len() {
                            let removed = asset.entries.remove(idx);
                            self.svg_bytes = self.svg_bytes.saturating_sub(removed.uploaded_bytes);
                        }
                        remove_asset = asset.entries.is_empty();
                    }
                    if remove_asset {
                        self.svg.remove(&src);
                    }
                }
                EvictTarget::ColdAnimation(src) => {
                    if let Some(entry) = self.animations.remove(&src) {
                        self.anim_cpu_bytes = self.anim_cpu_bytes.saturating_sub(entry.cpu_bytes);
                        self.anim_gpu_bytes = self.anim_gpu_bytes.saturating_sub(
                            entry.frame_bytes.saturating_mul(entry.uploaded_frames),
                        );
                    }
                    let prefix = format!("{src}\u{1}anim\u{1}");
                    self.picture_uses.retain(|k, _| !k.starts_with(&prefix));
                }
            }
        }
        self.arc_hashes
            .retain(|_, entry| entry.weak.strong_count() > 0);
    }

    /// Set the clock animations follow, in milliseconds since the view started.
    ///
    /// `None` (the default) draws every animation's first frame, which is what
    /// the canvas, thumbnails and offscreen renders need. Clearing the clock
    /// drops decoded animation frames so preview memory is freed on close.
    pub fn set_animation_time(&mut self, time_ms: Option<u64>) {
        if time_ms.is_none() && self.animation_time_ms.is_some() {
            self.animations.clear();
            self.anim_gpu_bytes = 0;
            self.anim_cpu_bytes = 0;
            self.picture_uses
                .retain(|key, _| !key.contains("\u{1}anim\u{1}"));
        }
        self.animation_time_ms = time_ms;
    }

    /// The animation clock used by the next painted frame.
    #[must_use]
    pub fn animation_time_ms(&self) -> Option<u64> {
        self.animation_time_ms
    }

    /// Load and parse a Lottie animation once per source.
    ///
    /// A pending asset is retried on the next frame. A missing or rejected file
    /// is remembered so it is not parsed again.
    pub fn lottie_view(&mut self, src: &str, source: &dyn MediaSource) -> LottieView<'_> {
        if !self.lottie.contains_key(src) {
            match source.get(src) {
                MediaFetch::Ready(bytes) => match lottie::parse_composition(&bytes) {
                    Ok(composition) => {
                        let Some(info) = lottie::info_of(&composition) else {
                            self.lottie.insert(
                                src.to_owned(),
                                LottieEntry::Failed("invalid animation timeline".to_string()),
                            );
                            return self.lottie_view(src, source);
                        };
                        self.lottie
                            .insert(src.to_owned(), LottieEntry::Ready { composition, info });
                    }
                    Err(error) => {
                        log::warn!("Lottie {src} could not be parsed: {error}");
                        self.lottie
                            .insert(src.to_owned(), LottieEntry::Failed(error));
                    }
                },
                MediaFetch::Pending => return LottieView::Pending,
                MediaFetch::Missing => {
                    self.lottie.insert(
                        src.to_owned(),
                        LottieEntry::Failed("animation not found".to_string()),
                    );
                }
            }
        }
        match self.lottie.get(src) {
            Some(LottieEntry::Ready { composition, info }) => LottieView::Ready {
                composition,
                info: *info,
            },
            Some(LottieEntry::Failed(message)) => LottieView::Failed(message.as_str()),
            None => LottieView::Pending,
        }
    }

    /// Parsed metadata for `src`, once the animation has been loaded.
    #[must_use]
    pub fn lottie_info(&self, src: &str) -> Option<LottieInfo> {
        match self.lottie.get(src) {
            Some(LottieEntry::Ready { info, .. }) => Some(*info),
            Some(LottieEntry::Failed(_)) | None => None,
        }
    }

    /// A non-fatal Lottie warning for `src`, if it failed to parse.
    #[must_use]
    pub fn lottie_warning(&self, src: &str) -> Option<&str> {
        match self.lottie.get(src) {
            Some(LottieEntry::Failed(message)) => Some(message.as_str()),
            Some(LottieEntry::Ready { .. }) | None => None,
        }
    }

    /// Non-looping Lottie objects that have played past their last frame.
    ///
    /// The host feeds these as [`Input::MediaEnded`], exactly as a playback
    /// backend reports finished audio or video. The runtime decides what
    /// `media_ends` triggers do.
    #[must_use]
    pub fn lottie_ended(&self, player: &Player<'_>) -> Vec<Input> {
        let Some(slide) = player.current_slide() else {
            return Vec::new();
        };
        let mut ended = Vec::new();
        for object in slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        {
            let ObjectKind::Lottie {
                src,
                loop_playback: false,
                speed,
                ..
            } = &object.kind
            else {
                continue;
            };
            if !player.is_media_playing(&object.id) {
                continue;
            }
            let (Some(clock), Some(info)) =
                (player.media_clock_ms(&object.id), self.lottie_info(src))
            else {
                continue;
            };
            // The clock runs in timeline time; the animation runs at `speed` times that.
            let played_ms =
                clock as f64 * f64::from(coursecraft_course::clamp_lottie_speed(*speed));
            if played_ms >= info.duration_ms as f64 {
                ended.push(Input::MediaEnded {
                    object: object.id.clone(),
                });
            }
        }
        ended
    }

    /// Load a texture for a picture object drawn at `target_px` pixels.
    ///
    /// Vector images are rasterised at the drawn size (with a size bucket and a
    /// debounce); animated GIF/WebP frames follow [`Self::set_animation_time`];
    /// raster images are decoded at the matching display tier while preserving
    /// natural dimensions.
    pub fn texture_for<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
        target_px: (u32, u32),
    ) -> MediaView<'a> {
        if is_svg_src(src) {
            return self.svg_texture(ctx, src, source, target_px);
        }
        if self.animation_time_ms.is_some() && is_animated_src(src) {
            self.prepare_animation(src, source);
            if self
                .animations
                .get(src)
                .is_some_and(|entry| entry.animation.is_some())
            {
                return self
                    .animation_texture(ctx, src)
                    .unwrap_or(MediaView::Pending);
            }
        }
        let tier = self.tier_for_target(target_px);
        self.texture_with_key(ctx, src, src, source, TextureOptions::LINEAR, tier, false)
    }

    /// Retrieve or load the full-resolution texture for `src`, using `source` to fetch missing bytes.
    pub fn texture<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
    ) -> MediaView<'a> {
        self.texture_with_key(
            ctx,
            src,
            src,
            source,
            TextureOptions::LINEAR,
            DecodeTier::Full,
            false,
        )
    }

    /// Retrieve or load a repeat-wrapped copy of `src`, for tiled picture fills.
    ///
    /// The copy has its own cache entry so ordinary images keep clamping at
    /// their edges. Both share the same source bytes and decode limits.
    pub fn texture_tiled<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
    ) -> MediaView<'a> {
        let options = TextureOptions {
            wrap_mode: egui::TextureWrapMode::Repeat,
            ..TextureOptions::LINEAR
        };
        let key = format!("{src}\u{1}tile");
        self.texture_with_key(ctx, &key, src, source, options, DecodeTier::Full, true)
    }

    /// Load and rasterise an SVG at (or close to) the drawn size.
    fn svg_texture<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
        target_px: (u32, u32),
    ) -> MediaView<'a> {
        if matches!(self.entries.get(src), Some(MediaEntry::Failed(_))) {
            return self.failed_view(src);
        }
        let now = ctx.input(|input| input.time);
        let now = if now.is_finite() { now } else { 0.0 };
        if !self.svg.contains_key(src) {
            match source.get(src) {
                MediaFetch::Ready(bytes) => {
                    let Some(natural) = svg_natural_size(&bytes) else {
                        self.release_entry_meta(src);
                        self.entries.insert(
                            src.to_owned(),
                            MediaEntry::Failed("invalid SVG".to_string()),
                        );
                        return self.failed_view(src);
                    };
                    let desired = fit_svg_target(natural, target_px, &self.limits);
                    match rasterise_svg(ctx, src, &bytes, desired, &self.limits) {
                        Ok((texture, size)) => {
                            let uploaded_bytes = u64::from(size.0)
                                .saturating_mul(u64::from(size.1))
                                .saturating_mul(4);
                            self.use_counter = self.use_counter.saturating_add(1);
                            let global_use = self.use_counter;
                            let frame = self.frame;
                            self.svg_bytes = self.svg_bytes.saturating_add(uploaded_bytes);
                            self.svg.insert(
                                src.to_owned(),
                                SvgAsset {
                                    bytes,
                                    natural,
                                    entries: vec![SvgEntry {
                                        texture,
                                        target: size,
                                        uploaded_bytes,
                                        last_used: 1,
                                        last_frame: frame,
                                        global_use,
                                    }],
                                    last_raster: now,
                                    use_counter: 1,
                                },
                            );
                            if self.total_bytes() > self.picture_budget {
                                self.evict_over_budget();
                            }
                        }
                        Err(error) => {
                            self.release_entry_meta(src);
                            self.entries
                                .insert(src.to_owned(), MediaEntry::Failed(error.to_string()));
                            return self.failed_view(src);
                        }
                    }
                }
                MediaFetch::Pending => return MediaView::Pending,
                MediaFetch::Missing => {
                    self.release_entry_meta(src);
                    self.entries.insert(
                        src.to_owned(),
                        MediaEntry::Failed("image not found".to_string()),
                    );
                    return self.failed_view(src);
                }
            }
        }

        let Some(asset) = self.svg.get(src) else {
            return MediaView::Pending;
        };
        let desired = fit_svg_target(asset.natural, target_px, &self.limits);

        // Prefer a cached raster within 25% of the new drawn size.
        let mut reusable: Option<usize> = None;
        let mut reusable_diff = f32::INFINITY;
        let mut nearest: Option<usize> = None;
        let mut nearest_diff = f32::INFINITY;
        for (index, entry) in asset.entries.iter().enumerate() {
            let diff = (entry.target.0 as f32 - desired.0 as f32).abs()
                + (entry.target.1 as f32 - desired.1 as f32).abs();
            if diff < nearest_diff {
                nearest = Some(index);
                nearest_diff = diff;
            }
            if !svg_needs_rerasterise(entry.target, desired) && diff < reusable_diff {
                reusable = Some(index);
                reusable_diff = diff;
            }
        }

        if let Some(index) = reusable {
            self.use_counter = self.use_counter.saturating_add(1);
            let global_use = self.use_counter;
            let frame = self.frame;
            if let Some(asset) = self.svg.get_mut(src) {
                asset.use_counter = asset.use_counter.saturating_add(1);
                if let Some(entry) = asset.entries.get_mut(index) {
                    entry.last_used = asset.use_counter;
                    entry.last_frame = frame;
                    entry.global_use = global_use;
                }
            }
            return self.svg_view(src, index);
        }

        // Debounce a resize drag: keep the old raster and come back later.
        let elapsed = (now - asset.last_raster).max(0.0);
        if nearest.is_some() && elapsed < SVG_RERASTERISE_DEBOUNCE_SECONDS {
            let wait = SVG_RERASTERISE_DEBOUNCE_SECONDS - elapsed;
            ctx.request_repaint_after(Duration::from_secs_f64(wait.max(0.01)));
            if let Some(index) = nearest {
                self.use_counter = self.use_counter.saturating_add(1);
                let global_use = self.use_counter;
                let frame = self.frame;
                if let Some(asset) = self.svg.get_mut(src)
                    && let Some(entry) = asset.entries.get_mut(index)
                {
                    entry.last_frame = frame;
                    entry.global_use = global_use;
                }
                return self.svg_view(src, index);
            }
            return MediaView::Pending;
        }

        let Some(asset) = self.svg.get(src) else {
            return MediaView::Pending;
        };
        let bytes = Arc::clone(&asset.bytes);
        match rasterise_svg(ctx, src, &bytes, desired, &self.limits) {
            Ok((texture, size)) => {
                let uploaded_bytes = u64::from(size.0)
                    .saturating_mul(u64::from(size.1))
                    .saturating_mul(4);
                self.use_counter = self.use_counter.saturating_add(1);
                let global_use = self.use_counter;
                let frame = self.frame;
                if let Some(asset) = self.svg.get_mut(src) {
                    asset.use_counter = asset.use_counter.saturating_add(1);
                    let used = asset.use_counter;
                    asset.entries.push(SvgEntry {
                        texture,
                        target: size,
                        uploaded_bytes,
                        last_used: used,
                        last_frame: frame,
                        global_use,
                    });
                    self.svg_bytes = self.svg_bytes.saturating_add(uploaded_bytes);
                    asset.last_raster = now;
                    // Keep at most three sizes per source.
                    while asset.entries.len() > 3 {
                        let Some(oldest) = asset
                            .entries
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, entry)| entry.last_used)
                            .map(|(index, _)| index)
                        else {
                            break;
                        };
                        let removed = asset.entries.remove(oldest);
                        self.svg_bytes = self.svg_bytes.saturating_sub(removed.uploaded_bytes);
                    }
                }
                if self.total_bytes() > self.picture_budget {
                    self.evict_over_budget();
                }
                let Some(index) = self
                    .svg
                    .get(src)
                    .and_then(|asset| asset.entries.len().checked_sub(1))
                else {
                    return MediaView::Pending;
                };
                self.svg_view(src, index)
            }
            Err(error) => {
                if let Some(index) = nearest {
                    log::warn!("SVG {src} could not be rasterised: {error}");
                    self.svg_view(src, index)
                } else {
                    self.release_entry_meta(src);
                    self.entries
                        .insert(src.to_owned(), MediaEntry::Failed(error.to_string()));
                    self.failed_view(src)
                }
            }
        }
    }

    fn svg_view(&self, src: &str, index: usize) -> MediaView<'_> {
        let Some(entry) = self.svg.get(src).and_then(|asset| asset.entries.get(index)) else {
            return MediaView::Pending;
        };
        MediaView::Ready {
            texture: &entry.texture,
            size: [entry.target.0, entry.target.1],
        }
    }

    fn failed_view(&self, src: &str) -> MediaView<'_> {
        match self.entries.get(src) {
            Some(MediaEntry::Failed(message)) => MediaView::Failed(message.as_str()),
            Some(MediaEntry::Loading) | None => MediaView::Pending,
            Some(MediaEntry::Texture(handle, size)) => MediaView::Ready {
                texture: handle,
                size: *size,
            },
        }
    }

    /// Decode an animation source once, remembering a warning on failure.
    fn prepare_animation(&mut self, src: &str, source: &dyn MediaSource) {
        if self.animations.contains_key(src) {
            return;
        }
        match source.get(src) {
            MediaFetch::Ready(bytes) => match decode_animation(&bytes, &self.limits) {
                Ok(Some(animation)) => {
                    let count = animation.frames.len();
                    let cpu_bytes = animation.frames.iter().fold(0u64, |acc, frame| {
                        acc.saturating_add(frame.image.pixels.len() as u64)
                    });
                    let frame_bytes = u64::from(animation.width)
                        .saturating_mul(u64::from(animation.height))
                        .saturating_mul(4);
                    self.use_counter = self.use_counter.saturating_add(1);
                    let global_use = self.use_counter;
                    let frame = self.frame;
                    self.anim_cpu_bytes = self.anim_cpu_bytes.saturating_add(cpu_bytes);
                    self.animations.insert(
                        src.to_owned(),
                        AnimationEntry {
                            animation: Some(animation),
                            frames: vec![None; count],
                            warning: None,
                            cpu_bytes,
                            frame_bytes,
                            uploaded_frames: 0,
                            last_frame: frame,
                            global_use,
                        },
                    );
                }
                Ok(None) => {
                    self.animations.insert(
                        src.to_owned(),
                        AnimationEntry {
                            animation: None,
                            frames: Vec::new(),
                            warning: None,
                            cpu_bytes: 0,
                            frame_bytes: 0,
                            uploaded_frames: 0,
                            last_frame: self.frame,
                            global_use: self.use_counter,
                        },
                    );
                }
                Err(error) => {
                    log::warn!("Animation {src} could not be decoded: {error}");
                    self.animations.insert(
                        src.to_owned(),
                        AnimationEntry {
                            animation: None,
                            frames: Vec::new(),
                            warning: Some(error.to_string()),
                            cpu_bytes: 0,
                            frame_bytes: 0,
                            uploaded_frames: 0,
                            last_frame: self.frame,
                            global_use: self.use_counter,
                        },
                    );
                }
            },
            MediaFetch::Pending | MediaFetch::Missing => {}
        }
    }

    /// Pick the animation frame for the current clock, uploading it lazily.
    fn animation_texture<'a>(&'a mut self, ctx: &Context, src: &str) -> Option<MediaView<'a>> {
        let elapsed = self.animation_time_ms?;
        let entry = self.animations.get(src)?;
        let animation = entry.animation.as_ref()?;
        let index = animation.frame_index_at(elapsed);
        let delay = animation.ms_until_next_frame(elapsed);
        ctx.request_repaint_after(Duration::from_millis(u64::from(delay.max(1))));

        let uploaded = entry.frames.get(index).is_some_and(Option::is_some);
        let key = format!("{src}\u{1}anim\u{1}{index}");

        if !uploaded {
            let frame = animation.frames.get(index)?;
            let color = ColorImage::from_rgba_unmultiplied(
                [frame.image.width as usize, frame.image.height as usize],
                &frame.image.pixels,
            );
            let handle = ctx.load_texture(key.clone(), color, TextureOptions::LINEAR);
            if let Some(entry) = self.animations.get_mut(src)
                && let Some(slot) = entry.frames.get_mut(index)
                && slot.is_none()
            {
                *slot = Some(handle);
                entry.uploaded_frames = entry.uploaded_frames.saturating_add(1);
                self.anim_gpu_bytes = self.anim_gpu_bytes.saturating_add(entry.frame_bytes);
            }
        }

        self.use_counter = self.use_counter.saturating_add(1);
        let global_use = self.use_counter;
        let current_frame = self.frame;
        if let Some(entry) = self.animations.get_mut(src) {
            entry.last_frame = current_frame;
            entry.global_use = global_use;
        }

        self.note_picture_use(&key);
        if self.total_bytes() > self.picture_budget {
            self.evict_over_budget();
        }

        let entry = self.animations.get(src)?;
        let animation = entry.animation.as_ref()?;
        let texture = entry.frames.get(index)?.as_ref()?;
        Some(MediaView::Ready {
            texture,
            size: [animation.width, animation.height],
        })
    }

    /// A non-fatal animation warning (for example, over the frame cap).
    #[must_use]
    pub fn animation_warning(&self, src: &str) -> Option<&str> {
        self.animations
            .get(src)
            .and_then(|entry| entry.warning.as_deref())
    }

    /// Whether an image source has been decoded (ordinary, vector or animated).
    #[must_use]
    pub fn is_loaded_image(&self, src: &str) -> bool {
        match self.entries.get(src) {
            Some(MediaEntry::Texture(..)) => true,
            _ => self
                .svg
                .get(src)
                .is_some_and(|asset| !asset.entries.is_empty()),
        }
    }

    /// Natural size of a vector source, if it has been parsed.
    #[must_use]
    pub fn natural_size(&self, src: &str) -> Option<(f32, f32)> {
        self.svg.get(src).map(|asset| asset.natural)
    }

    /// The most recently used texture for `src`, including vector rasters.
    #[must_use]
    pub fn texture_id(&self, src: &str) -> Option<egui::TextureId> {
        if let Some(asset) = self.svg.get(src) {
            let entry = asset.entries.iter().max_by_key(|entry| entry.last_used)?;
            return Some(entry.texture.id());
        }
        match self.entries.get(src) {
            Some(MediaEntry::Texture(handle, _)) => Some(handle.id()),
            _ => None,
        }
    }

    fn content_hash(&mut self, bytes: &Arc<[u8]>) -> u64 {
        let ptr_key = ArcPtrKey {
            ptr: bytes.as_ptr() as usize,
            len: bytes.len(),
        };
        if let Some(entry) = self.arc_hashes.get(&ptr_key)
            && entry
                .weak
                .upgrade()
                .is_some_and(|alive| Arc::ptr_eq(&alive, bytes))
        {
            return entry.hash;
        }
        let hash = content_hash_64(bytes);
        if !self.arc_hashes.is_empty() {
            self.arc_hashes
                .retain(|_, entry| entry.weak.strong_count() > 0);
            if self.arc_hashes.len() >= MAX_ARC_HASH_ENTRIES {
                self.arc_hashes.clear();
            }
        }
        self.arc_hashes.insert(
            ptr_key,
            ArcHashEntry {
                weak: Arc::downgrade(bytes),
                hash,
            },
        );
        hash
    }

    #[allow(clippy::too_many_arguments)]
    fn texture_with_key<'a>(
        &'a mut self,
        ctx: &Context,
        key: &str,
        src: &str,
        source: &dyn MediaSource,
        options: TextureOptions,
        requested_tier: DecodeTier,
        tiled: bool,
    ) -> MediaView<'a> {
        let should_fetch = match self.entries.get(key) {
            None | Some(MediaEntry::Loading) => true,
            Some(MediaEntry::Failed(..)) => false,
            Some(MediaEntry::Texture(..)) => self
                .picture_meta
                .get(key)
                .is_none_or(|meta| meta.tier < requested_tier),
        };

        if should_fetch {
            match source.get(src) {
                MediaFetch::Ready(bytes) => {
                    let content_key = ContentKey {
                        len: bytes.len(),
                        hash: self.content_hash(&bytes),
                        tier: requested_tier,
                        tiled,
                    };
                    if let Some(shared) = self.shared_textures.get_mut(&content_key) {
                        shared.refcount = shared.refcount.saturating_add(1);
                        let handle = shared.handle.clone();
                        let natural_size = shared.natural_size;
                        self.release_entry_meta(key);
                        self.picture_meta.insert(
                            key.to_owned(),
                            PictureMeta {
                                tier: requested_tier,
                                content_key: Some(content_key),
                            },
                        );
                        self.entries
                            .insert(key.to_owned(), MediaEntry::Texture(handle, natural_size));
                    } else {
                        let max_dim = self.tier_max_dim(requested_tier);
                        match decode_image_fitted(&bytes, max_dim, &self.limits) {
                            Ok((img, natural_size)) => {
                                #[cfg(debug_assertions)]
                                test_counts::note_picture_decode();
                                #[cfg(debug_assertions)]
                                if requested_tier == DecodeTier::Thumbnail
                                    && img.width.max(img.height) > self.max_thumb_dim
                                {
                                    test_counts::note_full_res_thumbnail_decode();
                                }
                                let uploaded_w = img.width;
                                let uploaded_h = img.height;
                                let uploaded_bytes = u64::from(uploaded_w)
                                    .saturating_mul(u64::from(uploaded_h))
                                    .saturating_mul(4);
                                let color_image = ColorImage::from_rgba_unmultiplied(
                                    [uploaded_w as usize, uploaded_h as usize],
                                    &img.pixels,
                                );
                                drop(img);
                                let handle = ctx.load_texture(key.to_owned(), color_image, options);
                                self.release_entry_meta(key);
                                self.shared_textures.insert(
                                    content_key,
                                    SharedTexture {
                                        handle: handle.clone(),
                                        natural_size,
                                        uploaded_bytes,
                                        refcount: 1,
                                    },
                                );
                                self.still_bytes = self.still_bytes.saturating_add(uploaded_bytes);
                                self.picture_meta.insert(
                                    key.to_owned(),
                                    PictureMeta {
                                        tier: requested_tier,
                                        content_key: Some(content_key),
                                    },
                                );
                                self.entries.insert(
                                    key.to_owned(),
                                    MediaEntry::Texture(handle, natural_size),
                                );
                            }
                            Err(e) => {
                                self.release_entry_meta(key);
                                self.entries
                                    .insert(key.to_owned(), MediaEntry::Failed(e.to_string()));
                            }
                        }
                    }
                }
                MediaFetch::Pending => {
                    if !matches!(self.entries.get(key), Some(MediaEntry::Texture(..))) {
                        self.entries.insert(key.to_owned(), MediaEntry::Loading);
                    }
                }
                MediaFetch::Missing => {
                    self.release_entry_meta(key);
                    self.entries.insert(
                        key.to_owned(),
                        MediaEntry::Failed("image not found".to_string()),
                    );
                }
            }
        }

        if matches!(self.entries.get(key), Some(MediaEntry::Texture(..))) {
            self.note_picture_use(key);
            if self.total_bytes() > self.picture_budget {
                self.evict_over_budget();
            }
        }

        match self.entries.get(key) {
            Some(MediaEntry::Texture(handle, size)) => MediaView::Ready {
                texture: handle,
                size: *size,
            },
            Some(MediaEntry::Loading) => MediaView::Pending,
            Some(MediaEntry::Failed(msg)) => MediaView::Failed(msg.as_str()),
            None => MediaView::Pending,
        }
    }

    fn retain_by<F: Fn(&str) -> bool>(&mut self, keep: F) {
        let mut removed_keys = Vec::new();
        self.entries.retain(|key, _| {
            let kept = keep(key_source(key));
            if !kept {
                removed_keys.push(key.clone());
            }
            kept
        });
        for key in &removed_keys {
            self.release_entry_meta(key);
        }
        self.picture_uses.retain(|key, _| keep(key_source(key)));
        self.svg.retain(|key, asset| {
            let kept = keep(key.as_str());
            if !kept {
                for entry in &asset.entries {
                    self.svg_bytes = self.svg_bytes.saturating_sub(entry.uploaded_bytes);
                }
            }
            kept
        });
        self.animations.retain(|key, entry| {
            let kept = keep(key.as_str());
            if !kept {
                self.anim_cpu_bytes = self.anim_cpu_bytes.saturating_sub(entry.cpu_bytes);
                self.anim_gpu_bytes = self
                    .anim_gpu_bytes
                    .saturating_sub(entry.frame_bytes.saturating_mul(entry.uploaded_frames));
            }
            kept
        });
        self.lottie.retain(|key, _| keep(key.as_str()));
        self.video.retain(|key, _| keep(key.as_str()));
        self.arc_hashes
            .retain(|_, entry| entry.weak.strong_count() > 0);
    }

    /// Retain only textures whose keys are in `srcs`, dropping all others.
    pub fn retain<S: std::borrow::Borrow<str> + std::hash::Hash + Eq>(
        &mut self,
        srcs: &HashSet<S>,
    ) {
        self.retain_by(|key| srcs.contains(key));
    }

    /// Retain only textures whose keys are in the provided slice.
    pub fn retain_slice(&mut self, srcs: &[&str]) {
        self.retain_by(|key| srcs.contains(&key));
    }

    /// Upload (or update in place) the decoded frame for a video object.
    ///
    /// Keyed by object id, like images are keyed by source. The texture is
    /// updated in place when the same `Arc` frame is not re-uploaded.
    pub fn video_texture<'a>(
        &'a mut self,
        ctx: &Context,
        object: &str,
        frame: &Arc<coursecraft_playback::VideoFrame>,
    ) -> Option<MediaView<'a>> {
        let (width, height) = (frame.width, frame.height);
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return None;
        }
        let expected = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(4);
        if frame.rgba.len() as u64 != expected {
            return None;
        }
        let size = [width, height];
        let frame_weak = Arc::downgrade(frame);
        let needs_load = self
            .video
            .get(object)
            .is_none_or(|entry| entry.size != size);
        if needs_load {
            let color =
                ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &frame.rgba);
            let key = format!("video:{object}");
            let handle = ctx.load_texture(key, color, TextureOptions::LINEAR);
            self.video.insert(
                object.to_string(),
                VideoTextureEntry {
                    handle,
                    size,
                    last_frame: frame_weak,
                },
            );
        } else if let Some(entry) = self.video.get_mut(object)
            && !Weak::ptr_eq(&entry.last_frame, &frame_weak)
        {
            let color =
                ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &frame.rgba);
            entry.handle.set(color, TextureOptions::LINEAR);
            entry.last_frame = frame_weak;
        }
        self.video.get(object).map(|entry| MediaView::Ready {
            texture: &entry.handle,
            size: entry.size,
        })
    }

    /// Look up the cached entry for `src`, if present.
    #[must_use]
    pub fn get_entry(&self, src: &str) -> Option<&MediaEntry> {
        self.entries.get(src)
    }

    /// Number of entries currently cached.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn key_source(key: &str) -> &str {
    key.split('\u{1}').next().unwrap_or(key)
}

/// Fit an SVG's natural aspect into the drawn pixel box.
fn fit_svg_target(natural: (f32, f32), target: (u32, u32), limits: &Limits) -> (u32, u32) {
    let max_width = target.0.max(1).min(limits.max_width.max(1));
    let max_height = target.1.max(1).min(limits.max_height.max(1));
    let (natural_w, natural_h) = natural;
    if !(natural_w.is_finite() && natural_h.is_finite() && natural_w > 0.0 && natural_h > 0.0) {
        return (max_width, max_height);
    }
    let aspect = natural_w / natural_h;
    let (mut width, mut height) = (max_width as f32, max_height as f32);
    if !aspect.is_finite() || aspect <= 0.0 {
        return (max_width, max_height);
    }
    if width / height > aspect {
        width = height * aspect;
    } else {
        height = width / aspect;
    }
    (
        (width.round() as u32).max(1),
        (height.round() as u32).max(1),
    )
}

fn rasterise_svg(
    ctx: &Context,
    src: &str,
    bytes: &[u8],
    target: (u32, u32),
    limits: &Limits,
) -> Result<(TextureHandle, (u32, u32)), coursecraft_media::MediaError> {
    let image = decode_svg(bytes, target, limits)?;
    let (width, height) = (image.width, image.height);
    let color =
        ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &image.pixels);
    drop(image);
    let key = format!("{src}\u{1}svg\u{1}{width}x{height}");
    let handle = ctx.load_texture(key, color, TextureOptions::LINEAR);
    Ok((handle, (width, height)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SlideTransform;
    use crate::paint::{Highlight, paint};
    use coursecraft_course::Size;
    use egui::{Pos2, Rect, vec2};
    use std::cell::Cell;

    struct NoMedia;
    impl MediaSource for NoMedia {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
    }

    fn solid_png(width: u32, height: u32) -> Vec<u8> {
        let pixels = vec![0u8; width as usize * height as usize * 4];
        coursecraft_media::encode_png_rgba(width, height, &pixels).expect("encode test png")
    }

    fn distinct_png(width: u32, height: u32, seed: usize) -> Vec<u8> {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        if pixels.len() >= 4 {
            pixels[0] = (seed & 0xFF) as u8;
            pixels[1] = ((seed >> 8) & 0xFF) as u8;
            pixels[2] = ((seed >> 16) & 0xFF) as u8;
            pixels[3] = 255;
        }
        coursecraft_media::encode_png_rgba(width, height, &pixels).expect("encode test png")
    }

    /// In-memory media for the cache tests. `fetches` proves a dropped picture is
    /// fetched again from the source when it is next needed.
    struct TestSource {
        bytes: HashMap<String, Arc<[u8]>>,
        fetches: Cell<usize>,
    }

    impl TestSource {
        fn new() -> Self {
            Self {
                bytes: HashMap::new(),
                fetches: Cell::new(0),
            }
        }

        fn add(&mut self, src: &str, bytes: Vec<u8>) {
            self.bytes.insert(src.to_owned(), Arc::from(bytes));
        }

        fn fetches(&self) -> usize {
            self.fetches.get()
        }
    }

    impl MediaSource for TestSource {
        fn get(&self, src: &str) -> MediaFetch {
            self.fetches.set(self.fetches.get().saturating_add(1));
            self.bytes
                .get(src)
                .cloned()
                .map_or(MediaFetch::Missing, MediaFetch::Ready)
        }
    }

    fn picture_key(n: usize) -> String {
        format!("p{n}.png")
    }

    #[test]
    fn svg_rerasterises_only_above_the_threshold() {
        assert!(!svg_needs_rerasterise((100, 100), (100, 100)));
        assert!(!svg_needs_rerasterise((100, 100), (120, 120)));
        assert!(!svg_needs_rerasterise((100, 100), (80, 80)));
        assert!(svg_needs_rerasterise((100, 100), (130, 100)));
        assert!(svg_needs_rerasterise((100, 100), (100, 70)));
        assert!(svg_needs_rerasterise((64, 64), (128, 64)));
    }

    #[test]
    fn static_paint_shows_the_first_animation_frame() {
        let ctx = egui::Context::default();
        let mut cache = MediaCache::new();
        cache.set_animation_time(Some(500));
        let transform = SlideTransform::fit(
            Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)),
            Size { w: 100, h: 100 },
        )
        .expect("transform");
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter();
            paint(
                painter,
                &transform,
                &[],
                Highlight::default(),
                Some((&mut cache, &NoMedia)),
                None,
                None,
            );
        });
        output.textures_delta.clear();
        assert_eq!(cache.animation_time_ms(), None);
    }

    /// Three pictures of 4×4 fit a budget of 3 × 64 bytes. Loading five, one per
    /// frame, keeps the three newest: the oldest-used ones go first.
    #[test]
    fn picture_budget_evicts_least_recently_used_first() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        for n in 0..5 {
            source.add(&picture_key(n), distinct_png(4, 4, n));
        }
        let mut cache = MediaCache::new();
        cache.set_picture_budget(3 * 4 * 4 * 4);

        for n in 0..5 {
            cache.begin_frame();
            let view = cache.texture(&ctx, &picture_key(n), &source);
            assert!(
                matches!(view, MediaView::Ready { size: [4, 4], .. }),
                "picture {n} should decode"
            );
        }

        assert!(cache.decoded_picture_count() <= 3);
        assert!(
            cache.get_entry(&picture_key(0)).is_none(),
            "p0 was the oldest"
        );
        assert!(
            cache.get_entry(&picture_key(1)).is_none(),
            "p1 was next oldest"
        );
        for n in 2..5 {
            assert!(
                matches!(
                    cache.get_entry(&picture_key(n)),
                    Some(MediaEntry::Texture(..))
                ),
                "picture {n} should be cached"
            );
        }
        assert!(cache.picture_bytes() <= cache.picture_budget());
    }

    /// A picture handed out on this frame is never evicted, even when that means
    /// the cache stays over budget until the next frame.
    #[test]
    fn picture_budget_keeps_pictures_used_this_frame() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        for n in 0..4 {
            source.add(&picture_key(n), distinct_png(4, 4, n));
        }
        let mut cache = MediaCache::new();
        let budget = 2 * 4 * 4 * 4;
        cache.set_picture_budget(budget);

        cache.begin_frame();
        for n in 0..2 {
            let _ = cache.texture(&ctx, &picture_key(n), &source);
        }
        assert!(cache.picture_bytes() <= budget);

        cache.begin_frame();
        // Both existing pictures are used now, then one more arrives.
        let _ = cache.texture(&ctx, &picture_key(0), &source);
        let _ = cache.texture(&ctx, &picture_key(1), &source);
        let _ = cache.texture(&ctx, &picture_key(2), &source);
        assert_eq!(cache.decoded_picture_count(), 3);
        assert!(
            cache.picture_bytes() > budget,
            "over budget until next frame"
        );
        for n in 0..3 {
            assert!(
                matches!(
                    cache.get_entry(&picture_key(n)),
                    Some(MediaEntry::Texture(..))
                ),
                "picture {n} was used this frame and must stay"
            );
        }

        // On the next frame those three are cold, so the oldest goes.
        cache.begin_frame();
        let _ = cache.texture(&ctx, &picture_key(3), &source);
        assert!(cache.get_entry(&picture_key(0)).is_none());
        assert!(cache.decoded_picture_count() <= 3);
    }

    /// An evicted picture is fetched and decoded again when it is next asked for.
    #[test]
    fn evicted_picture_is_decoded_again_on_demand() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        for n in 0..2 {
            source.add(&picture_key(n), distinct_png(8, 6, n));
        }
        let mut cache = MediaCache::new();
        cache.set_picture_budget(8 * 6 * 4);

        cache.begin_frame();
        let _ = cache.texture(&ctx, &picture_key(0), &source);
        let fetches_after_first = source.fetches();
        cache.begin_frame();
        let _ = cache.texture(&ctx, &picture_key(1), &source);
        assert!(cache.get_entry(&picture_key(0)).is_none(), "p0 was evicted");

        cache.begin_frame();
        let view = cache.texture(&ctx, &picture_key(0), &source);
        assert!(matches!(view, MediaView::Ready { size: [8, 6], .. }));
        assert_eq!(
            source.fetches(),
            fetches_after_first + 2,
            "p1 was fetched once, then p0 again"
        );
        assert!(cache.texture_id(&picture_key(0)).is_some());
    }

    /// Lottie compositions and failed entries are not evicted by the unified
    /// memory budget, while cold pictures and cold SVG rasters are evicted
    /// once they are no longer used on the current frame.
    #[test]
    fn picture_budget_ignores_svg_lottie_and_failed_entries() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        for n in 0..2 {
            source.add(&picture_key(n), distinct_png(4, 4, n));
        }
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#00aa00"/></svg>"##;
        source.add("vector.svg", svg.to_vec());
        let lottie = br#"{"v":"5.5.7","fr":30,"ip":0,"op":2,"w":8,"h":8,"nm":"tiny","ddd":0,"assets":[],"layers":[{"ddd":0,"ind":1,"ty":4,"nm":"Rect","sr":1,"ks":{"a":{"a":0,"k":[0,0]},"p":{"a":0,"k":[4,4]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}},"ao":0,"shapes":[{"ty":"gr","it":[{"ty":"rc","d":1,"s":{"a":0,"k":[4,4]},"p":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1},{"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}]}],"ip":0,"op":2,"st":0,"bm":0}]}"#;
        source.add("anim.json", lottie.to_vec());

        let mut cache = MediaCache::new();
        cache.set_picture_budget(4 * 4 * 4);
        cache.begin_frame();
        let _ = cache.texture(&ctx, &picture_key(0), &source);
        let _ = cache.texture_for(&ctx, "vector.svg", &source, (8, 8));
        let _ = cache.lottie_view("anim.json", &source);
        let _ = cache.texture(&ctx, "broken.png", &source);

        assert!(cache.natural_size("vector.svg").is_some());
        assert!(cache.texture_id("vector.svg").is_some());
        assert!(cache.lottie_info("anim.json").is_some());
        assert!(matches!(
            cache.get_entry("broken.png"),
            Some(MediaEntry::Failed(_))
        ));
        assert_eq!(cache.picture_bytes(), 4 * 4 * 4);

        // Next frame: cold picture and cold SVG raster are evicted to fit the
        // unified budget, while Lottie and failed entries remain cached.
        cache.begin_frame();
        let _ = cache.texture(&ctx, &picture_key(1), &source);
        assert!(cache.get_entry(&picture_key(0)).is_none());
        assert_eq!(cache.decoded_picture_count(), 1);
        assert!(cache.texture_id("vector.svg").is_none());
        assert!(cache.total_bytes() <= cache.picture_budget());
        assert!(cache.lottie_info("anim.json").is_some());
        assert!(matches!(
            cache.get_entry("broken.png"),
            Some(MediaEntry::Failed(_))
        ));
    }

    /// Minimal 2-frame 2×2 animated GIF (100 ms per frame, looping forever).
    fn tiny_animated_gif() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"GIF89a");
        // Logical screen descriptor: 2x2, GCT present (2 colors).
        out.extend_from_slice(&[2, 0, 2, 0, 0x80, 0, 0]);
        // Global color table: red, blue.
        out.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        // Netscape 2.0 looping application extension.
        out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
        out.extend_from_slice(b"NETSCAPE2.0");
        out.extend_from_slice(&[0x03, 0x01, 0, 0, 0]);
        // Two frames, each with 4 pixels of color 0 or 1.
        // LZW min code size 2: clear=4 (3 bits), pixel, pixel, pixel, pixel, eoi=5 (3 bits).
        // Frame 0 (all color 0): 100, 000, 000, 000, 000, 101 -> LSB-first bytes: 0x04, 0x00, 0x05?
        // Actually: bits 0..2=100, 3..5=000, 6..8=000, 9..11=000, 12..14=000, 15..17=101
        // Byte 0 (bits 0..7): 00_000_100 = 0x04
        // Byte 1 (bits 8..15): 1_000_000_0 = 0x80
        // Byte 2 (bits 16..17): 000000_10 = 0x02
        for lzw_bytes in [[0x04u8, 0x80, 0x02], [0x0Cu8, 0x82, 0x02]] {
            // Graphic control extension: 100 ms delay (10 * 10ms).
            out.extend_from_slice(&[0x21, 0xF9, 0x04, 0x04, 10, 0, 0, 0]);
            // Image descriptor: 2x2 at (0,0).
            out.extend_from_slice(&[0x2C, 0, 0, 0, 0, 2, 0, 2, 0, 0]);
            // Image data: min code size 2, sub-block of 3 bytes, terminator.
            out.push(0x02);
            out.push(3);
            out.extend_from_slice(&lzw_bytes);
            out.push(0x00);
        }
        out.push(0x3B);
        out
    }

    #[test]
    fn rejected_svg_is_not_fetched_or_parsed_again() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("bad.svg", b"<not-valid-svg".to_vec());
        let mut cache = MediaCache::new();
        cache.begin_frame();
        let first = cache.texture_for(&ctx, "bad.svg", &source, (100, 100));
        assert!(matches!(first, MediaView::Failed(_)));
        let fetches_after_first = source.fetches();

        for _ in 0..5 {
            cache.begin_frame();
            let view = cache.texture_for(&ctx, "bad.svg", &source, (100, 100));
            assert!(matches!(view, MediaView::Failed(_)));
        }
        assert_eq!(
            source.fetches(),
            fetches_after_first,
            "rejected SVG must not be fetched or parsed again on subsequent paints"
        );
    }

    #[test]
    fn animation_frames_count_in_picture_budget_and_evict_lru() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("anim.gif", tiny_animated_gif());
        source.add("still.png", solid_png(2, 2));

        let mut cache = MediaCache::new();
        // Budget fits 1 frame (2x2x4 = 16 bytes).
        cache.set_picture_budget(16);
        cache.set_animation_time(Some(0));
        cache.begin_frame();
        let view = cache.texture_for(&ctx, "anim.gif", &source, (2, 2));
        assert!(matches!(view, MediaView::Ready { size: [2, 2], .. }));
        assert_eq!(cache.picture_bytes(), 16);

        // Next frame advances to frame 1; frame 0 is evicted to stay within budget.
        cache.begin_frame();
        cache.set_animation_time(Some(100));
        let view = cache.texture_for(&ctx, "anim.gif", &source, (2, 2));
        assert!(matches!(view, MediaView::Ready { size: [2, 2], .. }));
        assert!(cache.picture_bytes() <= 16);
    }

    #[test]
    fn animation_frames_are_dropped_when_preview_closes() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("anim.gif", tiny_animated_gif());

        let mut cache = MediaCache::new();
        cache.set_animation_time(Some(0));
        cache.begin_frame();
        let view = cache.texture_for(&ctx, "anim.gif", &source, (2, 2));
        assert!(matches!(view, MediaView::Ready { size: [2, 2], .. }));
        assert!(cache.picture_bytes() > 0);
        assert!(!cache.animations.is_empty());

        cache.set_animation_time(None);
        assert_eq!(cache.picture_bytes(), 0);
        assert!(cache.animations.is_empty());
    }

    #[test]
    fn s1_media_cache_total_bytes_tracks_pictures_svgs_and_animations() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("pic.png", solid_png(4, 4));
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#00aa00"/></svg>"##;
        source.add("vec.svg", svg.to_vec());
        source.add("anim.gif", tiny_animated_gif());

        let mut cache = MediaCache::new();
        assert_eq!(cache.total_bytes(), 0);

        cache.begin_frame();
        let pic = cache.texture(&ctx, "pic.png", &source);
        assert!(matches!(pic, MediaView::Ready { size: [4, 4], .. }));
        let pic_bytes = 4 * 4 * 4;
        assert_eq!(cache.total_bytes(), pic_bytes);

        let svg_view = cache.texture_for(&ctx, "vec.svg", &source, (8, 8));
        assert!(matches!(svg_view, MediaView::Ready { size: [8, 8], .. }));
        let svg_bytes = 8 * 8 * 4;
        assert_eq!(cache.total_bytes(), pic_bytes + svg_bytes);

        cache.set_animation_time(Some(0));
        let anim_view = cache.texture_for(&ctx, "anim.gif", &source, (2, 2));
        assert!(matches!(anim_view, MediaView::Ready { size: [2, 2], .. }));
        // 2x2x4 = 16 bytes uploaded GPU texture for frame 0 + 2 * 16 bytes CPU frames.
        let anim_bytes = (2 * 2 * 4) + (2 * (2 * 2 * 4));
        assert_eq!(cache.total_bytes(), pic_bytes + svg_bytes + anim_bytes);

        cache.set_animation_time(None);
        assert_eq!(cache.total_bytes(), pic_bytes + svg_bytes);
    }

    /// U-02: Thumbnail paint uploads at <= 384 px while preserving natural
    /// `[1280, 720]` dimensions, canvas paint upgrades cleanly, and a subsequent
    /// thumbnail paint reuses the canvas texture with zero extra decodes.
    #[test]
    fn u02_thumbnail_tier_upgrades_to_canvas_without_thrashing() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("hero.png", solid_png(1280, 720));

        let mut cache = MediaCache::new();
        #[cfg(debug_assertions)]
        test_counts::reset_all();

        // 1. Thumbnail paint decodes at <= 384 px (384x216) and preserves natural size.
        cache.begin_frame();
        let thumb_tex_id = match cache.texture_for(&ctx, "hero.png", &source, (186, 105)) {
            MediaView::Ready { texture, size } => {
                assert_eq!(size, [1280, 720]);
                texture.id()
            }
            other => panic!("expected Ready thumbnail, got {other:?}"),
        };
        let thumb_bytes = cache.total_bytes();
        assert_eq!(thumb_bytes, 384 * 216 * 4);
        #[cfg(debug_assertions)]
        {
            assert_eq!(test_counts::picture_decode_count(), 1);
            assert_eq!(test_counts::full_res_thumbnail_decode_count(), 0);
        }

        // 2. Canvas paint upgrades cleanly and replaces the thumbnail texture.
        cache.begin_frame();
        let canvas_tex_id = match cache.texture_for(&ctx, "hero.png", &source, (1280, 720)) {
            MediaView::Ready { texture, size } => {
                assert_eq!(size, [1280, 720]);
                texture.id()
            }
            other => panic!("expected Ready canvas, got {other:?}"),
        };
        assert_ne!(canvas_tex_id, thumb_tex_id);
        assert_eq!(cache.total_bytes(), 1280 * 720 * 4);
        assert_eq!(cache.decoded_picture_count(), 1);
        #[cfg(debug_assertions)]
        assert_eq!(test_counts::picture_decode_count(), 2);

        // 3. Subsequent thumbnail paint reuses the canvas texture with 0 extra decodes.
        let fetches_before = source.fetches();
        cache.begin_frame();
        let reused_id = match cache.texture_for(&ctx, "hero.png", &source, (186, 105)) {
            MediaView::Ready { texture, size } => {
                assert_eq!(size, [1280, 720]);
                texture.id()
            }
            other => panic!("expected Ready reused texture, got {other:?}"),
        };
        assert_eq!(reused_id, canvas_tex_id);
        assert_eq!(source.fetches(), fetches_before);
        #[cfg(debug_assertions)]
        {
            assert_eq!(test_counts::picture_decode_count(), 2);
            assert_eq!(test_counts::full_res_thumbnail_decode_count(), 0);
        }
    }

    /// U-02: Distinct `src` keys with identical image bytes share one GPU
    /// texture handle (`TextureId`) and count VRAM bytes only once.
    #[test]
    fn u02_identical_bytes_share_one_texture_and_count_bytes_once() {
        let ctx = egui::Context::default();
        let bytes = solid_png(32, 24);
        let mut source = TestSource::new();
        source.add("slide1/logo.png", bytes.clone());
        source.add("slide2/logo.png", bytes);

        let mut cache = MediaCache::new();
        #[cfg(debug_assertions)]
        test_counts::reset_all();

        cache.begin_frame();
        let id_a = match cache.texture_for(&ctx, "slide1/logo.png", &source, (200, 150)) {
            MediaView::Ready { texture, size } => {
                assert_eq!(size, [32, 24]);
                texture.id()
            }
            other => panic!("expected Ready for slide1/logo.png, got {other:?}"),
        };
        let bytes_once = 32 * 24 * 4;
        assert_eq!(cache.total_bytes(), bytes_once);

        let id_b = match cache.texture_for(&ctx, "slide2/logo.png", &source, (200, 150)) {
            MediaView::Ready { texture, size } => {
                assert_eq!(size, [32, 24]);
                texture.id()
            }
            other => panic!("expected Ready for slide2/logo.png, got {other:?}"),
        };
        assert_eq!(id_a, id_b, "identical bytes must share one GPU TextureId");
        assert_eq!(
            cache.total_bytes(),
            bytes_once,
            "shared GPU texture bytes must be counted once"
        );
        assert_eq!(cache.decoded_picture_count(), 1);
        #[cfg(debug_assertions)]
        assert_eq!(
            test_counts::picture_decode_count(),
            1,
            "second key with identical bytes must not decode again"
        );

        // Retaining only one key keeps the shared texture; retaining none releases bytes.
        cache.retain_slice(&["slide1/logo.png"]);
        assert_eq!(cache.total_bytes(), bytes_once);
        cache.retain_slice(&[]);
        assert_eq!(cache.total_bytes(), 0);
        assert_eq!(cache.decoded_picture_count(), 0);
    }

    /// U-02: Unified memory ceiling tracks and evicts cold pictures, cold SVG
    /// rasters, and cold animation CPU buffers (> 120 unused frames).
    #[test]
    fn u02_unified_budget_evicts_pictures_svgs_and_cold_animations() {
        let ctx = egui::Context::default();
        let mut source = TestSource::new();
        source.add("pic.png", distinct_png(8, 8, 1));
        source.add("fresh.png", distinct_png(4, 4, 2));
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#00aa00"/></svg>"##;
        source.add("icon.svg", svg.to_vec());
        source.add("spinner.gif", tiny_animated_gif());

        let mut cache = MediaCache::new();
        cache.set_animation_time(Some(0));

        // Frame 1: load picture (256 B), SVG (256 B), and animated GIF (16 B GPU + 32 B CPU = 48 B).
        cache.begin_frame();
        assert!(matches!(
            cache.texture(&ctx, "pic.png", &source),
            MediaView::Ready { .. }
        ));
        assert!(matches!(
            cache.texture_for(&ctx, "icon.svg", &source, (8, 8)),
            MediaView::Ready { .. }
        ));
        assert!(matches!(
            cache.texture_for(&ctx, "spinner.gif", &source, (2, 2)),
            MediaView::Ready { .. }
        ));
        assert_eq!(cache.total_bytes(), 256 + 256 + 16 + 32);

        // Advance > 120 frames without touching any of the three assets so the
        // animation's CPU buffer becomes cold as well.
        for _ in 0..125 {
            cache.begin_frame();
        }

        // Lower unified budget to fit only a 4x4 picture (64 B): cold pic.png
        // (256 B) and cold icon.svg (256 B) evict immediately, leaving only
        // spinner.gif (48 B <= 64 B).
        cache.set_picture_budget(4 * 4 * 4);
        assert!(cache.get_entry("pic.png").is_none());
        assert!(cache.texture_id("icon.svg").is_none());
        assert_eq!(cache.total_bytes(), 48);

        // Painting fresh.png (64 B) on the current frame pushes total_bytes() to
        // 112 B > 64 B, evicting the cold spinner.gif animation (GPU + CPU frames).
        assert!(matches!(
            cache.texture(&ctx, "fresh.png", &source),
            MediaView::Ready { size: [4, 4], .. }
        ));
        assert!(cache.animations.is_empty());
        assert_eq!(cache.total_bytes(), 64);
        assert!(cache.total_bytes() <= cache.picture_budget());
    }

    #[test]
    fn src_extension_checks_are_ascii_case_insensitive_and_utf8_safe() {
        assert!(is_svg_src("icon.svg"));
        assert!(is_svg_src("assets/ICON.SVGZ"));
        assert!(is_svg_src("日本語/図.SvG"));
        assert!(!is_svg_src("svg"));
        assert!(!is_svg_src("photo.png"));

        assert!(is_animated_src("anim.gif"));
        assert!(is_animated_src("assets/LOOP.WEBP"));
        assert!(is_animated_src("émoji/🎞.GiF"));
        assert!(!is_animated_src("gif"));
        assert!(!is_animated_src("photo.jpg"));
    }

    #[test]
    fn content_hash_deduplicates_shared_and_distinct_arcs_and_prunes_dead_weaks() {
        let mut cache = MediaCache::new();
        let bytes = solid_png(16, 16);
        let shared = Arc::<[u8]>::from(bytes.clone());
        let distinct = Arc::<[u8]>::from(bytes);

        let h1 = cache.content_hash(&shared);
        let h2 = cache.content_hash(&shared);
        let h3 = cache.content_hash(&distinct);
        assert_eq!(h1, h2, "shared Arc hits identity cache with same hash");
        assert_eq!(
            h1, h3,
            "distinct Arc with identical bytes must produce the same content hash"
        );

        let different = Arc::<[u8]>::from(distinct_png(16, 16, 42));
        assert_ne!(
            h1,
            cache.content_hash(&different),
            "distinct image bytes must produce a different content hash"
        );

        // Verify tail-length sensitivity across block boundaries (0..=64 bytes).
        let base: Vec<u8> = (0u8..64).collect();
        for len in 1..=64 {
            let mut flipped = base[..len].to_vec();
            flipped[len - 1] ^= 0x01;
            assert_ne!(
                content_hash_64(&base[..len]),
                content_hash_64(&flipped),
                "single-bit flip at tail byte {len} must change content_hash_64"
            );
        }

        // Dropping an Arc must be unpinned immediately on the next begin_frame
        // (not 60 frames later) because Weak<[u8]> keeps the inline slice buffer
        // allocated until dropped.
        let distinct_key = ArcPtrKey {
            ptr: distinct.as_ptr() as usize,
            len: distinct.len(),
        };
        let weak_probe = Arc::downgrade(&distinct);
        drop(distinct);
        assert_eq!(weak_probe.strong_count(), 0);
        assert_eq!(cache.arc_hashes.len(), 3);
        cache.begin_frame();
        assert_eq!(
            cache.arc_hashes.len(),
            2,
            "begin_frame must immediately prune dropped Arc weak entries so buffers are not pinned"
        );
        assert!(!cache.arc_hashes.contains_key(&distinct_key));

        // Hashing a new Arc on a cache miss also prunes dead Weak entries
        // immediately, even if begin_frame() is never called (e.g. offscreen).
        let transient = Arc::<[u8]>::from(distinct_png(16, 16, 99));
        let transient_key = ArcPtrKey {
            ptr: transient.as_ptr() as usize,
            len: transient.len(),
        };
        let transient_probe = Arc::downgrade(&transient);
        let _ = cache.content_hash(&transient);
        assert!(cache.arc_hashes.contains_key(&transient_key));
        drop(transient);
        assert_eq!(transient_probe.strong_count(), 0);
        let replacement = Arc::<[u8]>::from(distinct_png(16, 16, 100));
        let _ = cache.content_hash(&replacement);
        assert!(
            !cache.arc_hashes.contains_key(&transient_key),
            "content_hash miss must immediately prune dropped Arc weak entries"
        );

        // retain_slice also prunes dropped Weak entries.
        let different_key = ArcPtrKey {
            ptr: different.as_ptr() as usize,
            len: different.len(),
        };
        drop(different);
        cache.retain_slice(&[]);
        assert!(
            !cache.arc_hashes.contains_key(&different_key),
            "retain_slice must prune dropped Arc weak entries so buffers are not pinned"
        );
        assert_eq!(cache.arc_hashes.len(), 2);
    }
}
