//! Timeline playback for the editor (M12 step 2).
//!
//! Authoring-only playback: the timeline Play button and scrubbing move
//! the playhead and ask the desktop backend for the frame at
//! `playhead_ms - timing.start_ms` for each video whose timing includes
//! the playhead. No `coursecraft-logic` player runs here, so triggers,
//! cues, layers and states never fire and the document is never changed:
//! nothing here touches `Session`, the undo stack, or stored fields.
//!
//! The backend is behind the [`TimelineMedia`] trait so tests can drive
//! the whole state machine with a fake frame source.
//!
//! # Web placeholder
//!
//! The web editor keeps its existing placeholder for now (plan M12 step
//! 2): on `wasm32` `TimelinePlayback::create_default()` uses the null
//! backend, so no video frames are drawn and the poster stays. The play
//! state machine still compiles and runs, but no media sounds.

use coursecraft_course::{Course, ObjectKind, Timing};
use coursecraft_playback::{MediaBackend, MediaKind, VideoFrame};
use std::sync::Arc;

/// Backend the timeline drives, without the runtime's trigger rules.
///
/// Implemented by the desktop (`RodioBackend`) and web backends through
/// [`MediaBackend`] and by fakes in tests.
pub trait TimelineMedia {
    /// Play or resume one track.
    fn play(&mut self, object: &str, src: &str, kind: MediaKind, bytes: Arc<[u8]>);
    /// Pause one track, keeping its last frame.
    fn pause(&mut self, object: &str);
    /// Move one track's decoder to `ms`.
    fn seek(&mut self, object: &str, ms: u64);
    /// Current playback position, when playing.
    fn position_ms(&self, object: &str) -> Option<u64>;
    /// Newest decoded frame, when the backend has one.
    fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>>;
    /// Objects that finished since the last call.
    fn take_ended(&mut self) -> Vec<String>;
    /// Stop every track and drop workers.
    fn stop_all(&mut self);
    /// Simulate real time passing (fakes only; the real backend's
    /// positions advance on their own). Default is a no-op.
    fn advance_clock(&mut self, _delta_ms: u64) {}
    /// Whether the backend has reported no audio output device. Default
    /// is `false`.
    fn no_device(&self) -> bool {
        false
    }
    /// Backend calls in order (tests only; default is empty).
    #[cfg(test)]
    fn calls_for_test(&self) -> Vec<String> {
        Vec::new()
    }
}

impl TimelineMedia for Box<dyn MediaBackend> {
    fn play(&mut self, object: &str, src: &str, kind: MediaKind, bytes: Arc<[u8]>) {
        (**self).play(object, src, kind, bytes);
    }

    fn pause(&mut self, object: &str) {
        (**self).pause(object);
    }

    fn seek(&mut self, object: &str, ms: u64) {
        (**self).seek(object, ms);
    }

    fn position_ms(&self, object: &str) -> Option<u64> {
        (**self).position_ms(object)
    }

    fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>> {
        (**self).video_frame(object)
    }

    fn take_ended(&mut self) -> Vec<String> {
        (**self).take_ended()
    }

    fn stop_all(&mut self) {
        (**self).stop_all();
    }

    fn no_device(&self) -> bool {
        (**self).no_device()
    }
}

/// One audio or video object on the slide being played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineTrack {
    /// Object id on the slide.
    pub object_id: String,
    /// Media `src` under `media/`.
    pub src: String,
    /// Audio or video.
    pub kind: MediaKind,
    /// Start of the track on the slide timeline.
    pub start_ms: u32,
    /// Effective end of the track on the slide timeline (exclusive).
    pub end_ms: u32,
    /// Media bytes, resolved from the editor's store at sync time.
    pub bytes: Arc<[u8]>,
    /// The backend has a live (paused or playing) track for this object.
    pub started: bool,
}

/// Tracks a timing row needs to build one [`TimelineTrack`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSpec {
    /// Object id on the slide.
    pub object_id: String,
    /// Media `src`.
    pub src: String,
    /// Audio or video.
    pub kind: MediaKind,
    /// Stored timing.
    pub timing: Timing,
}

/// Every audio and video object on `slide_id`, base and layers in order.
#[must_use]
pub fn timeline_track_specs(course: &Course, slide_id: &str) -> Vec<TrackSpec> {
    let Some(slide) = course.slide(slide_id) else {
        return Vec::new();
    };
    let mut specs = Vec::new();
    let mut push = |object: &coursecraft_course::Object| {
        let (src, kind) = match &object.kind {
            ObjectKind::Audio { src, .. } => (src.clone(), MediaKind::Audio),
            ObjectKind::Video { src, .. } => (src.clone(), MediaKind::Video),
            _ => return,
        };
        specs.push(TrackSpec {
            object_id: object.id.clone(),
            src,
            kind,
            timing: object.timing,
        });
    };
    for object in &slide.objects {
        push(object);
    }
    for layer in &slide.layers {
        for object in &layer.objects {
            push(object);
        }
    }
    specs
}

/// Whether `playhead_ms` falls inside `[start_ms, end_ms]`.
/// The end is included so scrubbing to the exact end of the timeline
/// still shows the track's last frame instead of the poster.
#[must_use]
pub fn timing_includes(start_ms: u32, end_ms: u32, playhead_ms: u32) -> bool {
    playhead_ms >= start_ms && playhead_ms <= end_ms
}

/// Clamp `ms` into the checked `u32` range for slide time.
fn clamp_playhead(ms: u64, duration_ms: u32) -> u32 {
    let duration = u64::from(duration_ms);
    ms.min(duration).min(u64::from(u32::MAX)) as u32
}

/// Editor timeline playback: owns the current slide's audio and video
/// tracks and advances the playhead during Play.
pub struct TimelinePlayback {
    backend: Box<dyn TimelineMedia>,
    tracks: Vec<TimelineTrack>,
    slide_id: String,
    duration_ms: u32,
    playing: bool,
    /// Playhead at the last start or seek, for the real-time fallback clock.
    anchor_playhead: u32,
    /// Monotonic clock value at the anchor.
    anchor_mono: u64,
    /// Newest monotonic clock value seen.
    last_mono: u64,
    /// The playhead was scrubbed or sought and no seek has been flushed yet.
    /// Rapid scrubs coalesce: only the newest position is ever sent.
    seek_pending: bool,
}

impl TimelinePlayback {
    /// Create playback on the default backend for this platform.
    #[must_use]
    pub fn create_default() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            // The web editor keeps its placeholder for now (plan M12
            // step 2): no video frames are drawn and no audio sounds,
            // so nothing changes there yet.
            let backend: Box<dyn MediaBackend> = Box::new(coursecraft_playback::NullBackend::new());
            Self::with_backend(Box::new(backend))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::with_backend(Box::new(coursecraft_playback::create_default_backend()))
        }
    }

    /// Create playback on an explicit backend (fakes in tests).
    #[must_use]
    pub fn with_backend(backend: Box<dyn TimelineMedia>) -> Self {
        Self {
            backend,
            tracks: Vec::new(),
            slide_id: String::new(),
            duration_ms: 100,
            playing: false,
            anchor_playhead: 0,
            anchor_mono: 0,
            last_mono: 0,
            seek_pending: false,
        }
    }

    /// Whether Play is running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Slide this state is bound to (`""` until the first sync).
    #[must_use]
    pub fn slide_id(&self) -> &str {
        &self.slide_id
    }

    /// The slide's tracks, in slide order.
    #[must_use]
    pub fn tracks(&self) -> &[TimelineTrack] {
        &self.tracks
    }

    /// Rebuild the track list for `slide_id` from the course.
    ///
    /// Changing the slide stops everything and drops workers. Changing
    /// the track list on the same slide (a document edit) stops playback
    /// but keeps the workers, so the frame on screen stays correct.
    /// Calling this every frame is cheap: an unchanged list is a no-op.
    pub fn sync_slide(
        &mut self,
        course: &Course,
        slide_id: &str,
        duration_ms: u32,
        media: &[(String, Arc<[u8]>)],
        playhead_ms: u32,
    ) {
        let duration_ms = duration_ms.max(100);
        let specs = timeline_track_specs(course, slide_id);
        let next: Vec<TimelineTrack> = specs
            .iter()
            .map(|spec| {
                let bytes = media
                    .iter()
                    .find(|(name, _)| name == &spec.src)
                    .map(|(_, bytes)| Arc::clone(bytes))
                    .unwrap_or_else(|| Arc::new([]));
                let start_ms = spec.timing.start_ms.min(duration_ms);
                let end_ms = spec
                    .timing
                    .end_ms
                    .unwrap_or(duration_ms)
                    .max(start_ms)
                    .min(duration_ms);
                TimelineTrack {
                    object_id: spec.object_id.clone(),
                    src: spec.src.clone(),
                    kind: spec.kind,
                    start_ms,
                    end_ms,
                    bytes,
                    started: false,
                }
            })
            .collect();
        if slide_id != self.slide_id.as_str() {
            self.backend.stop_all();
            self.playing = false;
            self.seek_pending = false;
            self.slide_id = slide_id.to_owned();
            self.tracks = next;
            self.duration_ms = duration_ms;
            // Arm each video whose timing includes the playhead and
            // request its frame, without needing a click: a worker only
            // exists after a seek, so without this the canvas would show
            // the placeholder on a freshly selected slide.
            self.arm_videos_at(playhead_ms.min(duration_ms));
            return;
        }
        // Same slide: compare without `started`, so an unchanged list
        // does not stop a running Play.
        let same = self.tracks.len() == next.len()
            && self.tracks.iter().zip(next.iter()).all(|(old, new)| {
                old.object_id == new.object_id
                    && old.src == new.src
                    && old.kind == new.kind
                    && old.start_ms == new.start_ms
                    && old.end_ms == new.end_ms
            });
        if same {
            self.duration_ms = duration_ms;
            // A track whose worker ended (e.g. Play reached the end and
            // the backend removed the worker) is re-armed here so a later
            // scrub still produces a frame.
            self.drain_ended();
            if !self.playing {
                self.arm_videos_at(playhead_ms.min(duration_ms));
            }
            return;
        }
        if self.playing {
            for track in &self.tracks {
                if track.started {
                    self.backend.pause(&track.object_id);
                }
            }
            self.playing = false;
        }
        // Keep the frame for tracks that survive the edit; forget the rest.
        let started: std::collections::HashMap<String, bool> = self
            .tracks
            .iter()
            .map(|track| (track.object_id.clone(), track.started))
            .collect();
        self.tracks = next;
        for track in &mut self.tracks {
            track.started = started.get(&track.object_id).copied().unwrap_or(false);
        }
        self.duration_ms = duration_ms;
        if !self.playing {
            self.arm_videos_at(playhead_ms.min(duration_ms));
        }
    }

    /// Drain finished tracks from the backend and forget their workers,
    /// so a later seek re-arms them instead of seeking a removed worker.
    fn drain_ended(&mut self) {
        for object in self.backend.take_ended() {
            if let Some(track) = self
                .tracks
                .iter_mut()
                .find(|track| track.object_id == object)
            {
                track.started = false;
            }
        }
    }

    /// Arm each video whose timing includes `playhead_ms` (play then
    /// immediate pause, so the worker exists) and seek it to the frame
    /// at the playhead. Used when a slide is selected or its tracks
    /// change, so the frame appears without a click.
    fn arm_videos_at(&mut self, playhead_ms: u32) {
        if self.playing {
            return;
        }
        for index in 0..self.tracks.len() {
            let Some(track) = self.tracks.get(index).cloned() else {
                continue;
            };
            if track.kind != MediaKind::Video || track.started {
                continue;
            }
            if !timing_includes(track.start_ms, track.end_ms, playhead_ms) {
                continue;
            }
            let offset = u64::from(playhead_ms.saturating_sub(track.start_ms));
            self.backend.play(
                &track.object_id,
                &track.src,
                track.kind,
                Arc::clone(&track.bytes),
            );
            self.backend.pause(&track.object_id);
            self.backend.seek(&track.object_id, offset);
            if let Some(track) = self.tracks.get_mut(index) {
                track.started = true;
            }
        }
    }

    /// Start (or restart) Play from `playhead_ms`.
    ///
    /// Play from the end restarts at 0. Each track whose window includes
    /// the starting playhead starts at its own offset; later tracks are
    /// scheduled as the playhead reaches them. Returns the playhead the
    /// author now sees.
    pub fn start_play(&mut self, playhead_ms: u32, now_mono: u64, duration_ms: u32) -> u32 {
        let duration_ms = duration_ms.max(100);
        self.duration_ms = duration_ms;
        self.last_mono = now_mono;
        let effective = if playhead_ms >= duration_ms {
            0
        } else {
            playhead_ms
        };
        self.playing = true;
        self.anchor_playhead = effective;
        self.anchor_mono = now_mono;
        self.seek_pending = false;
        self.start_tracks_in_window(effective);
        effective
    }

    /// Pause Play, keeping every track's last frame.
    pub fn pause_play(&mut self) {
        if !self.playing {
            return;
        }
        self.playing = false;
        for track in &self.tracks {
            if track.started {
                self.backend.pause(&track.object_id);
            }
        }
    }

    /// Forget everything: stop the backend and drop the track list.
    pub fn reset(&mut self) {
        self.backend.stop_all();
        self.tracks.clear();
        self.slide_id.clear();
        self.playing = false;
        self.seek_pending = false;
        self.anchor_playhead = 0;
        self.anchor_mono = 0;
        self.last_mono = 0;
    }

    /// Record a scrub or seek to `playhead_ms`.
    ///
    /// Seeks are coalesced: several calls before [`Self::flush_seeks`]
    /// send only the newest request. Seeking while playing keeps
    /// playing and re-anchors the fallback clock at the new position.
    pub fn seek_to(&mut self, playhead_ms: u32, duration_ms: u32, now_mono: u64) {
        let duration_ms = duration_ms.max(100);
        self.duration_ms = duration_ms;
        self.last_mono = now_mono;
        let playhead_ms = playhead_ms.min(duration_ms);
        self.seek_pending = true;
        if self.playing {
            self.anchor_playhead = playhead_ms;
            self.anchor_mono = now_mono;
        }
    }

    /// Whether a seek is waiting to be sent to the backend.
    #[must_use]
    pub fn seek_pending(&self) -> bool {
        self.seek_pending
    }

    /// Send the newest pending seek (if any) to the backend.
    ///
    /// Scrubbing while paused arms each video in the window
    /// (play then immediate pause) so the worker exists, and seeks it;
    /// the desktop worker decodes one frame after a paused seek, so the
    /// canvas keeps showing the last frame until the new one arrives,
    /// never a blank flash and never a blocked frame. While playing,
    /// every started track is sought; tracks whose window the seek
    /// left are paused.
    pub fn flush_seeks(&mut self, playhead_ms: u32) {
        if !self.seek_pending {
            return;
        }
        self.seek_pending = false;
        // A worker removed after ending (Play reached the end) must be
        // re-armed by the branches below, not just sought: seeking a
        // removed worker is a no-op and no frame would ever arrive.
        self.drain_ended();
        let playhead_ms = playhead_ms.min(self.duration_ms);
        let playing = self.playing;
        for index in 0..self.tracks.len() {
            let in_window = self
                .tracks
                .get(index)
                .is_some_and(|track| timing_includes(track.start_ms, track.end_ms, playhead_ms));
            if in_window {
                let Some(track) = self.tracks.get(index).cloned() else {
                    continue;
                };
                let (object_id, src, kind, start_ms, bytes, started) = (
                    track.object_id.clone(),
                    track.src.clone(),
                    track.kind,
                    track.start_ms,
                    Arc::clone(&track.bytes),
                    track.started,
                );
                let offset = u64::from(playhead_ms.saturating_sub(start_ms));
                if playing {
                    if !started {
                        self.backend
                            .play(&object_id, &src, kind, Arc::clone(&bytes));
                        self.backend.seek(&object_id, offset);
                        if let Some(track) = self.tracks.get_mut(index) {
                            track.started = true;
                        }
                    } else {
                        self.backend.seek(&object_id, offset);
                    }
                } else if kind == MediaKind::Video {
                    if !started {
                        self.backend
                            .play(&object_id, &src, kind, Arc::clone(&bytes));
                        self.backend.pause(&object_id);
                        if let Some(track) = self.tracks.get_mut(index) {
                            track.started = true;
                        }
                    }
                    self.backend.seek(&object_id, offset);
                }
            } else if playing {
                let Some(track) = self.tracks.get(index).cloned() else {
                    continue;
                };
                if track.started {
                    self.backend.pause(&track.object_id);
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.started = false;
                    }
                }
            }
        }
    }

    /// Advance Play to `now_mono` and return the new playhead.
    ///
    /// The playhead follows the audio clock when a sound track reports
    /// a position (a video with sound or an audio object), else real
    /// time from the anchor. Stops at the end of the timeline and stays
    /// there; the next Play restarts at 0.
    pub fn tick(&mut self, playhead_ms: u32, now_mono: u64, duration_ms: u32) -> u32 {
        let duration_ms = duration_ms.max(100);
        self.duration_ms = duration_ms;
        self.last_mono = now_mono;
        if !self.playing {
            return playhead_ms.min(duration_ms);
        }
        for object in self.backend.take_ended() {
            if let Some(track) = self
                .tracks
                .iter_mut()
                .find(|track| track.object_id == object)
            {
                track.started = false;
            }
        }
        let mut master: Option<u32> = None;
        // Audio objects first: they are pure sound clocks.
        for track in self.tracks.iter() {
            if track.kind == MediaKind::Audio
                && track.started
                && let Some(position) = self.backend.position_ms(&track.object_id)
            {
                master = Some(
                    track
                        .start_ms
                        .saturating_add(clamp_playhead(position, u32::MAX))
                        .min(duration_ms),
                );
                break;
            }
        }
        // Then videos: with sound their position is the audio clock,
        // without sound it is the worker's own clock (real time).
        if master.is_none() {
            for track in self.tracks.iter() {
                if track.kind == MediaKind::Video
                    && track.started
                    && let Some(position) = self.backend.position_ms(&track.object_id)
                {
                    master = Some(
                        track
                            .start_ms
                            .saturating_add(clamp_playhead(position, u32::MAX))
                            .min(duration_ms),
                    );
                    break;
                }
            }
        }
        let mut new_playhead = if let Some(master) = master {
            master
        } else {
            let elapsed = now_mono.saturating_sub(self.anchor_mono);
            playhead_ms.saturating_add(clamp_playhead(elapsed, u32::MAX))
        }
        .min(duration_ms);
        // Schedule tracks whose window the playhead just entered and
        // pause tracks whose window it left.
        for index in 0..self.tracks.len() {
            let (in_window, started, start_ms) = self
                .tracks
                .get(index)
                .map(|track| {
                    (
                        timing_includes(track.start_ms, track.end_ms, new_playhead),
                        track.started,
                        track.start_ms,
                    )
                })
                .unwrap_or((false, false, 0));
            if in_window && !started {
                let Some(track) = self.tracks.get(index).cloned() else {
                    continue;
                };
                let offset = u64::from(new_playhead.saturating_sub(start_ms));
                self.backend.play(
                    &track.object_id,
                    &track.src,
                    track.kind,
                    Arc::clone(&track.bytes),
                );
                self.backend.seek(&track.object_id, offset);
                if let Some(track) = self.tracks.get_mut(index) {
                    track.started = true;
                }
            }
        }
        for index in 0..self.tracks.len() {
            let (in_window, started) = self
                .tracks
                .get(index)
                .map(|track| {
                    (
                        timing_includes(track.start_ms, track.end_ms, new_playhead),
                        track.started,
                    )
                })
                .unwrap_or((false, false));
            if !in_window && started && new_playhead < duration_ms {
                // A slot whose window the playhead just left keeps its
                // last frame; the canvas draws the poster outside the
                // timing. Pause only when playback passes the end, or
                // when a seek moved before the window; the latter is
                // handled by flush_seeks.
                if let Some(track) = self.tracks.get(index)
                    && new_playhead < track.start_ms
                {
                    let object_id = track.object_id.clone();
                    self.backend.pause(&object_id);
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.started = false;
                    }
                }
            }
        }
        self.anchor_playhead = new_playhead;
        self.anchor_mono = now_mono;
        if new_playhead >= duration_ms {
            self.playing = false;
            new_playhead = duration_ms;
            for track in &self.tracks {
                if track.started {
                    self.backend.pause(&track.object_id);
                }
            }
        }
        new_playhead
    }

    /// The frame the canvas should draw for `object` at `playhead_ms`.
    ///
    /// `None` outside the track's timing (the poster), and `None` until
    /// the backend has produced a frame. While a seek is in flight the
    /// backend's newest frame keeps showing: never a blank flash.
    #[must_use]
    pub fn video_frame_for(&self, object: &str, playhead_ms: u32) -> Option<Arc<VideoFrame>> {
        let track = self.tracks.iter().find(|track| track.object_id == object)?;
        if track.kind != MediaKind::Video {
            return None;
        }
        if !timing_includes(track.start_ms, track.end_ms, playhead_ms) {
            return None;
        }
        self.backend.video_frame(object)
    }

    /// Whether the canvas has a frame in flight for `playhead_ms`:
    /// any video in the window whose frame has not arrived yet, or whose
    /// newest frame is not the one at the playhead (a seek is still in
    /// flight and the old frame is on screen). Keeping repaint true
    /// until the requested frame lands is what lets the canvas pick it
    /// up without a click.
    #[must_use]
    pub fn has_pending_frame(&self, playhead_ms: u32) -> bool {
        self.tracks.iter().any(|track| {
            if track.kind != MediaKind::Video
                || !timing_includes(track.start_ms, track.end_ms, playhead_ms)
            {
                return false;
            }
            match self.backend.video_frame(&track.object_id) {
                None => true,
                Some(frame) => {
                    let expected = u64::from(playhead_ms.saturating_sub(track.start_ms));
                    // One frame of tolerance (the decoder returns the
                    // frame at or just after the seek).
                    frame.pts_ms.abs_diff(expected) > 100
                }
            }
        })
    }

    /// Whether the UI should request a repaint: playback runs, a seek
    /// is pending, or a frame has not arrived yet.
    #[must_use]
    pub fn needs_repaint(&self, playhead_ms: u32) -> bool {
        self.playing || self.seek_pending || self.has_pending_frame(playhead_ms)
    }

    fn start_tracks_in_window(&mut self, playhead_ms: u32) {
        for index in 0..self.tracks.len() {
            let Some(track) = self.tracks.get(index).cloned() else {
                continue;
            };
            if !timing_includes(track.start_ms, track.end_ms, playhead_ms) {
                if let Some(track) = self.tracks.get_mut(index) {
                    track.started = false;
                }
                continue;
            }
            let offset = u64::from(playhead_ms.saturating_sub(track.start_ms));
            self.backend.play(
                &track.object_id,
                &track.src,
                track.kind,
                Arc::clone(&track.bytes),
            );
            self.backend.seek(&track.object_id, offset);
            if let Some(track) = self.tracks.get_mut(index) {
                track.started = true;
            }
        }
    }

    /// All backend calls in order (tests).
    #[cfg(test)]
    #[must_use]
    pub fn backend_calls_for_test(&self) -> Vec<String> {
        self.backend.calls_for_test()
    }

    /// Simulate real time passing for a fake backend (tests).
    #[cfg(test)]
    pub fn advance_clock_for_test(&mut self, delta_ms: u64) {
        self.backend.advance_clock(delta_ms);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// Fake frame source: positions only move when the test moves them,
    /// and a seek publishes its frame immediately (like the worker
    /// thread would, but synchronously for determinism).
    #[derive(Debug, Default)]
    pub(crate) struct FakeTimelineMedia {
        pub calls: Vec<String>,
        playing: HashSet<String>,
        positions: HashMap<String, u64>,
        frames: HashMap<String, u64>,
        ended: Vec<String>,
    }

    impl FakeTimelineMedia {
        pub(crate) fn new() -> Self {
            Self::default()
        }
    }

    impl TimelineMedia for FakeTimelineMedia {
        fn play(&mut self, object: &str, src: &str, _kind: MediaKind, _bytes: Arc<[u8]>) {
            let key = format!("play:{object}:{src}");
            if !self.playing.insert(object.to_owned()) {
                self.calls.push(format!("resume:{object}:{src}"));
            } else {
                self.calls.push(key);
            }
        }

        fn pause(&mut self, object: &str) {
            self.playing.remove(object);
            self.calls.push(format!("pause:{object}"));
        }

        fn seek(&mut self, object: &str, ms: u64) {
            self.calls.push(format!("seek:{object}:{ms}"));
            self.positions.insert(object.to_owned(), ms);
            self.frames.insert(object.to_owned(), ms);
        }

        fn position_ms(&self, object: &str) -> Option<u64> {
            if self.playing.contains(object) {
                self.positions.get(object).copied().or(Some(0))
            } else {
                None
            }
        }

        fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>> {
            self.frames.get(object).map(|pts| {
                Arc::new(VideoFrame {
                    pts_ms: *pts,
                    width: 16,
                    height: 16,
                    rgba: vec![0; 16 * 16 * 4],
                })
            })
        }

        fn take_ended(&mut self) -> Vec<String> {
            std::mem::take(&mut self.ended)
        }

        fn stop_all(&mut self) {
            self.playing.clear();
            self.positions.clear();
            self.calls.push("stop_all".to_owned());
        }

        fn calls_for_test(&self) -> Vec<String> {
            self.calls.clone()
        }

        fn advance_clock(&mut self, delta_ms: u64) {
            for object in self.playing.clone() {
                let entry = self.positions.entry(object).or_insert(0);
                *entry = entry.saturating_add(delta_ms);
            }
        }
    }

    fn two_slide_course() -> Course {
        let json = serde_json::json!({
            "schema_version": 1,
            "id": "timeline-media",
            "title": "Timeline media",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{
                "id": "scene1",
                "title": "Scene 1",
                "slides": [
                    {
                        "id": "s1",
                        "title": "One",
                        "duration_ms": 10000,
                        "objects": [
                            {
                                "id": "vid",
                                "kind": { "type": "video", "src": "intro.mp4", "autoplay": false },
                                "frame": { "x": 0, "y": 0, "w": 640, "h": 360 },
                                "timing": { "start_ms": 1000, "end_ms": 6000 }
                            },
                            {
                                "id": "aud",
                                "kind": { "type": "audio", "src": "note.wav", "autoplay": false },
                                "frame": { "x": 0, "y": 400, "w": 48, "h": 48 },
                                "timing": { "start_ms": 2000, "end_ms": 8000 }
                            }
                        ]
                    },
                    {
                        "id": "s2",
                        "title": "Two",
                        "duration_ms": 5000,
                        "objects": [
                            {
                                "id": "vid2",
                                "kind": { "type": "video", "src": "intro.mp4", "autoplay": false },
                                "frame": { "x": 0, "y": 0, "w": 640, "h": 360 },
                                "timing": { "start_ms": 0, "end_ms": 5000 }
                            }
                        ]
                    }
                ]
            }]
        });
        Course::from_json(&json.to_string()).expect("valid timeline course")
    }

    fn media() -> Vec<(String, Arc<[u8]>)> {
        vec![
            (
                "intro.mp4".to_owned(),
                Arc::from(vec![7u8; 32].into_boxed_slice()),
            ),
            (
                "note.wav".to_owned(),
                Arc::from(vec![9u8; 32].into_boxed_slice()),
            ),
        ]
    }

    fn fake_playback() -> TimelinePlayback {
        TimelinePlayback::with_backend(Box::new(FakeTimelineMedia::new()) as Box<dyn TimelineMedia>)
    }

    #[test]
    fn scrub_shows_the_requested_frame_and_coalesces() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        assert_eq!(playback.tracks().len(), 2, "video and audio tracks");
        // Rapid scrubs: only the newest seek is sent.
        playback.seek_to(2500, 10000, 0);
        playback.seek_to(3000, 10000, 0);
        playback.seek_to(3500, 10000, 0);
        assert!(playback.seek_pending());
        playback.flush_seeks(3500);
        let frame = playback
            .video_frame_for("vid", 3500)
            .expect("frame for scrubbed playhead");
        assert_eq!(frame.pts_ms, 2500, "3500 - start 1000");
        let vid_calls: Vec<String> = playback
            .backend_calls_for_test()
            .into_iter()
            .filter(|call| call.contains(":vid"))
            .collect();
        assert_eq!(
            vid_calls,
            vec![
                "play:vid:intro.mp4".to_owned(),
                "pause:vid".to_owned(),
                "seek:vid:2500".to_owned()
            ],
            "only the newest seek is sent"
        );
        assert!(!playback.seek_pending());
        assert!(!playback.has_pending_frame(3500), "frame arrived");
    }

    #[test]
    fn selecting_the_slide_arms_the_video_at_the_playhead() {
        // No scrub, no click: syncing the slide at a playhead inside the
        // video's timing arms the worker and requests the frame.
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 2500);
        let frame = playback
            .video_frame_for("vid", 2500)
            .expect("frame appears with no scrub after selecting the slide");
        assert_eq!(frame.pts_ms, 1500, "2500 - start 1000");
        assert!(!playback.has_pending_frame(2500), "frame already arrived");
        // A playhead whose frame has not arrived (no seek sent for it)
        // stays pending, so the canvas keeps repainting.
        assert!(
            playback.has_pending_frame(4000),
            "stale frame means a frame is still in flight"
        );
        assert!(playback.needs_repaint(4000));
    }

    #[test]
    fn timing_includes_the_exact_end() {
        assert!(timing_includes(1000, 6000, 6000), "exact end is included");
        assert!(!timing_includes(1000, 6000, 6001), "past the end is not");
        assert!(timing_includes(1000, 6000, 1000), "start is included");
        assert!(!timing_includes(1000, 6000, 999), "before start is not");
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        playback.seek_to(6000, 10000, 0);
        playback.flush_seeks(6000);
        let frame = playback
            .video_frame_for("vid", 6000)
            .expect("frame at the exact end of the timing");
        assert_eq!(frame.pts_ms, 5000, "6000 - start 1000");
    }

    #[test]
    fn poster_outside_timing_and_kept_frame_while_pending() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        playback.seek_to(2500, 10000, 0);
        playback.flush_seeks(2500);
        assert_eq!(
            playback.video_frame_for("vid", 2500).map(|f| f.pts_ms),
            Some(1500)
        );
        // Outside the timing (video runs 1000..6000) the poster shows.
        assert!(
            playback.video_frame_for("vid", 7000).is_none(),
            "poster outside timing"
        );
        assert!(
            playback.video_frame_for("vid", 500).is_none(),
            "poster before timing"
        );
    }

    #[test]
    fn play_advances_and_stops_at_the_end() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let started = playback.start_play(0, 0, 10000);
        assert_eq!(started, 0);
        assert!(playback.is_playing());
        // Real-time clock: the fake's positions advance with the mono
        // clock, as the real backend's would.
        playback.advance_clock_for_test(1000);
        let next = playback.tick(0, 1000, 10000);
        assert_eq!(next, 1000, "real time drives the playhead");
        assert!(playback.is_playing());
        playback.advance_clock_for_test(9000);
        let end = playback.tick(next, 10000, 10000);
        assert_eq!(end, 10000, "stops at the end");
        assert!(!playback.is_playing(), "stays there");
        let stays = playback.tick(10000, 12000, 10000);
        assert_eq!(stays, 10000, "tick while stopped changes nothing");
    }

    #[test]
    fn play_from_the_end_restarts_at_zero() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let _ = playback.start_play(10000, 5000, 10000);
        assert!(playback.is_playing());
        assert_eq!(playback.tracks().len(), 2);
        let stopped = playback.tick(0, 5000, 10000);
        assert_eq!(stopped, 0, "restarted playhead");
        assert!(playback.is_playing());
    }

    #[test]
    fn seek_during_play_keeps_playing() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let _ = playback.start_play(0, 0, 10000);
        playback.advance_clock_for_test(2000);
        let at = playback.tick(0, 2000, 10000);
        assert_eq!(at, 2000);
        playback.seek_to(5000, 10000, 2000);
        assert!(playback.is_playing(), "seek keeps playing");
        playback.flush_seeks(5000);
        assert!(playback.is_playing(), "flush keeps playing");
        assert_eq!(
            playback.video_frame_for("vid", 5000).map(|f| f.pts_ms),
            Some(4000),
            "5000 - start 1000"
        );
        playback.advance_clock_for_test(1000);
        let next = playback.tick(5000, 3000, 10000);
        assert_eq!(next, 6000, "playhead continues from the seek");
    }

    #[test]
    fn audio_start_offset_when_playing_from_the_middle() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let started = playback.start_play(3500, 0, 10000);
        assert_eq!(started, 3500);
        let aud_calls: Vec<String> = playback
            .backend_calls_for_test()
            .into_iter()
            .filter(|call| call.contains(":aud"))
            .collect();
        assert_eq!(
            aud_calls,
            vec!["play:aud:note.wav".to_owned(), "seek:aud:1500".to_owned()],
            "audio starts at 3500 - start 2000 = 1500"
        );
        // The audio clock then drives the playhead: position 1500 -> 3500.
        let next = playback.tick(3500, 1000, 10000);
        assert_eq!(next, 3500, "fake audio clock holds the playhead");
    }

    #[test]
    fn slide_change_stops_playback() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let _ = playback.start_play(0, 0, 10000);
        assert!(playback.is_playing());
        playback.sync_slide(&course, "s2", 5000, &media(), 0);
        assert!(!playback.is_playing(), "slide change stops playback");
        assert_eq!(playback.slide_id(), "s2");
        assert_eq!(playback.tracks().len(), 1, "only the new slide's tracks");
        assert!(
            playback
                .backend_calls_for_test()
                .contains(&"stop_all".to_owned())
        );
    }

    #[test]
    fn document_edit_on_the_slide_stops_playback() {
        let course = two_slide_course();
        let mut playback = fake_playback();
        playback.sync_slide(&course, "s1", 10000, &media(), 0);
        let _ = playback.start_play(0, 0, 10000);
        assert!(playback.is_playing());
        // An edit that moves the video: same slide, changed timing.
        let mut edited = course.clone();
        if let Some(slide) = edited.slide_mut("s1") {
            for object in &mut slide.objects {
                if object.id == "vid" {
                    object.timing.start_ms = 1500;
                }
            }
        }
        playback.sync_slide(&edited, "s1", 10000, &media(), 0);
        assert!(!playback.is_playing(), "document edit stops playback");
        let track = playback
            .tracks()
            .iter()
            .find(|track| track.object_id == "vid")
            .expect("video track after edit");
        assert_eq!(track.start_ms, 1500, "new timing is in place");
    }

    /// One-slide course whose video spans the whole 3 s showcase clip.
    #[cfg(target_os = "macos")]
    fn showcase_course() -> Course {
        let json = serde_json::json!({
            "schema_version": 1,
            "id": "timeline-real",
            "title": "Timeline real decoder",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{
                "id": "scene1",
                "title": "Scene 1",
                "slides": [{
                    "id": "s1",
                    "title": "One",
                    "duration_ms": 3000,
                    "objects": [{
                        "id": "vid",
                        "kind": { "type": "video", "src": "intro.mp4", "autoplay": false },
                        "frame": { "x": 0, "y": 0, "w": 640, "h": 360 },
                        "timing": { "start_ms": 0, "end_ms": 3000 }
                    }]
                }]
            }]
        });
        Course::from_json(&json.to_string()).expect("valid showcase course")
    }

    /// Poll `playback` until a frame near `want_ms` arrives or `timeout`
    /// passes. Returns the frame, or `None` on timeout (the caller
    /// decides whether a missing audio device explains it).
    #[cfg(target_os = "macos")]
    fn poll_frame_near(
        playback: &TimelinePlayback,
        want_ms: u64,
        timeout: std::time::Duration,
    ) -> Option<std::sync::Arc<VideoFrame>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(frame) = playback.video_frame_for("vid", want_ms as u32) {
                let diff = frame.pts_ms.abs_diff(want_ms);
                if diff <= 100 {
                    return Some(frame);
                }
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Real decoder, real file: scrubbing must deliver frames whose
    /// presentation time matches the request, and different positions
    /// must give different pixels (the clip has a burnt-in timecode).
    #[cfg(target_os = "macos")]
    #[test]
    fn real_decoder_scrubs_to_the_requested_frames() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/showcase/media/intro.mp4");
        let bytes = std::fs::read(&path).expect("showcase intro.mp4 reads");
        assert!(!bytes.is_empty(), "showcase clip is not empty");
        let course = showcase_course();
        let media = vec![("intro.mp4".to_owned(), Arc::from(bytes.into_boxed_slice()))];
        let backend: Box<dyn TimelineMedia> =
            Box::new(coursecraft_playback::create_default_backend());
        let mut playback = TimelinePlayback::with_backend(backend);
        playback.sync_slide(&course, "s1", 3000, &media, 0);
        assert_eq!(playback.tracks().len(), 1, "one video track");

        // No scrub at all: selecting the slide arms the video at the
        // playhead (0) and a frame must appear on its own. Opening the
        // file (and the first seek) is slow on a cold AVFoundation and
        // happens on the worker thread, so allow a long poll; once the
        // first seek has run, later seeks are fast.
        let Some(_) = poll_frame_near(&playback, 0, std::time::Duration::from_secs(180)) else {
            assert!(
                playback.backend.no_device(),
                "no frame near 0 ms without a scrub and the backend did not report no device"
            );
            return;
        };

        playback.seek_to(500, 3000, 0);
        playback.flush_seeks(500);
        let Some(first) = poll_frame_near(&playback, 500, std::time::Duration::from_secs(20))
        else {
            assert!(
                playback.backend.no_device(),
                "no frame near 500 ms and the backend did not report no device"
            );
            return;
        };
        assert!(
            first.pts_ms.abs_diff(500) <= 100,
            "first frame pts {} is near 500 ms",
            first.pts_ms
        );
        assert!(!first.rgba.is_empty(), "first frame has pixels");

        playback.seek_to(2000, 3000, 0);
        playback.flush_seeks(2000);
        let Some(second) = poll_frame_near(&playback, 2000, std::time::Duration::from_secs(20))
        else {
            assert!(
                playback.backend.no_device(),
                "no frame near 2000 ms and the backend did not report no device"
            );
            return;
        };
        assert!(
            second.pts_ms.abs_diff(2000) <= 100,
            "second frame pts {} is near 2000 ms",
            second.pts_ms
        );
        assert_ne!(
            first.rgba, second.rgba,
            "different positions give different pixels"
        );

        // Scrubbing to the exact end of the timeline shows the last frame
        // (not the poster, not nothing).
        playback.seek_to(3000, 3000, 0);
        playback.flush_seeks(3000);
        let Some(last) = poll_frame_near(&playback, 3000, std::time::Duration::from_secs(30))
        else {
            assert!(
                playback.backend.no_device(),
                "no frame near the end and the backend did not report no device"
            );
            return;
        };
        assert!(
            last.pts_ms >= 2800,
            "last frame pts {} is near the end",
            last.pts_ms
        );

        // Play to the end, then scrub: a worker that reported `ended`
        // must still produce a frame for a later scrub.
        let started = playback.start_play(0, 0, 3000);
        assert_eq!(started, 0);
        let mut playhead = 0u32;
        let mut mono = 0u64;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while playback.is_playing() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
            mono = mono.saturating_add(50);
            playhead = playback.tick(playhead, mono, 3000);
        }
        assert!(
            !playback.is_playing(),
            "playback stopped at the end (playhead={playhead})"
        );
        assert_eq!(playhead, 3000, "playhead reached the end");

        playback.seek_to(500, 3000, mono);
        playback.flush_seeks(500);
        let Some(after_end) = poll_frame_near(&playback, 500, std::time::Duration::from_secs(30))
        else {
            assert!(
                playback.backend.no_device(),
                "no frame near 500 ms after playing to the end"
            );
            return;
        };
        assert!(
            after_end.pts_ms.abs_diff(500) <= 100,
            "frame after playing to the end has pts {} near 500 ms",
            after_end.pts_ms
        );
        playback.reset();
    }
}
