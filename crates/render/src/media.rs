//! In-memory media cache and abstractions for the slide compositor (L3).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use coursecraft_course::ObjectKind;
use coursecraft_logic::{Input, Player};
use coursecraft_media::{
    Animation, Limits, decode_animation, decode_image, decode_svg, svg_natural_size,
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

fn is_svg_src(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    lower.ends_with(".svg") || lower.ends_with(".svgz")
}

fn is_animated_src(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    lower.ends_with(".gif") || lower.ends_with(".webp")
}

struct SvgEntry {
    texture: TextureHandle,
    target: (u32, u32),
    last_used: u64,
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

/// Texture cache for slide media assets keyed by `src`.
///
/// Ensures each asset is decoded at most once and uploaded to egui's texture manager.
/// Failed decodes and missing assets are retained so they are not re-queried or re-decoded
/// on every frame.
#[derive(Default)]
pub struct MediaCache {
    entries: HashMap<String, MediaEntry>,
    svg: HashMap<String, SvgAsset>,
    animations: HashMap<String, AnimationEntry>,
    lottie: HashMap<String, LottieEntry>,
    video: HashMap<String, VideoTextureEntry>,
    limits: Limits,
    animation_time_ms: Option<u64>,
}

impl std::fmt::Debug for MediaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaCache")
            .field("entries_count", &self.entries.len())
            .field("svg_count", &self.svg.len())
            .field("animation_count", &self.animations.len())
            .field("lottie_count", &self.lottie.len())
            .field("video_count", &self.video.len())
            .field("limits", &self.limits)
            .finish()
    }
}

struct VideoTextureEntry {
    handle: TextureHandle,
    size: [u32; 2],
    /// Identity of the last uploaded frame (same Arc = same frame).
    last_ptr: *const coursecraft_playback::VideoFrame,
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
            entries: HashMap::new(),
            svg: HashMap::new(),
            animations: HashMap::new(),
            lottie: HashMap::new(),
            video: HashMap::new(),
            limits,
            animation_time_ms: None,
        }
    }

    /// Set the clock animations follow, in milliseconds since the view started.
    ///
    /// `None` (the default) draws every animation's first frame, which is what
    /// the canvas, thumbnails and offscreen renders need.
    pub fn set_animation_time(&mut self, time_ms: Option<u64>) {
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
            if clock >= info.duration_ms {
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
    /// debounce); animated GIF/WebP frames follow [`Self::set_animation_time`].
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
        self.texture_with_key(ctx, src.to_string(), src, source, TextureOptions::LINEAR)
    }

    /// Retrieve or load the texture for `src`, using `source` to fetch missing bytes.
    pub fn texture<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
    ) -> MediaView<'a> {
        self.texture_with_key(ctx, src.to_string(), src, source, TextureOptions::LINEAR)
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
        self.texture_with_key(ctx, format!("{src}\u{1}tile"), src, source, options)
    }

    /// Load and rasterise an SVG at (or close to) the drawn size.
    fn svg_texture<'a>(
        &'a mut self,
        ctx: &Context,
        src: &str,
        source: &dyn MediaSource,
        target_px: (u32, u32),
    ) -> MediaView<'a> {
        let now = ctx.input(|input| input.time);
        let now = if now.is_finite() { now } else { 0.0 };
        if !self.svg.contains_key(src) {
            match source.get(src) {
                MediaFetch::Ready(bytes) => {
                    let Some(natural) = svg_natural_size(&bytes) else {
                        self.entries.insert(
                            src.to_owned(),
                            MediaEntry::Failed("invalid SVG".to_string()),
                        );
                        return self.failed_view(src);
                    };
                    let desired = fit_svg_target(natural, target_px, &self.limits);
                    match rasterise_svg(ctx, src, &bytes, desired, &self.limits) {
                        Ok((texture, size)) => {
                            self.svg.insert(
                                src.to_owned(),
                                SvgAsset {
                                    bytes,
                                    natural,
                                    entries: vec![SvgEntry {
                                        texture,
                                        target: size,
                                        last_used: 1,
                                    }],
                                    last_raster: now,
                                    use_counter: 1,
                                },
                            );
                        }
                        Err(error) => {
                            self.entries
                                .insert(src.to_owned(), MediaEntry::Failed(error.to_string()));
                            return self.failed_view(src);
                        }
                    }
                }
                MediaFetch::Pending => return MediaView::Pending,
                MediaFetch::Missing => return MediaView::Failed("image not found"),
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
            if let Some(asset) = self.svg.get_mut(src) {
                asset.use_counter = asset.use_counter.saturating_add(1);
                if let Some(entry) = asset.entries.get_mut(index) {
                    entry.last_used = asset.use_counter;
                }
            }
            return self.svg_view(src, index);
        }

        // Debounce a resize drag: keep the old raster and come back later.
        let elapsed = (now - asset.last_raster).max(0.0);
        if elapsed < SVG_RERASTERISE_DEBOUNCE_SECONDS {
            let wait = SVG_RERASTERISE_DEBOUNCE_SECONDS - elapsed;
            ctx.request_repaint_after(Duration::from_secs_f64(wait.max(0.01)));
            if let Some(index) = nearest {
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
                if let Some(asset) = self.svg.get_mut(src) {
                    asset.use_counter = asset.use_counter.saturating_add(1);
                    let used = asset.use_counter;
                    asset.entries.push(SvgEntry {
                        texture,
                        target: size,
                        last_used: used,
                    });
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
                        asset.entries.remove(oldest);
                    }
                    let index = asset.entries.len().saturating_sub(1);
                    return self.svg_view(src, index);
                }
                MediaView::Pending
            }
            Err(error) => {
                if let Some(index) = nearest {
                    log::warn!("SVG {src} could not be rasterised: {error}");
                    self.svg_view(src, index)
                } else {
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
                    self.animations.insert(
                        src.to_owned(),
                        AnimationEntry {
                            animation: Some(animation),
                            frames: vec![None; count],
                            warning: None,
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

        if !uploaded {
            let frame = animation.frames.get(index)?;
            let color = ColorImage::from_rgba_unmultiplied(
                [frame.image.width as usize, frame.image.height as usize],
                &frame.image.pixels,
            );
            let key = format!("{src}\u{1}anim\u{1}{index}");
            let handle = ctx.load_texture(key, color, TextureOptions::LINEAR);
            if let Some(entry) = self.animations.get_mut(src)
                && let Some(slot) = entry.frames.get_mut(index)
            {
                *slot = Some(handle);
            }
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

    fn texture_with_key<'a>(
        &'a mut self,
        ctx: &Context,
        key: String,
        src: &str,
        source: &dyn MediaSource,
        options: TextureOptions,
    ) -> MediaView<'a> {
        let should_fetch = match self.entries.get(&key) {
            None => true,
            Some(MediaEntry::Loading) => true,
            Some(MediaEntry::Texture(..)) | Some(MediaEntry::Failed(..)) => false,
        };

        if should_fetch {
            match source.get(src) {
                MediaFetch::Ready(bytes) => match decode_image(&bytes, &self.limits) {
                    Ok(img) => {
                        let color_image = ColorImage::from_rgba_unmultiplied(
                            [img.width as usize, img.height as usize],
                            &img.pixels,
                        );
                        let handle = ctx.load_texture(key.clone(), color_image, options);
                        self.entries.insert(
                            key.clone(),
                            MediaEntry::Texture(handle, [img.width, img.height]),
                        );
                    }
                    Err(e) => {
                        self.entries
                            .insert(key.clone(), MediaEntry::Failed(e.to_string()));
                    }
                },
                MediaFetch::Pending => {
                    self.entries.insert(key.clone(), MediaEntry::Loading);
                }
                MediaFetch::Missing => {
                    self.entries.insert(
                        key.clone(),
                        MediaEntry::Failed("image not found".to_string()),
                    );
                }
            }
        }

        match self.entries.get(&key) {
            Some(MediaEntry::Texture(handle, size)) => MediaView::Ready {
                texture: handle,
                size: *size,
            },
            Some(MediaEntry::Loading) => MediaView::Pending,
            Some(MediaEntry::Failed(msg)) => MediaView::Failed(msg.as_str()),
            None => MediaView::Pending,
        }
    }

    /// Retain only textures whose keys are in `srcs`, dropping all others.
    pub fn retain<S: std::borrow::Borrow<str> + std::hash::Hash + Eq>(
        &mut self,
        srcs: &HashSet<S>,
    ) {
        self.entries.retain(|key, _| srcs.contains(key_source(key)));
        self.svg.retain(|key, _| srcs.contains(key.as_str()));
        self.animations.retain(|key, _| srcs.contains(key.as_str()));
        self.lottie.retain(|key, _| srcs.contains(key.as_str()));
        self.video.retain(|key, _| srcs.contains(key.as_str()));
    }

    /// Retain only textures whose keys are in the provided slice.
    pub fn retain_slice(&mut self, srcs: &[&str]) {
        self.entries
            .retain(|key, _| srcs.contains(&key_source(key)));
        self.svg.retain(|key, _| srcs.contains(&key.as_str()));
        self.animations
            .retain(|key, _| srcs.contains(&key.as_str()));
        self.lottie.retain(|key, _| srcs.contains(&key.as_str()));
        self.video.retain(|key, _| srcs.contains(&key.as_str()));
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
        let frame_ptr = Arc::as_ptr(frame);
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
                    last_ptr: frame_ptr,
                },
            );
        } else if let Some(entry) = self.video.get_mut(object)
            && entry.last_ptr != frame_ptr
        {
            let color =
                ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &frame.rgba);
            entry.handle.set(color, TextureOptions::LINEAR);
            entry.last_ptr = frame_ptr;
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
    let color = ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.pixels,
    );
    let key = format!("{src}\u{1}svg\u{1}{}x{}", image.width, image.height);
    let handle = ctx.load_texture(key, color, TextureOptions::LINEAR);
    Ok((handle, (image.width, image.height)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SlideTransform;
    use crate::paint::{Highlight, paint};
    use coursecraft_course::Size;
    use egui::{Pos2, Rect, vec2};

    struct NoMedia;
    impl MediaSource for NoMedia {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
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
}
