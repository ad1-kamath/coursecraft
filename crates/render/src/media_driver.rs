//! Shared audio/video driver for the player app and the editor preview (L3).
//!
//! Turns `media` effects from the runtime into [`MediaBackend`] calls, waits for
//! asynchronously fetched bytes before starting a play, reports finished media
//! back as runtime inputs, and positions video elements over the slide. It makes
//! no play-time decisions: the rules live in `coursecraft-logic`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use coursecraft_course::{BACKGROUND_AUDIO_ID, ObjectKind, Slide};
use coursecraft_logic::{Effect, Input, MediaOp, Player};
use coursecraft_media::{Cue, cue_text_at, parse_webvtt};
use coursecraft_playback::{MediaBackend, MediaKind, create_default_backend, sanitize_volume};
use egui::Rect;

use crate::compose::{Hit, Visual};
use crate::media::{MediaFetch, MediaSource};
use crate::view::SlideView;

/// A play request waiting for its bytes to be fetched.
#[derive(Debug, Clone, PartialEq)]
struct PendingPlay {
    object: String,
    src: String,
    kind: MediaKind,
    volume: f32,
}

/// The caption shown for the current frame.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionFrame {
    /// Media object whose captions are showing.
    pub object: String,
    /// Cue text at the object's current position.
    pub text: String,
    /// Audio or video; video wins when both are playing.
    pub kind: MediaKind,
    /// Video object's screen rect on the slide, when it has one.
    pub rect: Option<Rect>,
}

#[derive(Debug, Clone)]
enum CaptionEntry {
    Ready(Vec<Cue>),
    Missing,
    Failed,
}

/// Parsed WebVTT cues, loaded once per caption path.
#[derive(Debug, Default)]
pub struct CaptionStore {
    entries: HashMap<String, CaptionEntry>,
}

impl CaptionStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of parsed (or failed) caption paths retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been loaded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Caption text active at `position_ms`, loading and parsing `path` once.
    ///
    /// A pending asset is retried next frame. A missing file or a parse error is
    /// logged once and then shows nothing.
    pub fn text_at(
        &mut self,
        path: &str,
        source: Option<&dyn MediaSource>,
        position_ms: u64,
    ) -> Option<String> {
        if !self.entries.contains_key(path) {
            match source.map_or(MediaFetch::Pending, |source| source.get(path)) {
                MediaFetch::Ready(bytes) => match parse_webvtt(&bytes) {
                    Ok(cues) => {
                        self.entries
                            .insert(path.to_owned(), CaptionEntry::Ready(cues));
                    }
                    Err(error) => {
                        log::warn!("Caption file {path} could not be parsed: {error}");
                        self.entries.insert(path.to_owned(), CaptionEntry::Failed);
                        return None;
                    }
                },
                MediaFetch::Pending => return None,
                MediaFetch::Missing => {
                    log::warn!("Caption file missing: {path}");
                    self.entries.insert(path.to_owned(), CaptionEntry::Missing);
                    return None;
                }
            }
        }
        match self.entries.get(path) {
            Some(CaptionEntry::Ready(cues)) => cue_text_at(cues, position_ms),
            Some(CaptionEntry::Missing | CaptionEntry::Failed) | None => None,
        }
    }
}

/// Source, kind and object volume of the audio or video object `object_id`.
///
/// Object ids are unique only per slide, so the lookup for a normal object
/// never leaves the current slide; base and layer objects are both searched.
/// The reserved [`BACKGROUND_AUDIO_ID`] resolves to the course's background
/// track instead.
#[must_use]
pub fn find_media_target(player: &Player<'_>, object_id: &str) -> Option<(String, MediaKind, f32)> {
    if object_id == BACKGROUND_AUDIO_ID {
        let audio = player.course().background_audio.as_ref()?;
        return Some((audio.src.clone(), MediaKind::Audio, audio.volume));
    }
    let (obj, _) = player.current_slide()?.object(object_id)?;
    match &obj.kind {
        ObjectKind::Audio { src, volume, .. } => Some((src.clone(), MediaKind::Audio, *volume)),
        ObjectKind::Video { src, volume, .. } => Some((src.clone(), MediaKind::Video, *volume)),
        _ => None,
    }
}

/// The object a media effect or seek targets.
#[must_use]
pub fn effect_object(effect: &Effect) -> Option<&str> {
    match effect {
        Effect::Media { object, .. } | Effect::MediaSeek { object, .. } => Some(object),
        _ => None,
    }
}

/// Whether `object` on the current slide is a Lottie animation.
fn is_lottie(player: &Player<'_>, object: Option<&str>) -> bool {
    let Some(object) = object else {
        return false;
    };
    player
        .current_slide()
        .and_then(|slide| slide.object(object))
        .is_some_and(|(obj, _)| matches!(obj.kind, ObjectKind::Lottie { .. }))
}

/// Whether a video object carries cues.
#[cfg(not(target_arch = "wasm32"))]
fn video_has_cues(player: &Player<'_>, object: &str) -> bool {
    player
        .current_slide()
        .and_then(|slide| slide.object(object))
        .is_some_and(
            |(obj, _)| matches!(&obj.kind, ObjectKind::Video { cues, .. } if !cues.is_empty()),
        )
}

/// Ids of every video object on `slide` (base and layers).
#[must_use]
pub fn collect_video_ids(slide: &Slide) -> Vec<String> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
        .filter(|o| matches!(o.kind, ObjectKind::Video { .. }))
        .map(|o| o.id.clone())
        .collect()
}

/// Whether any audio or video object on `slide` declares captions.
#[must_use]
pub fn slide_has_captions(slide: &Slide) -> bool {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
        .any(|object| match &object.kind {
            ObjectKind::Audio { captions, .. } | ObjectKind::Video { captions, .. } => {
                captions.is_some()
            }
            _ => false,
        })
}

/// Drives a [`MediaBackend`] from runtime effects.
pub struct MediaDriver {
    backend: Box<dyn MediaBackend>,
    pending: Vec<PendingPlay>,
    /// Objects that currently have a live backend track (playing or paused).
    live: HashSet<String>,
    /// Desktop-only: videos with cues whose playback is simulated because the
    /// desktop backend draws a poster instead of playing the file.
    #[cfg(not(target_arch = "wasm32"))]
    simulated_videos: HashMap<String, u64>,
    /// Seeks for objects that have not started yet; applied once they play.
    pending_seeks: HashMap<String, u64>,
    volume: f32,
    rate: f32,
    laid_out: HashSet<String>,
    captions: CaptionStore,
    shown_video: Option<String>,
    shown_text: Option<String>,
}

impl Default for MediaDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaDriver {
    /// Create a driver on the platform's default backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            backend: create_default_backend(),
            pending: Vec::new(),
            live: HashSet::new(),
            #[cfg(not(target_arch = "wasm32"))]
            simulated_videos: HashMap::new(),
            pending_seeks: HashMap::new(),
            volume: 1.0,
            rate: 1.0,
            laid_out: HashSet::new(),
            captions: CaptionStore::new(),
            shown_video: None,
            shown_text: None,
        }
    }

    /// Replace the backend (for example with a null backend in tests). Plays
    /// still waiting for bytes are kept.
    #[must_use]
    pub fn with_backend(mut self, backend: Box<dyn MediaBackend>) -> Self {
        self.set_backend(backend);
        self
    }

    /// In-place variant of [`MediaDriver::with_backend`].
    pub fn set_backend(&mut self, backend: Box<dyn MediaBackend>) {
        self.backend.stop_all();
        self.live.clear();
        #[cfg(not(target_arch = "wasm32"))]
        self.simulated_videos.clear();
        self.pending_seeks.clear();
        self.shown_video = None;
        self.shown_text = None;
        self.backend = backend;
        self.backend.set_volume(self.volume);
        self.backend.set_rate(self.rate);
    }

    /// Number of plays still waiting for their bytes.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Stop everything and forget plays that are still waiting for bytes.
    pub fn stop_all(&mut self) {
        self.pending.clear();
        self.live.clear();
        #[cfg(not(target_arch = "wasm32"))]
        self.simulated_videos.clear();
        self.pending_seeks.clear();
        self.present_captions(None, false);
        self.backend.stop_all();
    }

    /// Set player volume (`0.0`–`1.0`) for current and later media.
    ///
    /// NaN becomes silence; infinities and out-of-range values are clamped.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = sanitize_volume(volume);
        self.backend.set_volume(self.volume);
    }

    /// Set the playback rate for current and later media. A no-op on backends
    /// that cannot change rate.
    pub fn set_rate(&mut self, rate: f32) {
        self.rate = if rate.is_finite() { rate } else { 1.0 };
        self.backend.set_rate(self.rate);
    }

    /// The playback rate last set.
    #[must_use]
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Whether the active backend can change the playback rate. The host passes
    /// this to the player frame, which hides the speed control otherwise.
    #[must_use]
    pub fn supports_rate(&self) -> bool {
        self.backend.supports_rate()
    }

    /// Current player volume (`0.0`–`1.0`).
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Dispatch the media effects in `effects` (emitted by `player`).
    ///
    /// A missing or unknown media object yields a follow-up
    /// [`Input::MediaEnded`]; the caller feeds the returned inputs to the player
    /// and applies the effects they produce. With `source` set to `None` every
    /// play waits as pending.
    pub fn apply(
        &mut self,
        player: &Player<'_>,
        effects: &[Effect],
        source: Option<&dyn MediaSource>,
    ) -> Vec<Input> {
        let mut followups = Vec::new();
        for effect in effects {
            // Lottie playback is drawn by the compositor from the runtime's
            // media clock; it has no audio/video backend track.
            if is_lottie(player, effect_object(effect)) {
                continue;
            }
            match effect {
                Effect::Media { object, op } => match op {
                    MediaOp::Play => self.play(player, object, source, &mut followups),
                    MediaOp::Pause => {
                        // The runtime re-sends Play on resume.
                        self.pending.retain(|p| p.object != *object);
                        self.backend.pause(object);
                    }
                    MediaOp::Stop => {
                        self.pending.retain(|p| p.object != *object);
                        self.live.remove(object);
                        self.pending_seeks.remove(object);
                        #[cfg(not(target_arch = "wasm32"))]
                        self.simulated_videos.remove(object);
                        self.backend.stop(object);
                    }
                },
                Effect::MediaSeek { object, ms } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.simulated_videos.contains_key(object)
                        && !self.backend.is_video_decoding(object)
                    {
                        if let Some(position) = self.simulated_videos.get_mut(object) {
                            *position = *ms;
                        }
                        self.pending_seeks.remove(object);
                        // Fallback only: no real decoder to seek.
                        continue;
                    }
                    if self.live.contains(object) {
                        self.backend.seek(object, *ms);
                    } else {
                        // The bytes may still be fetching, or the media is paused
                        // by the player before it ever started.
                        self.pending_seeks.insert(object.clone(), *ms);
                        self.backend.seek(object, *ms);
                    }
                }
                Effect::Exit => self.stop_all(),
                _ => {}
            }
        }
        followups
    }

    fn play(
        &mut self,
        player: &Player<'_>,
        object: &str,
        source: Option<&dyn MediaSource>,
        followups: &mut Vec<Input>,
    ) {
        let Some((src, kind, volume)) = find_media_target(player, object) else {
            log::warn!("Media object not found: {object}");
            followups.push(Input::MediaEnded {
                object: object.to_owned(),
            });
            return;
        };
        // Desktop plays real video now; the simulated clock is only the
        // fallback when decoding fails (backend reports no worker), so cues
        // can still be tested. Real position replaces it whenever available.
        #[cfg(not(target_arch = "wasm32"))]
        if kind == MediaKind::Video && video_has_cues(player, object) {
            self.pending.retain(|p| p.object != object);
            self.live.insert(object.to_owned());
            if let Some(ms) = self.pending_seeks.remove(object) {
                self.simulated_videos.insert(object.to_owned(), ms);
            }
            // Start the real backend in parallel; the fallback clock drives
            // cues only until decoding reports a worker.
            match source.map_or(MediaFetch::Pending, |s| s.get(&src)) {
                MediaFetch::Ready(bytes) => self.start(object, &src, kind, volume, bytes),
                MediaFetch::Pending => self.pending.push(PendingPlay {
                    object: object.to_owned(),
                    src,
                    kind,
                    volume,
                }),
                MediaFetch::Missing => {
                    log::warn!("Media missing for object {object}: {src}");
                    // Missing bytes: fall back to simulation (never ends).
                }
            }
            if !self.backend.is_video_decoding(object) {
                self.simulated_videos.entry(object.to_owned()).or_insert(0);
            }
            return;
        }
        self.pending.retain(|p| p.object != object);
        match source.map_or(MediaFetch::Pending, |s| s.get(&src)) {
            MediaFetch::Ready(bytes) => self.start(object, &src, kind, volume, bytes),
            MediaFetch::Pending => self.pending.push(PendingPlay {
                object: object.to_owned(),
                src,
                kind,
                volume,
            }),
            MediaFetch::Missing => {
                log::warn!("Media missing for object {object}: {src}");
                followups.push(Input::MediaEnded {
                    object: object.to_owned(),
                });
            }
        }
    }

    /// Start one play, remembering it as live and applying a seek that arrived
    /// while it was still being fetched.
    fn start(&mut self, object: &str, src: &str, kind: MediaKind, volume: f32, bytes: Arc<[u8]>) {
        self.backend.set_object_volume(object, volume);
        self.backend.set_rate(self.rate);
        self.backend.play(object, src, kind, bytes);
        self.live.insert(object.to_owned());
        if let Some(ms) = self.pending_seeks.remove(object) {
            self.backend.seek(object, ms);
        }
    }

    /// Collect finished media and start plays whose bytes have arrived.
    ///
    /// Returns [`Input::MediaEnded`] for every finished or missing media object.
    pub fn poll(&mut self, source: Option<&dyn MediaSource>) -> Vec<Input> {
        let mut inputs: Vec<Input> = self
            .backend
            .take_ended()
            .into_iter()
            // A cue-bearing fallback video never ends via decode failure;
            // its simulated clock keeps driving cues (as before).
            .filter(|object| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    !self.simulated_videos.contains_key(object)
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = object;
                    true
                }
            })
            .map(|object| {
                self.live.remove(&object);
                self.pending_seeks.remove(&object);
                Input::MediaEnded { object }
            })
            .collect();

        for p in std::mem::take(&mut self.pending) {
            match source.map_or(MediaFetch::Pending, |s| s.get(&p.src)) {
                MediaFetch::Ready(bytes) => self.start(&p.object, &p.src, p.kind, p.volume, bytes),
                MediaFetch::Pending => self.pending.push(p),
                MediaFetch::Missing => {
                    log::warn!("Pending media missing for object {}: {}", p.object, p.src);
                    inputs.push(Input::MediaEnded { object: p.object });
                }
            }
        }
        inputs
    }

    /// Report each playing media object's current position as a runtime input.
    ///
    /// Reports every video with cues, as before, plus every audio or video with
    /// a trim end (the runtime ends the clip at that file time). The web
    /// backend reads the real HTML element. On desktop the real decoder or
    /// sink position is used whenever it is available; the simulated clock is
    /// only the fallback for cued videos whose decoding failed. Call this next
    /// to [`MediaDriver::layout_videos`] every frame.
    pub fn media_time_inputs(&mut self, player: &Player<'_>, dt_ms: u64) -> Vec<Input> {
        #[cfg(target_arch = "wasm32")]
        let _ = dt_ms;
        let Some(slide) = player.current_slide() else {
            return Vec::new();
        };
        let tracked: Vec<String> = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
            .filter(|object| match &object.kind {
                ObjectKind::Video { cues, trim, .. } => {
                    !cues.is_empty() || trim.as_ref().is_some_and(|trim| trim.end_ms.is_some())
                }
                ObjectKind::Audio { trim, .. } => {
                    trim.as_ref().is_some_and(|trim| trim.end_ms.is_some())
                }
                _ => false,
            })
            .map(|object| object.id.clone())
            .collect();
        let mut inputs = Vec::new();
        for object in tracked {
            if !player.is_media_playing(&object) {
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            let position =
                if video_has_cues(player, &object) && !self.backend.is_video_decoding(&object) {
                    let position = self.simulated_videos.entry(object.clone()).or_insert(0);
                    *position = position.saturating_add(dt_ms);
                    Some(*position)
                } else {
                    self.backend.position_ms(&object)
                };
            #[cfg(target_arch = "wasm32")]
            let position = self.backend.position_ms(&object);
            if let Some(position) = position {
                inputs.push(Input::MediaTime {
                    object,
                    at_ms: position as f64,
                });
            }
        }
        inputs
    }

    /// Position the current slide's video elements over the slide, hiding videos
    /// whose object is not on screen (including those of slides left behind).
    pub fn layout_videos(&mut self, player: &Player<'_>, view: &SlideView) {
        // A lightbox needs the dim backdrop above every DOM element, so all
        // video elements (base and box) hide while one is open.
        if player.lightbox_open() {
            for vid in self.laid_out.drain() {
                self.backend.layout_video(&vid, None);
            }
            return;
        }
        let current = player.current_slide().map(collect_video_ids);
        let current = current.unwrap_or_default();
        let transform = view.last_transform();
        let items = view.last_items();

        for gone in self.laid_out.difference(&current.iter().cloned().collect()) {
            self.backend.layout_video(gone, None);
        }
        // HTML video sits above the canvas and can't follow a sliding slide, so it
        // stays hidden until the transition ends.
        let transitioning = view.transition_active();
        for vid in &current {
            let rect = transform.filter(|_| !transitioning).and_then(|t| {
                items.iter().find_map(|item| match &item.hit {
                    Hit::Object(id) if id == vid && matches!(item.visual, Visual::Video { .. }) => {
                        let r = t.to_screen_rect(item.rect);
                        Some([r.min.x, r.min.y, r.width(), r.height()])
                    }
                    _ => None,
                })
            });
            self.backend.layout_video(vid, rect);
        }
        self.laid_out = current.into_iter().collect();
    }

    /// The caption to present for this frame, if any media object with captions
    /// is playing and has active cue text.
    ///
    /// Video wins over audio; otherwise the first object in slide order wins.
    /// Video objects carry their screen rect so a DOM layer can place the text.
    pub fn caption_frame(
        &mut self,
        player: &Player<'_>,
        source: Option<&dyn MediaSource>,
        view: &SlideView,
    ) -> Option<CaptionFrame> {
        let slide = player.current_slide()?;
        let mut chosen: Option<CaptionFrame> = None;
        for object in slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        {
            let (path, kind) = match &object.kind {
                ObjectKind::Audio {
                    captions: Some(path),
                    ..
                } => (path, MediaKind::Audio),
                ObjectKind::Video {
                    captions: Some(path),
                    ..
                } => (path, MediaKind::Video),
                _ => continue,
            };
            if !player.is_media_playing(&object.id) {
                continue;
            }
            let Some(position) = self.backend.position_ms(&object.id) else {
                continue;
            };
            let Some(text) = self.captions.text_at(path, source, position) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let rect = if kind == MediaKind::Video {
                video_screen_rect(view, &object.id)
            } else {
                None
            };
            let candidate = CaptionFrame {
                object: object.id.clone(),
                text,
                kind,
                rect,
            };
            match &chosen {
                None => chosen = Some(candidate),
                Some(current) if current.kind == MediaKind::Audio && kind == MediaKind::Video => {
                    chosen = Some(candidate);
                }
                _ => {}
            }
        }
        chosen
    }

    /// Newest decoded frame for a video object via the backend. Never blocks.
    #[must_use]
    pub fn video_frame(&self, object: &str) -> Option<Arc<coursecraft_playback::VideoFrame>> {
        self.backend.video_frame(object)
    }

    /// Source that merges a byte source with live video frames for painting.
    #[must_use]
    pub fn as_media_source<'a>(&'a self, inner: &'a dyn MediaSource) -> PlaybackMediaSource<'a> {
        PlaybackMediaSource {
            inner,
            backend: &*self.backend,
        }
    }

    /// Show or clear the DOM caption box over a web video.
    ///
    /// This writes to the DOM only when the cue text changes, never every frame.
    pub fn present_captions(&mut self, frame: Option<&CaptionFrame>, on: bool) {
        let active = if on {
            frame.filter(|frame| frame.kind == MediaKind::Video)
        } else {
            None
        };
        let object = active.map(|frame| frame.object.as_str());
        let text = active.map(|frame| frame.text.as_str());
        if self.shown_video.as_deref() != object {
            if let Some(old) = self.shown_video.take() {
                self.backend.show_caption(&old, None);
            }
            self.shown_text = None;
            if let Some(object) = object {
                self.backend.show_caption(object, text);
                self.shown_video = Some(object.to_owned());
                self.shown_text = text.map(str::to_owned);
            }
        } else if let Some(object) = object
            && self.shown_text.as_deref() != text
        {
            self.backend.show_caption(object, text);
            self.shown_text = text.map(str::to_owned);
        }
    }
}

fn video_screen_rect(view: &SlideView, object: &str) -> Option<Rect> {
    let transform = view.last_transform()?;
    view.last_items().iter().find_map(|item| match &item.hit {
        Hit::Object(id) if id == object && matches!(item.visual, Visual::Video { .. }) => {
            Some(transform.to_screen_rect(item.rect))
        }
        _ => None,
    })
}

/// Media source merging a byte source with live desktop video frames.
///
/// Hosts with a [`MediaDriver`] pass this (not the bare byte source) to the
/// slide view so [`Visual::Video`](crate::compose::Visual::Video) can draw the
/// current decoded frame.
pub struct PlaybackMediaSource<'a> {
    /// Byte source (images, captions).
    pub inner: &'a dyn MediaSource,
    /// Playback backend for video frames and positions.
    pub backend: &'a dyn MediaBackend,
}

impl MediaSource for PlaybackMediaSource<'_> {
    fn get(&self, src: &str) -> MediaFetch {
        self.inner.get(src)
    }
    fn video_frame(&self, object: &str) -> Option<Arc<coursecraft_playback::VideoFrame>> {
        self.backend.video_frame(object)
    }
}
