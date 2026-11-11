//! MP4 export (desktop only).
//!
//! Runs one [`Player`] through the linear play order on an export clock,
//! drawing every frame with the same compositor the player uses, and writes
//! H.264 video plus mixed AAC audio through the OS encoder.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use coursecraft_course::{Action, Course, ObjectKind, Slide, SlideTarget};
use coursecraft_logic::{Effect, Input, MediaOp, Player};
use coursecraft_playback::{MediaKind, VideoFrame};
use coursecraft_render::{MediaFetch, MediaSource, find_media_target, render_player_rgba};
use coursecraft_video_native::VideoFile;
use coursecraft_video_native::mp4::{AudioSettings, Mp4Settings, Mp4Writer};

use crate::audio_mix::{AudioMixer, SAMPLE_RATE};

/// Options for an MP4 export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp4Options {
    /// Video width in pixels (1280 or 1920 for the usual export sizes).
    pub width: u32,
    /// Frames per second.
    pub fps: u32,
    /// Hold after a slide's timeline ends before pressing Next.
    pub slide_tail_ms: u32,
    /// Hard limit for the whole export, in milliseconds (2 hours by default).
    pub max_total_ms: u64,
}

impl Default for Mp4Options {
    fn default() -> Self {
        Self {
            width: 1280,
            fps: 30,
            slide_tail_ms: 500,
            max_total_ms: 2 * 60 * 60 * 1000,
        }
    }
}

/// Progress reported after every written frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportProgress {
    /// Zero-based index of the slide being drawn.
    pub slide_index: usize,
    /// Number of slides in the play order.
    pub slide_total: usize,
    /// Frames written so far.
    pub frames_written: u64,
    /// Export clock time in milliseconds.
    pub elapsed_ms: u64,
}

/// What an export produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportReport {
    /// Duration of the written track in milliseconds.
    pub duration_ms: u64,
    /// Number of video frames written.
    pub frames: u64,
    /// Question slides drawn without an answer.
    pub skipped_questions: Vec<String>,
    /// Slides whose triggers would have branched away from the linear path.
    pub branch_slides: Vec<String>,
    /// Non-fatal problems (missing or undecodable media, runtime warnings).
    pub warnings: Vec<String>,
}

/// Why an export could not finish.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The course is not valid and cannot run in the player.
    #[error("course is not playable: {0:?}")]
    NotPlayable(Vec<String>),
    /// The compositor could not draw a frame, or the options are invalid.
    #[error("render failed: {0}")]
    Render(String),
    /// Media could not be prepared.
    #[error("media failed: {0}")]
    Media(String),
    /// The OS encoder failed.
    #[error("encoder failed: {0}")]
    Encoder(String),
    /// The progress callback asked to stop.
    #[error("export cancelled")]
    Cancelled,
    /// The export reached [`Mp4Options::max_total_ms`].
    #[error("export is longer than the configured limit")]
    TooLong,
}

/// Synchronous byte source for media used by an export.
///
/// Export runs offline, so a missing asset is final rather than pending.
pub trait ExportMedia {
    /// Bytes for the media source `src`, or `None` when it is missing.
    fn bytes(&self, src: &str) -> Option<Arc<[u8]>>;
}

/// Export `course` to an MP4 at `out`, following the linear play path.
///
/// `progress` is called after every frame; returning `false` cancels the
/// export, deletes the partial file, and returns [`ExportError::Cancelled`].
///
/// # Errors
/// Returns an [`ExportError`] for an unplayable course, invalid options, a
/// render or encoder failure, cancellation, or the configured time limit.
pub fn export_mp4(
    course: &Course,
    media: &dyn ExportMedia,
    out: &Path,
    options: &Mp4Options,
    progress: &mut dyn FnMut(&ExportProgress) -> bool,
) -> Result<ExportReport, ExportError> {
    let width = checked_width(options.width)?;
    let height = frame_height(course, width)?;
    let fps = options.fps.clamp(1, 60);
    let settings = Mp4Settings {
        width,
        height,
        fps,
        audio: Some(AudioSettings {
            sample_rate: SAMPLE_RATE,
            channels: 2,
        }),
        video_bitrate_bps: 0,
    };
    let _ = std::fs::remove_file(out);
    let mut writer = Mp4Writer::create(out, settings).map_err(encoder)?;

    match run_export(course, media, options, progress, &mut writer, width, fps) {
        Ok(report) => match writer.finish() {
            Ok(()) => Ok(report),
            Err(error) => {
                let _ = std::fs::remove_file(out);
                Err(encoder(error))
            }
        },
        Err(error) => {
            drop(writer);
            let _ = std::fs::remove_file(out);
            Err(error)
        }
    }
}

fn checked_width(width: u32) -> Result<u32, ExportError> {
    if !(16..=4096).contains(&width) {
        return Err(ExportError::Render(format!(
            "export width {width} is outside 16..=4096"
        )));
    }
    Ok((width & !1).max(16))
}

fn frame_height(course: &Course, width: u32) -> Result<u32, ExportError> {
    let slide_w = f64::from(course.slide_size.w);
    let slide_h = f64::from(course.slide_size.h);
    if !slide_w.is_finite() || !slide_h.is_finite() || slide_w <= 0.0 || slide_h <= 0.0 {
        return Err(ExportError::Render(format!(
            "invalid course slide size {}x{}",
            course.slide_size.w, course.slide_size.h
        )));
    }
    let height = (f64::from(width) * slide_h / slide_w).round();
    if !height.is_finite() {
        return Err(ExportError::Render("invalid export height".to_string()));
    }
    Ok((height.clamp(16.0, 4096.0) as u32) & !1)
}

fn encoder(error: coursecraft_video_native::VideoError) -> ExportError {
    ExportError::Encoder(error.to_string())
}

/// Whether `t_ms` is past the configured export limit.
pub(crate) fn exceeds_max_total(t_ms: u64, max_total_ms: u64) -> bool {
    t_ms > max_total_ms
}

/// Export clock time of frame `index`, in milliseconds.
pub(crate) fn frame_time_ms(index: u64, fps: u32) -> u64 {
    index
        .saturating_mul(1_000_000)
        .checked_div(u64::from(fps.max(1)))
        .unwrap_or(0)
        / 1000
}

fn run_export(
    course: &Course,
    media: &dyn ExportMedia,
    options: &Mp4Options,
    progress: &mut dyn FnMut(&ExportProgress) -> bool,
    writer: &mut Mp4Writer,
    width: u32,
    fps: u32,
) -> Result<ExportReport, ExportError> {
    let branch = branch_slides(course);
    let mut exporter = Exporter::new(course, width)?;
    exporter.note_current_slide();
    let start = exporter.player.handle(&Input::Start);
    exporter.apply(&start, media, 0);
    exporter.note_current_slide();

    let mut t = 0u64;
    let mut index = 0u64;
    let mut frames = 0u64;
    let mut duration_ms: u64;
    loop {
        exporter.render_and_write(t, width, media, writer)?;
        let next_t = frame_time_ms(index.saturating_add(1), fps);
        let ended = exporter.mixer.mix_step(t, next_t);
        writer
            .write_audio(exporter.mixer.samples(), t)
            .map_err(encoder)?;
        frames = frames.saturating_add(1);
        duration_ms = next_t;

        let slides = exporter.player.play_order().len();
        let report = ExportProgress {
            slide_index: exporter.player.state().slide,
            slide_total: slides,
            frames_written: frames,
            elapsed_ms: t,
        };
        if !progress(&report) {
            return Err(ExportError::Cancelled);
        }

        for object in ended {
            let effects = exporter.player.handle(&Input::MediaEnded {
                object: object.clone(),
            });
            exporter.apply(&effects, media, next_t);
            exporter.note_current_slide();
            if exporter.exit {
                break;
            }
        }
        if exporter.exit {
            break;
        }
        if exceeds_max_total(next_t, options.max_total_ms) {
            return Err(ExportError::TooLong);
        }
        let Some(step) = next_t.checked_sub(t).and_then(|ms| u32::try_from(ms).ok()) else {
            return Err(ExportError::Render(
                "export clock did not advance".to_string(),
            ));
        };
        t = next_t;
        index = index.saturating_add(1);
        let effects = exporter.player.handle(&Input::Tick { ms: step });
        exporter.apply(&effects, media, t);
        exporter.report_media_times(media, t);
        exporter.note_current_slide();
        if exporter.slide_finished(t, options.slide_tail_ms, media) {
            break;
        }
    }

    Ok(ExportReport {
        duration_ms,
        frames,
        skipped_questions: exporter.skipped_questions,
        branch_slides: branch,
        warnings: exporter.warnings,
    })
}

/// Slides whose triggers would take the course off the linear path.
///
/// A `jump_to_slide` to `Next` is the linear path and does not count.
pub fn branch_slides(course: &Course) -> Vec<String> {
    let mut out = Vec::new();
    let slides = course
        .scenes
        .iter()
        .flat_map(|scene| scene.slides.iter())
        .chain(course.banks.iter().flat_map(|bank| bank.slides.iter()));
    for slide in slides {
        if has_branch_trigger(slide) && !out.contains(&slide.id) {
            out.push(slide.id.clone());
        }
    }
    out
}

fn has_branch_trigger(slide: &Slide) -> bool {
    slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
        .any(|trigger| match &trigger.action {
            Action::JumpToSlide { target } => !matches!(target, SlideTarget::Next),
            _ => false,
        })
}

/// The exporter's live state: one player, the audio mix and video decoders.
struct Exporter<'a> {
    player: Player<'a>,
    mixer: AudioMixer,
    video: VideoPlayback,
    warnings: Vec<String>,
    skipped_questions: Vec<String>,
    slide_end_t: Option<u64>,
    slide_id: Option<String>,
    exit: bool,
}

impl<'a> Exporter<'a> {
    fn new(course: &'a Course, width: u32) -> Result<Self, ExportError> {
        let player = Player::new(course).map_err(|issues| {
            ExportError::NotPlayable(
                issues
                    .into_iter()
                    .map(|issue| format!("{}: {}", issue.path, issue.message))
                    .collect(),
            )
        })?;
        Ok(Self {
            player,
            mixer: AudioMixer::new(),
            video: VideoPlayback::new(width),
            warnings: Vec::new(),
            skipped_questions: Vec::new(),
            slide_end_t: None,
            slide_id: None,
            exit: false,
        })
    }

    /// Record the current slide when the exporter enters it.
    fn note_current_slide(&mut self) {
        let Some(slide) = self.player.current_slide() else {
            return;
        };
        if self.slide_id.as_deref() == Some(slide.id.as_str()) {
            return;
        }
        self.slide_id = Some(slide.id.clone());
        if slide.question.is_some() && !self.skipped_questions.contains(&slide.id) {
            self.skipped_questions.push(slide.id.clone());
        }
    }

    fn apply(&mut self, effects: &[Effect], media: &dyn ExportMedia, t: u64) {
        for effect in effects {
            match effect {
                Effect::Media { object, op } => match op {
                    MediaOp::Play => self.start_media(object, media, t),
                    MediaOp::Pause => {
                        self.mixer.end(object);
                        self.video.pause(object, t);
                    }
                    MediaOp::Stop => {
                        self.mixer.end(object);
                        self.video.stop(object);
                    }
                },
                Effect::MediaSeek { object, ms } => {
                    self.mixer.seek(object, *ms, t);
                    self.video.seek(object, *ms, t);
                }
                Effect::Exit => self.exit = true,
                Effect::Warning { message } => self.warnings.push(message.clone()),
                _ => {}
            }
        }
    }

    fn start_media(&mut self, object: &str, media: &dyn ExportMedia, t: u64) {
        let Some((src, kind, volume)) = find_media_target(&self.player, object) else {
            return;
        };
        self.mixer
            .play(object, &src, volume, t, media, &mut self.warnings);
        if kind == MediaKind::Video {
            self.video.play(object, &src, media, t, &mut self.warnings);
        }
    }

    fn report_media_times(&mut self, media: &dyn ExportMedia, t: u64) {
        let Some(slide) = self.player.current_slide() else {
            return;
        };
        for object in tracked_media_ids(slide) {
            if !self.player.is_media_playing(&object) {
                continue;
            }
            let position = self
                .mixer
                .position_ms(&object, t)
                .or_else(|| self.video.position_ms(&object, t));
            let Some(ms) = position else {
                continue;
            };
            let effects = self.player.handle(&Input::MediaTime {
                object,
                at_ms: ms as f64,
            });
            self.apply(&effects, media, t);
        }
    }

    fn render_and_write(
        &mut self,
        t: u64,
        width: u32,
        media: &dyn ExportMedia,
        writer: &mut Mp4Writer,
    ) -> Result<(), ExportError> {
        self.video.update(t, &mut self.warnings);
        let source = ExportMediaSource {
            media,
            video_frames: self.video.frames(),
        };
        let time = t as f64 / 1000.0;
        let image = render_player_rgba(&self.player, width, &[], &source, Some(time))
            .map_err(|error| ExportError::Render(error.to_string()))?;
        writer.write_frame(&image.pixels, t).map_err(encoder)
    }

    /// Press Next once the slide's base clock ended and the tail passed.
    ///
    /// Returns `true` when the export must stop: the last slide, a course with
    /// navigation turned off, or a jump that did not move.
    fn slide_finished(&mut self, t: u64, tail_ms: u32, media: &dyn ExportMedia) -> bool {
        let Some(slide) = self.player.current_slide() else {
            return true;
        };
        let ended =
            self.player.state().base.ended || self.player.state().base.ms >= slide.duration_ms;
        if !ended {
            self.slide_end_t = None;
            return false;
        }
        let start = self.slide_end_t.unwrap_or(t);
        self.slide_end_t = Some(start);
        if t.saturating_sub(start) < u64::from(tail_ms) {
            return false;
        }
        let before = self.player.state().slide;
        let effects = self.player.handle(&Input::Next);
        self.apply(&effects, media, t);
        self.slide_end_t = None;
        if self.player.state().slide == before {
            return true;
        }
        self.note_current_slide();
        false
    }
}

/// Media objects whose cues or trim end need the runtime's clock.
fn tracked_media_ids(slide: &Slide) -> Vec<String> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
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
        .collect()
}

/// Byte source merging export media with the frames decoded this frame.
struct ExportMediaSource<'a> {
    media: &'a dyn ExportMedia,
    video_frames: &'a HashMap<String, Arc<VideoFrame>>,
}

impl MediaSource for ExportMediaSource<'_> {
    fn get(&self, src: &str) -> MediaFetch {
        match self.media.bytes(src) {
            Some(bytes) => MediaFetch::Ready(bytes),
            None => MediaFetch::Missing,
        }
    }

    fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>> {
        self.video_frames.get(object).map(Arc::clone)
    }
}

/// One open video decoder and where its media clock currently is.
struct VideoState {
    src: String,
    _temp: tempfile::NamedTempFile,
    file: VideoFile,
    start_t: u64,
    offset_ms: u64,
    paused_ms: u64,
    playing: bool,
    last: Option<Arc<VideoFrame>>,
    warned: bool,
}

impl VideoState {
    fn current_ms(&self, t: u64) -> u64 {
        if self.playing {
            self.offset_ms
                .saturating_add(t.saturating_sub(self.start_t))
        } else {
            self.paused_ms
        }
    }
}

/// Video decoders for the objects currently playing, keyed by object id.
struct VideoPlayback {
    width: u32,
    states: HashMap<String, VideoState>,
    frames: HashMap<String, Arc<VideoFrame>>,
}

impl VideoPlayback {
    fn new(width: u32) -> Self {
        Self {
            width,
            states: HashMap::new(),
            frames: HashMap::new(),
        }
    }

    fn play(
        &mut self,
        object: &str,
        src: &str,
        media: &dyn ExportMedia,
        t: u64,
        warnings: &mut Vec<String>,
    ) {
        if let Some(state) = self.states.get_mut(object)
            && state.src == src
        {
            if state.playing {
                state.offset_ms = 0;
                state.paused_ms = 0;
            } else {
                state.offset_ms = state.paused_ms;
            }
            state.start_t = t;
            state.playing = true;
            return;
        }
        self.stop(object);
        let Some(bytes) = media.bytes(src) else {
            warnings.push(format!("video {src} is missing; showing its poster"));
            return;
        };
        if bytes.is_empty() {
            warnings.push(format!("video {src} is empty; showing its poster"));
            return;
        }
        let suffix = crate::audio_mix::temp_suffix(src);
        let Ok(mut temp) = tempfile::Builder::new().suffix(&suffix).tempfile() else {
            warnings.push(format!("video {src} could not be prepared"));
            return;
        };
        if temp.write_all(&bytes).is_err() {
            warnings.push(format!("video {src} could not be written"));
            return;
        }
        let Ok(mut file) = VideoFile::open(temp.path()) else {
            warnings.push(format!("video {src} could not be opened"));
            return;
        };
        file.set_max_width(self.width);
        self.states.insert(
            object.to_owned(),
            VideoState {
                src: src.to_owned(),
                _temp: temp,
                file,
                start_t: t,
                offset_ms: 0,
                paused_ms: 0,
                playing: true,
                last: None,
                warned: false,
            },
        );
    }

    fn pause(&mut self, object: &str, t: u64) {
        if let Some(state) = self.states.get_mut(object)
            && state.playing
        {
            state.paused_ms = state.current_ms(t);
            state.playing = false;
        }
    }

    fn stop(&mut self, object: &str) {
        self.states.remove(object);
        self.frames.remove(object);
    }

    fn seek(&mut self, object: &str, ms: u64, t: u64) {
        if let Some(state) = self.states.get_mut(object) {
            state.offset_ms = ms;
            state.paused_ms = ms;
            state.start_t = t;
        }
    }

    fn position_ms(&self, object: &str, t: u64) -> Option<u64> {
        self.states.get(object).map(|state| state.current_ms(t))
    }

    fn update(&mut self, t: u64, warnings: &mut Vec<String>) {
        for (object, state) in &mut self.states {
            if state.warned || !state.playing {
                if let Some(last) = &state.last {
                    self.frames.insert(object.clone(), Arc::clone(last));
                }
                continue;
            }
            let ms = state.current_ms(t);
            match state.file.frame_at(ms) {
                Ok(Some(frame)) => {
                    let frame = Arc::new(VideoFrame {
                        pts_ms: frame.pts_ms,
                        width: frame.width,
                        height: frame.height,
                        rgba: frame.rgba,
                    });
                    self.frames.insert(object.clone(), Arc::clone(&frame));
                    state.last = Some(frame);
                }
                Ok(None) => {}
                Err(error) => {
                    warnings.push(format!(
                        "video {object} could not be decoded at {ms} ms: {error}"
                    ));
                    state.warned = true;
                }
            }
        }
    }

    fn frames(&self) -> &HashMap<String, Arc<VideoFrame>> {
        &self.frames
    }
}
