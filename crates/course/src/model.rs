//! Serde types for `course.json`. See `docs/spec/course-model.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Version of the `course.json` schema this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Every learner-facing string the player paints itself, with its English
/// default. A course can override any of them in [`Course::strings`].
///
/// Placeholders use `{name}` and are filled with [`fill`]. The keys are stable
/// because XLIFF unit ids are `chrome/{key}`.
pub const CHROME_STRINGS: &[(&str, &str)] = &[
    ("play", "Play"),
    ("pause", "Pause"),
    ("replaySlide", "Replay slide"),
    ("mute", "Mute"),
    ("unmute", "Unmute"),
    ("slidePosition", "Slide position"),
    ("volume", "Volume"),
    ("playTooltip", "Play (Space)"),
    ("pauseTooltip", "Pause (Space)"),
    ("captionsTooltip", "Captions (C)"),
    ("outlineTooltip", "Outline (M)"),
    ("previousTooltip", "Previous (←)"),
    ("nextTooltip", "Next (→)"),
    ("nextLockedTooltip", "Complete this slide to continue"),
    ("locked", "locked"),
    ("playbackSpeed", "Playback speed"),
    ("speedValue", "{n} times"),
    ("fullScreen", "Full screen"),
    ("tabMenu", "Menu"),
    ("tabResources", "Resources"),
    ("tabGlossary", "Glossary"),
    ("tabTranscript", "Transcript"),
    ("glossarySearch", "Search the glossary"),
    ("transcriptEmpty", "This slide has no transcript."),
    ("closeOutlineTooltip", "Close outline (Esc)"),
    ("courseComplete", "Course complete"),
    ("closeWindow", "You can close this window."),
    ("loading", "Loading"),
    ("fillInPlaceholder", "Type your answer"),
    ("choosePlaceholder", "Choose…"),
    ("captions", "Captions"),
    ("courseOutline", "Course outline"),
    ("previousSlide", "Previous slide"),
    ("nextSlide", "Next slide"),
    ("closeOutline", "Close outline"),
    ("closeLightbox", "Close lightbox"),
    ("lightbox", "Lightbox"),
    ("placeMarker", "Place a marker on the image"),
    ("placeMarkerHint", "Use the arrow keys, then press Enter."),
    ("markersPlaced", "{n} placed"),
    (
        "dragHint",
        "Press Space to pick up, arrows to choose a target, Space to drop.",
    ),
    (
        "sequenceMove",
        "Move \"{text}\" {direction}, position {n} of {m}",
    ),
    ("directionUp", "up"),
    ("directionDown", "down"),
    ("dragOnTarget", "{label}, on {target}"),
    ("pickedUp", "Picked up {item}"),
    ("itemOver", "{item} over {target}"),
    ("itemDropped", "{item} dropped on {target}"),
    ("slideAnnouncement", "Slide {n} of {m}: {title}"),
    ("answerRequired", "Please answer the question"),
    (
        "reportingLost",
        "Your progress isn't being saved. To keep your results, close this and reopen the course from your LMS.",
    ),
    ("dismissNotice", "Dismiss"),
    ("reviewing", "Reviewing answers"),
    ("reviewQuestion", "Question {n} of {total}"),
    ("reviewCorrect", "Correct"),
    ("reviewIncorrect", "Incorrect"),
    ("reviewPoints", "{points} of {max} points"),
    ("correctAnswer", "Correct answer: {answer}"),
    ("exitReview", "Exit review"),
    ("markCorrect", "Correct"),
    ("markIncorrect", "Incorrect"),
    ("markMissed", "Correct answer"),
    ("printBlocked", "Allow pop-ups to print this page."),
    ("printTitle", "Certificate"),
];

/// Fill `{name}` placeholders in a chrome string.
///
/// Unknown placeholders are left untouched. The output is capped so hostile
/// values cannot grow it without bound.
#[must_use]
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    const MAX_LEN: usize = 65_536;
    let mut out = String::with_capacity(template.len().min(MAX_LEN));
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        if out.len() >= MAX_LEN {
            return out;
        }
        let Some(after_open) = rest.get(open.saturating_add(1)..) else {
            break;
        };
        let Some(close) = after_open.find('}') else {
            break;
        };
        let prefix = rest.get(..open).unwrap_or_default();
        if !push_capped(&mut out, prefix, MAX_LEN) {
            return out;
        }
        let name = after_open.get(..close).unwrap_or_default();
        match args.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => {
                if !push_capped(&mut out, value, MAX_LEN) {
                    return out;
                }
            }
            None => {
                let end = open.saturating_add(close).saturating_add(2);
                let original = rest.get(open..end).unwrap_or_default();
                if !push_capped(&mut out, original, MAX_LEN) {
                    return out;
                }
            }
        }
        rest = after_open
            .get(close.saturating_add(1)..)
            .unwrap_or_default();
    }
    let _ = push_capped(&mut out, rest, MAX_LEN);
    out
}

fn push_capped(out: &mut String, text: &str, max: usize) -> bool {
    let room = max.saturating_sub(out.len());
    if room == 0 {
        return false;
    }
    if text.len() <= room {
        out.push_str(text);
        true
    } else {
        let mut end = room;
        while end > 0 && !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        out.push_str(text.get(..end).unwrap_or_default());
        out.len() < max
    }
}

/// Whether a BCP 47 language tag has the simple shape CourseCraft accepts:
/// hyphen-separated alphanumeric subtags whose first subtag is 2–8 letters.
#[must_use]
pub fn is_language_tag_valid(tag: &str) -> bool {
    if tag.is_empty() || tag.len() > 64 {
        return false;
    }
    let mut parts = tag.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    if !(2..=8).contains(&first.chars().count()) || !first.chars().all(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    parts.all(|part| {
        (1..=8).contains(&part.chars().count()) && part.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// Whether the bundled Inter font lacks glyphs for `c`.
///
/// Inter covers Latin, Greek and Cyrillic. CJK, Arabic, Hebrew, Indic and the
/// other blocks listed here are reported so validation can warn instead of
/// silently painting empty boxes.
#[must_use]
pub fn script_unsupported(c: char) -> bool {
    let c = c as u32;
    matches!(
        c,
        0x0530..=0x058F   // Armenian
            | 0x0590..=0x05FF // Hebrew
            | 0x0600..=0x06FF // Arabic
            | 0x0700..=0x074F // Syriac
            | 0x0750..=0x077F // Arabic Supplement
            | 0x0780..=0x07BF // Thaana
            | 0x07C0..=0x07FF // NKo
            | 0x0800..=0x083F // Samaritan
            | 0x0840..=0x085F // Mandaic
            | 0x0860..=0x086F // Syriac Supplement
            | 0x08A0..=0x08FF // Arabic Extended-A
            | 0x0900..=0x0DFF // Indic scripts
            | 0x0E00..=0x0E7F // Thai
            | 0x0E80..=0x0EFF // Lao
            | 0x0F00..=0x0FFF // Tibetan
            | 0x1000..=0x109F // Myanmar
            | 0x10A0..=0x10FF // Georgian
            | 0x1100..=0x11FF // Hangul Jamo
            | 0x1200..=0x137F // Ethiopic
            | 0x1380..=0x139F // Ethiopic Supplement
            | 0x13A0..=0x13FF // Cherokee
            | 0x1400..=0x167F // Unified Canadian Aboriginal Syllabics
            | 0x1680..=0x169F // Ogham
            | 0x16A0..=0x16FF // Runic
            | 0x1700..=0x171F // Tagalog
            | 0x1720..=0x173F // Hanunoo
            | 0x1740..=0x175F // Buhid
            | 0x1760..=0x177F // Tagbanwa
            | 0x1780..=0x17FF // Khmer
            | 0x1800..=0x18AF // Mongolian
            | 0x1A00..=0x1AAF // Buginese
            | 0x1B00..=0x1B7F // Balinese
            | 0x1B80..=0x1BBF // Sundanese
            | 0x1C00..=0x1C4F // Lepcha
            | 0x2E80..=0x2FFF // CJK radicals and punctuation
            | 0x3000..=0x303F // CJK symbols and punctuation
            | 0x3040..=0x309F // Hiragana
            | 0x30A0..=0x30FF // Katakana
            | 0x3100..=0x312F // Bopomofo
            | 0x3130..=0x318F // Hangul Compatibility Jamo
            | 0x31A0..=0x31BF // Bopomofo Extended
            | 0x31F0..=0x31FF // Katakana Phonetic Extensions
            | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xA000..=0xA48F // Yi Syllables
            | 0xA4D0..=0xA4FF // Lisu
            | 0xA960..=0xA97F // Hangul Jamo Extended-A
            | 0xAC00..=0xD7AF // Hangul Syllables
            | 0xD7B0..=0xD7FF // Hangul Jamo Extended-B
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
            | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
            | 0xFE30..=0xFE4F // CJK Compatibility Forms
            | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
            | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
    )
}

/// Built-in state names with player-defined meaning (see `docs/spec/course-model.md`).
pub const BUILTIN_STATES: &[&str] = &[
    "normal", "hover", "down", "visited", "selected", "disabled", "hidden",
];

/// Valid key names for [`Event::KeyPressed`].
pub const VALID_TRIGGER_KEYS: &[&str] = &[
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "enter",
    "space",
    "escape",
];

/// Whether `key` is a valid trigger key name.
#[must_use]
pub fn is_valid_trigger_key(key: &str) -> bool {
    VALID_TRIGGER_KEYS.contains(&key)
}

/// Built-in read-only variable names and kinds (see `docs/spec/triggers.md`).
pub const BUILTIN_VARIABLES: &[(&str, ValueKind)] = &[
    ("Course.SlideNumber", ValueKind::Number),
    ("Course.TotalSlides", ValueKind::Number),
    ("Course.Progress", ValueKind::Number),
    ("Course.ScorePoints", ValueKind::Number),
    ("Course.ScorePercent", ValueKind::Number),
    ("Quiz.Passed", ValueKind::Bool),
    ("Quiz.Attempts", ValueKind::Number),
    ("Quiz.Reviewing", ValueKind::Bool),
    ("Slide.ElapsedMs", ValueKind::Number),
    ("Learner.Name", ValueKind::Text),
    ("Course.Title", ValueKind::Text),
    ("Course.Date", ValueKind::Text),
];

/// Look up the value kind of a built-in variable name.
#[must_use]
pub fn builtin_variable_kind(name: &str) -> Option<ValueKind> {
    BUILTIN_VARIABLES
        .iter()
        .find_map(|&(n, k)| if n == name { Some(k) } else { None })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub slide_size: Size,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VariableDef>,
    pub scenes: Vec<Scene>,
    /// Slide masters, drawn under the slides that name them. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masters: Vec<Slide>,
    /// Reusable slides the editor can add to the course. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<Slide>,
    /// Named banks of question slides, kept outside play order. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub banks: Vec<QuestionBank>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_slide: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiz: Option<QuizSettings>,
    #[serde(default)]
    pub player: PlayerSettings,
    #[serde(default, skip_serializing_if = "is_default_theme")]
    pub theme: Theme,
    /// BCP 47 language tag for the learner-facing text, e.g. `fr-CA`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Overrides for the player's own chrome text, keyed by [`CHROME_STRINGS`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub strings: BTreeMap<String, String>,
    /// Custom font faces imported into the course. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontFace>,
    /// One track that plays across the whole course. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_audio: Option<BackgroundAudio>,
    /// Named text styles, up to [`MAX_TEXT_STYLES`]. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_styles: Vec<NamedTextStyle>,
    /// Links and files the learner can open from the player. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<Resource>,
    /// Terms the learner can look up in the player. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glossary: Vec<GlossaryEntry>,
}

/// One entry in the player's Resources tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub target: ResourceTarget,
}

/// What a resource opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceTarget {
    /// An `http`, `https` or `mailto` link.
    Url { url: String },
    /// A project-relative file under `media/`, packaged with the course.
    File { src: String },
}

/// One entry in the player's Glossary tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlossaryEntry {
    pub id: String,
    pub term: String,
    pub definition: String,
}

/// A course-wide music or narration track that keeps playing across slides.
///
/// The runtime treats it as the reserved media object
/// [`BACKGROUND_AUDIO_ID`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundAudio {
    pub src: String,
    #[serde(default = "half_volume", skip_serializing_if = "is_half_volume")]
    pub volume: f32,
    #[serde(rename = "loop", default = "yes", skip_serializing_if = "is_true")]
    pub looping: bool,
}

/// Reserved media object id for a course's background audio. The dot keeps it
/// apart from every valid object id.
pub const BACKGROUND_AUDIO_ID: &str = "Course.BackgroundAudio";

/// Style of a registered font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    /// Every style, in registration order.
    pub const ALL: &'static [Self] = &[Self::Regular, Self::Bold, Self::Italic, Self::BoldItalic];

    /// A stable lower-case name used in font family keys.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::BoldItalic => "bold_italic",
        }
    }
}

/// One custom font file, copied into the project's `media/fonts/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    /// Family name read from the file, e.g. `Lato`.
    pub family: String,
    pub style: FontStyle,
    /// Media path under `fonts/`, e.g. `fonts/lato-regular.ttf`.
    pub src: String,
}

/// Maximum number of font faces a course may register.
pub const MAX_FONT_FACES: usize = 40;
/// Maximum length of a font family name, in characters.
pub const MAX_FONT_FAMILY_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

impl Default for Size {
    fn default() -> Self {
        Self { w: 1280, h: 720 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The kept window of an image's source picture, as fractions of the source
/// (`0..=1`): `x` and `y` are where the window starts, `w` and `h` its size.
///
/// Stored on [`ObjectKind::Image`] and omitted from JSON when unset, so existing
/// courses stay byte-identical. Validation and the drawing code both refuse a
/// window that is non-finite, empty, outside `0..=1`, or running past the source.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropRect {
    /// The whole source, which draws exactly like no crop.
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// Whether this is a positive, finite window inside the source picture.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
            return false;
        }
        x >= 0.0 && y >= 0.0 && x + w <= 1.0 + 1e-6 && y + h <= 1.0 + 1e-6
    }

    /// Whether this keeps the whole source (within a hair), i.e. draws like no
    /// crop at all.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.is_valid()
            && self.x <= 1e-6
            && self.y <= 1e-6
            && self.w >= 1.0 - 1e-6
            && self.h >= 1.0 - 1e-6
    }

    /// This crop as a plain rect, so geometry code can share one type.
    #[must_use]
    pub fn as_rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
    }
}

impl From<Rect> for CropRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSettings {
    /// Show the player's Previous/Next buttons.
    #[serde(default = "yes")]
    pub show_nav: bool,
    /// Show the slide menu (scene/slide outline).
    #[serde(default = "yes")]
    pub show_menu: bool,
    /// Show the seekbar.
    #[serde(default = "yes")]
    pub show_seekbar: bool,
    /// Offer the playback speed control (when the host can change media rate).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_speed: bool,
    /// Offer the full screen toggle (when the host can go full screen).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_fullscreen: bool,
    /// A project-relative image shown at the left of the strip, at most 28 px high.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Show the slide title in the strip.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_title: bool,
    /// Show the volume button and slider.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_volume: bool,
    /// Show the captions button.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_captions: bool,
    /// Show the current slide's notes as a transcript tab in the player.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub show_transcript: bool,
    /// How freely the learner may jump around with the slide menu.
    #[serde(default, skip_serializing_if = "is_default_nav_mode")]
    pub navigation: NavMode,
    /// The course-wide rule for when Next becomes available.
    #[serde(default, skip_serializing_if = "is_default_next_requirement")]
    pub next_requires: NextRequirement,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            show_nav: true,
            show_menu: true,
            show_seekbar: true,
            show_speed: true,
            show_fullscreen: true,
            show_transcript: false,
            logo: None,
            show_title: true,
            show_volume: true,
            show_captions: true,
            navigation: NavMode::Free,
            next_requires: NextRequirement::Nothing,
        }
    }
}

/// How the slide menu lets the learner move around the course.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavMode {
    /// Any slide can be opened from the menu.
    #[default]
    Free,
    /// The menu opens only visited slides and the current one.
    Restricted,
    /// The menu lists slides but cannot jump.
    Locked,
}

/// What must happen before the Next button works.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NextRequirement {
    /// Next is always available.
    #[default]
    Nothing,
    /// Next unlocks once the base timeline has reached its end.
    TimelineEnds,
    /// Next unlocks while the named true/false variable is true.
    Variable { name: String },
}

/// Per-slide player settings. Omitted from JSON when unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlidePlayerOverrides {
    /// Replaces the course's Next rule on this slide; `None` uses the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_requires: Option<NextRequirement>,
    /// Replaces `player.show_nav` on this slide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_nav: Option<bool>,
    /// Replaces `player.show_seekbar` on this slide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_seekbar: Option<bool>,
    /// Replaces `player.show_menu` on this slide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_menu: Option<bool>,
}

fn is_false_bool(value: &bool) -> bool {
    !*value
}

fn is_default_nav_mode(mode: &NavMode) -> bool {
    *mode == NavMode::Free
}

fn is_default_next_requirement(rule: &NextRequirement) -> bool {
    *rule == NextRequirement::Nothing
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizSettings {
    /// Pass mark as a percentage of available points.
    pub passing_percent: u8,
}

// ---------------------------------------------------------------- variables

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableDef {
    pub name: String,
    /// Initial value; its JSON type fixes the variable's type for the whole course.
    pub value: Value,
}

/// A variable value. JSON `true`/`false`, numbers and strings map directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Bool,
    Number,
    Text,
}

impl Value {
    #[must_use]
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Bool(_) => ValueKind::Bool,
            Self::Number(_) => ValueKind::Number,
            Self::Text(_) => ValueKind::Text,
        }
    }
}

// ---------------------------------------------------------------- structure

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub slides: Vec<Slide>,
}

/// A named bank of question slides, kept outside play order like masters.
/// Every slide in a bank must carry a question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionBank {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub slides: Vec<Slide>,
}

/// A draw from a question bank: at play time the carrying slide is replaced
/// by `count` questions picked at random from the bank named `bank`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BankDraw {
    pub bank: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slide {
    pub id: String,
    pub title: String,
    /// Length of the base timeline in milliseconds.
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
    /// Background colour override for this slide (overrides `theme.background`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Color>,
    /// Objects on the base layer, bottom to top.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Object>,
    /// Slide layers, bottom to top.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// Triggers owned by the base layer, in firing order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<Question>,
    /// Draw `count` questions at random from a bank. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw: Option<BankDraw>,
    /// Optional slide transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    /// Optional speaker / slide notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Master this slide draws under. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master: Option<String>,
    /// How entering an already-visited slide treats clock and object states.
    #[serde(default, skip_serializing_if = "is_default_revisit")]
    pub revisit: RevisitMode,
    /// Per-slide player overrides. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<SlidePlayerOverrides>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    #[serde(flatten)]
    pub effect: TransitionEffect,
    #[serde(default = "d500")]
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransitionEffect {
    Fade,
    Push {
        #[serde(default)]
        from: Direction,
    },
    Cover {
        #[serde(default)]
        from: Direction,
    },
}

fn default_duration() -> u32 {
    5000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub id: String,
    pub name: String,
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    /// Showing this layer hides every other visible layer.
    #[serde(default = "yes")]
    pub hide_other_layers: bool,
    /// The base timeline pauses while this layer is visible.
    #[serde(default)]
    pub pause_base_timeline: bool,
    /// Clicks on base-layer objects are ignored while this layer is visible.
    #[serde(default)]
    pub prevent_base_clicks: bool,
    /// Base-layer objects are not drawn, clicked, focused or read while this
    /// layer is visible. Master items stay.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_base_objects: bool,
    /// Hide this layer when its clock reaches its duration.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_when_timeline_ends: bool,
    /// How the learner's seekbar treats this layer while it is visible.
    #[serde(default, skip_serializing_if = "is_default_seek")]
    pub allow_seek: SeekMode,
    /// How showing this layer again treats its clock and object states.
    #[serde(default, skip_serializing_if = "is_default_revisit")]
    pub revisit: RevisitMode,
}

/// How the learner's seekbar treats a visible layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeekMode {
    /// Seek this layer only when it pauses the base timeline.
    #[default]
    Auto,
    /// Always seek this layer.
    Yes,
    /// Refuse seeking while this layer is visible.
    No,
}

/// How showing a layer or entering a slide again treats saved state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisitMode {
    /// Keep object states and restart the clock at 0.
    #[default]
    Auto,
    /// Reset object states and restart the clock at 0.
    Reset,
    /// Keep object states and continue from the saved clock.
    Resume,
}

fn is_default_seek(mode: &SeekMode) -> bool {
    *mode == SeekMode::Auto
}

fn is_default_revisit(mode: &RevisitMode) -> bool {
    *mode == RevisitMode::Auto
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub kind: ObjectKind,
    pub frame: Rect,
    #[serde(default)]
    pub timing: Timing,
    /// Extra states beyond `normal`, which is the object's own appearance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<StateDef>,
    /// State on first visit. `hidden` = starts invisible.
    #[serde(default = "normal_state")]
    pub initial_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance: Option<Animation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Animation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motion_paths: Vec<MotionPath>,
    /// Author-set keyboard focus position. Lower runs first; ties keep the
    /// default reading order. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_order: Option<u32>,
    /// Ignored by assistive technology and by keyboard focus. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub decorative: bool,
    /// Text alternative for any object. For images, `alt` wins when this is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Authoring-only group id. Logic, render and publish ignore it.
    /// Omitted when the object is not grouped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Id of the scrolling panel this object scrolls inside. Omitted when the
    /// object is not in a panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// Clockwise rotation in degrees, normalised to `0..360`. Omitted when 0.
    #[serde(
        default,
        deserialize_with = "deserialize_rotation",
        skip_serializing_if = "is_zero_rotation"
    )]
    pub rotation: f32,
    /// Mirror horizontally about the frame centre. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_h: bool,
    /// Mirror vertically about the frame centre. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_v: bool,
    /// Overall opacity in `0..=1`. Omitted when 1. Multiplies with animation fades.
    #[serde(
        default = "default_opacity",
        deserialize_with = "deserialize_opacity",
        skip_serializing_if = "is_default_opacity"
    )]
    pub opacity: f32,
    /// Drop shadow drawn behind the object. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// Outer glow drawn behind the object. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<Glow>,
    /// Drag-and-drop specification for freeform draggable objects. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<DragSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DragSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub snap: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub return_on_miss: bool,
}

impl Default for DragSpec {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            snap: true,
            return_on_miss: true,
        }
    }
}

/// Clamp an opacity into `0..=1`, repairing non-finite input.
#[must_use]
pub fn clamp_opacity(value: f32) -> f32 {
    if value.is_nan() {
        1.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn deserialize_opacity<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(clamp_opacity(f32::deserialize(deserializer)?))
}

fn is_default_opacity(value: &f32) -> bool {
    !value.is_finite() || (*value - 1.0).abs() <= f32::EPSILON
}

fn default_opacity() -> f32 {
    1.0
}

/// One colour stop of a gradient fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    pub color: Color,
    /// Position along the gradient in `0..=1`.
    #[serde(default, deserialize_with = "deserialize_unit_fraction")]
    pub position: f32,
}

/// Upper bound on gradient stops, including a hostile file.
pub const MAX_GRADIENT_STOPS: usize = 10;

/// The fill of a shape or button beyond its solid fallback colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FillStyle {
    /// Linear gradient at `angle` degrees clockwise from left-to-right.
    Linear {
        #[serde(default, deserialize_with = "deserialize_angle")]
        angle: f32,
        #[serde(default, deserialize_with = "deserialize_gradient_stops")]
        stops: Vec<GradientStop>,
    },
    /// Radial gradient from `center`, as fractions of the frame.
    Radial {
        #[serde(
            default = "default_gradient_center",
            deserialize_with = "deserialize_gradient_center"
        )]
        center: [f32; 2],
        #[serde(default, deserialize_with = "deserialize_gradient_stops")]
        stops: Vec<GradientStop>,
    },
    /// A picture clipped to the shape, stretched or tiled.
    Picture {
        #[serde(default)]
        src: String,
        #[serde(default)]
        mode: PictureFillMode,
    },
}

impl FillStyle {
    /// The media a picture fill references, if any.
    #[must_use]
    pub fn picture_src(&self) -> Option<&str> {
        match self {
            Self::Picture { src, .. } => Some(src.as_str()),
            Self::Linear { .. } | Self::Radial { .. } => None,
        }
    }
}

/// How a picture fill covers its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PictureFillMode {
    /// One copy fills the frame.
    #[default]
    Stretch,
    /// Repeat the picture at its natural size.
    Tile,
}

fn default_gradient_center() -> [f32; 2] {
    [0.5, 0.5]
}

fn deserialize_angle<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(normalize_rotation(f32::deserialize(deserializer)?))
}

fn deserialize_unit_fraction<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    })
}

fn deserialize_gradient_center<'de, D>(deserializer: D) -> Result<[f32; 2], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let center = <[f32; 2]>::deserialize(deserializer)?;
    Ok([
        if center[0].is_nan() {
            0.5
        } else {
            center[0].clamp(0.0, 1.0)
        },
        if center[1].is_nan() {
            0.5
        } else {
            center[1].clamp(0.0, 1.0)
        },
    ])
}

fn deserialize_gradient_stops<'de, D>(deserializer: D) -> Result<Vec<GradientStop>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut stops = Vec::<GradientStop>::deserialize(deserializer)?;
    for stop in &mut stops {
        let value = stop.position;
        stop.position = if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        };
    }
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    stops.truncate(MAX_GRADIENT_STOPS);
    Ok(stops)
}

/// A drop shadow behind an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    /// Shadow colour; its alpha is used.
    pub color: Color,
    /// Horizontal offset in slide pixels, clamped to `-200..=200`.
    #[serde(default, deserialize_with = "deserialize_shadow_offset")]
    pub offset_x: f32,
    /// Vertical offset in slide pixels, clamped to `-200..=200`.
    #[serde(default, deserialize_with = "deserialize_shadow_offset")]
    pub offset_y: f32,
    /// Blur radius in slide pixels, clamped to `0..=100`.
    #[serde(default, deserialize_with = "deserialize_effect_radius")]
    pub blur: f32,
}

/// An outer glow behind an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glow {
    pub color: Color,
    /// Glow radius in slide pixels, clamped to `0..=100`.
    #[serde(default, deserialize_with = "deserialize_effect_radius")]
    pub size: f32,
}

fn deserialize_shadow_offset<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(finite_clamp(value, -200.0, 200.0, 0.0))
}

fn deserialize_effect_radius<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(finite_clamp(value, 0.0, 100.0, 0.0))
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(min, max)
    }
}

/// Normalise an object rotation into `0..360` degrees, repairing non-finite
/// values to 0.
#[must_use]
pub fn normalize_rotation(degrees: f32) -> f32 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let normalised = degrees % 360.0;
    if normalised < 0.0 {
        normalised + 360.0
    } else if normalised == 360.0 {
        0.0
    } else {
        normalised
    }
}

fn deserialize_rotation<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(normalize_rotation(value))
}

fn is_zero_rotation(degrees: &f32) -> bool {
    !degrees.is_finite() || *degrees == 0.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub effect: AnimEffect,
    #[serde(default = "d500")]
    pub duration_ms: u32,
    #[serde(default)]
    pub easing: Easing,
}

pub fn d500() -> u32 {
    500
}

/// Slowest Lottie playback rate.
pub const LOTTIE_MIN_SPEED: f32 = 0.1;
/// Fastest Lottie playback rate.
pub const LOTTIE_MAX_SPEED: f32 = 4.0;

/// Clamp a Lottie speed into `0.1..=4`, repairing non-finite input.
#[must_use]
pub fn clamp_lottie_speed(value: f32) -> f32 {
    if !value.is_finite() {
        1.0
    } else {
        value.clamp(LOTTIE_MIN_SPEED, LOTTIE_MAX_SPEED)
    }
}

fn default_speed() -> f32 {
    1.0
}

fn deserialize_speed<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(clamp_lottie_speed(f32::deserialize(deserializer)?))
}

fn is_default_speed(value: &f32) -> bool {
    !value.is_finite() || (*value - 1.0).abs() <= f32::EPSILON
}

pub fn d1000() -> u32 {
    1000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimEffect {
    Fade,
    FlyIn {
        #[serde(default)]
        from: Direction,
    },
    Zoom,
    Wipe {
        #[serde(default)]
        from: Direction,
    },
    Float {
        #[serde(default)]
        from: Direction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Right,
    Top,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionPath {
    pub id: String,
    pub points: Vec<Point>,
    #[serde(default = "d1000")]
    pub duration_ms: u32,
    #[serde(default)]
    pub easing: Easing,
    #[serde(default = "yes")]
    pub relative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

fn normal_state() -> String {
    "normal".to_owned()
}

/// Most files a web object's folder source may hold.
pub const WEB_MAX_FILES: usize = 500;
/// Most bytes a web object's folder source may hold in total (100 MiB).
pub const WEB_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// The entry file a folder source uses unless the author changes it.
pub const WEB_DEFAULT_ENTRY: &str = "index.html";

fn default_web_entry() -> String {
    WEB_DEFAULT_ENTRY.to_owned()
}

fn is_default_web_entry(entry: &str) -> bool {
    entry == WEB_DEFAULT_ENTRY
}

/// Where a [`ObjectKind::Web`] object gets its content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WebSource {
    /// A page on the web. Only `https://` URLs are accepted.
    Url { url: String },
    /// A folder of HTML, CSS, JavaScript and media under `media/`.
    /// `path` is relative to `media/` and starts with `web/`; `entry` is a
    /// relative file inside it.
    Folder {
        path: String,
        #[serde(
            default = "default_web_entry",
            skip_serializing_if = "is_default_web_entry"
        )]
        entry: String,
    },
}

impl WebSource {
    /// The URL for a URL source.
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Url { url } => Some(url),
            Self::Folder { .. } => None,
        }
    }

    /// The folder path and entry file for a folder source.
    #[must_use]
    pub fn folder(&self) -> Option<(&str, &str)> {
        match self {
            Self::Folder { path, entry } => Some((path, entry)),
            Self::Url { .. } => None,
        }
    }

    /// The media path of the folder's entry file, relative to `media/`.
    #[must_use]
    pub fn entry_media_path(&self) -> Option<String> {
        let (path, entry) = self.folder()?;
        Some(format!("{path}/{entry}"))
    }

    /// The short label shown on the placeholder card: the URL host or the
    /// folder's last segment.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Url { url } => {
                let rest = url.strip_prefix("https://").unwrap_or(url.as_str());
                let host = rest.split(['/', '?', '#']).next().unwrap_or(rest).trim();
                if host.is_empty() {
                    url.clone()
                } else {
                    host.to_owned()
                }
            }
            Self::Folder { path, .. } => path
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or(path)
                .to_owned(),
        }
    }
}

/// Whether `url` is an acceptable web-object address: `https://` with a host.
#[must_use]
pub fn is_valid_web_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    !host.is_empty()
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
        && !host.contains('@')
}

/// Whether `src` is a safe relative path to a picture the player can draw
/// (png, jpg, webp, gif or svg).
#[must_use]
pub fn is_image_src(src: &str) -> bool {
    if !is_safe_media_path(src) {
        return false;
    }
    let extension = src
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "svgz"
    )
}

/// Whether `path` is a safe relative media path (no root, drive, `..` or
/// empty segments), as used by object media sources.
#[must_use]
pub fn is_safe_media_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    path.split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Whether a folder source path is safe and lives under `web/`.
#[must_use]
pub fn is_valid_web_folder_path(path: &str) -> bool {
    is_safe_media_path(path)
        && path
            .strip_prefix("web/")
            .is_some_and(|name| !name.is_empty())
}

/// Whether a folder source entry is a safe relative file name.
#[must_use]
pub fn is_valid_web_entry(entry: &str) -> bool {
    is_safe_media_path(entry) && !entry.ends_with('/')
}

/// Whether a folder of `files` and `bytes` stays inside the web-object caps.
#[must_use]
pub fn web_folder_within_caps(files: usize, bytes: u64) -> bool {
    files <= WEB_MAX_FILES && bytes <= WEB_MAX_BYTES
}

// ---------------------------------------------------------------- input objects

/// Most characters a text-entry input may accept. `0` means no limit.
pub const MAX_INPUT_CHARS: u32 = 10_000;
/// A radio group has at least this many options.
pub const MIN_RADIO_OPTIONS: usize = 2;
/// A radio group has at most this many options.
pub const MAX_RADIO_OPTIONS: usize = 20;

/// Direction of a slider's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

fn default_start_angle() -> f32 {
    -135.0
}

fn default_end_angle() -> f32 {
    135.0
}

fn is_default_start_angle(value: &f32) -> bool {
    (*value - default_start_angle()).abs() <= f32::EPSILON
}

fn is_default_end_angle(value: &f32) -> bool {
    (*value - default_end_angle()).abs() <= f32::EPSILON
}

fn repair_finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn deserialize_input_min<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(repair_finite(f64::deserialize(deserializer)?, 0.0))
}

fn deserialize_input_max<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(repair_finite(f64::deserialize(deserializer)?, 100.0))
}

fn deserialize_input_step<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(repair_finite(f64::deserialize(deserializer)?, 0.0).max(0.0))
}

fn deserialize_start_angle<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(if value.is_finite() {
        value.clamp(-360.0, 360.0)
    } else {
        default_start_angle()
    })
}

fn deserialize_end_angle<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(if value.is_finite() {
        value.clamp(-360.0, 360.0)
    } else {
        default_end_angle()
    })
}

/// One option of a radio-button group. `value` is what the variable stores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadioOption {
    pub label: String,
    pub value: String,
}

// ---------------------------------------------------------------- markers and panels

/// The icon drawn inside a marker dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerIcon {
    #[default]
    Info,
    Plus,
    Question,
    Star,
    Pin,
}

fn is_default_marker_icon(icon: &MarkerIcon) -> bool {
    *icon == MarkerIcon::Info
}

/// Whether a scrolling panel draws its scrollbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollBar {
    #[default]
    Visible,
    Hidden,
}

fn is_default_scroll_bar(bar: &ScrollBar) -> bool {
    *bar == ScrollBar::Visible
}

/// How a [`ObjectKind::Timer`] prints its remaining time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerFormat {
    /// `minutes:seconds`, with minutes rolling up past 60 (`90:00`).
    #[default]
    MmSs,
    /// A plain whole-second count.
    Seconds,
    /// Nothing is drawn in the player; the canvas shows a labelled placeholder.
    Hidden,
}

fn is_default_timer_format(format: &TimerFormat) -> bool {
    *format == TimerFormat::MmSs
}

/// Longest duration a timer may count down from, in milliseconds (24 hours).
pub const MAX_TIMER_DURATION_MS: u32 = 86_400_000;
/// Duration a timer gets when the JSON omits it.
pub const DEFAULT_TIMER_DURATION_MS: u32 = 60_000;

fn default_timer_duration() -> u32 {
    DEFAULT_TIMER_DURATION_MS
}

fn deserialize_timer_duration<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = i64::deserialize(deserializer)?;
    Ok(value.clamp(1, i64::from(MAX_TIMER_DURATION_MS)) as u32)
}

/// The read-only variable name holding a timer's remaining whole seconds.
#[must_use]
pub fn timer_remaining_variable(object_id: &str) -> String {
    format!("Timer.{object_id}.Remaining")
}

/// The timer object id named by a `Timer.<id>.Remaining` variable, if it is one.
#[must_use]
pub fn timer_id_for_remaining_variable(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("Timer.")?;
    let id = rest.strip_suffix(".Remaining")?;
    if id.is_empty() { None } else { Some(id) }
}

/// Whether `name` is the `Timer.<id>.Remaining` variable of a timer on `slide`.
#[must_use]
pub fn is_timer_remaining_variable_on(slide: &Slide, name: &str) -> bool {
    let Some(id) = timer_id_for_remaining_variable(name) else {
        return false;
    };
    slide
        .object(id)
        .is_some_and(|(o, _)| matches!(o.kind, ObjectKind::Timer { .. }))
}

/// Whole seconds remaining, rounded up, so a timer shows its full duration
/// until a whole second has elapsed.
#[must_use]
pub fn timer_remaining_seconds(remaining_ms: u32) -> u64 {
    u64::from(remaining_ms).div_ceil(1000)
}

/// The text a timer shows for `remaining_ms` in `format`. `hidden` shows nothing.
#[must_use]
pub fn format_timer_text(remaining_ms: u32, format: TimerFormat) -> String {
    match format {
        TimerFormat::Hidden => String::new(),
        TimerFormat::Seconds => timer_remaining_seconds(remaining_ms).to_string(),
        TimerFormat::MmSs => {
            let seconds = timer_remaining_seconds(remaining_ms);
            format!("{:02}:{:02}", seconds / 60, seconds % 60)
        }
    }
}

/// How a [`ObjectKind::ProgressMeter`] fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterStyle {
    #[default]
    Bar,
    Ring,
}

fn is_default_meter_style(style: &MeterStyle) -> bool {
    *style == MeterStyle::Bar
}

/// Largest `content_height` a scrolling panel may store, in slide pixels.
pub const MAX_SCROLL_CONTENT_HEIGHT: f32 = 20_000.0;

fn deserialize_scroll_content_height<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    Ok(if value.is_finite() {
        value.clamp(0.0, MAX_SCROLL_CONTENT_HEIGHT)
    } else {
        0.0
    })
}

/// Clamp a panel scroll offset into `0..=` the content that can scroll.
///
/// The result is always finite. A non-finite offset becomes 0, and a panel
/// whose content fits its frame never scrolls.
#[must_use]
pub fn clamp_panel_offset(content_height: f32, viewport_height: f32, offset: f32) -> f32 {
    let content = if content_height.is_finite() {
        content_height.max(0.0)
    } else {
        0.0
    };
    let viewport = if viewport_height.is_finite() {
        viewport_height.max(0.0)
    } else {
        0.0
    };
    let max = (content - viewport).max(0.0);
    if offset.is_finite() {
        offset.clamp(0.0, max)
    } else {
        0.0
    }
}

/// Which input widget an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    TextEntry,
    Slider,
    Dial,
    Checkbox,
    RadioGroup,
}

impl ObjectKind {
    /// The input kind this object is, or `None` for non-input objects.
    #[must_use]
    pub fn input_kind(&self) -> Option<InputKind> {
        match self {
            Self::TextEntry { .. } => Some(InputKind::TextEntry),
            Self::Slider { .. } => Some(InputKind::Slider),
            Self::Dial { .. } => Some(InputKind::Dial),
            Self::Checkbox { .. } => Some(InputKind::Checkbox),
            Self::RadioGroup { .. } => Some(InputKind::RadioGroup),
            _ => None,
        }
    }

    /// The input's variable and the kind it must have.
    #[must_use]
    pub fn input_variable(&self) -> Option<(&str, ValueKind)> {
        match self {
            Self::TextEntry { variable, .. } | Self::RadioGroup { variable, .. } => {
                Some((variable.as_str(), ValueKind::Text))
            }
            Self::Slider { variable, .. }
            | Self::Dial { variable, .. }
            | Self::ProgressMeter { variable, .. } => Some((variable.as_str(), ValueKind::Number)),
            Self::Checkbox { variable, .. } => Some((variable.as_str(), ValueKind::Bool)),
            _ => None,
        }
    }

    /// The label an input announces to assistive technology when it has no
    /// author description. Empty for non-input objects.
    #[must_use]
    pub fn input_label(&self) -> Option<&str> {
        match self {
            Self::TextEntry { placeholder, .. } => Some(placeholder.as_str()),
            Self::Checkbox { label, .. } => Some(label.as_str()),
            Self::RadioGroup { .. } => None,
            Self::Slider { .. } => None,
            Self::Dial { .. } => None,
            _ => None,
        }
    }
}

/// Clamp `value` into `min..=max`, then snap it to `step` measured from `min`.
///
/// `step` of zero (or below) means continuous: only the clamp applies. The
/// result is always finite and inside the range when the inputs are.
#[must_use]
pub fn snap_to_step(min: f64, max: f64, step: f64, value: f64) -> f64 {
    if !value.is_finite() {
        return if min.is_finite() { min } else { 0.0 };
    }
    let (low, high) = if min.is_finite() && max.is_finite() && min <= max {
        (min, max)
    } else {
        (0.0, 0.0)
    };
    let clamped = value.clamp(low, high);
    if !step.is_finite() || step <= 0.0 {
        return clamped;
    }
    let steps = ((clamped - low) / step).round();
    let snapped = low + steps * step;
    if snapped.is_finite() {
        snapped.clamp(low, high)
    } else {
        clamped
    }
}

/// Most cue points one video object may have.
pub const MAX_CUES: usize = 200;

/// One interactive-video cue point, at `at_ms` milliseconds into the video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    pub id: String,
    pub at_ms: f64,
    pub name: String,
    /// Stop the video here and hold it until a trigger resumes it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pause: bool,
}

impl Cue {
    /// The cue time as whole milliseconds, clamped into the `u64` range.
    #[must_use]
    pub fn at_ms_u64(&self) -> u64 {
        if !self.at_ms.is_finite() || self.at_ms <= 0.0 {
            0
        } else if self.at_ms >= u64::MAX as f64 {
            u64::MAX
        } else {
            self.at_ms.round() as u64
        }
    }
}

/// Sort cues by time and repair a non-finite or negative time to 0, like the
/// model's other numeric fields. Authored order breaks ties.
pub fn sort_and_repair_cues(cues: &mut [Cue]) {
    for cue in cues.iter_mut() {
        cue.at_ms = if cue.at_ms.is_finite() {
            cue.at_ms.max(0.0)
        } else {
            0.0
        };
    }
    cues.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
}

fn deserialize_cues<'de, D>(deserializer: D) -> Result<Vec<Cue>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut cues = Vec::<Cue>::deserialize(deserializer)?;
    sort_and_repair_cues(&mut cues);
    Ok(cues)
}

/// Longest trim point an audio or video object may declare, in milliseconds
/// (24 hours).
pub const MAX_TRIM_MS: u64 = 86_400_000;

/// The kept window of an audio or video clip, in file time.
///
/// `start_ms` is where playback begins; `end_ms` is where it ends, or `None`
/// for the rest of the file. Stored on [`ObjectKind::Audio`] and
/// [`ObjectKind::Video`] and omitted from JSON when unset, so existing courses
/// stay byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trim {
    pub start_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

fn full_volume() -> f32 {
    1.0
}

fn is_full_volume(value: &f32) -> bool {
    !value.is_finite() || (*value - 1.0).abs() <= f32::EPSILON
}

fn half_volume() -> f32 {
    0.5
}

fn is_half_volume(value: &f32) -> bool {
    !value.is_finite() || (*value - 0.5).abs() <= f32::EPSILON
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectKind {
    Text {
        text: String,
        #[serde(default)]
        style: TextStyle,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        runs: Vec<TextRun>,
        /// Styles for the `\n`-separated paragraphs of `text`. Missing entries
        /// use the defaults; extra entries are ignored. Omitted when empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        paragraphs: Vec<Paragraph>,
        #[serde(default, skip_serializing_if = "is_default_autofit")]
        autofit: Autofit,
        /// Id of the named text style this box was last given. The box keeps
        /// its own concrete style; a link to a missing style is a warning.
        /// Omitted when unlinked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_style: Option<String>,
    },
    Shape {
        shape: ShapeKind,
        fill: Color,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_style: Option<FillStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outline: Option<Outline>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<ShapeText>,
        /// Per-kind adjustment values. Empty means the kind's defaults.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        adjust: Vec<f32>,
    },
    Image {
        src: String,
        #[serde(default)]
        alt: String,
        /// The part of the source picture the frame shows, in fractions of the
        /// source (0..=1). Omitted means the whole picture. See [`CropRect`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<CropRect>,
    },
    Audio {
        src: String,
        #[serde(default)]
        autoplay: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captions: Option<String>,
        /// Playback volume in `0..=1`; omitted when 1.
        #[serde(default = "full_volume", skip_serializing_if = "is_full_volume")]
        volume: f32,
        /// The kept part of the clip, in file time. Omitted when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trim: Option<Trim>,
    },
    Video {
        src: String,
        #[serde(default)]
        autoplay: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captions: Option<String>,
        /// Playback volume in `0..=1`; omitted when 1.
        #[serde(default = "full_volume", skip_serializing_if = "is_full_volume")]
        volume: f32,
        /// The kept part of the clip, in file time. Omitted when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trim: Option<Trim>,
        /// Cue points, sorted by `at_ms` on load. Omitted when empty.
        #[serde(
            default,
            deserialize_with = "deserialize_cues",
            skip_serializing_if = "Vec::is_empty"
        )]
        cues: Vec<Cue>,
    },
    /// A web page shown as an iframe in the published course.
    Web { source: WebSource },
    /// A field the learner types text into. Writes a text variable on commit.
    TextEntry {
        variable: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        placeholder: String,
        #[serde(default, skip_serializing_if = "is_false")]
        multiline: bool,
        /// Maximum characters accepted; `0` means no limit.
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        max_chars: u32,
    },
    /// A horizontal or vertical slider writing a number variable.
    Slider {
        variable: String,
        #[serde(deserialize_with = "deserialize_input_min")]
        min: f64,
        #[serde(deserialize_with = "deserialize_input_max")]
        max: f64,
        #[serde(default, deserialize_with = "deserialize_input_step")]
        step: f64,
        #[serde(default, skip_serializing_if = "is_default_orientation")]
        orientation: Orientation,
    },
    /// A knob writing a number variable. Angles are clockwise degrees from 12 o'clock.
    Dial {
        variable: String,
        #[serde(deserialize_with = "deserialize_input_min")]
        min: f64,
        #[serde(deserialize_with = "deserialize_input_max")]
        max: f64,
        #[serde(default, deserialize_with = "deserialize_input_step")]
        step: f64,
        #[serde(
            default = "default_start_angle",
            deserialize_with = "deserialize_start_angle",
            skip_serializing_if = "is_default_start_angle"
        )]
        start_angle: f32,
        #[serde(
            default = "default_end_angle",
            deserialize_with = "deserialize_end_angle",
            skip_serializing_if = "is_default_end_angle"
        )]
        end_angle: f32,
    },
    /// A true/false checkbox writing a bool variable.
    Checkbox {
        variable: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
    },
    /// A single-choice group writing the selected option's value to a text variable.
    RadioGroup {
        variable: String,
        options: Vec<RadioOption>,
    },
    /// A pulsing dot that opens a small rich-text label when toggled.
    Marker {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
        /// The label's body, using the same runs and paragraphs model as a text box.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        body: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        runs: Vec<TextRun>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        paragraphs: Vec<Paragraph>,
        #[serde(default, skip_serializing_if = "is_default_marker_icon")]
        icon: MarkerIcon,
        /// Dot colour; the theme accent when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
    },
    /// A frame whose child objects scroll inside it.
    ScrollPanel {
        /// Total scrollable content height in slide pixels.
        #[serde(deserialize_with = "deserialize_scroll_content_height")]
        content_height: f32,
        #[serde(default, skip_serializing_if = "is_default_scroll_bar")]
        bar: ScrollBar,
    },
    /// A countdown. Counts down from `duration_ms` in `coursecraft-logic`
    /// while its owning timeline runs; autostart begins at `timing.start_ms`.
    Timer {
        #[serde(
            default = "default_timer_duration",
            deserialize_with = "deserialize_timer_duration"
        )]
        duration_ms: u32,
        #[serde(default, skip_serializing_if = "is_default_timer_format")]
        format: TimerFormat,
        /// Start when the slide clock reaches the object's start time.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        autostart: bool,
        /// The digits' text style, like a text box's.
        #[serde(default)]
        style: TextStyle,
    },
    /// A bar or ring showing a number variable's value between `min` and `max`.
    ProgressMeter {
        variable: String,
        #[serde(deserialize_with = "deserialize_input_min")]
        min: f64,
        #[serde(deserialize_with = "deserialize_input_max")]
        max: f64,
        #[serde(default, skip_serializing_if = "is_default_meter_style")]
        style: MeterStyle,
        /// Draw the percentage in the middle.
        #[serde(default, skip_serializing_if = "is_false")]
        show_value: bool,
    },
    /// A Lottie animation played through the shared media runtime.
    Lottie {
        /// A `.json` or `.lottie` file under `media/`.
        src: String,
        /// Loop playback (`true` by default, omitted when true).
        #[serde(rename = "loop", default = "yes", skip_serializing_if = "is_true")]
        loop_playback: bool,
        /// Start when the timeline reaches the object's `start_ms`
        /// (`true` by default, omitted when true).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        autoplay: bool,
        /// Playback rate, `0.1..=4`; omitted when 1.
        #[serde(
            default = "default_speed",
            deserialize_with = "deserialize_speed",
            skip_serializing_if = "is_default_speed"
        )]
        speed: f32,
    },
    Button {
        label: String,
        #[serde(default = "button_fill")]
        fill: Color,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_style: Option<FillStyle>,
    },
    Path {
        #[serde(flatten)]
        path: Path,
    },
    /// A grid of rich-text cells. Column widths and row heights are fractions
    /// summing to 1, normalised on load.
    Table {
        #[serde(default, deserialize_with = "deserialize_table_columns")]
        columns: Vec<f32>,
        #[serde(default, deserialize_with = "deserialize_table_rows")]
        rows: Vec<TableRow>,
        #[serde(default, skip_serializing_if = "TableBorders::is_empty")]
        borders: TableBorders,
        #[serde(default, skip_serializing_if = "is_false")]
        header_row: bool,
    },
}

/// One row of a table. `height` is its fraction of the table's height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableRow {
    #[serde(default = "one_f32")]
    pub height: f32,
    #[serde(default)]
    pub cells: Vec<TableCell>,
}

/// One table cell with the same rich text model as a text box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableCell {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<TextRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<Paragraph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default = "one_u8", skip_serializing_if = "is_one_u8")]
    pub row_span: u8,
    #[serde(default = "one_u8", skip_serializing_if = "is_one_u8")]
    pub col_span: u8,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub valign: VAlign,
}

impl Default for TableCell {
    fn default() -> Self {
        Self {
            text: String::new(),
            style: TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            fill: None,
            row_span: 1,
            col_span: 1,
            valign: VAlign::Top,
        }
    }
}

/// Outer and inner borders of a table. A missing border draws nothing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableBorders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer: Option<Outline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<Outline>,
}

impl TableBorders {
    /// True when the table draws no outer or inner border.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outer.is_none() && self.inner.is_none()
    }
}

fn one_u8() -> u8 {
    1
}

fn is_one_u8(value: &u8) -> bool {
    *value == 1
}

/// The largest number of rows or columns a table may have.
pub const TABLE_MAX_AXIS: usize = 50;
/// The largest number of cells (rows × columns) a table may have.
pub const TABLE_MAX_CELLS: usize = 2500;
/// The smallest fraction a column or row may keep while resizing.
pub const TABLE_MIN_FRACTION: f32 = 0.05;

/// Repair table fractions in place: non-finite and negative values become 0,
/// and all values are scaled to sum to 1. An empty or all-zero list becomes
/// equal fractions.
pub fn normalize_table_fractions(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    for value in values.iter_mut() {
        if !value.is_finite() || *value < 0.0 {
            *value = 0.0;
        }
    }
    let sum: f64 = values.iter().map(|value| f64::from(*value)).sum();
    if !sum.is_finite() || sum <= f64::EPSILON {
        let equal = 1.0 / values.len() as f32;
        for value in values.iter_mut() {
            *value = equal;
        }
    } else {
        for value in values.iter_mut() {
            *value = (f64::from(*value) / sum) as f32;
        }
    }
    // Make the rounded values sum to exactly 1 by putting the remainder on the
    // last positive entry. This keeps every serializer round-trip stable.
    let total: f32 = values.iter().copied().sum();
    if let Some(last) = values.iter_mut().rev().find(|value| **value > 0.0) {
        *last = (*last + (1.0 - total)).max(0.0);
    }
}

/// Start and end fractions of each value, normalised defensively.
#[must_use]
pub fn table_fraction_offsets(values: &[f32]) -> Vec<(f32, f32)> {
    let mut clean: Vec<f32> = values
        .iter()
        .map(|value| {
            if value.is_finite() && *value > 0.0 {
                *value
            } else {
                0.0
            }
        })
        .collect();
    normalize_table_fractions(&mut clean);
    let mut out = Vec::with_capacity(clean.len());
    let mut at = 0.0;
    for value in clean {
        let end = (at + value).min(1.0);
        out.push((at, end));
        at = end;
    }
    out
}

/// For every grid slot, the anchor `(row, column)` of the cell that owns it.
/// A slot whose owner is not itself is covered by a merged cell and is kept
/// but not drawn.
#[must_use]
pub fn table_cell_owners(rows: &[TableRow], columns: usize) -> Vec<Vec<Option<(usize, usize)>>> {
    let mut owners = vec![vec![None; columns]; rows.len()];
    for row in 0..rows.len() {
        for column in 0..columns {
            if owners
                .get(row)
                .and_then(|line| line.get(column))
                .is_some_and(Option::is_some)
            {
                continue;
            }
            let cell = rows
                .get(row)
                .and_then(|line| line.cells.get(column))
                .cloned()
                .unwrap_or_default();
            let row_span = usize::from(cell.row_span.max(1));
            let col_span = usize::from(cell.col_span.max(1));
            for covered_row in row..rows.len().min(row.saturating_add(row_span)) {
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

/// Whether a cell should be drawn (the anchor of and not covered by a merge).
#[must_use]
pub fn table_cell_is_visible(rows: &[TableRow], columns: usize, row: usize, column: usize) -> bool {
    table_cell_owners(rows, columns)
        .get(row)
        .and_then(|line| line.get(column))
        .is_some_and(|owner| *owner == Some((row, column)))
}

fn deserialize_table_columns<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut values = Vec::<f32>::deserialize(deserializer)?;
    normalize_table_fractions(&mut values);
    Ok(values)
}

fn deserialize_table_rows<'de, D>(deserializer: D) -> Result<Vec<TableRow>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut rows = Vec::<TableRow>::deserialize(deserializer)?;
    let mut heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    normalize_table_fractions(&mut heights);
    for (row, height) in rows.iter_mut().zip(heights) {
        row.height = height;
    }
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Line through the text. `None` inherits the style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    /// Superscript or subscript. `None` inherits the style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<Baseline>,
    /// Extra space after each letter in points. `None` inherits the style.
    #[serde(
        default,
        deserialize_with = "deserialize_run_letter_spacing",
        skip_serializing_if = "Option::is_none"
    )]
    pub letter_spacing: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// A URL or slide this run links to. Omitted when the run is not a link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// Font family for this run. Omitted when it inherits the paragraph style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// Where a text run links to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Link {
    /// An external address. Must start with `http://`, `https://` or `mailto:`.
    Url { url: String },
    /// A slide id in this course.
    Slide { slide: String },
}

impl Link {
    /// Whether the link is valid for `course`: a safe external URL, or a slide
    /// that exists.
    #[must_use]
    pub fn is_valid(&self, course: &Course) -> bool {
        match self {
            Self::Url { url } => is_valid_link_url(url),
            Self::Slide { slide } => course.play_slide(slide).is_some(),
        }
    }
}

/// Whether `url` is an external link this player may open.
#[must_use]
pub fn is_valid_link_url(url: &str) -> bool {
    if url.len() > 2048 || url.chars().any(char::is_whitespace) || url.chars().any(char::is_control)
    {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("mailto:") {
        rest
    } else {
        return false;
    };
    !rest.is_empty()
}

/// What a paragraph does before its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    #[default]
    None,
    Bullet,
    Number,
}

fn is_default_list_kind(list: &ListKind) -> bool {
    *list == ListKind::None
}

/// Paragraph formatting. Entry `i` of `text.paragraphs` styles the `i`-th
/// `\n`-separated paragraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "is_default_list_kind")]
    pub list: ListKind,
    /// Indent level, `0..=4`. Omitted when 0.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub level: u8,
    /// Line height multiple, `0.8..=3`. Omitted when 1.
    #[serde(default = "one_f32", skip_serializing_if = "is_one_f32")]
    pub line_spacing: f32,
    /// Space above the paragraph in slide pixels, `0..=200`. Omitted when 0.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub space_before: f32,
    /// Space below the paragraph in slide pixels, `0..=200`. Omitted when 0.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub space_after: f32,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            list: ListKind::None,
            level: 0,
            line_spacing: 1.0,
            space_before: 0.0,
            space_after: 0.0,
        }
    }
}

/// How text shrinks when it does not fit its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autofit {
    #[default]
    None,
    Shrink,
}

fn is_default_autofit(autofit: &Autofit) -> bool {
    *autofit == Autofit::None
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_f32(value: &f32) -> bool {
    *value == 0.0
}

fn one_f32() -> f32 {
    1.0
}

fn is_one_f32(value: &f32) -> bool {
    *value == 1.0
}

/// A shape outline. A solid band centred on the shape's edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub color: Color,
    /// Width in slide pixels. Validation allows `0..=50`.
    pub width: f32,
    #[serde(default)]
    pub dash: DashStyle,
}

/// How an outline is broken along its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashStyle {
    #[default]
    Solid,
    Dash,
    Dot,
}

/// How many points a path needs and may carry.
pub const PATH_MIN_POINTS: usize = 2;
/// Upper bound on points in one path, including a hostile file.
pub const PATH_MAX_POINTS: usize = 2000;

/// One point of a [`Path`], as a fraction of its frame (`0..=1`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPoint {
    pub x: f32,
    pub y: f32,
}

impl PathPoint {
    /// Whether this point is finite and inside its frame.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && (0.0..=1.0).contains(&self.x)
            && (0.0..=1.0).contains(&self.y)
    }
}

/// How a path joins its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathStyle {
    /// Straight segments between points.
    #[default]
    Straight,
    /// A Catmull-Rom curve through the points.
    Curved,
}

/// A decoration at the start or end of an open path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnd {
    #[default]
    None,
    /// A filled arrowhead.
    Arrow,
    /// Two strokes in a V, not filled.
    OpenArrow,
    /// A filled dot.
    Dot,
    /// A filled diamond.
    Diamond,
}

/// How a connector routes between its two ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStyle {
    Straight,
    Elbow,
    Curved,
}

/// One end of a connector attached to an object's frame.
///
/// `site` is one of four points on the target's frame: 0 top centre, 1 right
/// centre, 2 bottom centre, 3 left centre. Other values are read as the nearest
/// valid one (`site % 4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glue {
    /// Id of the target object in the same owner (base or one layer).
    pub object: String,
    pub site: u8,
}

impl Glue {
    /// The site normalised to `0..=3`.
    #[must_use]
    pub fn site(&self) -> u8 {
        self.site % 4
    }
}

fn is_default_path_style(style: &PathStyle) -> bool {
    *style == PathStyle::Straight
}

fn is_default_line_end(end: &LineEnd) -> bool {
    *end == LineEnd::None
}

/// A drawn line, curve, freeform or connector.
///
/// Points are fractions of the object's frame, so resizing the frame scales the
/// path. The frame itself may be as narrow as one pixel on one axis, which is how
/// a horizontal or vertical line is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub points: Vec<PathPoint>,
    #[serde(default, skip_serializing_if = "is_default_path_style")]
    pub style: PathStyle,
    #[serde(default, skip_serializing_if = "is_false")]
    pub closed: bool,
    /// Fill colour; only present on a closed path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    pub outline: Outline,
    #[serde(default, skip_serializing_if = "is_default_line_end")]
    pub start_end: LineEnd,
    #[serde(default, skip_serializing_if = "is_default_line_end")]
    pub end_end: LineEnd,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorStyle>,
    /// Target of the start end, only used by a connector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_glue: Option<Glue>,
    /// Target of the end end, only used by a connector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_glue: Option<Glue>,
}

impl Path {
    /// Whether the points and colours can be drawn: 2..=2000 finite points inside
    /// the frame, a usable outline, and a fill only when closed.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        (PATH_MIN_POINTS..=PATH_MAX_POINTS).contains(&self.points.len())
            && self.points.iter().all(PathPoint::is_valid)
            && self
                .fill
                .as_ref()
                .is_none_or(|fill| self.closed && fill.is_valid())
            && self.outline.color.is_valid()
            && self.outline.width.is_finite()
            && (0.0..=50.0).contains(&self.outline.width)
    }

    /// The point in slide coordinates for a point in this path's frame.
    #[must_use]
    pub fn slide_point(&self, frame: &Rect, index: usize) -> Option<Point> {
        let point = self.points.get(index)?;
        Some(Point {
            x: frame.x + point.x * frame.w,
            y: frame.y + point.y * frame.h,
        })
    }

    /// Every point in slide coordinates, using `frame`.
    #[must_use]
    pub fn slide_points(&self, frame: &Rect) -> Vec<Point> {
        self.points
            .iter()
            .map(|point| Point {
                x: frame.x + point.x * frame.w,
                y: frame.y + point.y * frame.h,
            })
            .collect()
    }
}

/// Compute a path's frame and fractional points from points in slide coordinates.
///
/// A zero-width or zero-height span still produces a one-pixel frame: the points
/// sit on the frame's centre line, so a horizontal or vertical line is valid.
/// Returns `None` for fewer than two points, too many points, or non-finite
/// coordinates.
#[must_use]
pub fn path_frame_and_points(points: &[Point]) -> Option<(Rect, Vec<PathPoint>)> {
    if points.len() < PATH_MIN_POINTS || points.len() > PATH_MAX_POINTS {
        return None;
    }
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for point in points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    let span_x = max_x - min_x;
    let span_y = max_y - min_y;
    if !span_x.is_finite() || !span_y.is_finite() {
        return None;
    }
    let (x, w) = if span_x > 1e-6 {
        (min_x, span_x)
    } else {
        (min_x - 0.5, 1.0)
    };
    let (y, h) = if span_y > 1e-6 {
        (min_y, span_y)
    } else {
        (min_y - 0.5, 1.0)
    };
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0
    {
        return None;
    }
    let fractions: Vec<PathPoint> = points
        .iter()
        .map(|point| PathPoint {
            x: ((point.x - x) / w).clamp(0.0, 1.0),
            y: ((point.y - y) / h).clamp(0.0, 1.0),
        })
        .collect();
    Some((Rect { x, y, w, h }, fractions))
}

/// Text carried inside a shape. It moves, translates and imports with the shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeText {
    pub text: String,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<TextRun>,
    /// Paragraph styles, indexed by `\n`-separated paragraph. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<Paragraph>,
    #[serde(default, skip_serializing_if = "is_default_autofit")]
    pub autofit: Autofit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

fn is_default_align(a: &Align) -> bool {
    *a == Align::Left
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

fn is_default_valign(v: &VAlign) -> bool {
    *v == VAlign::Top
}

fn button_fill() -> Color {
    Color("#2F6FEB".to_owned())
}

/// A shape kind. JSON names are snake_case (`rounded_rectangle`, `arrow_right`).
///
/// The labels, categories and DrawingML names live in [`crate::SHAPE_CATALOG`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    // Lines
    LineHorizontal,
    LineVertical,
    LineDiagonalDown,
    LineDiagonalUp,
    LineArrowRight,
    LineArrowLeft,
    LineArrowUp,
    LineArrowDown,
    LineArrowBothHorizontal,
    LineArrowBothVertical,
    // Rectangles
    Rectangle,
    RoundedRectangle,
    SnipSingleCorner,
    SnipSameSide,
    SnipDiagonal,
    SnipRound,
    RoundSingleCorner,
    RoundSameSide,
    RoundDiagonal,
    // Basic shapes
    Ellipse,
    Triangle,
    RightTriangle,
    Parallelogram,
    Trapezoid,
    Diamond,
    Pentagon,
    Hexagon,
    Heptagon,
    Octagon,
    Decagon,
    Dodecagon,
    Pie,
    Chord,
    Teardrop,
    Frame,
    HalfFrame,
    Corner,
    DiagonalStripe,
    Plus,
    Plaque,
    Can,
    Cube,
    Bevel,
    Donut,
    NoSymbol,
    BlockArc,
    FoldedCorner,
    Smiley,
    Heart,
    LightningBolt,
    Sun,
    Moon,
    Cloud,
    Arc,
    BracketPair,
    BracePair,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    // Block arrows
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    ArrowLeftRight,
    ArrowUpDown,
    ArrowQuad,
    ArrowLeftRightUp,
    ArrowBent,
    ArrowUTurn,
    ArrowLeftUp,
    ArrowBentUp,
    ArrowCurvedRight,
    ArrowCurvedLeft,
    ArrowCurvedUp,
    ArrowCurvedDown,
    ArrowStripedRight,
    ArrowNotchedRight,
    ArrowPentagon,
    ArrowChevron,
    CalloutRightArrow,
    CalloutDownArrow,
    CalloutLeftArrow,
    CalloutUpArrow,
    CalloutLeftRightArrow,
    CalloutQuadArrow,
    ArrowCircular,
    // Equation shapes
    MathPlus,
    MathMinus,
    MathMultiply,
    MathDivide,
    MathEqual,
    MathNotEqual,
    // Flowchart
    FlowProcess,
    FlowAlternateProcess,
    FlowDecision,
    FlowInputOutput,
    FlowPredefinedProcess,
    FlowInternalStorage,
    FlowDocument,
    FlowMultidocument,
    FlowTerminator,
    FlowPreparation,
    FlowManualInput,
    FlowManualOperation,
    FlowConnector,
    FlowOffpageConnector,
    FlowCard,
    FlowPunchedTape,
    FlowSummingJunction,
    FlowOr,
    FlowCollate,
    FlowSort,
    FlowExtract,
    FlowMerge,
    FlowStoredData,
    FlowDelay,
    FlowSequentialAccess,
    FlowMagneticDisk,
    FlowDirectAccess,
    FlowDisplay,
    // Stars and banners
    #[serde(rename = "explosion_1")]
    Explosion1,
    #[serde(rename = "explosion_2")]
    Explosion2,
    #[serde(rename = "star_4")]
    Star4,
    #[serde(rename = "star_5")]
    Star5,
    #[serde(rename = "star_6")]
    Star6,
    #[serde(rename = "star_7")]
    Star7,
    #[serde(rename = "star_8")]
    Star8,
    #[serde(rename = "star_10")]
    Star10,
    #[serde(rename = "star_12")]
    Star12,
    #[serde(rename = "star_16")]
    Star16,
    #[serde(rename = "star_24")]
    Star24,
    #[serde(rename = "star_32")]
    Star32,
    RibbonUp,
    RibbonDown,
    CurvedRibbonUp,
    CurvedRibbonDown,
    ScrollVertical,
    ScrollHorizontal,
    Wave,
    DoubleWave,
    // Callouts
    CalloutRectangle,
    CalloutRounded,
    CalloutOval,
    CalloutCloud,
    CalloutLine,
    CalloutLineBent,
    CalloutLineDoubleBent,
    // Action buttons
    ActionBlank,
    ActionHome,
    ActionHelp,
    ActionInformation,
    ActionBack,
    ActionForward,
    ActionBeginning,
    ActionEnd,
    ActionReturn,
    ActionDocument,
    ActionSound,
    ActionMovie,
    // Learning
    CheckMark,
    CrossMark,
    CheckCircle,
    CrossCircle,
    InfoCircle,
    QuestionCircle,
    WarningTriangle,
    Lightbulb,
    Target,
    Trophy,
    Medal,
    Flag,
    LocationPin,
    Clock,
    Lock,
    Key,
    Shield,
    Book,
    GraduationCap,
    Person,
    People,
    ThumbsUp,
    SpeechBubble,
    ThoughtBubble,
    PlayCircle,
    PauseCircle,
    Gear,
    Magnifier,
    StarBadge,
}

impl ShapeKind {
    /// Every kind, in catalog order.
    pub const ALL: &'static [Self] = &[
        Self::LineHorizontal,
        Self::LineVertical,
        Self::LineDiagonalDown,
        Self::LineDiagonalUp,
        Self::LineArrowRight,
        Self::LineArrowLeft,
        Self::LineArrowUp,
        Self::LineArrowDown,
        Self::LineArrowBothHorizontal,
        Self::LineArrowBothVertical,
        Self::Rectangle,
        Self::RoundedRectangle,
        Self::SnipSingleCorner,
        Self::SnipSameSide,
        Self::SnipDiagonal,
        Self::SnipRound,
        Self::RoundSingleCorner,
        Self::RoundSameSide,
        Self::RoundDiagonal,
        Self::Ellipse,
        Self::Triangle,
        Self::RightTriangle,
        Self::Parallelogram,
        Self::Trapezoid,
        Self::Diamond,
        Self::Pentagon,
        Self::Hexagon,
        Self::Heptagon,
        Self::Octagon,
        Self::Decagon,
        Self::Dodecagon,
        Self::Pie,
        Self::Chord,
        Self::Teardrop,
        Self::Frame,
        Self::HalfFrame,
        Self::Corner,
        Self::DiagonalStripe,
        Self::Plus,
        Self::Plaque,
        Self::Can,
        Self::Cube,
        Self::Bevel,
        Self::Donut,
        Self::NoSymbol,
        Self::BlockArc,
        Self::FoldedCorner,
        Self::Smiley,
        Self::Heart,
        Self::LightningBolt,
        Self::Sun,
        Self::Moon,
        Self::Cloud,
        Self::Arc,
        Self::BracketPair,
        Self::BracePair,
        Self::LeftBracket,
        Self::RightBracket,
        Self::LeftBrace,
        Self::RightBrace,
        Self::ArrowRight,
        Self::ArrowLeft,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::ArrowLeftRight,
        Self::ArrowUpDown,
        Self::ArrowQuad,
        Self::ArrowLeftRightUp,
        Self::ArrowBent,
        Self::ArrowUTurn,
        Self::ArrowLeftUp,
        Self::ArrowBentUp,
        Self::ArrowCurvedRight,
        Self::ArrowCurvedLeft,
        Self::ArrowCurvedUp,
        Self::ArrowCurvedDown,
        Self::ArrowStripedRight,
        Self::ArrowNotchedRight,
        Self::ArrowPentagon,
        Self::ArrowChevron,
        Self::CalloutRightArrow,
        Self::CalloutDownArrow,
        Self::CalloutLeftArrow,
        Self::CalloutUpArrow,
        Self::CalloutLeftRightArrow,
        Self::CalloutQuadArrow,
        Self::ArrowCircular,
        Self::MathPlus,
        Self::MathMinus,
        Self::MathMultiply,
        Self::MathDivide,
        Self::MathEqual,
        Self::MathNotEqual,
        Self::FlowProcess,
        Self::FlowAlternateProcess,
        Self::FlowDecision,
        Self::FlowInputOutput,
        Self::FlowPredefinedProcess,
        Self::FlowInternalStorage,
        Self::FlowDocument,
        Self::FlowMultidocument,
        Self::FlowTerminator,
        Self::FlowPreparation,
        Self::FlowManualInput,
        Self::FlowManualOperation,
        Self::FlowConnector,
        Self::FlowOffpageConnector,
        Self::FlowCard,
        Self::FlowPunchedTape,
        Self::FlowSummingJunction,
        Self::FlowOr,
        Self::FlowCollate,
        Self::FlowSort,
        Self::FlowExtract,
        Self::FlowMerge,
        Self::FlowStoredData,
        Self::FlowDelay,
        Self::FlowSequentialAccess,
        Self::FlowMagneticDisk,
        Self::FlowDirectAccess,
        Self::FlowDisplay,
        Self::Explosion1,
        Self::Explosion2,
        Self::Star4,
        Self::Star5,
        Self::Star6,
        Self::Star7,
        Self::Star8,
        Self::Star10,
        Self::Star12,
        Self::Star16,
        Self::Star24,
        Self::Star32,
        Self::RibbonUp,
        Self::RibbonDown,
        Self::CurvedRibbonUp,
        Self::CurvedRibbonDown,
        Self::ScrollVertical,
        Self::ScrollHorizontal,
        Self::Wave,
        Self::DoubleWave,
        Self::CalloutRectangle,
        Self::CalloutRounded,
        Self::CalloutOval,
        Self::CalloutCloud,
        Self::CalloutLine,
        Self::CalloutLineBent,
        Self::CalloutLineDoubleBent,
        Self::ActionBlank,
        Self::ActionHome,
        Self::ActionHelp,
        Self::ActionInformation,
        Self::ActionBack,
        Self::ActionForward,
        Self::ActionBeginning,
        Self::ActionEnd,
        Self::ActionReturn,
        Self::ActionDocument,
        Self::ActionSound,
        Self::ActionMovie,
        Self::CheckMark,
        Self::CrossMark,
        Self::CheckCircle,
        Self::CrossCircle,
        Self::InfoCircle,
        Self::QuestionCircle,
        Self::WarningTriangle,
        Self::Lightbulb,
        Self::Target,
        Self::Trophy,
        Self::Medal,
        Self::Flag,
        Self::LocationPin,
        Self::Clock,
        Self::Lock,
        Self::Key,
        Self::Shield,
        Self::Book,
        Self::GraduationCap,
        Self::Person,
        Self::People,
        Self::ThumbsUp,
        Self::SpeechBubble,
        Self::ThoughtBubble,
        Self::PlayCircle,
        Self::PauseCircle,
        Self::Gear,
        Self::Magnifier,
        Self::StarBadge,
    ];
}

/// `#RRGGBB` or `#RRGGBBAA`. Checked by validation, never trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Color(pub String);

impl<'de> Deserialize<'de> for Color {
    /// Reads the text as is, except while a course-wide recolour is running
    /// (see `recolor_course`): then every colour passes through its rules, so
    /// each field of type `Color` is covered without naming it.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Ok(Self(crate::recolor::map_color(text)))
    }
}

impl Color {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let s = self.0.as_str();
        s.strip_prefix('#')
            .is_some_and(|h| matches!(h.len(), 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default = "default_text_color")]
    pub color: Color,
    #[serde(default)]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "is_normal_baseline")]
    pub baseline: Baseline,
    /// Extra space after each letter in points, `-5..=50`. Omitted when 0.
    #[serde(
        default,
        deserialize_with = "deserialize_letter_spacing",
        skip_serializing_if = "is_zero_spacing"
    )]
    pub letter_spacing: f32,
    #[serde(default, skip_serializing_if = "is_default_align")]
    pub align: Align,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub valign: VAlign,
}

/// Where text sits relative to the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Baseline {
    #[default]
    Normal,
    Superscript,
    Subscript,
}

/// Smallest and largest letter spacing, in points.
pub const MIN_LETTER_SPACING: f32 = -5.0;
pub const MAX_LETTER_SPACING: f32 = 50.0;

/// Repair a letter spacing: non-finite becomes 0, the rest is clamped.
#[must_use]
pub fn clamp_letter_spacing(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_LETTER_SPACING, MAX_LETTER_SPACING)
    } else {
        0.0
    }
}

fn deserialize_letter_spacing<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(clamp_letter_spacing(f32::deserialize(deserializer)?))
}

fn deserialize_run_letter_spacing<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f32>::deserialize(deserializer)?.map(clamp_letter_spacing))
}

fn is_normal_baseline(b: &Baseline) -> bool {
    *b == Baseline::Normal
}

fn is_zero_spacing(v: &f32) -> bool {
    *v == 0.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_default_orientation(value: &Orientation) -> bool {
    *value == Orientation::Horizontal
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

fn is_zero_i32(n: &i32) -> bool {
    *n == 0
}

fn is_all_or_nothing(s: &Scoring) -> bool {
    *s == Scoring::AllOrNothing
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: default_font(),
            size: default_size(),
            color: default_text_color(),
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            baseline: Baseline::Normal,
            letter_spacing: 0.0,
            align: Align::Left,
            valign: VAlign::Top,
        }
    }
}

fn default_font() -> String {
    "Open Sans".to_owned()
}
fn default_size() -> f32 {
    20.0
}
fn default_text_color() -> Color {
    Color("#1F1F1F".to_owned())
}

/// Course visual theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(default = "default_accent_color")]
    pub accent: Color,
    #[serde(default = "default_background_color")]
    pub background: Color,
    #[serde(default = "default_theme_text_color")]
    pub text: Color,
    #[serde(default = "default_theme_font")]
    pub font: String,
    #[serde(default)]
    pub player: PlayerTheme,
    /// Named colours the editor offers first in every colour picker, up to
    /// [`MAX_PALETTE_COLORS`]. Objects keep their own concrete colours; the
    /// palette is a list to pick from and to recolour the course by. Omitted
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette: Vec<PaletteColor>,
}

/// The most colours a theme palette holds.
pub const MAX_PALETTE_COLORS: usize = 12;

/// The most named text styles a course holds.
pub const MAX_TEXT_STYLES: usize = 50;

/// One named colour in the theme palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteColor {
    /// Stable id, `[A-Za-z0-9_-]`, unique in the palette.
    pub id: String,
    /// What the editor shows, such as "Brand blue".
    pub name: String,
    pub color: Color,
}

/// A named text style a text object can link to. Applying one copies its look
/// into the object, so the renderer and player never read it; the link lets
/// the editor re-apply a changed style to every object that uses it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedTextStyle {
    /// Stable id, `[A-Za-z0-9_-]`, unique among the course's text styles.
    pub id: String,
    /// What the editor shows, such as "Heading".
    pub name: String,
    /// Font, size, colour, bold, italic, underline, strikethrough, baseline,
    /// letter spacing, align and valign.
    pub style: TextStyle,
    /// Paragraph formatting: line spacing and spacing before and after always
    /// apply; the list kind and indent level apply only when they are not the
    /// defaults. Omitted when the style leaves paragraphs alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraphs: Option<Paragraph>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: default_accent_color(),
            background: default_background_color(),
            text: default_theme_text_color(),
            font: default_theme_font(),
            player: PlayerTheme::default(),
            palette: Vec::new(),
        }
    }
}

fn is_default_theme(t: &Theme) -> bool {
    t == &Theme::default()
}

pub fn default_accent_color() -> Color {
    Color("#2F6FEB".to_owned())
}

pub fn default_background_color() -> Color {
    Color("#FFFFFF".to_owned())
}

pub fn default_theme_text_color() -> Color {
    Color("#1F1F1F".to_owned())
}

pub fn default_theme_font() -> String {
    "Inter".to_owned()
}

/// Player chrome theme colours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerTheme {
    #[serde(default = "default_bar_color")]
    pub bar: Color,
    #[serde(default = "default_bar_text_color")]
    pub bar_text: Color,
}

impl Default for PlayerTheme {
    fn default() -> Self {
        Self {
            bar: default_bar_color(),
            bar_text: default_bar_text_color(),
        }
    }
}

pub fn default_bar_color() -> Color {
    Color("#1B1B1B".to_owned())
}

pub fn default_bar_text_color() -> Color {
    Color("#FFFFFF".to_owned())
}

/// When an object is on its layer's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    #[serde(default)]
    pub start_ms: u32,
    /// `None` = until the end of the slide (or layer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u32>,
}

/// Property overrides applied while an object is in a named state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
}

// ---------------------------------------------------------------- triggers

/// "Do `action` when `when` happens, if `condition` holds."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub action: Action,
    pub when: Event,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    /// Never fires and never evaluates its condition. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    /// A timeline starts. `layer: None` = the slide's base timeline.
    TimelineStarts {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<String>,
    },
    TimelineEnds {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<String>,
    },
    TimelineReaches {
        ms: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<String>,
    },
    Click {
        object: String,
    },
    HoverStart {
        object: String,
    },
    HoverEnd {
        object: String,
    },
    VariableChanges {
        variable: String,
    },
    /// An object's state changes; `to: None` = any change.
    StateChanges {
        object: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    MediaEnds {
        object: String,
    },
    /// Playback crossed a video object's cue point going forward.
    CueReached {
        object: String,
        cue: String,
    },
    KeyPressed {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        object: Option<String>,
    },
    /// The learner dropped a draggable object on a target.
    DroppedOn {
        object: String,
        target: String,
    },
    /// Two objects' frames overlap.
    ObjectsIntersect {
        a: String,
        b: String,
    },
    /// Two objects' frames stop overlapping.
    ObjectsStopIntersecting {
        a: String,
        b: String,
    },
    /// The learner presses Submit on a question slide.
    Submit,
    /// An object's entrance animation or motion path finishes.
    AnimationCompletes {
        object: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    /// A marker's label opened.
    MarkerOpened {
        object: String,
    },
    /// A marker's label closed.
    MarkerClosed {
        object: String,
    },
    /// A timer's countdown reached zero.
    TimerEnds {
        timer: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    JumpToSlide {
        target: SlideTarget,
    },
    ShowLayer {
        layer: String,
    },
    /// `layer: None` = the layer that owns this trigger.
    HideLayer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<String>,
    },
    ChangeState {
        object: String,
        state: String,
    },
    SetVariable {
        variable: String,
        #[serde(default)]
        op: VarOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    PlayMedia {
        object: String,
    },
    PauseMedia {
        object: String,
    },
    StopMedia {
        object: String,
    },
    MoveOnPath {
        object: String,
        path: String,
    },
    PauseTimeline,
    ResumeTimeline,
    /// Grade the current slide's question.
    SubmitQuestion,
    /// Compute the quiz score and report pass/fail.
    SubmitResults,
    /// Clear all graded answers and start the quiz again.
    RetryQuiz,
    /// Walk the answered questions with the learner's answers restored.
    ReviewQuiz,
    /// Leave answer review and return to where review started.
    ExitReview,
    /// Ask the host to print the current slide.
    PrintSlide,
    CompleteCourse {
        status: CompletionStatus,
    },
    ExitCourse,
    OpenUrl {
        url: String,
    },
    /// Run code in the published web player. Other hosts ignore it.
    #[serde(rename = "run_javascript")]
    RunJavaScript {
        code: String,
    },
    /// Show `slide` over the current slide in a lightbox.
    LightboxSlide {
        slide: String,
    },
    /// Close the open lightbox, if there is one.
    CloseLightbox,
    /// Start a timer's countdown (no effect on an ended timer).
    StartTimer {
        timer: String,
    },
    /// Pause a timer's countdown, keeping its remaining time.
    PauseTimer {
        timer: String,
    },
    /// Return a timer to its full duration, stopped.
    ResetTimer {
        timer: String,
    },
}

/// Most bytes a `run_javascript` action may carry (64 KiB).
pub const MAX_JAVASCRIPT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideTarget {
    Next,
    Previous,
    Id(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarOp {
    #[default]
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Flip a boolean; `value` is ignored.
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Passed,
    Failed,
    Completed,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    Compare {
        variable: String,
        op: CmpOp,
        value: Value,
    },
    ObjectState {
        object: String,
        state: String,
        /// `false` = "is not in state".
        #[serde(default = "yes")]
        is: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Text contains (case-sensitive).
    Contains,
}

// ---------------------------------------------------------------- questions

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub kind: QuestionKind,
    pub prompt: String,
    pub choices: Vec<Choice>,
    #[serde(default = "default_points")]
    pub points: u32,
    #[serde(default, skip_serializing_if = "is_all_or_nothing")]
    pub scoring: Scoring,
    /// Attempts allowed; 0 = unlimited.
    #[serde(default = "one")]
    pub attempts: u32,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct_layer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incorrect_layer: Option<String>,
    /// Accepted answers for a fill-in question. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted: Vec<String>,
    /// Fill-in comparison is case-sensitive when true. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub case_sensitive: bool,
    /// Left/right pairs for a matching question. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pairs: Vec<MatchPair>,
    /// Base-layer image the learner clicks. Omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotspot_image: Option<String>,
    /// Hotspot regions in slide coordinates. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<HotspotRegion>,
    /// How many hotspot markers the learner may place. Omitted when 1.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub markers: u32,
    /// Drag item → correct target. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drag_items: Vec<DragItem>,
    /// Object ids that accept drops, including distractors. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop_targets: Vec<String>,
    /// Centre a dropped item on its target. Omitted when true.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub snap_to_center: bool,
    /// Send a wrongly dropped item home immediately. Omitted when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub return_if_wrong: bool,
    /// Only one item may occupy a target. Omitted when true.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub one_per_target: bool,
    /// Freeform questions grade by this condition. Omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct_when: Option<Condition>,
}

fn default_points() -> u32 {
    10
}
fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    MultipleChoice,
    MultipleResponse,
    /// Two choices, exactly one correct. The editor seeds ids `true` and `false`.
    TrueFalse,
    /// The learner types an answer matched against `accepted`.
    FillIn,
    /// `choices` are listed in the correct order and shown shuffled.
    Sequence,
    /// Each `pairs` left is matched to that pair's right.
    Matching,
    /// The learner places markers on a base-layer image.
    Hotspot,
    /// The learner drags base-layer objects onto drop targets.
    DragDrop,
    /// A scale of labelled choices, pick one. Not graded.
    Likert,
    /// One line of free text. Not graded.
    ShortAnswer,
    /// Several lines of free text. Not graded.
    Essay,
    /// The author's own slide design, graded by `correct_when`.
    Freeform,
}

/// How a question awards points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scoring {
    /// All points or none, based on correctness.
    #[default]
    AllOrNothing,
    /// Points are summed per chosen answer.
    PerAnswer,
    /// Partial credit proportional to correct answers.
    Partial,
}

impl Question {
    /// Whether this question is a survey question (not graded, no points).
    #[must_use]
    pub fn is_survey(&self) -> bool {
        matches!(
            self.kind,
            QuestionKind::Likert | QuestionKind::ShortAnswer | QuestionKind::Essay
        )
    }

    /// The most points this question can award.
    #[must_use]
    pub fn max_points(&self) -> u32 {
        if self.is_survey() {
            return 0;
        }
        match (self.scoring, self.kind) {
            (Scoring::PerAnswer, QuestionKind::MultipleResponse) => {
                let mut total: u32 = 0;
                for choice in &self.choices {
                    if choice.points > 0 {
                        total = total.saturating_add(choice.points as u32);
                    }
                }
                total
            }
            (Scoring::PerAnswer, _) => self
                .choices
                .iter()
                .map(|c| c.points)
                .max()
                .map_or(0, |p| if p < 0 { 0 } else { p as u32 }),
            _ => self.points,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub correct: bool,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub points: i32,
}

/// One row of a matching question. `id` identifies both the left item and its correct right.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchPair {
    pub id: String,
    pub left: String,
    pub right: String,
}

/// Outline of a hotspot region, in slide coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotShape {
    Rect,
    Ellipse,
}

/// One clickable region of a hotspot question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotspotRegion {
    pub id: String,
    pub shape: HotspotShape,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub correct: bool,
}

/// A drag item and the drop target that grades as correct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DragItem {
    pub item: String,
    pub target: String,
}
