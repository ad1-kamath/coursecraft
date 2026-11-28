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
pub mod effects;
mod run;
pub mod suspend;

pub use animation::{
    AnimationPhase, ClipFrac, ConvexPoly, MAX_MASK_POLYS, MAX_POLY_POINTS, Mask, ObjectSample,
    SlideLook, Tint, TransitionSample, ease, sample_animation, sample_motion_path,
    sample_transition,
};
pub use suspend::{SUSPEND_PREFIX, SuspendError, decode_suspend, encode_suspend};

/// Entrance and exit animation of `obj` at `clock_ms` on a timeline of
/// `duration_ms` (the slide's or the object's layer's).
///
/// `None` when the object is not on screen: before `start_ms`, at or after
/// a stored `end_ms`, or after its exit animation has finished. Motion
/// paths, drags and states are not included; the player adds those. The
/// editor's timeline Play uses this alone, so it previews the same
/// entrances and exits the learner sees.
#[must_use]
pub fn sample_timed_animation(
    obj: &coursecraft_course::Object,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
) -> Option<ObjectSample> {
    sample_timed_animation_reduced(obj, clock_ms, duration_ms, slide_size, false)
}

/// [`sample_timed_animation`] for a learner who may prefer reduced motion. With `reduced` on,
/// an entrance or exit other than Fade (and Appear, which does not move) plays as a Fade of the
/// same duration and timing; an emphasis plays one pass only, and only if it changes colour or
/// opacity alone (spin, shake, blink, jump and the other moving effects stay at rest). Motion
/// paths are not touched (they carry meaning).
#[must_use]
pub fn sample_timed_animation_reduced(
    obj: &coursecraft_course::Object,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    reduced: bool,
) -> Option<ObjectSample> {
    sample_timed_part(obj, clock_ms, duration_ms, slide_size, (0, 1), reduced)
}

/// Whether `obj`'s entrance or exit plays its text in parts (by paragraph or letter).
#[must_use]
pub fn plays_in_parts(obj: &coursecraft_course::Object) -> Option<coursecraft_course::Sequence> {
    [&obj.entrance, &obj.exit]
        .into_iter()
        .flatten()
        .map(|a| a.sequence)
        .find(|s| !s.is_as_one())
}

/// [`sample_timed_animation`] for each of `parts` parts of the object's text (paragraphs or
/// letters, counted by the renderer): an entrance or exit with a `sequence` gives each part its
/// own window inside the animation's duration. Emphasis applies to every part alike. `None`
/// when the object is not on screen.
#[must_use]
pub fn sample_timed_parts(
    obj: &coursecraft_course::Object,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    parts: usize,
) -> Option<Vec<ObjectSample>> {
    sample_timed_parts_reduced(obj, clock_ms, duration_ms, slide_size, parts, false)
}

/// [`sample_timed_parts`] with the reduced-motion switch of [`sample_timed_animation_reduced`].
#[must_use]
pub fn sample_timed_parts_reduced(
    obj: &coursecraft_course::Object,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    parts: usize,
    reduced: bool,
) -> Option<Vec<ObjectSample>> {
    let n = parts.clamp(1, MAX_SEQUENCE_PARTS);
    (0..n)
        .map(|k| sample_timed_part(obj, clock_ms, duration_ms, slide_size, (k, n), reduced))
        .collect()
}

/// Most parts a sequenced animation plays; longer text plays in this many groups.
pub const MAX_SEQUENCE_PARTS: usize = 200;

/// Linear progress `raw` of a whole animation turned into the progress of part `k` of `n`.
/// By paragraph the parts play one after another; by letter each letter takes a quarter of the
/// time (or its share, if larger) and they overlap.
#[must_use]
pub fn part_progress(seq: coursecraft_course::Sequence, (k, n): (usize, usize), raw: f32) -> f32 {
    use coursecraft_course::Sequence;
    if n <= 1 || seq == Sequence::AsOne || !raw.is_finite() {
        return raw;
    }
    let nf = n as f32;
    let w = match seq {
        Sequence::ByLetter => (1.0 / nf).max(0.25),
        _ => 1.0 / nf,
    };
    let step = (1.0 - w) / (nf - 1.0);
    ((raw - k as f32 * step) / w).clamp(0.0, 1.0)
}

fn sample_timed_part(
    obj: &coursecraft_course::Object,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    part: (usize, usize),
    reduced: bool,
) -> Option<ObjectSample> {
    sample_timed_anim_fields(
        obj.timing,
        obj.frame,
        obj.entrance.as_ref(),
        obj.exit.as_ref(),
        &obj.emphasis,
        clock_ms,
        duration_ms,
        slide_size,
        part,
        reduced,
    )
}

/// Sample a group's entrance, exit, and emphasis animations at `clock_ms` on a
/// timeline of `duration_ms`, using `timing` (its members' union timing window)
/// and `frame` (its members' union bounding box).
#[must_use]
pub fn sample_group_animation(
    group: &coursecraft_course::Group,
    timing: coursecraft_course::Timing,
    frame: coursecraft_course::Rect,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
) -> Option<ObjectSample> {
    sample_group_animation_reduced(
        group,
        timing,
        frame,
        clock_ms,
        duration_ms,
        slide_size,
        false,
    )
}

/// [`sample_group_animation`] with the reduced-motion switch of
/// [`sample_timed_animation_reduced`].
#[must_use]
pub fn sample_group_animation_reduced(
    group: &coursecraft_course::Group,
    timing: coursecraft_course::Timing,
    frame: coursecraft_course::Rect,
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    reduced: bool,
) -> Option<ObjectSample> {
    sample_timed_anim_fields(
        timing,
        frame,
        group.entrance.as_ref(),
        group.exit.as_ref(),
        &group.emphasis,
        clock_ms,
        duration_ms,
        slide_size,
        (0, 1),
        reduced,
    )
}

#[allow(clippy::too_many_arguments)]
fn sample_timed_anim_fields(
    timing: coursecraft_course::Timing,
    frame: coursecraft_course::Rect,
    entrance: Option<&coursecraft_course::Animation>,
    exit: Option<&coursecraft_course::Animation>,
    emphasis: &[coursecraft_course::Emphasis],
    clock_ms: u32,
    duration_ms: u32,
    slide_size: coursecraft_course::Size,
    part: (usize, usize),
    reduced: bool,
) -> Option<ObjectSample> {
    if clock_ms < timing.start_ms {
        return None;
    }

    // A clock clamped at the timeline's duration means the timeline is over: an entrance or
    // exit that did not fit has completed at once (entrance: fully shown, exit: hidden).
    let timeline_over = clock_ms >= duration_ms;
    let end_ms = timing.end_ms.unwrap_or(duration_ms);
    if let Some(end) = timing.end_ms
        && clock_ms >= end
    {
        return None;
    }
    if exit.is_some() && (clock_ms >= end_ms || timeline_over) {
        return None;
    }

    let mut anim_sample = ObjectSample::IDENTITY;

    let exit_running = if let Some(exit_anim) = exit {
        // The exit never starts before the object/group does; a longer exit is squeezed into
        // its window.
        let start = end_ms
            .saturating_sub(exit_anim.duration_ms)
            .max(timing.start_ms);
        if clock_ms >= start && clock_ms < end_ms {
            let window = end_ms.saturating_sub(start);
            let progress = if window == 0 {
                1.0
            } else {
                (clock_ms.saturating_sub(start) as f32 / window as f32).clamp(0.0, 1.0)
            };
            anim_sample = sample_animation(
                &motion_safe(exit_anim, reduced),
                AnimationPhase::Exiting,
                part_progress(exit_anim.sequence, part, progress),
                frame,
                slide_size,
            );
            true
        } else {
            false
        }
    } else {
        false
    };

    if !exit_running
        && !timeline_over
        && let Some(ent_anim) = entrance
    {
        let dur = ent_anim.duration_ms;
        let start = timing.start_ms;
        let end = start.saturating_add(dur);
        if clock_ms >= start && clock_ms < end {
            let progress = if dur == 0 {
                1.0
            } else {
                (clock_ms.saturating_sub(start) as f32 / dur as f32).clamp(0.0, 1.0)
            };
            anim_sample = sample_animation(
                &motion_safe(ent_anim, reduced),
                AnimationPhase::Entering,
                part_progress(ent_anim.sequence, part, progress),
                frame,
                slide_size,
            );
        }
    }
    // Emphasis plays over whatever the entrance or exit is doing.
    for e in emphasis {
        let dur = e.duration_ms.max(1);
        let end = if reduced {
            // One pass only, and only effects that change colour or opacity alone.
            if !effects::emphasis_is_motion_free(&e.effect.id) {
                continue;
            }
            e.start_ms.saturating_add(dur)
        } else {
            e.end_ms()
        };
        if clock_ms < e.start_ms || clock_ms >= end {
            continue;
        }
        let into_pass = clock_ms.saturating_sub(e.start_ms) % dur;
        let raw = into_pass as f32 / dur as f32;
        let over = effects::sample_emphasis_eased(&e.effect, raw, ease(e.easing, raw), frame);
        anim_sample = anim_sample.then(&over);
    }
    Some(anim_sample)
}

/// `anim` as it plays under the learner's motion preference: with `reduced` on, any effect other
/// than Fade and Appear (which does not move) becomes a linear Fade of the same duration.
fn motion_safe(
    anim: &coursecraft_course::Animation,
    reduced: bool,
) -> Cow<'_, coursecraft_course::Animation> {
    if !reduced || matches!(anim.effect.id.as_str(), "fade" | "appear") {
        return Cow::Borrowed(anim);
    }
    Cow::Owned(coursecraft_course::Animation {
        effect: coursecraft_course::AnimEffect::new("fade"),
        easing: coursecraft_course::Easing::Linear,
        ..anim.clone()
    })
}

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

use coursecraft_course::{
    CompletionStatus, Course, Issue, Link, MotionPath, NavMode, NextRequirement, ObjectKind,
    QuestionKind, SeekbarBehaviour, Severity, Slide, Value,
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
    /// Where each dropped item rested when the answer was graded (drag offsets).
    #[serde(default)]
    pub drop_offsets: BTreeMap<String, (f32, f32)>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_cue_progress: BTreeMap<String, CueProgress>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_cue_held: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_cue_layers: BTreeMap<String, Vec<String>>,
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
    /// slide id -> object (or interactive group) id -> state. Kept across visits ("resume saved state").
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
    /// Runtime-only: recently visited slide IDs in visit order (at most 50).
    /// Never in suspend data; after a resume this starts empty and
    /// `SlideTarget::LastVisited` falls back to `Previous`.
    #[serde(default, skip_serializing)]
    pub visit_history: Vec<String>,
    /// Runtime-only: whether automatic `slides_viewed` tracking has already reported completion.
    /// Never written to suspend data.
    #[serde(default, skip_serializing)]
    pub slides_tracking_reported: bool,
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

/// Per-slide trigger metadata cached while the active slide is unchanged.
#[derive(Default)]
pub(crate) struct SlideCache<'c> {
    pub(crate) has_intersection_trigger: bool,
    pub(crate) pointer_trigger_objects: std::collections::HashSet<&'c str>,
}

/// Precomputed play order for a validated course and draw seed, stored without
/// borrowing [`Course`] so hosts like the editor preview can cache it across frames.
#[derive(Clone, Debug)]
pub struct PlayPlan {
    draw_seed: u32,
    slides: Vec<SlideLoc>,
    preview_subset: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug)]
enum SlideLoc {
    Scene(usize, usize),
    Bank(usize, usize),
}

/// A running course.
pub struct Player<'c> {
    course: &'c Course,
    slides: Vec<&'c Slide>,
    st: PlayerState,
    /// Whether `st` currently belongs to an entered slide. Not serialised; used
    /// to avoid saving a slide state that was never played.
    entered: bool,
    slide_cache: SlideCache<'c>,
    reach_buf: Vec<(u32, Option<&'c str>)>,
    /// Preview-only slide subset: when `Some`, navigation is restricted to these
    /// slide IDs and leaving them sets `preview_boundary_hit` instead of moving.
    preview_subset: Option<Vec<String>>,
    /// Set to `true` when a navigation, jump, or completion action in a partial
    /// preview attempts to leave the previewed subset.
    preview_boundary_hit: bool,
    /// The learner prefers reduced motion (see "Reduced motion" in `docs/spec/triggers.md`).
    /// Presentation only: set by the host, never saved, and it changes no timing or event.
    reduced_motion: AtomicBool,
}

impl<'c> Player<'c> {
    /// Tell the player whether the learner prefers reduced motion. Sampling then swaps
    /// entrances and exits for fades and drops or shortens emphasis (see
    /// [`sample_timed_animation_reduced`]). Timing, events and triggers do not change. The hosts
    /// set it from the platform preference; it is not saved.
    pub fn set_reduced_motion(&self, reduced: bool) {
        self.reduced_motion.store(reduced, Ordering::Relaxed);
    }

    /// Whether reduced motion is on.
    #[must_use]
    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion.load(Ordering::Relaxed)
    }

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
        Self::init_with_subset(course, seed, None)
    }

    /// Create a player for a validated course restricted to a preview slide subset.
    ///
    /// Only slides in `subset` are included in the play order (in course order),
    /// starting at the first slide of the subset. Attempts to navigate outside the
    /// subset set [`Player::take_preview_boundary`] instead of changing slides.
    ///
    /// # Errors
    /// Returns the validation errors if the course is not playable.
    pub fn with_preview_subset(
        course: &'c Course,
        seed: u32,
        subset: &[String],
    ) -> Result<Self, Vec<Issue>> {
        Self::init_with_subset(course, seed, Some(subset))
    }

    fn init_with_subset(
        course: &'c Course,
        seed: u32,
        subset: Option<&[String]>,
    ) -> Result<Self, Vec<Issue>> {
        let errors: Vec<Issue> = course
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let full_slides = draw::play_order(course, seed);
        let (slides, preview_subset, start_slide) = if let Some(ids) = subset {
            let filtered: Vec<&'c Slide> = full_slides
                .iter()
                .copied()
                .filter(|s| ids.iter().any(|id| id == &s.id))
                .collect();
            if filtered.is_empty() {
                let start = draw::expanded_start(course, &full_slides);
                (full_slides, None, start)
            } else {
                let normalized: Vec<String> = filtered.iter().map(|s| s.id.clone()).collect();
                (filtered, Some(normalized), 0)
            }
        } else {
            let start = draw::expanded_start(course, &full_slides);
            (full_slides, None, start)
        };
        let st = PlayerState {
            slide: start_slide,
            draw_seed: seed,
            variables: course
                .variables
                .iter()
                .map(|v| (v.name.clone(), v.value.clone()))
                .collect(),
            ..PlayerState::default()
        };
        let mut p = Self {
            course,
            slides,
            st,
            entered: false,
            slide_cache: SlideCache::default(),
            reach_buf: Vec::new(),
            preview_subset,
            preview_boundary_hit: false,
            reduced_motion: AtomicBool::new(false),
        };
        p.refresh_slide_cache();
        Ok(p)
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
        let mut p = Self {
            course,
            slides,
            st,
            entered: false,
            slide_cache: SlideCache::default(),
            reach_buf: Vec::new(),
            preview_subset: None,
            preview_boundary_hit: false,
            reduced_motion: AtomicBool::new(false),
        };
        p.refresh_slide_cache();
        p
    }

    /// Snapshot the validated play order for use with [`Player::resume_trusted`].
    #[must_use]
    pub fn play_plan(&self) -> PlayPlan {
        let mut by_ptr: std::collections::HashMap<*const Slide, SlideLoc> =
            std::collections::HashMap::new();
        for (si, scene) in self.course.scenes.iter().enumerate() {
            for (li, slide) in scene.slides.iter().enumerate() {
                by_ptr.insert(std::ptr::from_ref(slide), SlideLoc::Scene(si, li));
            }
        }
        for (bi, bank) in self.course.banks.iter().enumerate() {
            for (li, slide) in bank.slides.iter().enumerate() {
                by_ptr.insert(std::ptr::from_ref(slide), SlideLoc::Bank(bi, li));
            }
        }
        let slides = self
            .slides
            .iter()
            .filter_map(|slide| by_ptr.get(&std::ptr::from_ref(*slide)).copied())
            .collect();
        PlayPlan {
            draw_seed: self.st.draw_seed,
            slides,
            preview_subset: self.preview_subset.clone(),
        }
    }

    /// Restore a player from saved state (SCORM resume).
    ///
    /// # Errors
    /// Validation errors, or a state that does not fit this course.
    pub fn resume(course: &'c Course, state: PlayerState) -> Result<Self, Vec<Issue>> {
        Self::resume_with_subset(course, state, None)
    }

    fn resume_with_subset(
        course: &'c Course,
        state: PlayerState,
        subset: Option<&[String]>,
    ) -> Result<Self, Vec<Issue>> {
        let errors: Vec<Issue> = course
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut slides = draw::play_order(course, state.draw_seed);
        let preview_subset = if let Some(ids) = subset {
            let filtered: Vec<&'c Slide> = slides
                .iter()
                .copied()
                .filter(|s| ids.iter().any(|id| id == &s.id))
                .collect();
            if filtered.is_empty() {
                None
            } else {
                slides = filtered;
                Some(slides.iter().map(|s| s.id.clone()).collect())
            }
        } else {
            None
        };
        Self::finish_resume(course, slides, state, preview_subset)
    }

    /// Restore a player from a state produced by this same validated course
    /// and [`PlayPlan`], skipping full course validation and draw expansion.
    ///
    /// Falls back to [`Player::resume`] if the draw seed changed (for example
    /// after `RetryQuiz`) or if the plan no longer matches `course`.
    ///
    /// # Errors
    /// Validation errors, or a state that does not fit this course.
    pub fn resume_trusted(
        course: &'c Course,
        state: PlayerState,
        plan: &PlayPlan,
    ) -> Result<Self, Vec<Issue>> {
        if plan.draw_seed != state.draw_seed {
            return Self::resume_with_subset(course, state, plan.preview_subset.as_deref());
        }
        let mut slides: Vec<&'c Slide> = Vec::with_capacity(plan.slides.len());
        for loc in &plan.slides {
            let resolved = match *loc {
                SlideLoc::Scene(si, li) => course.scenes.get(si).and_then(|s| s.slides.get(li)),
                SlideLoc::Bank(bi, li) => course.banks.get(bi).and_then(|b| b.slides.get(li)),
            };
            let Some(slide) = resolved else {
                return Self::resume_with_subset(course, state, plan.preview_subset.as_deref());
            };
            slides.push(slide);
        }
        Self::finish_resume(course, slides, state, plan.preview_subset.clone())
    }

    fn finish_resume(
        course: &'c Course,
        slides: Vec<&'c Slide>,
        state: PlayerState,
        preview_subset: Option<Vec<String>>,
    ) -> Result<Self, Vec<Issue>> {
        if state.slide >= slides.len() {
            return Err(vec![Issue {
                severity: Severity::Error,
                path: "/slide".into(),
                message: "saved slide index is outside this course".into(),
            }]);
        }
        // Hosts rebuild a `Player` from its state every frame, so the learner's pause must
        // survive this. Suspend data never carries it, so a saved session still starts playing.
        let entered = state.started;
        let mut p = Self {
            course,
            slides,
            st: state,
            entered,
            slide_cache: SlideCache::default(),
            reach_buf: Vec::new(),
            preview_subset,
            preview_boundary_hit: false,
            reduced_motion: AtomicBool::new(false),
        };
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
            // With a lightbox open the current slide is the box's; the slide under it has
            // not been left either.
            let base =
                p.st.lightbox
                    .as_ref()
                    .and_then(|lb| p.slides.get(lb.base_slide))
                    .map(|slide| slide.id.clone());
            let earlier: Vec<String> =
                p.st.visited
                    .iter()
                    .filter(|id| Some(*id) != current.as_ref() && Some(*id) != base.as_ref())
                    .cloned()
                    .collect();
            p.st.timeline_completed.extend(earlier);
        }
        if let Some(threshold) = p.slides_viewed_threshold()
            && p.visited_play_slides() >= threshold
        {
            p.st.slides_tracking_reported = true;
        }
        p.refresh_slide_cache();
        Ok(p)
    }

    pub(crate) fn slides_viewed_threshold(&self) -> Option<usize> {
        if self.preview_subset.is_some() || self.slides.is_empty() {
            return None;
        }
        match self.course.tracking.slides_viewed? {
            coursecraft_course::SlidesViewedTracking::All => Some(self.slides.len()),
            coursecraft_course::SlidesViewedTracking::AtLeast { count } => Some(
                usize::try_from(count)
                    .unwrap_or(usize::MAX)
                    .clamp(1, self.slides.len()),
            ),
        }
    }

    pub(crate) fn visited_play_slides(&self) -> usize {
        self.slides
            .iter()
            .filter(|s| self.st.visited.contains(&s.id))
            .count()
    }

    #[must_use]
    pub fn state(&self) -> &PlayerState {
        &self.st
    }

    /// Consume the player and return its owned [`PlayerState`].
    #[must_use]
    pub fn into_state(self) -> PlayerState {
        self.st
    }

    /// Wall-clock milliseconds until the next time-driven change (animation
    /// frame, object appearance/disappearance, cue point, `timeline_reaches`,
    /// `timeline_ends`, running timer, motion path, playing media, or seekbar
    /// update).
    ///
    /// Returns `Some(0)` when smooth visual motion needs the next display
    /// frame immediately, `Some(ms)` (`ms >= 1`) when the host may sleep for
    /// `ms` milliseconds before waking, or `None` when nothing time-driven is
    /// pending and the host can stay idle until user input arrives.
    #[must_use]
    pub fn next_wake_ms(&self) -> Option<u32> {
        if !self.st.started
            || self.st.exited
            || self.st.player_paused
            || self.st.reviewing_from.is_some()
        {
            return None;
        }
        let slide = self.current_slide()?;
        let base_held = self.base_held_by_layer();
        let base_paused = self.st.base.paused || base_held;

        // a. Active motion paths on an unpaused, visible timeline need immediate frames.
        for obj_id in self.st.running_paths.keys() {
            let Some((_, layer_id)) = slide.object(obj_id) else {
                continue;
            };
            let is_paused = match layer_id {
                None => base_paused,
                Some(lid) => {
                    !self.st.visible_layers.iter().any(|vl| vl == lid)
                        || self.st.layer_clocks.get(lid).is_none_or(|c| c.paused)
                }
            };
            if !is_paused {
                return Some(0);
            }
        }

        let mut best: Option<u32> = None;
        let mut record_wall = |ms: u32| {
            best = Some(best.map_or(ms, |cur| cur.min(ms)));
        };

        // b. Playing media: visual media (Lottie / Video) and captioned audio need
        //    immediate frames; non-captioned audio polls periodically for end/trim.
        for (id, state) in &self.st.media_playback {
            if *state != MediaPlaybackState::Playing {
                continue;
            }
            let Some((obj, layer_id)) = slide.object(id) else {
                continue;
            };
            let visible =
                layer_id.is_none_or(|lid| self.st.visible_layers.iter().any(|vl| vl == lid));
            if !visible {
                continue;
            }
            match &obj.kind {
                ObjectKind::Lottie { .. } | ObjectKind::Video { .. } => return Some(0),
                ObjectKind::Audio { captions, .. } => {
                    if captions.is_some() {
                        return Some(0);
                    }
                    record_wall(50);
                }
                _ => {}
            }
        }
        if self.st.background.playing {
            record_wall(100);
        }

        let speed_quarters = if self.course.player.show_speed && self.st.speed_quarters != 0 {
            self.st.speed_quarters
        } else {
            4
        };

        // c. Running (or due-to-autostart) countdown timers whose owning timeline can advance.
        if let Some(ms) = self.timers_next_wake(slide, base_paused, speed_quarters) {
            if ms == 0 {
                return Some(0);
            }
            record_wall(ms);
        }

        // d. Base slide clock and visible layer clocks.
        let show_seekbar = self.course.player_for(slide).seekbar_available();
        let seek_target = self.seek_target();

        if !base_held {
            let base_has_seekbar = show_seekbar && matches!(seek_target, SeekTarget::Base);
            if let Some(delta) = clock_next_wake(
                self.st.base,
                slide.duration_ms,
                &slide.objects,
                slide,
                &slide.cue_points,
                None,
                base_has_seekbar,
            ) {
                if delta == 0 {
                    return Some(0);
                }
                record_wall(scale_wake_delta_ms(delta, speed_quarters));
            }
        }

        for lid in &self.st.visible_layers {
            let (Some(layer), Some(&clock)) = (slide.layer(lid), self.st.layer_clocks.get(lid))
            else {
                continue;
            };
            let layer_has_seekbar =
                show_seekbar && matches!(&seek_target, SeekTarget::Layer(target) if target == lid);
            if let Some(delta) = clock_next_wake(
                clock,
                layer.duration_ms,
                &layer.objects,
                slide,
                &layer.cue_points,
                Some(lid.as_str()),
                layer_has_seekbar,
            ) {
                if delta == 0 {
                    return Some(0);
                }
                record_wall(scale_wake_delta_ms(delta, speed_quarters));
            }
        }

        best
    }

    fn timers_next_wake(
        &self,
        slide: &Slide,
        base_paused: bool,
        speed_quarters: u8,
    ) -> Option<u32> {
        let mut best: Option<u32> = None;
        for (id, timer) in &self.st.timers {
            if timer.ended {
                continue;
            }
            let Some((obj, layer_id)) = slide.object(id) else {
                continue;
            };
            let owner_clock_ms = match layer_id {
                None => (!base_paused && !self.st.base.ended).then_some(self.st.base.ms),
                Some(lid) => self
                    .st
                    .visible_layers
                    .iter()
                    .any(|vl| vl == lid)
                    .then(|| {
                        self.st
                            .layer_clocks
                            .get(lid)
                            .filter(|c| !c.paused && !c.ended)
                            .map(|c| c.ms)
                    })
                    .flatten(),
            };
            let Some(clock_ms) = owner_clock_ms else {
                continue;
            };
            let autostart_due = timer.autostart_armed
                && matches!(
                    obj.kind,
                    ObjectKind::Timer {
                        autostart: true,
                        ..
                    }
                )
                && clock_ms >= obj.timing.start_ms;
            if !timer.running && !autostart_due {
                continue;
            }
            if timer.remaining_ms == 0 {
                return Some(0);
            }
            let wall = scale_wake_delta_ms(timer.remaining_ms.min(100), speed_quarters);
            best = Some(best.map_or(wall, |cur| cur.min(wall)));
        }
        best
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

    /// Whether this player is running a partial preview subset of slides.
    #[must_use]
    pub fn is_partial_preview(&self) -> bool {
        self.preview_subset.is_some()
    }

    /// Returns and clears whether a navigation, jump, or course-end action hit
    /// the boundary of a partial preview subset.
    pub fn take_preview_boundary(&mut self) -> bool {
        std::mem::take(&mut self.preview_boundary_hit)
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
    /// with the slide's `show_bar`, `show_nav`, `show_seekbar` and `show_menu`
    /// overrides applied. Every host and the runtime read this, never
    /// `course.player` directly, for those flags.
    #[must_use]
    pub fn effective_player(&self) -> coursecraft_course::PlayerSettings {
        self.course.player.for_slide(self.current_slide())
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
        self.effective_player().next_requires
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

    /// If `object_id` belongs to an interactive group on the current slide,
    /// returns that group's id.
    #[must_use]
    pub fn interactive_group_of(&self, object_id: &str) -> Option<&'c str> {
        let slide = self.current_slide()?;
        slide
            .interactive_group_of_object(object_id)
            .map(|(g, _)| g.id.as_str())
    }

    /// Whether `id` names an interactive group on the current slide.
    #[must_use]
    pub fn is_interactive_group(&self, id: &str) -> bool {
        self.current_slide()
            .is_some_and(|slide| slide.interactive_group(id).is_some())
    }

    /// Member objects of `group_id` on the current slide.
    #[must_use]
    pub fn group_members(&self, group_id: &str) -> Vec<&'c coursecraft_course::Object> {
        self.current_slide()
            .map(|slide| slide.group_members(group_id))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn object_state(&self, object: &str) -> Option<&str> {
        let slide = self.current_slide()?;
        let states = self.st.object_states.get(&slide.id)?;
        if let Some((group, _)) = slide
            .interactive_group(object)
            .or_else(|| slide.interactive_group_of_object(object))
        {
            return states
                .get(&group.id)
                .map(String::as_str)
                .or(Some(group.initial_state.as_str()));
        }
        states.get(object).map(String::as_str)
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

                if let Some((name, next)) = coursecraft_course::parse_variable_reference_at(text, i)
                    && let Some(val) = self.value(name)
                {
                    let formatted = format_display_value(&val);
                    if !push_str_capped(&mut out, &formatted, MAX_LEN) {
                        break;
                    }
                    i = next;
                    continue;
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

    /// Whether `player.seekbar` keeps the learner from seeking right now.
    #[must_use]
    pub fn seekbar_locked(&self) -> bool {
        match self.course.player.seekbar {
            SeekbarBehaviour::Free => false,
            SeekbarBehaviour::Locked => true,
            SeekbarBehaviour::UntilTimelineEnds => self.current_slide().is_some_and(|slide| {
                !self.st.timeline_completed.contains(&slide.id)
                    && self.st.base.ms < slide.duration_ms
            }),
        }
    }

    /// Whether a seek is allowed right now.
    #[must_use]
    pub fn seek_allowed(&self) -> bool {
        self.current_slide().is_some()
            && !self.seekbar_locked()
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
        let unlocked = !self.seekbar_locked();
        match self.seek_target() {
            SeekTarget::Layer(id) => {
                let elapsed_ms = self.st.layer_clocks.get(&id).map_or(0, |c| c.ms);
                let duration_ms = slide.layer(&id).map_or(0, |layer| layer.duration_ms);
                SeekInfo {
                    elapsed_ms,
                    duration_ms,
                    paused,
                    seek_allowed: unlocked,
                }
            }
            SeekTarget::Base => SeekInfo {
                elapsed_ms: self.st.base.ms,
                duration_ms: slide.duration_ms,
                paused,
                seek_allowed: unlocked && !self.base_held_by_layer(),
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
            None => {
                if self.base_objects_hidden() {
                    return None;
                }
                (self.st.base.ms, slide.duration_ms)
            }
            Some(l) => {
                if !self.st.visible_layers.iter().any(|vl| vl == l) {
                    return None;
                }
                let c = self.st.layer_clocks.get(l)?;
                let layer = slide.layer(l)?;
                (c.ms, layer.duration_ms)
            }
        };

        let anim_sample = sample_timed_animation_reduced(
            obj,
            clock_ms,
            duration_ms,
            self.course.slide_size,
            self.reduced_motion(),
        )?;

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
            ..anim_sample
        })
    }

    /// Sample a group's own entrance, exit, and emphasis animations at the
    /// current slide or layer clock.
    ///
    /// Returns `None` if the group is unknown, has no members, is on a hidden
    /// layer, or is outside its timing window / after its exit completes.
    #[must_use]
    pub fn sample_group(&self, group_id: &str) -> Option<ObjectSample> {
        let slide = self.current_slide()?;
        let (group, layer_id) = slide.group(group_id)?;
        let (clock_ms, duration_ms) = match layer_id {
            None => {
                if self.base_objects_hidden() {
                    return None;
                }
                (self.st.base.ms, slide.duration_ms)
            }
            Some(l) => {
                if !self.st.visible_layers.iter().any(|vl| vl == l) {
                    return None;
                }
                let c = self.st.layer_clocks.get(l)?;
                let layer = slide.layer(l)?;
                (c.ms, layer.duration_ms)
            }
        };
        let timing = slide.group_timing(group_id, layer_id)?;
        let frame = slide.group_frame(group_id, layer_id)?;
        sample_group_animation_reduced(
            group,
            timing,
            frame,
            clock_ms,
            duration_ms,
            self.course.slide_size,
            self.reduced_motion(),
        )
    }

    /// [`Self::sample_object`] for each of `parts` parts of the object's text (see
    /// [`sample_timed_parts`]).
    #[must_use]
    pub fn sample_object_parts(&self, object_id: &str, parts: usize) -> Option<Vec<ObjectSample>> {
        let whole = self.sample_object(object_id)?;
        let slide = self.current_slide()?;
        let (obj, layer_id) = slide.object(object_id)?;
        let (clock_ms, duration_ms) = match layer_id {
            None => (self.st.base.ms, slide.duration_ms),
            Some(l) => {
                let c = self.st.layer_clocks.get(l)?;
                (c.ms, slide.layer(l)?.duration_ms)
            }
        };
        let reduced = self.reduced_motion();
        let own = sample_timed_animation_reduced(
            obj,
            clock_ms,
            duration_ms,
            self.course.slide_size,
            reduced,
        )?;
        // Motion paths and drags move every part alike.
        let extra = (whole.offset.0 - own.offset.0, whole.offset.1 - own.offset.1);
        let parts = sample_timed_parts_reduced(
            obj,
            clock_ms,
            duration_ms,
            self.course.slide_size,
            parts,
            reduced,
        )?;
        Some(
            parts
                .into_iter()
                .map(|p| ObjectSample {
                    offset: (p.offset.0 + extra.0, p.offset.1 + extra.1),
                    ..p
                })
                .collect(),
        )
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
        let cx = obj.frame.x + sample.offset.0 + obj.frame.w * 0.5;
        let cy = obj.frame.y + sample.offset.1 + obj.frame.h * 0.5;
        let w = (obj.frame.w * sample.scale.0).abs();
        let h = (obj.frame.h * sample.scale.1).abs();
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

fn scale_wake_delta_ms(delta: u32, speed_quarters: u8) -> u32 {
    if delta == 0 {
        return 0;
    }
    let q = u64::from(speed_quarters.max(1));
    let wall = u64::from(delta).saturating_mul(4).saturating_add(q - 1) / q;
    u32::try_from(wall).unwrap_or(u32::MAX).max(1)
}

fn clock_next_wake(
    clock: Clock,
    duration_ms: u32,
    objects: &[coursecraft_course::Object],
    slide: &Slide,
    cue_points: &[coursecraft_course::SlideCue],
    layer_id: Option<&str>,
    has_seekbar: bool,
) -> Option<u32> {
    if clock.paused || clock.ended {
        return None;
    }
    if clock.ms >= duration_ms {
        return Some(0);
    }

    let mut best = duration_ms - clock.ms;

    let mut inspect_anim = |timing: coursecraft_course::Timing,
                            entrance: Option<&coursecraft_course::Animation>,
                            exit: Option<&coursecraft_course::Animation>,
                            emphasis: &[coursecraft_course::Emphasis]|
     -> Option<u32> {
        if let Some(ent) = entrance {
            let start = timing.start_ms;
            let finish = start.saturating_add(ent.duration_ms);
            if clock.ms >= start && clock.ms < finish {
                return Some(0);
            }
        }
        if let Some(exit) = exit {
            let end_ms = timing.end_ms.unwrap_or(duration_ms);
            let exit_start = end_ms.saturating_sub(exit.duration_ms).max(timing.start_ms);
            if clock.ms >= exit_start && clock.ms < end_ms {
                return Some(0);
            }
            if exit_start > clock.ms {
                best = best.min(exit_start - clock.ms);
            }
            if end_ms > clock.ms {
                best = best.min(end_ms - clock.ms);
            }
        }
        for emph in emphasis {
            let end = emph.end_ms();
            if clock.ms >= emph.start_ms && clock.ms < end {
                return Some(0);
            }
            if emph.start_ms > clock.ms {
                best = best.min(emph.start_ms - clock.ms);
            }
        }
        if timing.start_ms > clock.ms {
            best = best.min(timing.start_ms - clock.ms);
        }
        if let Some(end_ms) = timing.end_ms
            && end_ms > clock.ms
        {
            best = best.min(end_ms - clock.ms);
        }
        None
    };

    for obj in objects {
        if let Some(wake) = inspect_anim(
            obj.timing,
            obj.entrance.as_ref(),
            obj.exit.as_ref(),
            &obj.emphasis,
        ) {
            return Some(wake);
        }
    }

    let groups = match layer_id {
        None => slide.groups.as_slice(),
        Some(id) => slide.layer(id).map_or(&[][..], |l| l.groups.as_slice()),
    };
    for group in groups {
        if !group.has_animation() {
            continue;
        }
        let Some(timing) = slide.group_timing(&group.id, layer_id) else {
            continue;
        };
        if let Some(wake) = inspect_anim(
            timing,
            group.entrance.as_ref(),
            group.exit.as_ref(),
            &group.emphasis,
        ) {
            return Some(wake);
        }
    }

    for cue in cue_points {
        if cue.at_ms > clock.ms {
            best = best.min(cue.at_ms - clock.ms);
        }
    }

    for trigger in slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
    {
        if trigger.disabled {
            continue;
        }
        if let coursecraft_course::Event::TimelineReaches {
            ms,
            layer,
            cue: None,
        } = &trigger.when
            && layer.as_deref() == layer_id
            && *ms > clock.ms
        {
            best = best.min(*ms - clock.ms);
        }
    }

    if has_seekbar {
        best = best.min(100);
    }

    Some(best)
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
mod bughunt_m26_a3_exit_transitions;
#[cfg(test)]
mod bughunt_r2_motion;
#[cfg(test)]
mod f23_media_timeline_tests;
#[cfg(test)]
mod preview_subset_tests;
#[cfg(test)]
mod tests;
