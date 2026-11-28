//! CourseCraft document model (L1): pure data, no behaviour beyond lookup and
//! validation. The authoritative description is `docs/spec/course-model.md`;
//! trigger semantics live in `docs/spec/triggers.md`.
//!
//! The model serialises to the `course.json` inside a `.ccraft` project. Every
//! field that is not required has a serde default so hand-written and
//! agent-written courses stay short.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod audit;
mod connector_route;
mod course_map;
mod diff;
pub mod effects;
mod media;
mod model;
mod named_styles;
pub mod outline;
mod player_settings;
pub mod recolor;
mod shapes;
pub mod strings;
pub mod text;
mod validate;

pub use connector_route::{
    CONNECTOR_MIN_LEAD, connector_site, connector_site_direction, route_connector,
};
pub use effects::{
    EFFECTS, EMPHASIS, EffectCategory, EffectDef, EffectOptionDef, EmphasisDef, FALLBACK_EFFECT,
    FALLBACK_EMPHASIS, OPTION_FIELDS, TRANSITIONS, TransitionDef, effect_def, emphasis_def,
    transition_def,
};

pub use course_map::{
    CourseMap, MapEdge, MapEdgeKind, MapScene, MapSlide, course_map, describe_condition,
};

pub use diff::{Change, ChangeKind, MediaHashes, diff, diff_with_media};
pub use media::{
    MediaOwner, MediaRef, MediaRole, MediaScope, SlideContainer, visit_course_media,
    visit_object_media, visit_slide_media,
};
pub use named_styles::{
    StyleLink, apply_named_style, clear_run_look, style_link, style_of_text_box,
};
pub use recolor::{RecolorError, normalize_hex, recolor_course, same_rgb};

pub use audit::{
    AuditFinding, AuditReport, AuditSeverity, BUTTON_DARK_INK, LARGE_BOLD_PX, LARGE_CONTRAST,
    LARGE_TEXT_PX, MANUAL_CHECKS, MIN_TARGET_PX, MIN_TEXT_PX, NORMAL_CONTRAST, Rgba,
    button_label_color, contrast_passes, contrast_ratio, is_large_text, parse_color,
    relative_luminance,
};

pub use model::{
    Action, Align, AnimEffect, Animation, Autofit, BACKGROUND_AUDIO_ID, BUILTIN_STATES,
    BUILTIN_VARIABLES, BackgroundAudio, BankDraw, BaseSlideView, Baseline, CHROME_STRINGS, Choice,
    CmpOp, Color, CompletionStatus, Condition, ConnectorStyle, Course, CropRect, Cue,
    DEFAULT_TIMER_DURATION_MS, DashStyle, Direction, DragItem, DragSpec, Easing, EditorSettings,
    Emphasis, EmphasisEffect, Event, FillStyle, FontFace, FontStyle, GlossaryEntry, Glow, Glue,
    GradientStop, Group, Guide, HotspotRegion, HotspotShape, InOut, InputKind, LOTTIE_MAX_SPEED,
    LOTTIE_MIN_SPEED, Layer, LineEnd, LineJoin, Link, ListKind, LmsStatus, MAX_CUES,
    MAX_EMPHASIS_PER_OBJECT, MAX_EMPHASIS_REPEAT, MAX_FONT_FACES, MAX_FONT_FAMILY_CHARS,
    MAX_GRADIENT_STOPS, MAX_INPUT_CHARS, MAX_JAVASCRIPT_BYTES, MAX_LETTER_SPACING,
    MAX_OBJECT_FRAME_SIZE, MAX_PALETTE_COLORS, MAX_RADIO_OPTIONS, MAX_SCROLL_CONTENT_HEIGHT,
    MAX_STATE_EXTRAS, MAX_STATE_NAME_CHARS, MAX_STATES_PER_OBJECT, MAX_TEXT_STYLES,
    MAX_TIMELINE_DURATION_MS, MAX_TIMER_DURATION_MS, MAX_TRIM_MS, MIN_LETTER_SPACING,
    MIN_RADIO_OPTIONS, MarkerIcon, MatchPair, MeterStyle, MotionPath, NamedTextStyle, NavMode,
    NextRequirement, NumberScheme, Object, ObjectKind, Orientation, Outline, PATH_MAX_POINTS,
    PATH_MIN_POINTS, PaletteColor, Paragraph, Path, PathPoint, PathStyle, PathSubpath,
    PictureFillMode, PlayerSettings, PlayerTheme, Point, PublishSettings, Question, QuestionBank,
    QuestionKind, QuizSettings, QuizTracking, RadioOption, Rect, Resource, ResourceTarget,
    RevealShape, RevisitMode, SCHEMA_VERSION, Scene, Scoring, ScrollBar, SeekMode,
    SeekbarBehaviour, Sequence, Shadow, ShapeKind, ShapeText, Size, Slide, SlideCue,
    SlidePlayerOverrides, SlideTarget, SlidesViewedTracking, StateDef, StateExtra, StateExtraKind,
    TABLE_MAX_AXIS, TABLE_MAX_CELLS, TABLE_MIN_FRACTION, TableBorders, TableCell, TableRow,
    TextDirection, TextInsets, TextRun, TextStyle, Theme, TimerFormat, Timing, TrackingSettings,
    Transition, TransitionEffect, Trigger, Trim, VALID_TRIGGER_KEYS, VAlign, Value, ValueKind,
    ValueSource, VarOp, VariableDef, WEB_DEFAULT_ENTRY, WEB_MAX_BYTES, WEB_MAX_FILES, WebSource,
    builtin_variable_kind, clamp_letter_spacing, clamp_lottie_speed, clamp_opacity,
    clamp_panel_offset, d500, d1000, default_accent_color, default_background_color,
    default_bar_color, default_bar_text_color, default_theme_font, default_theme_text_color, fill,
    format_timer_text, is_image_src, is_language_tag_valid, is_safe_media_path,
    is_timer_remaining_variable_on, is_valid_link_url, is_valid_trigger_key, is_valid_web_entry,
    is_valid_web_folder_path, is_valid_web_url, normalize_rotation, normalize_table_fractions,
    path_frame_and_points, safe_clamp, safe_clamp_ord, script_unsupported, snap_to_step,
    sort_and_repair_cues, table_cell_is_visible, table_cell_owners, table_fraction_offsets,
    timer_id_for_remaining_variable, timer_remaining_seconds, timer_remaining_variable,
    web_folder_within_caps,
};
pub use outline::{
    CourseOutline, MAX_BODY_LINES, MAX_CHOICES, MAX_LINE_BYTES, MAX_OUTLINE_SLIDES, OutlineChoice,
    OutlineProblem, OutlineQuestion, OutlineScene, OutlineSlide, outline_from_markdown,
};
pub use player_settings::{PlayerOverrideField, PlayerOverrideUse};
pub use shapes::{
    AdjustSpec, SHAPE_CATALOG, ShapeCategory, ShapeInfo, adjust_defaults, adjust_spec,
    clamp_adjust, shape_allows_text, shape_info, shape_is_adjustable, shape_kind_for_ooxml,
    shape_label,
};
pub use strings::{
    Hit, Location, MAX_HITS, ReplaceOutcome, StringItem, StringTarget, course_string_locations,
    course_strings, find_in_course, find_matches, parse_variable_reference_at, replace_in_target,
    snippet, string_mut, variable_reference_spans,
};
pub use text::{
    CaseChange, TextRunPatch, align_paragraphs, apply_link_to_range, apply_style_to_range,
    apply_style_to_range_with_base, apply_text_edit, change_case_in_range,
    clear_formatting_in_range, normalize_runs, paragraph_range_at, paragraphs_for,
};
pub use validate::{Issue, MAX_CONDITION_DEPTH, Severity};

/// One place in a course that uses a media file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaReference {
    /// The media key (`src`) the course points at. A web folder's key ends in
    /// `/` and stands for every file under that folder.
    pub src: String,
    /// Human label such as `Slide 3 ▸ Logo`.
    pub place: String,
    /// The slide that owns the reference, if any.
    pub slide: Option<String>,
    /// The object that owns the reference, if any.
    pub object: Option<String>,
}

/// Every media reference in a course, in play order then masters, templates
/// and banks.
///
/// Objects contribute their image, audio, video and Lottie sources, captions,
/// picture fills and state sources. Course fonts, background audio and web
/// folders are included too. A web folder contributes one reference to its
/// `web/<name>/` prefix, which every file under it matches.
#[must_use]
pub fn media_references(course: &Course) -> Vec<MediaReference> {
    use std::fmt::Write as _;

    let mut out = Vec::new();
    visit_course_media(course, MediaScope::AllIncludingTemplates, &mut |r| {
        if r.role != MediaRole::WebFolder && r.src.is_empty() {
            return;
        }
        let src = if r.role == MediaRole::WebFolder {
            format!("{}/", r.src)
        } else {
            r.src.to_owned()
        };
        let (place, slide, object) = match r.owner {
            MediaOwner::Font { family } => {
                let name = if family.trim().is_empty() {
                    r.src
                } else {
                    family
                };
                (format!("Fonts ▸ {name}"), None, None)
            }
            MediaOwner::BackgroundAudio => ("Course ▸ Background audio".to_owned(), None, None),
            MediaOwner::PlayerLogo => ("Course ▸ Player logo".to_owned(), None, None),
            MediaOwner::Resource { title, .. } => (format!("Resources ▸ {title}"), None, None),
            MediaOwner::Object {
                container,
                slide,
                layer,
                object,
                state_name,
            } => {
                let mut place = match container {
                    SlideContainer::SceneSlide { number } => format!("Slide {number}"),
                    SlideContainer::Master => {
                        let name = slide.map_or("", |s| slide_place_name(&s.title, &s.id));
                        format!("Master \"{name}\"")
                    }
                    SlideContainer::Template => {
                        let name = slide.map_or("", |s| slide_place_name(&s.title, &s.id));
                        format!("Template \"{name}\"")
                    }
                    SlideContainer::Bank { bank } => {
                        let name = slide_place_name(&bank.title, &bank.id);
                        format!("Bank \"{name}\"")
                    }
                    SlideContainer::Detached => slide.map_or_else(String::new, |s| {
                        slide_place_name(&s.title, &s.id).to_owned()
                    }),
                };
                if let Some(layer) = layer {
                    let _ = write!(
                        place,
                        " ▸ Layer \"{}\"",
                        slide_place_name(&layer.name, &layer.id)
                    );
                }
                let _ = write!(place, " ▸ {}", slide_place_name(&object.name, &object.id));
                if let Some(state_name) = state_name {
                    let _ = write!(place, " ▸ State \"{state_name}\"");
                }
                (place, slide.map(|s| s.id.clone()), Some(object.id.clone()))
            }
        };
        out.push(MediaReference {
            src,
            place,
            slide,
            object,
        });
    });
    out
}

fn slide_place_name<'a>(title: &'a str, id: &'a str) -> &'a str {
    if title.is_empty() { id } else { title }
}

/// The `schema_version` of a course file when it is newer than this build reads.
fn newer_version(text: &str) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Probe {
        schema_version: u64,
    }
    let probe: Probe = serde_json::from_str(text).ok()?;
    (probe.schema_version > u64::from(SCHEMA_VERSION)).then_some(probe.schema_version)
}

impl Course {
    /// Parse a course from JSON text.
    ///
    /// # Errors
    /// Returns the serde error (with line/column) for malformed or unknown fields.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let newer = |version: u64| {
            <serde_json::Error as serde::de::Error>::custom(format!(
                "this course was made with a newer CourseCraft (format version {version}); \
                 update CourseCraft to open it"
            ))
        };
        match serde_json::from_str::<Self>(text) {
            Ok(course) if course.schema_version > SCHEMA_VERSION => {
                Err(newer(u64::from(course.schema_version)))
            }
            Ok(course) => Ok(course),
            // A newer file may use fields this build does not know, so say
            // that plainly instead of naming a field.
            Err(error) => Err(newer_version(text).map_or(error, newer)),
        }
    }

    /// Serialise to pretty JSON.
    ///
    /// # Errors
    /// Only if a value cannot be represented in JSON (e.g. a non-finite number).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialise to compact (single-line, no whitespace) JSON for published packages.
    ///
    /// # Errors
    /// Only if a value cannot be represented in JSON (e.g. a non-finite number).
    pub fn to_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// All slides in play order (scene order, then slide order). Masters and
    /// templates are never in play order.
    pub fn slides(&self) -> impl Iterator<Item = &Slide> {
        self.scenes.iter().flat_map(|s| s.slides.iter())
    }

    /// Play-order index of a slide id. Masters and templates have no index.
    #[must_use]
    pub fn slide_index(&self, id: &str) -> Option<usize> {
        self.slides().position(|s| s.id == id)
    }

    /// A slide the player can reach: slides in scenes, then bank slides.
    #[must_use]
    pub fn play_slide(&self, id: &str) -> Option<&Slide> {
        self.slides()
            .find(|s| s.id == id)
            .or_else(|| self.bank_slide(id))
    }

    /// A slide master by id.
    #[must_use]
    pub fn master(&self, id: &str) -> Option<&Slide> {
        self.masters.iter().find(|s| s.id == id)
    }

    /// A reusable template by id.
    #[must_use]
    pub fn template(&self, id: &str) -> Option<&Slide> {
        self.templates.iter().find(|s| s.id == id)
    }

    /// A question bank by id.
    #[must_use]
    pub fn bank(&self, id: &str) -> Option<&QuestionBank> {
        self.banks.iter().find(|bank| bank.id == id)
    }

    /// A slide in a question bank by id.
    #[must_use]
    pub fn bank_slide(&self, id: &str) -> Option<&Slide> {
        self.banks
            .iter()
            .flat_map(|bank| bank.slides.iter())
            .find(|s| s.id == id)
    }

    /// Every slide id in the course: scenes, masters, templates and banks.
    /// Slide ids are unique across all of them.
    pub fn all_slide_ids(&self) -> impl Iterator<Item = &str> {
        self.slides()
            .map(|s| s.id.as_str())
            .chain(self.masters.iter().map(|s| s.id.as_str()))
            .chain(self.templates.iter().map(|s| s.id.as_str()))
            .chain(
                self.banks
                    .iter()
                    .flat_map(|bank| bank.slides.iter().map(|s| s.id.as_str())),
            )
    }

    /// Find a slide the editor can edit: a scene slide first, then a bank
    /// slide, then a master. Templates are never found here.
    #[must_use]
    pub fn slide(&self, id: &str) -> Option<&Slide> {
        self.play_slide(id).or_else(|| self.master(id))
    }

    #[must_use]
    pub fn slide_mut(&mut self, id: &str) -> Option<&mut Slide> {
        if self
            .scenes
            .iter()
            .any(|s| s.slides.iter().any(|sl| sl.id == id))
        {
            return self
                .scenes
                .iter_mut()
                .flat_map(|s| s.slides.iter_mut())
                .find(|s| s.id == id);
        }
        if self
            .banks
            .iter()
            .any(|bank| bank.slides.iter().any(|sl| sl.id == id))
        {
            return self
                .banks
                .iter_mut()
                .flat_map(|bank| bank.slides.iter_mut())
                .find(|s| s.id == id);
        }
        self.masters.iter_mut().find(|s| s.id == id)
    }

    /// The background a slide draws with: its own, else its master's, else the
    /// theme's. This is the one helper every drawing path uses.
    #[must_use]
    pub fn slide_background<'a>(&'a self, slide: &'a Slide) -> &'a Color {
        slide
            .background
            .as_ref()
            .or_else(|| {
                slide
                    .master
                    .as_deref()
                    .and_then(|id| self.master(id))
                    .and_then(|master| master.background.as_ref())
            })
            .unwrap_or(&self.theme.background)
    }

    #[must_use]
    pub fn variable(&self, name: &str) -> Option<&VariableDef> {
        self.variables.iter().find(|v| v.name == name)
    }

    /// Whether `name` is the built-in font or one of this course's families.
    /// Matching is trimmed and case-insensitive, like render lookup.
    #[must_use]
    pub fn has_font_family(&self, name: &str) -> bool {
        let name = name.trim();
        name.eq_ignore_ascii_case("Inter")
            // The pre-M11 default; it has always been drawn with Inter.
            || name.eq_ignore_ascii_case("Open Sans")
            || self
                .fonts
                .iter()
                .any(|face| face.family.trim().eq_ignore_ascii_case(name))
    }

    /// Distinct custom font families, in first-registration order.
    #[must_use]
    pub fn font_families(&self) -> Vec<&str> {
        let mut seen = std::collections::BTreeSet::new();
        let mut families = Vec::new();
        for face in &self.fonts {
            let family = face.family.trim();
            let key = family.to_ascii_lowercase();
            if seen.insert(key) {
                families.push(family);
            }
        }
        families
    }

    /// The player's own text for `key`: the course override, else the English
    /// default from [`CHROME_STRINGS`]. An unknown key is the empty string.
    #[must_use]
    pub fn chrome_string(&self, key: &str) -> &str {
        self.strings
            .get(key)
            .map(String::as_str)
            .or_else(|| {
                CHROME_STRINGS
                    .iter()
                    .find_map(|&(k, v)| if k == key { Some(v) } else { None })
            })
            .unwrap_or_default()
    }

    /// Total number of slides in play order, expanding bank-draw placeholders.
    #[must_use]
    pub fn play_slide_count(&self) -> usize {
        self.slides().fold(0usize, |acc, slide| {
            let count = slide.draw.as_ref().map_or(1, |d| d.count as usize);
            acc.saturating_add(count)
        })
    }

    /// All play-order slides that carry an enabled `SubmitResults` trigger.
    pub fn results_slides(&self) -> impl Iterator<Item = &Slide> {
        self.slides().filter(|s| s.has_submit_results_trigger())
    }

    /// True when the course has at least one graded question in play order
    /// (either directly on a scene slide or in a non-empty drawn question bank).
    #[must_use]
    pub fn has_graded_questions(&self) -> bool {
        self.slides().any(|slide| {
            if slide.question.as_ref().is_some_and(|q| q.max_points() > 0) {
                return true;
            }
            if let Some(draw) = &slide.draw
                && draw.count > 0
                && let Some(bank) = self.bank(&draw.bank)
            {
                return bank
                    .slides
                    .iter()
                    .any(|bs| bs.question.as_ref().is_some_and(|q| q.max_points() > 0));
            }
            false
        })
    }

    /// True when the course can report a result to the LMS via triggers or
    /// configured [`TrackingSettings`].
    #[must_use]
    pub fn reports_result(&self) -> bool {
        let has_trigger = self.slides().any(|slide| {
            slide
                .triggers
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
                .any(|trigger| {
                    // A disabled trigger never fires.
                    !trigger.disabled
                        && matches!(
                            trigger.action,
                            Action::SubmitResults | Action::CompleteCourse { .. }
                        )
                })
        });
        if has_trigger {
            return true;
        }
        if self.tracking.slides_viewed.is_some() && self.play_slide_count() > 0 {
            return true;
        }
        self.tracking
            .quiz
            .as_ref()
            .is_some_and(|qt| match &qt.slide {
                Some(id) => self.slide_index(id).is_some(),
                None => self.has_graded_questions(),
            })
    }

    /// The slide the player opens on: `start_slide`, else the first slide.
    #[must_use]
    pub fn start_index(&self) -> Option<usize> {
        match &self.start_slide {
            Some(id) => self.slide_index(id),
            None => self.slides().next().map(|_| 0),
        }
    }
}

impl Slide {
    /// True when the slide or any of its layers has an enabled `SubmitResults` trigger.
    #[must_use]
    pub fn has_submit_results_trigger(&self) -> bool {
        self.triggers
            .iter()
            .chain(self.layers.iter().flat_map(|layer| layer.triggers.iter()))
            .any(|trigger| !trigger.disabled && matches!(trigger.action, Action::SubmitResults))
    }

    #[must_use]
    pub fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    #[must_use]
    pub fn layer_mut(&mut self, id: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Find an object on the base layer or any slide layer. Returns the object
    /// and the id of the layer that owns it (`None` = base layer).
    #[must_use]
    pub fn object(&self, id: &str) -> Option<(&Object, Option<&str>)> {
        if let Some(o) = self.objects.iter().find(|o| o.id == id) {
            return Some((o, None));
        }
        self.layers.iter().find_map(|l| {
            l.objects
                .iter()
                .find(|o| o.id == id)
                .map(|o| (o, Some(l.id.as_str())))
        })
    }

    /// Find a group record on the base layer or any slide layer. Returns the
    /// group and the id of the layer that owns it (`None` = base layer).
    #[must_use]
    pub fn group(&self, id: &str) -> Option<(&Group, Option<&str>)> {
        if let Some(g) = self.groups.iter().find(|g| g.id == id) {
            return Some((g, None));
        }
        self.layers.iter().find_map(|l| {
            l.groups
                .iter()
                .find(|g| g.id == id)
                .map(|g| (g, Some(l.id.as_str())))
        })
    }

    /// Find an interactive group record on the base layer or any slide layer.
    #[must_use]
    pub fn interactive_group(&self, id: &str) -> Option<(&Group, Option<&str>)> {
        self.group(id).filter(|(g, _)| g.interactive)
    }

    /// If `obj_id` belongs to a recorded [`Group`] on its own layer (interactive
    /// or not), return that group and its owning layer id (`None` = base layer).
    #[must_use]
    pub fn group_of_object(&self, obj_id: &str) -> Option<(&Group, Option<&str>)> {
        let (obj, owner) = self.object(obj_id)?;
        let group_id = obj.group.as_deref()?;
        let groups = match owner {
            None => self.groups.as_slice(),
            Some(layer_id) => self.layer(layer_id)?.groups.as_slice(),
        };
        groups.iter().find(|g| g.id == group_id).map(|g| (g, owner))
    }

    /// If `obj_id` belongs to an interactive group on its own layer, return
    /// that group and its owning layer id (`None` = base layer).
    #[must_use]
    pub fn interactive_group_of_object(&self, obj_id: &str) -> Option<(&Group, Option<&str>)> {
        self.group_of_object(obj_id).filter(|(g, _)| g.interactive)
    }

    /// Compute the union timing window of all member objects of `group_id` on
    /// `owner` (`None` = base layer). Returns `None` if the group has no members.
    #[must_use]
    pub fn group_timing(&self, group_id: &str, owner: Option<&str>) -> Option<Timing> {
        let objects = match owner {
            None => self.objects.as_slice(),
            Some(layer_id) => self.layer(layer_id)?.objects.as_slice(),
        };
        let mut start_ms: Option<u32> = None;
        let mut end_ms: Option<u32> = Some(0);
        for o in objects
            .iter()
            .filter(|o| o.group.as_deref() == Some(group_id))
        {
            start_ms = Some(start_ms.map_or(o.timing.start_ms, |s| s.min(o.timing.start_ms)));
            end_ms = match (end_ms, o.timing.end_ms) {
                (Some(cur), Some(e)) => Some(cur.max(e)),
                _ => None,
            };
        }
        start_ms.map(|start_ms| Timing { start_ms, end_ms })
    }

    /// Compute the union bounding [`Rect`] of all member objects of `group_id` on
    /// `owner` (`None` = base layer), skipping non-finite or negative-size frames.
    /// Returns `None` if no valid member frames exist.
    #[must_use]
    pub fn group_frame(&self, group_id: &str, owner: Option<&str>) -> Option<Rect> {
        let objects = match owner {
            None => self.objects.as_slice(),
            Some(layer_id) => self.layer(layer_id)?.objects.as_slice(),
        };
        let mut bounds: Option<(f32, f32, f32, f32)> = None;
        for o in objects
            .iter()
            .filter(|o| o.group.as_deref() == Some(group_id))
        {
            let f = &o.frame;
            if !f.x.is_finite()
                || !f.y.is_finite()
                || !f.w.is_finite()
                || !f.h.is_finite()
                || f.w < 0.0
                || f.h < 0.0
            {
                continue;
            }
            let x2 = f.x + f.w;
            let y2 = f.y + f.h;
            bounds = Some(match bounds {
                None => (f.x, f.y, x2, y2),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(f.x), min_y.min(f.y), max_x.max(x2), max_y.max(y2))
                }
            });
        }
        bounds.map(|(x, y, x2, y2)| Rect {
            x,
            y,
            w: x2 - x,
            h: y2 - y,
        })
    }

    /// Return the member objects of `group_id`. When a [`Group`] record exists,
    /// returns matching objects on that group's owning object list; otherwise
    /// returns all matching objects across the slide.
    #[must_use]
    pub fn group_members(&self, group_id: &str) -> Vec<&Object> {
        if let Some((_, owner)) = self.group(group_id) {
            let objects = match owner {
                None => self.objects.as_slice(),
                Some(layer_id) => self
                    .layer(layer_id)
                    .map_or(&[][..], |l| l.objects.as_slice()),
            };
            return objects
                .iter()
                .filter(|o| o.group.as_deref() == Some(group_id))
                .collect();
        }
        self.objects
            .iter()
            .chain(self.layers.iter().flat_map(|l| l.objects.iter()))
            .filter(|o| o.group.as_deref() == Some(group_id))
            .collect()
    }

    /// Whether `state` is valid for interactive group `group_id`: `"normal"`,
    /// `"hidden"`, `"disabled"`, or a state defined on at least one member.
    #[must_use]
    pub fn group_has_state(&self, group_id: &str, state: &str) -> bool {
        matches!(state, "normal" | "hidden" | "disabled")
            || self
                .group_members(group_id)
                .into_iter()
                .any(|member| member.states.iter().any(|s| s.name == state))
    }

    /// Every id used on this slide (objects, layers and group records share one
    /// namespace).
    pub fn local_ids(&self) -> impl Iterator<Item = &str> {
        self.objects
            .iter()
            .map(|o| o.id.as_str())
            .chain(self.layers.iter().map(|l| l.id.as_str()))
            .chain(
                self.layers
                    .iter()
                    .flat_map(|l| l.objects.iter().map(|o| o.id.as_str())),
            )
            .chain(self.groups.iter().map(|g| g.id.as_str()))
            .chain(
                self.layers
                    .iter()
                    .flat_map(|l| l.groups.iter().map(|g| g.id.as_str())),
            )
    }
}

/// Whether `obj` is disallowed as a member of an interactive group (inputs,
/// scrolling panels, web objects, audio/video media, markers, tables, draggable
/// objects, or question hotspot images / drag items).
#[must_use]
pub fn is_disallowed_interactive_group_member(obj: &Object, question: Option<&Question>) -> bool {
    if obj.kind.input_kind().is_some()
        || matches!(
            obj.kind,
            ObjectKind::ScrollPanel { .. }
                | ObjectKind::Web { .. }
                | ObjectKind::Audio { .. }
                | ObjectKind::Video { .. }
                | ObjectKind::Marker { .. }
                | ObjectKind::Table { .. }
        )
        || obj.drag.is_some()
    {
        return true;
    }
    question.is_some_and(|q| {
        q.hotspot_image.as_deref() == Some(obj.id.as_str())
            || q.drag_items.iter().any(|d| d.item == obj.id)
    })
}

impl Object {
    /// Whether `state` is valid for this object: a built-in that needs no
    /// definition (`normal`, `hidden`) or one listed in `states`.
    #[must_use]
    pub fn has_state(&self, state: &str) -> bool {
        matches!(state, "normal" | "hidden") || self.states.iter().any(|s| s.name == state)
    }
}

#[cfg(test)]
mod anim_window_tests;
#[cfg(test)]
mod bughunt_m25_strings;
#[cfg(test)]
mod m29_state_validation_tests;
#[cfg(test)]
mod m39_t_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod u19_state_src_tests;
#[cfg(test)]
mod u21_lms_status_tests;
