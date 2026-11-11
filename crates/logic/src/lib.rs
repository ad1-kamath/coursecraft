//! CourseCraft player runtime (L2).
//!
//! A pure state machine: the renderer feeds it [`Input`]s (clicks, clock
//! ticks, media ends, key presses) and draws whatever [`Effect`]s come back.
//! The same code runs inside the editor preview and inside published packages,
//! so preview and LMS behaviour cannot drift apart.
//!
//! Semantics are specified in `docs/spec/triggers.md`. Problems at run time
//! (a type mismatch the validator missed, a runaway trigger loop) become
//! [`Effect::Warning`]s; the runtime never panics and the learner can keep going.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod animation;
mod draw;
mod run;
pub mod suspend;

pub use animation::{
    AnimationPhase, ClipFrac, ObjectSample, TransitionSample, ease, sample_animation,
    sample_motion_path, sample_transition,
};
pub use suspend::{SUSPEND_PREFIX, SuspendError, decode_suspend, encode_suspend};

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{
    CompletionStatus, Course, Issue, Link, MotionPath, NavMode, NextRequirement, ObjectKind,
    QuestionKind, Severity, Slide, Value,
};
use serde::{Deserialize, Serialize};

/// Events processed per input before the runtime assumes a trigger loop and
/// stops (e.g. two `variable_changes` triggers that keep setting each other).
pub const MAX_EVENTS_PER_INPUT: usize = 1024;

/// What the renderer reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "input", rename_all = "snake_case")]
pub enum Input {
    /// Open the course on its start slide.
    Start,
    /// Wall-clock time passed.
    Tick {
        ms: u32,
    },
    /// The learner opened a resource from the player's Resources tab.
    OpenResource {
        resource: String,
    },
    /// The learner picked a playback speed. Snapped to an allowed rate.
    SetSpeed {
        rate: f32,
    },
    Click {
        object: String,
    },
    /// The learner clicked a text link inside `object`. Runs the link without
    /// firing the object's own click triggers.
    LinkClicked {
        object: String,
        link: Link,
    },
    HoverStart {
        object: String,
    },
    HoverEnd {
        object: String,
    },
    MediaEnded {
        object: String,
    },
    /// A video's playback position, in milliseconds from the start of the
    /// clip. Hosts report it each frame while the video is playing.
    MediaTime {
        object: String,
        at_ms: f64,
    },
    Key {
        key: String,
    },
    KeyPressed {
        key: String,
    },
    /// The focused player control changed. `object: None` means no object has
    /// focus (the canvas or player chrome has it). Used so `key_pressed`
    /// triggers with an object fire only while that object has focus.
    FocusChanged {
        object: Option<String>,
    },
    /// Learner picks (or, for multiple response, toggles) a choice.
    SelectChoice {
        choice: String,
    },
    /// The player's Submit button.
    Submit,
    /// The player's Next / Previous buttons (only when `player.show_nav`).
    Next,
    Previous,
    /// The player's slide menu (only when `player.show_menu`).
    GoTo {
        slide: String,
    },
    /// Replace the fill-in answer. Capped at 2000 characters on a char boundary.
    TextEntered {
        text: String,
    },
    /// Move `choice` to index `to` in the current sequence (clamped).
    SequenceMoved {
        choice: String,
        to: usize,
    },
    /// Set or clear (`right: None`) the match for a left pair id.
    MatchSelected {
        left: String,
        right: Option<String>,
    },
    /// Place or move a hotspot marker. `x` and `y` are slide coordinates.
    HotspotClicked {
        x: f32,
        y: f32,
    },
    /// A `CourseCraft.setVar` call from published-course JavaScript. The value
    /// must match the variable's kind; otherwise it is ignored with a warning.
    SetVariable {
        variable: String,
        value: Value,
    },
    /// A committed value from an input object (text entry, slider, dial,
    /// checkbox or radio group). The runtime checks the object, its kind and
    /// its disabled state, then clamps and assigns through the normal
    /// variable path so `variable_changes` fires.
    SetInputValue {
        object: String,
        value: Value,
    },
    /// Open or close a marker's label. Opening one closes the other marker on
    /// the slide. The runtime re-checks the object and its live state.
    MarkerToggled {
        object: String,
    },
    /// Close whichever marker label is open, if any.
    MarkersClosed,
    /// Close the open lightbox, if there is one. The close button and Escape
    /// both send this.
    CloseLightbox,
    /// Scroll a scrolling panel. `offset` is in slide pixels and is clamped to
    /// the panel's content.
    PanelScrolled {
        object: String,
        offset: f32,
    },
    /// A `CourseCraft.jump` call from published-course JavaScript.
    Jump {
        slide: String,
    },
    /// The learner released `item`. `x` and `y` are its new top-left in slide coordinates.
    /// `target` is the drop target under the item's centre, or `None` when there is none.
    DragDropped {
        item: String,
        target: Option<String>,
        x: f32,
        y: f32,
    },
    /// A draggable object was dropped. `x` and `y` are its new top-left in slide coordinates.
    ObjectDropped {
        object: String,
        x: f32,
        y: f32,
    },
    /// The learner's pause control (see Learner pause, resume and seek).
    PlayerPause,
    /// The learner's play control. Resumes only media the player paused.
    PlayerResume,
    /// Move the current slide's base clock. Clamped to `0..=duration`.
    PlayerSeek {
        ms: u64,
    },
    /// The review bar's Exit review button. Runs the same code as
    /// `Action::ExitReview`.
    ExitReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaOp {
    Play,
    Pause,
    Stop,
}

/// Runtime playback state of a media object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaPlaybackState {
    Playing,
    Paused,
    PausedByTimeline,
    PausedByPlayer,
    /// A pausing cue stopped the video; it restarts from the cue.
    PausedAtCue,
}

/// What the renderer (and the LMS bridge) must do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    SlideEntered {
        slide: String,
    },
    LayerShown {
        layer: String,
    },
    LayerHidden {
        layer: String,
    },
    StateChanged {
        object: String,
        state: String,
    },
    VariableChanged {
        variable: String,
        value: Value,
    },
    Media {
        object: String,
        op: MediaOp,
    },
    /// A video crossed a cue point going forward.
    CueReached {
        object: String,
        cue: String,
    },
    /// Move one media object's playback position to `ms`. Emitted after any `Play`.
    MediaSeek {
        object: String,
        ms: u64,
    },
    TimelinePaused {
        layer: Option<String>,
    },
    TimelineResumed {
        layer: Option<String>,
    },
    ChoiceSelected {
        choice: String,
        selected: bool,
    },
    /// Fill-in text replaced. Hosts mirror `text`.
    TextEntryChanged {
        text: String,
    },
    /// Sequence order changed. `order` is the choice ids, top to bottom.
    SequenceChanged {
        order: Vec<String>,
    },
    /// One matching row changed. `right` is `None` when the learner cleared it.
    MatchChanged {
        left: String,
        right: Option<String>,
    },
    /// Hotspot markers replaced. Coordinates are slide coordinates, oldest first.
    HotspotMarked {
        markers: Vec<(f32, f32)>,
    },
    /// A drag item moved. `target` is `None` and `offset` is `(0, 0)` when it returned home.
    ItemDropped {
        item: String,
        target: Option<String>,
        offset: (f32, f32),
    },
    /// A draggable object moved.
    ObjectMoved {
        object: String,
        offset: (f32, f32),
    },
    /// A draggable object hit a target.
    ObjectDropped {
        object: String,
        target: String,
    },
    /// Submit pressed with nothing selected: show the "please answer" prompt.
    AnswerRequired,
    /// A marker's label opened.
    MarkerOpened {
        object: String,
    },
    /// A marker's label closed.
    MarkerClosed {
        object: String,
    },
    /// A timer started counting down.
    TimerStarted {
        timer: String,
    },
    /// A timer's countdown was paused.
    TimerPaused {
        timer: String,
    },
    /// A timer's countdown reached zero.
    TimerEnded {
        timer: String,
    },
    /// A timer returned to its full duration, stopped.
    TimerReset {
        timer: String,
    },
    /// A slide opened in a lightbox over the current slide.
    LightboxOpened {
        slide: String,
    },
    /// The lightbox closed and the base slide resumed.
    LightboxClosed {
        slide: String,
    },
    QuestionAnswered {
        slide: String,
        correct: bool,
        points: u32,
        attempts_left: Option<u32>,
    },
    Score {
        raw: u32,
        max: u32,
        percent: u8,
        status: CompletionStatus,
    },
    Completion {
        status: CompletionStatus,
    },
    OpenUrl {
        url: String,
    },
    /// Run `code` in the published course's web player. Other hosts log that
    /// JavaScript only runs in the published course and do nothing.
    #[serde(rename = "run_javascript")]
    RunJavaScript {
        code: String,
    },
    Exit,
    /// Answer review started (`true`) or ended (`false`).
    ReviewChanged {
        reviewing: bool,
    },
    /// The host should print the current slide without `exclude`.
    Print {
        slide: String,
        exclude: Vec<String>,
    },
    Warning {
        message: String,
    },
}

/// Per-timeline clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Clock {
    pub ms: u32,
    pub ended: bool,
    pub paused: bool,
}

/// A slide's clock and layers, saved when the learner leaves it so a later
/// visit with `revisit: resume` can continue where they left off.
///
/// Runtime-only: never encoded in `suspend_data`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SavedSlide {
    pub base: Clock,
    pub visible_layers: Vec<String>,
    pub layer_clocks: BTreeMap<String, Clock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub correct: bool,
    pub attempts: u32,
    pub points: u32,
}

/// A copy of the answer state as it was when a question was graded, kept so
/// answer review can restore it. Runtime-only; never encoded in suspend data.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnswerSnapshot {
    pub selected: BTreeSet<String>,
    pub text_entry: String,
    pub sequence: Vec<String>,
    pub matches: BTreeMap<String, String>,
    pub hotspot_markers: Vec<(f32, f32)>,
    pub drops: BTreeMap<String, String>,
}

/// What the learner submitted for the last graded question.
///
/// Captured before any shuffle or reset so LMS reporting sees the answer the
/// learner actually gave. Runtime-only; never encoded in suspend data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnerResponse {
    /// Selected choice ids, in the course's authored choice order.
    Choices(Vec<String>),
    /// Trimmed fill-in text.
    Text(String),
    /// Choice ids in the order the learner submitted.
    Sequence(Vec<String>),
    /// Left pair id and chosen right pair id, in the course's pair order.
    Pairs(Vec<(String, String)>),
    /// Hotspot region ids containing a marker, in the course's region order.
    Regions(Vec<String>),
}

/// An active motion path being traversed by an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningPath {
    pub path: MotionPath,
    pub elapsed_ms: u32,
    pub from: (f32, f32),
}

/// Runtime state of one timer object. Runtime-only; never encoded in
/// `suspend_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TimerState {
    /// Milliseconds left before zero.
    pub remaining_ms: u32,
    /// Whether the countdown is currently running.
    pub running: bool,
    /// Whether the countdown reached zero since the last reset.
    pub ended: bool,
    /// Whether autostart may still begin the countdown on this slide visit.
    pub autostart_armed: bool,
}

/// A slide shown in a lightbox over the base slide.
///
/// The base slide's per-slide runtime is saved here while the lightbox is open,
/// so closing can put it back exactly where it was. Runtime-only; never encoded
/// in `suspend_data`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Lightbox {
    /// Play-order index of the slide inside the box.
    pub slide: usize,
    /// Play-order index of the slide underneath.
    pub base_slide: usize,
    /// Base slide clock and layers, saved while the lightbox is open.
    pub base: Clock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_visible_layers: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_layer_clocks: BTreeMap<String, Clock>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_media_playback: BTreeMap<String, MediaPlaybackState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_media_clocks: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_running_paths: BTreeMap<String, RunningPath>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_object_offsets: BTreeMap<String, (f32, f32)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub base_selected: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_text_entry: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_sequence: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_matches: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_hotspot_markers: Vec<(f32, f32)>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_drops: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_marker_open: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_panel_offsets: BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_object_positions: BTreeMap<String, (f32, f32)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub base_intersecting_pairs: BTreeSet<(String, String)>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_timers: BTreeMap<String, TimerState>,
}

/// Runtime cue progress for one video object: which cues have fired and the
/// last playback position reported by the host.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CueProgress {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub fired: BTreeSet<String>,
    #[serde(default)]
    pub last_ms: f64,
}

/// Learner name and date supplied by the host for certificates.
///
/// Runtime-only: never written to `suspend_data`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HostInfo {
    /// The learner's name, when the host knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learner_name: Option<String>,
    /// The host's `YYYY-MM-DD` date string, when it set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// Most characters kept from a host learner name.
pub const MAX_HOST_NAME_CHARS: usize = 200;
/// Most characters kept from a host date string.
pub const MAX_HOST_DATE_CHARS: usize = 32;

impl HostInfo {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.learner_name.is_none() && self.date.is_none()
    }
}

/// Runtime playback state of the course's background audio.
///
/// Runtime-only: never encoded in `suspend_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundState {
    /// Whether the track is currently playing (or starting).
    pub playing: bool,
    /// The author last paused it with `pause_media`.
    pub paused_by_author: bool,
    /// The track was started but no tick has confirmed the host started it.
    pub started_pending: bool,
}

impl BackgroundState {
    /// Whether nothing is playing or paused: omitted from serialised state.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        !self.playing && !self.paused_by_author && !self.started_pending
    }
}

/// Everything that changes while a learner plays. Serialisable so it can be
/// stored as SCORM `suspend_data` and resumed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerState {
    pub started: bool,
    pub exited: bool,
    /// Play-order index of the current slide.
    pub slide: usize,
    /// Random draw seed chosen when the learner starts. Saved with progress so
    /// resuming shows the same drawn questions.
    #[serde(default)]
    pub draw_seed: u32,
    /// How many times `submit_results` has run this session. Saved with
    /// progress when the course has a quiz or question banks.
    #[serde(default)]
    pub quiz_attempts: u32,
    /// Runtime-only: answer snapshots taken at grading, for answer review.
    /// Never in suspend data.
    #[serde(default, skip_serializing)]
    pub review_answers: BTreeMap<String, AnswerSnapshot>,
    /// Runtime-only: slide id review started from, while reviewing. Never in
    /// suspend data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewing_from: Option<String>,
    /// Bumped on every slide entry; queued events from an older slide are dropped.
    pub epoch: u64,
    pub base: Clock,
    /// Visible layers, bottom to top.
    pub visible_layers: Vec<String>,
    pub layer_clocks: BTreeMap<String, Clock>,
    /// Runtime-only: clocks and visible layers saved when leaving a slide.
    /// Used by `revisit: resume`; never encoded in `suspend_data`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub saved_slides: BTreeMap<String, SavedSlide>,
    /// slide id -> object id -> state. Kept across visits ("resume saved state").
    pub object_states: BTreeMap<String, BTreeMap<String, String>>,
    pub visited: BTreeSet<String>,
    /// Runtime-only: playback speed in quarters (`2..=8`); `0` means 1x. Never in
    /// suspend data; it stays across slide changes.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub speed_quarters: u8,
    /// Runtime-only: leftover quarter-milliseconds (`0..4`) carried between ticks.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub speed_carry: u8,
    /// Runtime-only: slides whose base timeline has reached its end on some
    /// visit. Never in suspend data; [`Player::resume`] seeds it from `visited`.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub timeline_completed: BTreeSet<String>,
    pub variables: BTreeMap<String, Value>,
    pub selected: BTreeSet<String>,
    /// slide id -> graded answer.
    pub answers: BTreeMap<String, Answer>,
    /// Runtime-only: tracking playing and paused media objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub media_playback: BTreeMap<String, MediaPlaybackState>,
    /// Runtime-only: elapsed playback position of each Lottie object, in
    /// milliseconds. Audio and video positions live in their backend.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub media_clocks: BTreeMap<String, u64>,
    /// Runtime-only: cue progress for each video object. Never in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cue_progress: BTreeMap<String, CueProgress>,
    /// Runtime-only: the cue each held video is paused at. Never in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cue_held: BTreeMap<String, String>,
    /// Runtime-only: layers that were open when a cue paused its video, so the
    /// layer keeps control of playback until it closes. Never in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cue_layers: BTreeMap<String, Vec<String>>,
    /// Runtime-only: the learner's pause flag. Never in suspend data.
    #[serde(default, skip_serializing_if = "is_false")]
    pub player_paused: bool,
    /// Runtime-only: active motion paths.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub running_paths: BTreeMap<String, RunningPath>,
    /// Runtime-only: resting offsets of moved objects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_offsets: BTreeMap<String, (f32, f32)>,
    /// Runtime-only: freeform draggable object positions (offsets). Reset on slide entry. Not in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_positions: BTreeMap<String, (f32, f32)>,
    /// Runtime-only: pairs of objects currently intersecting `(a, b)` with `a < b`. Reset on slide entry. Not in suspend data.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub intersecting_pairs: BTreeSet<(String, String)>,
    /// Runtime-only: fill-in text. Cleared on slide entry. Not in suspend data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text_entry: String,
    /// Runtime-only: the object that currently has keyboard focus, if any.
    /// Reset on slide entry and never written to suspend data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_object: Option<String>,
    /// Runtime-only: current sequence order (choice ids). Not in suspend data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sequence: Vec<String>,
    /// Runtime-only: matching left pair id → chosen right pair id. Not in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub matches: BTreeMap<String, String>,
    /// Runtime-only: hotspot markers in slide coordinates. Not in suspend data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotspot_markers: Vec<(f32, f32)>,
    /// Runtime-only: drag item id → drop target id. Positions live in `object_offsets`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub drops: BTreeMap<String, String>,
    /// Runtime-only: the last submitted answer, kept for LMS question reporting.
    /// Not in suspend data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_response: Option<LearnerResponse>,
    /// Runtime-only: the marker whose label is open on this slide, if any.
    /// Reset on slide entry and never in suspend data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker_open: Option<String>,
    /// Runtime-only: scrolling panel id → scroll offset in slide pixels.
    /// Reset on slide entry and never in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panel_offsets: BTreeMap<String, f32>,
    /// Runtime-only: timer id → countdown state for the current slide.
    /// Reset on slide entry and never in suspend data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub timers: BTreeMap<String, TimerState>,
    /// Runtime-only: the open lightbox, with the base slide's saved state.
    /// Never in suspend data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lightbox: Option<Lightbox>,
    /// Runtime-only: learner name and date set by the host. Never in suspend
    /// data.
    #[serde(default, skip_serializing_if = "HostInfo::is_empty")]
    pub host: HostInfo,
    /// Runtime-only: the course-wide background track. Never in suspend data.
    #[serde(default, skip_serializing_if = "BackgroundState::is_idle")]
    pub background: BackgroundState,
}

/// Read-only view of the current slide's player-clock state for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeekInfo {
    /// Current slide base clock in milliseconds.
    pub elapsed_ms: u32,
    /// Current slide duration in milliseconds.
    pub duration_ms: u32,
    /// Whether the learner's pause flag is set.
    pub paused: bool,
    /// Whether a seek is allowed right now.
    pub seek_allowed: bool,
}

/// What `player_seek` and `seek_info` act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SeekTarget {
    Base,
    Layer(String),
    Refused,
}

/// A running course.
pub struct Player<'c> {
    course: &'c Course,
    slides: Vec<&'c Slide>,
    st: PlayerState,
    /// Whether `st` currently belongs to an entered slide. Not serialised; used
    /// to avoid saving a slide state that was never played.
    entered: bool,
}

impl<'c> Player<'c> {
    /// Create a player for a validated course.
    ///
    /// # Errors
    /// Returns the validation errors if the course is not playable.
    pub fn new(course: &'c Course) -> Result<Self, Vec<Issue>> {
        Self::with_draw_seed(course, 0)
    }

    /// Create a player for a validated course with an explicit draw seed.
    ///
    /// # Errors
    /// Returns the validation errors if the course is not playable.
    pub fn with_draw_seed(course: &'c Course, seed: u32) -> Result<Self, Vec<Issue>> {
        let errors: Vec<Issue> = course
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let slides = draw::play_order(course, seed);
        let st = PlayerState {
            slide: draw::expanded_start(course, &slides),
            draw_seed: seed,
            variables: course
                .variables
                .iter()
                .map(|v| (v.name.clone(), v.value.clone()))
                .collect(),
            ..PlayerState::default()
        };
        Ok(Self {
            course,
            slides,
            st,
            entered: false,
        })
    }

    /// Test helper to create a player without running course validation.
    #[cfg(test)]
    #[must_use]
    pub fn new_unvalidated(course: &'c Course) -> Self {
        let slides = draw::play_order(course, 0);
        let st = PlayerState {
            slide: draw::expanded_start(course, &slides),
            variables: course
                .variables
                .iter()
                .map(|v| (v.name.clone(), v.value.clone()))
                .collect(),
            ..PlayerState::default()
        };
        Self {
            course,
            slides,
            st,
            entered: false,
        }
    }

    /// Restore a player from saved state (SCORM resume).
    ///
    /// # Errors
    /// Validation errors, or a state that does not fit this course.
    pub fn resume(course: &'c Course, state: PlayerState) -> Result<Self, Vec<Issue>> {
        let seed = state.draw_seed;
        let mut p = Self::with_draw_seed(course, seed)?;
        if state.slide >= p.slides.len() {
            return Err(vec![Issue {
                severity: Severity::Error,
                path: "/slide".into(),
                message: "saved slide index is outside this course".into(),
            }]);
        }
        // Hosts rebuild a `Player` from its state every frame, so the learner's pause must
        // survive this. Suspend data never carries it, so a saved session still starts playing.
        p.st = state;
        // Saved states can list visible layers in a different order. Put them back
        // into `slide.layers` order (back to front), dropping nothing.
        let authored: Vec<String> = p
            .current_slide()
            .map(|slide| slide.layers.iter().map(|layer| layer.id.clone()).collect())
            .unwrap_or_default();
        p.st.visible_layers.sort_by_key(|id| {
            authored
                .iter()
                .position(|layer| layer == id)
                .unwrap_or(usize::MAX)
        });
        // Suspend data does not record which timelines finished. A slide the learner has
        // already left counts as completed, so it stays unlocked on a return visit.
        if p.st.timeline_completed.is_empty() {
            let current = p.slides.get(p.st.slide).map(|slide| slide.id.clone());
            let earlier: Vec<String> =
                p.st.visited
                    .iter()
                    .filter(|id| Some(*id) != current.as_ref())
                    .cloned()
                    .collect();
            p.st.timeline_completed.extend(earlier);
        }
        Ok(p)
    }

    #[must_use]
    pub fn state(&self) -> &PlayerState {
        &self.st
    }

    /// Replace the learner name and date supplied by the host.
    ///
    /// Names are capped at [`MAX_HOST_NAME_CHARS`] characters and dates at
    /// [`MAX_HOST_DATE_CHARS`], cut on a character boundary.
    pub fn set_host_info(&mut self, info: HostInfo) {
        self.st.host = HostInfo {
            learner_name: info
                .learner_name
                .map(|name| name.chars().take(MAX_HOST_NAME_CHARS).collect()),
            date: info
                .date
                .map(|date| date.chars().take(MAX_HOST_DATE_CHARS).collect()),
        };
    }

    /// Current fill-in text. Empty when the slide is not a fill-in question.
    #[must_use]
    pub fn text_entry(&self) -> &str {
        &self.st.text_entry
    }

    /// The marker whose label is open on the current slide, if any.
    #[must_use]
    pub fn marker_open(&self) -> Option<&str> {
        self.st.marker_open.as_deref()
    }

    /// The current scroll offset of a scrolling panel, in slide pixels.
    #[must_use]
    pub fn panel_offset(&self, object: &str) -> f32 {
        self.st.panel_offsets.get(object).copied().unwrap_or(0.0)
    }

    /// Whether a lightbox is open. While it is, [`Player::current_slide`] is
    /// the slide inside the box.
    #[must_use]
    pub fn lightbox_open(&self) -> bool {
        self.st.lightbox.is_some()
    }

    /// The slide inside the open lightbox, if any.
    #[must_use]
    pub fn lightbox_slide(&self) -> Option<&'c Slide> {
        let lb = self.st.lightbox.as_ref()?;
        self.slides.get(lb.slide).copied()
    }

    /// The id of the slide inside the open lightbox, if any.
    #[must_use]
    pub fn lightbox_slide_id(&self) -> Option<&str> {
        self.lightbox_slide().map(|slide| slide.id.as_str())
    }

    /// The slide underneath the open lightbox, if any.
    #[must_use]
    pub fn lightbox_base_slide(&self) -> Option<&'c Slide> {
        let lb = self.st.lightbox.as_ref()?;
        self.slides.get(lb.base_slide).copied()
    }

    /// Whether an object in a scrolling panel at least partly intersects its
    /// panel's viewport. Objects outside every panel are always visible.
    #[must_use]
    pub fn object_in_panel_viewport(&self, object: &str) -> bool {
        let Some(slide) = self.current_slide() else {
            return false;
        };
        let Some((obj, _)) = slide.object(object) else {
            return false;
        };
        let Some(panel_id) = obj.panel.as_deref() else {
            return true;
        };
        let Some((panel, _)) = slide.object(panel_id) else {
            return true;
        };
        let offset = self.panel_offset(panel_id);
        let top = obj.frame.y - offset;
        let bottom = top + obj.frame.h;
        let panel_bottom = panel.frame.y + panel.frame.h;
        bottom > panel.frame.y && top < panel_bottom
    }

    /// Current sequence order (choice ids). Empty until a sequence slide is entered.
    #[must_use]
    pub fn sequence_order(&self) -> &[String] {
        &self.st.sequence
    }

    /// Shuffled right-hand pair ids for the current matching question.
    ///
    /// Computed from the slide id and the attempt count. Empty for any other kind.
    #[must_use]
    pub fn matching_options(&self) -> Vec<String> {
        let Some(slide) = self.current_slide() else {
            return Vec::new();
        };
        let Some(question) = &slide.question else {
            return Vec::new();
        };
        if question.kind != QuestionKind::Matching {
            return Vec::new();
        }
        let ids: Vec<String> = question.pairs.iter().map(|p| p.id.clone()).collect();
        let attempt = self
            .st
            .answers
            .get(&slide.id)
            .map(|a| a.attempts)
            .unwrap_or(0);
        run::shuffled_ids(&ids, run::seed_for(&slide.id, attempt))
    }

    /// Chosen right pair id for a matching left, if the learner has picked one.
    #[must_use]
    pub fn match_for(&self, left: &str) -> Option<&str> {
        self.st.matches.get(left).map(String::as_str)
    }

    #[must_use]
    pub fn course(&self) -> &Course {
        self.course
    }

    /// The slides in play order, with every draw slide expanded.
    #[must_use]
    pub fn play_order(&self) -> &[&'c Slide] {
        &self.slides
    }

    /// The position of `id` in the expanded play order. A draw slide resolves
    /// to the position of its first drawn question. A bank slide that was not
    /// drawn, or any unknown id, has no position.
    #[must_use]
    pub fn slide_position(&self, id: &str) -> Option<usize> {
        draw::position_in_expanded(self.course, &self.slides, id)
    }

    /// Current slide. Valid after construction even before [`Input::Start`].
    #[must_use]
    pub fn current_slide(&self) -> Option<&'c Slide> {
        self.slides.get(self.st.slide).copied()
    }

    /// Whether answer review is currently showing.
    #[must_use]
    pub fn is_reviewing(&self) -> bool {
        self.st.reviewing_from.is_some()
    }

    /// The player settings in effect on the current slide: the course settings
    /// with the slide's `show_nav`, `show_seekbar` and `show_menu` overrides
    /// applied. Every host and the runtime read this, never `course.player`
    /// directly, for those three flags.
    #[must_use]
    pub fn effective_player(&self) -> coursecraft_course::PlayerSettings {
        let mut settings = self.course.player.clone();
        if let Some(overrides) = self.current_slide().and_then(|slide| slide.player.as_ref()) {
            if let Some(show) = overrides.show_nav {
                settings.show_nav = show;
            }
            if let Some(show) = overrides.show_seekbar {
                settings.show_seekbar = show;
            }
            if let Some(show) = overrides.show_menu {
                settings.show_menu = show;
            }
        }
        settings
    }

    /// Set the playback speed. Snaps to the nearest of 0.5, 0.75, 1, 1.25, 1.5
    /// and 2; NaN and infinity become 1. Ignored while `show_speed` is off.
    pub fn set_speed(&mut self, rate: f32) {
        if !self.course.player.show_speed {
            return;
        }
        let quarters: u8 = if rate.is_finite() {
            let mut best = 4u8;
            let mut best_gap = f32::MAX;
            for allowed in PLAYBACK_RATES {
                let gap = (allowed - rate).abs();
                if gap < best_gap {
                    best_gap = gap;
                    best = (allowed * 4.0).round() as u8;
                }
            }
            best
        } else {
            4
        };
        self.st.speed_quarters = if quarters == 4 { 0 } else { quarters };
        self.st.speed_carry = 0;
    }

    /// The playback speed in effect (always 1 when `show_speed` is off).
    #[must_use]
    pub fn speed(&self) -> f32 {
        if !self.course.player.show_speed || self.st.speed_quarters == 0 {
            1.0
        } else {
            f32::from(self.st.speed_quarters) / 4.0
        }
    }

    /// Wall-clock milliseconds scaled by the speed, carrying the remainder.
    pub(crate) fn scale_tick(&mut self, ms: u32) -> u32 {
        let quarters = if self.course.player.show_speed && self.st.speed_quarters != 0 {
            u64::from(self.st.speed_quarters)
        } else {
            4
        };
        let total = u64::from(ms)
            .saturating_mul(quarters)
            .saturating_add(u64::from(self.st.speed_carry));
        self.st.speed_carry = (total % 4) as u8;
        u32::try_from(total / 4).unwrap_or(u32::MAX)
    }

    /// Record that the current slide's base timeline reached its end, if it did.
    pub(crate) fn note_timeline_completed(&mut self) {
        if let Some(slide) = self.slides.get(self.st.slide).copied()
            && self.st.started
            && self.st.base.ms >= slide.duration_ms
        {
            self.st.timeline_completed.insert(slide.id.clone());
        }
    }

    /// The rule that decides when Next unlocks on the current slide.
    fn next_rule(&self) -> NextRequirement {
        self.current_slide()
            .and_then(|slide| slide.player.as_ref())
            .and_then(|overrides| overrides.next_requires.clone())
            .unwrap_or_else(|| self.course.player.next_requires.clone())
    }

    /// Whether the Next button works right now. False while the slide's "Next
    /// unlocks" rule is unmet. Author navigation (jump actions) is never gated.
    #[must_use]
    pub fn can_next(&self) -> bool {
        let Some(slide) = self.current_slide() else {
            return true;
        };
        if !self.st.started || self.st.reviewing_from.is_some() {
            return true;
        }
        match self.next_rule() {
            NextRequirement::Nothing => true,
            NextRequirement::TimelineEnds => {
                self.st.timeline_completed.contains(&slide.id)
                    || self.st.base.ms >= slide.duration_ms
            }
            NextRequirement::Variable { name } => self.value(&name) == Some(Value::Bool(true)),
        }
    }

    /// Whether the learner can open `slide` from the slide menu under the course's
    /// navigation mode. Author navigation (jump actions) is never gated.
    #[must_use]
    pub fn can_go_to(&self, slide: &str) -> bool {
        if self.st.reviewing_from.is_some() {
            return true;
        }
        match self.course.player.navigation {
            NavMode::Free => true,
            NavMode::Restricted => {
                self.current_slide().is_some_and(|cur| cur.id == slide)
                    || self.st.visited.contains(slide)
            }
            NavMode::Locked => false,
        }
    }

    /// Whether the current slide's question is answered or out of attempts.
    #[must_use]
    pub fn question_locked(&self) -> bool {
        self.current_slide()
            .is_some_and(|slide| self.question_locked_on(slide))
    }

    pub(crate) fn question_locked_on(&self, slide: &Slide) -> bool {
        if self.st.reviewing_from.is_some() && slide.question.is_some() {
            return true;
        }
        let (Some(q), Some(a)) = (&slide.question, self.st.answers.get(&slide.id)) else {
            return false;
        };
        a.correct || (q.attempts != 0 && a.attempts >= q.attempts)
    }

    #[must_use]
    pub fn object_state(&self, object: &str) -> Option<&str> {
        let slide = self.current_slide()?;
        self.st
            .object_states
            .get(&slide.id)?
            .get(object)
            .map(String::as_str)
    }

    #[must_use]
    pub fn variable(&self, name: &str) -> Option<&Value> {
        self.st.variables.get(name)
    }

    pub(crate) fn score_points_and_max(&self) -> (u64, u64) {
        let mut raw: u64 = 0;
        let mut max: u64 = 0;
        for s in &self.slides {
            if let Some(q) = &s.question {
                max += u64::from(q.max_points());
                raw += self
                    .st
                    .answers
                    .get(&s.id)
                    .map_or(0, |a| u64::from(a.points));
            }
        }
        (raw, max)
    }

    /// Read a variable value, resolving built-in variables or user variables.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<Value> {
        match name {
            "Course.SlideNumber" => {
                let num = if self.st.started {
                    (self.st.slide.saturating_add(1)) as f64
                } else {
                    0.0
                };
                Some(Value::Number(num))
            }
            "Course.TotalSlides" => Some(Value::Number(self.slides.len() as f64)),
            "Course.Progress" => {
                let total = self.slides.len() as u64;
                let progress = if total == 0 {
                    0
                } else {
                    let visited = self
                        .slides
                        .iter()
                        .filter(|s| self.st.visited.contains(&s.id))
                        .count() as u64;
                    visited
                        .saturating_mul(100)
                        .checked_div(total)
                        .unwrap_or(0)
                        .min(100)
                };
                Some(Value::Number(progress as f64))
            }
            "Course.ScorePoints" => {
                let (raw, _) = self.score_points_and_max();
                Some(Value::Number(raw as f64))
            }
            "Course.ScorePercent" => {
                let (raw, max) = self.score_points_and_max();
                let percent = if max == 0 {
                    100
                } else {
                    raw.saturating_mul(100)
                        .checked_div(max)
                        .unwrap_or(100)
                        .min(100)
                };
                Some(Value::Number(percent as f64))
            }
            "Slide.ElapsedMs" => Some(Value::Number(self.st.base.ms as f64)),
            "Quiz.Passed" => {
                let passed = match self.course.quiz {
                    Some(qz) => {
                        let (raw, max) = self.score_points_and_max();
                        let percent = if max == 0 {
                            100
                        } else {
                            raw.saturating_mul(100)
                                .checked_div(max)
                                .unwrap_or(100)
                                .min(100)
                        };
                        percent >= u64::from(qz.passing_percent)
                    }
                    None => false,
                };
                Some(Value::Bool(passed))
            }
            "Quiz.Attempts" => Some(Value::Number(self.st.quiz_attempts as f64)),
            "Quiz.Reviewing" => Some(Value::Bool(self.st.reviewing_from.is_some())),
            "Learner.Name" => Some(Value::Text(
                self.st.host.learner_name.clone().unwrap_or_default(),
            )),
            "Course.Title" => Some(Value::Text(self.course.title.clone())),
            "Course.Date" => Some(Value::Text(self.st.host.date.clone().unwrap_or_default())),
            _ => {
                if let Some(id) = coursecraft_course::timer_id_for_remaining_variable(name) {
                    return self
                        .timer_remaining_seconds(id)
                        .map(|s| Value::Number(s as f64));
                }
                self.st.variables.get(name).cloned()
            }
        }
    }

    /// Whole seconds remaining on timer `object`, rounded up. `None` when the
    /// object is not a timer on the current slide.
    #[must_use]
    pub fn timer_remaining_seconds(&self, object: &str) -> Option<u64> {
        let slide = self.current_slide()?;
        let (obj, _) = slide.object(object)?;
        let ObjectKind::Timer { duration_ms, .. } = obj.kind else {
            return None;
        };
        let remaining = self
            .st
            .timers
            .get(object)
            .map_or(duration_ms, |t| t.remaining_ms);
        Some(coursecraft_course::timer_remaining_seconds(remaining))
    }

    /// Milliseconds remaining on timer `object`. `None` when the object is not
    /// a timer on the current slide; before the slide is entered the full
    /// duration is reported.
    #[must_use]
    pub fn timer_remaining_ms(&self, object: &str) -> Option<u32> {
        let slide = self.current_slide()?;
        let (obj, _) = slide.object(object)?;
        let ObjectKind::Timer { duration_ms, .. } = obj.kind else {
            return None;
        };
        Some(
            self.st
                .timers
                .get(object)
                .map_or(duration_ms, |t| t.remaining_ms),
        )
    }

    /// Whether timer `object` is currently counting down.
    #[must_use]
    pub fn timer_running(&self, object: &str) -> bool {
        self.st.timers.get(object).is_some_and(|t| t.running)
    }

    /// Whether timer `object` reached zero since its last reset.
    #[must_use]
    pub fn timer_ended(&self, object: &str) -> bool {
        self.st.timers.get(object).is_some_and(|t| t.ended)
    }

    /// Replaces `%Name%` with the current value of the referenced user or built-in variable.
    #[must_use]
    pub fn display_text(&self, text: &str) -> String {
        const MAX_LEN: usize = 65536;
        let mut out = String::with_capacity(text.len().min(MAX_LEN));
        let mut i = 0;
        let bytes = text.as_bytes();

        while i < bytes.len() {
            if out.len() >= MAX_LEN {
                break;
            }
            if bytes.get(i) == Some(&b'%') {
                if bytes.get(i + 1) == Some(&b'%') {
                    if !push_str_capped(&mut out, "%", MAX_LEN) {
                        break;
                    }
                    i += 2;
                    continue;
                }

                let mut found_end = None;
                let mut j = i + 1;
                while j < bytes.len() {
                    if bytes.get(j) == Some(&b'%') {
                        found_end = Some(j);
                        break;
                    }
                    j += 1;
                }

                if let Some(end) = found_end {
                    let name_bytes = bytes.get(i + 1..end).unwrap_or(&[]);
                    let name_len = name_bytes.len();
                    if (1..=128).contains(&name_len) {
                        let maybe_val = std::str::from_utf8(name_bytes)
                            .ok()
                            .and_then(|name| self.value(name));
                        if let Some(val) = maybe_val {
                            let formatted = format_display_value(&val);
                            if !push_str_capped(&mut out, &formatted, MAX_LEN) {
                                break;
                            }
                            i = end + 1;
                            continue;
                        }
                    }
                }

                if !push_str_capped(&mut out, "%", MAX_LEN) {
                    break;
                }
                i += 1;
            } else {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes.get(i) != Some(&b'%') {
                    i += 1;
                }
                if text
                    .get(start..i)
                    .is_some_and(|slice| !push_str_capped(&mut out, slice, MAX_LEN))
                {
                    break;
                }
            }
        }

        out
    }

    #[must_use]
    pub fn media_state(&self, object: &str) -> Option<MediaPlaybackState> {
        self.st.media_playback.get(object).copied()
    }

    /// Elapsed playback position of a Lottie object, in milliseconds.
    ///
    /// `None` before it has ever played or after a stop. A paused animation keeps
    /// its position, and a finished one keeps the position where it stopped so
    /// the renderer can hold its last frame.
    #[must_use]
    pub fn media_clock_ms(&self, object: &str) -> Option<u64> {
        self.st.media_clocks.get(object).copied()
    }

    #[must_use]
    pub fn is_media_playing(&self, object: &str) -> bool {
        if object == coursecraft_course::BACKGROUND_AUDIO_ID {
            return self.st.background.playing;
        }
        matches!(
            self.st.media_playback.get(object),
            Some(MediaPlaybackState::Playing)
        )
    }

    #[must_use]
    pub fn is_media_paused(&self, object: &str) -> bool {
        matches!(
            self.st.media_playback.get(object),
            Some(
                MediaPlaybackState::Paused
                    | MediaPlaybackState::PausedByTimeline
                    | MediaPlaybackState::PausedByPlayer
                    | MediaPlaybackState::PausedAtCue
            )
        )
    }

    /// Whether the learner's pause flag is set.
    #[must_use]
    pub fn player_paused(&self) -> bool {
        self.st.player_paused
    }

    /// Current slide base clock in milliseconds.
    #[must_use]
    pub fn slide_elapsed_ms(&self) -> u32 {
        self.st.base.ms
    }

    /// Current slide duration in milliseconds.
    #[must_use]
    pub fn slide_duration_ms(&self) -> u32 {
        self.current_slide().map_or(0, |slide| slide.duration_ms)
    }

    /// Whether a seek is allowed right now.
    #[must_use]
    pub fn seek_allowed(&self) -> bool {
        self.current_slide().is_some()
            && match self.seek_target() {
                SeekTarget::Base => !self.base_held_by_layer(),
                SeekTarget::Layer(_) => true,
                SeekTarget::Refused => false,
            }
    }

    /// Read-only clock state for the player's seek bar.
    #[must_use]
    pub fn seek_info(&self) -> SeekInfo {
        let paused = self.st.player_paused;
        let Some(slide) = self.current_slide() else {
            return SeekInfo {
                elapsed_ms: 0,
                duration_ms: 0,
                paused,
                seek_allowed: false,
            };
        };
        match self.seek_target() {
            SeekTarget::Layer(id) => {
                let elapsed_ms = self.st.layer_clocks.get(&id).map_or(0, |c| c.ms);
                let duration_ms = slide.layer(&id).map_or(0, |layer| layer.duration_ms);
                SeekInfo {
                    elapsed_ms,
                    duration_ms,
                    paused,
                    seek_allowed: true,
                }
            }
            SeekTarget::Base => SeekInfo {
                elapsed_ms: self.st.base.ms,
                duration_ms: slide.duration_ms,
                paused,
                seek_allowed: !self.base_held_by_layer(),
            },
            SeekTarget::Refused => SeekInfo {
                elapsed_ms: self.st.base.ms,
                duration_ms: slide.duration_ms,
                paused,
                seek_allowed: false,
            },
        }
    }

    /// Whether a visible layer hides the base layer's objects.
    #[must_use]
    pub fn base_objects_hidden(&self) -> bool {
        let Some(slide) = self.current_slide() else {
            return false;
        };
        self.st
            .visible_layers
            .iter()
            .any(|id| slide.layer(id).is_some_and(|layer| layer.hide_base_objects))
    }

    /// Sample the visual transform, opacity and clip for `object_id` at the current clock.
    ///
    /// Combines entrance/exit animation with any active or resting motion path offset.
    /// Returns `None` if the object is off-screen (before `start_ms`, after `end_ms`,
    /// after an exit animation completes, or on a hidden layer).
    #[must_use]
    pub fn sample_object(&self, object_id: &str) -> Option<ObjectSample> {
        let slide = self.current_slide()?;
        let (obj, layer_id) = slide.object(object_id)?;

        let (clock_ms, duration_ms) = match layer_id {
            None => (self.st.base.ms, slide.duration_ms),
            Some(l) => {
                if !self.st.visible_layers.iter().any(|vl| vl == l) {
                    return None;
                }
                let c = self.st.layer_clocks.get(l)?;
                let layer = slide.layer(l)?;
                (c.ms, layer.duration_ms)
            }
        };

        if clock_ms < obj.timing.start_ms {
            return None;
        }

        let end_ms = obj.timing.end_ms.unwrap_or(duration_ms);
        if let Some(end) = obj.timing.end_ms
            && clock_ms >= end
        {
            return None;
        }
        if obj.exit.is_some() && clock_ms >= end_ms {
            return None;
        }

        let mut anim_sample = ObjectSample::IDENTITY;

        let exit_running = if let Some(ref exit_anim) = obj.exit {
            let dur = exit_anim.duration_ms;
            let start = end_ms.saturating_sub(dur);
            if clock_ms >= start && clock_ms < end_ms {
                let progress = if dur == 0 {
                    1.0
                } else {
                    (clock_ms.saturating_sub(start) as f32 / dur as f32).clamp(0.0, 1.0)
                };
                anim_sample = sample_animation(
                    exit_anim,
                    AnimationPhase::Exiting,
                    progress,
                    obj.frame,
                    self.course.slide_size,
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        if !exit_running && let Some(ref ent_anim) = obj.entrance {
            let dur = ent_anim.duration_ms;
            let start = obj.timing.start_ms;
            let end = start.saturating_add(dur);
            if clock_ms >= start && clock_ms < end {
                let progress = if dur == 0 {
                    1.0
                } else {
                    (clock_ms.saturating_sub(start) as f32 / dur as f32).clamp(0.0, 1.0)
                };
                anim_sample = sample_animation(
                    ent_anim,
                    AnimationPhase::Entering,
                    progress,
                    obj.frame,
                    self.course.slide_size,
                );
            }
        }

        let motion_offset = if let Some(running) = self.st.running_paths.get(object_id) {
            sample_motion_path(&running.path, running.elapsed_ms, running.from, obj.frame)
        } else {
            self.st
                .object_offsets
                .get(object_id)
                .copied()
                .unwrap_or((0.0, 0.0))
        };

        let drag_pos = self
            .st
            .object_positions
            .get(object_id)
            .copied()
            .unwrap_or((0.0, 0.0));

        Some(ObjectSample {
            offset: (
                anim_sample.offset.0 + motion_offset.0 + drag_pos.0,
                anim_sample.offset.1 + motion_offset.1 + drag_pos.1,
            ),
            scale: anim_sample.scale,
            opacity: anim_sample.opacity,
            clip: anim_sample.clip,
        })
    }

    /// Return the current resting position offset for `object_id`.
    #[must_use]
    pub fn object_position(&self, object_id: &str) -> (f32, f32) {
        self.st
            .object_positions
            .get(object_id)
            .copied()
            .unwrap_or((0.0, 0.0))
    }

    /// Return the current bounding rectangle of `object_id` in slide coordinates,
    /// taking into account its base frame, motion paths, and drag offset.
    ///
    /// Returns `None` if the object is not on the current slide, or if it is currently hidden.
    #[must_use]
    pub fn object_current_bounds(&self, object_id: &str) -> Option<coursecraft_course::Rect> {
        let slide = self.current_slide()?;
        let (obj, _layer) = slide.object(object_id)?;
        if self.object_state(object_id) == Some("hidden") {
            return None;
        }
        let sample = self.sample_object(object_id)?;
        if sample.opacity <= 0.0 {
            return None;
        }
        let scale = sample.scale;
        let cx = obj.frame.x + sample.offset.0 + obj.frame.w * 0.5;
        let cy = obj.frame.y + sample.offset.1 + obj.frame.h * 0.5;
        let w = (obj.frame.w * scale).abs();
        let h = (obj.frame.h * scale).abs();
        Some(coursecraft_course::Rect {
            x: cx - w * 0.5,
            y: cy - h * 0.5,
            w,
            h,
        })
    }

    /// The listed drag target that currently contains `object_id`'s centre,
    /// after it has been moved. `None` when the object is not draggable, has
    /// not been moved, or is not over one of its targets.
    #[must_use]
    pub fn dropped_target(&self, object_id: &str) -> Option<String> {
        if !self.st.object_positions.contains_key(object_id) {
            return None;
        }
        let slide = self.current_slide()?;
        let (object, _) = slide.object(object_id)?;
        let drag = object.drag.as_ref()?;
        let bounds = self.object_current_bounds(object_id)?;
        let centre_x = bounds.x + bounds.w * 0.5;
        let centre_y = bounds.y + bounds.h * 0.5;
        for target in &drag.targets {
            let Some(target_bounds) = self.object_current_bounds(target) else {
                continue;
            };
            if centre_x >= target_bounds.x
                && centre_x <= target_bounds.x + target_bounds.w
                && centre_y >= target_bounds.y
                && centre_y <= target_bounds.y + target_bounds.h
            {
                return Some(target.clone());
            }
        }
        None
    }

    /// Feed one input; returns the effects in the order they happened.
    pub fn handle(&mut self, input: &Input) -> Vec<Effect> {
        let mut out = Vec::new();
        self.handle_into(input, &mut out);
        out
    }
}

/// Format Unix seconds as a UTC `YYYY-MM-DD` date.
///
/// Returns an empty string when the date falls outside 1970–9999. Pure
/// arithmetic: no clock, no locale, no panic.
#[must_use]
pub fn civil_date(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let Some(z) = days.checked_add(719_468) else {
        return String::new();
    };
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(month <= 2);
    if !(1970..=9999).contains(&year) {
        return String::new();
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

/// The playback rates a learner can pick.
pub const PLAYBACK_RATES: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

fn is_false(value: &bool) -> bool {
    !*value
}

fn format_display_value(val: &Value) -> String {
    match val {
        Value::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Text(s) => s.clone(),
        Value::Number(n) => {
            if !n.is_finite() {
                return "0".to_string();
            }
            if *n == 0.0 {
                return "0".to_string();
            }
            let formatted = format!("{:.2}", n);
            let trimmed = if formatted.contains('.') {
                formatted.trim_end_matches('0').trim_end_matches('.')
            } else {
                &formatted
            };
            if trimmed == "-0" || trimmed.is_empty() {
                "0".to_string()
            } else {
                trimmed.to_string()
            }
        }
    }
}

fn push_str_capped(out: &mut String, s: &str, max_len: usize) -> bool {
    if out.len() >= max_len {
        return false;
    }
    let remaining = max_len.saturating_sub(out.len());
    if s.len() <= remaining {
        out.push_str(s);
    } else {
        let mut end = remaining;
        while end > 0 && !s.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        if let Some(sub) = s.get(..end) {
            out.push_str(sub);
        }
    }
    out.len() < max_len
}

#[cfg(test)]
mod tests;
