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
mod model;
mod named_styles;
pub mod outline;
pub mod recolor;
mod shapes;
pub mod strings;
pub mod text;
mod validate;

pub use connector_route::{
    CONNECTOR_MIN_LEAD, connector_site, connector_site_direction, route_connector,
};

pub use course_map::{
    CourseMap, MapEdge, MapEdgeKind, MapScene, MapSlide, course_map, describe_condition,
};

pub use diff::{Change, ChangeKind, MediaHashes, diff, diff_with_media};
pub use named_styles::{StyleLink, apply_named_style, style_link, style_of_text_box};
pub use recolor::{RecolorError, normalize_hex, recolor_course, same_rgb};

pub use audit::{
    AuditFinding, AuditReport, AuditSeverity, LARGE_BOLD_PX, LARGE_CONTRAST, LARGE_TEXT_PX,
    MANUAL_CHECKS, MIN_TARGET_PX, MIN_TEXT_PX, NORMAL_CONTRAST, Rgba, contrast_passes,
    contrast_ratio, is_large_text, parse_color, relative_luminance,
};

pub use model::{
    Action, Align, AnimEffect, Animation, Autofit, BACKGROUND_AUDIO_ID, BUILTIN_STATES,
    BUILTIN_VARIABLES, BackgroundAudio, BankDraw, Baseline, CHROME_STRINGS, Choice, CmpOp, Color,
    CompletionStatus, Condition, ConnectorStyle, Course, CropRect, Cue, DEFAULT_TIMER_DURATION_MS,
    DashStyle, Direction, DragItem, DragSpec, Easing, Event, FillStyle, FontFace, FontStyle,
    GlossaryEntry, Glow, Glue, GradientStop, HotspotRegion, HotspotShape, InputKind,
    LOTTIE_MAX_SPEED, LOTTIE_MIN_SPEED, Layer, LineEnd, Link, ListKind, MAX_CUES, MAX_FONT_FACES,
    MAX_FONT_FAMILY_CHARS, MAX_GRADIENT_STOPS, MAX_INPUT_CHARS, MAX_JAVASCRIPT_BYTES,
    MAX_LETTER_SPACING, MAX_PALETTE_COLORS, MAX_RADIO_OPTIONS, MAX_SCROLL_CONTENT_HEIGHT,
    MAX_TEXT_STYLES, MAX_TIMER_DURATION_MS, MAX_TRIM_MS, MIN_LETTER_SPACING, MIN_RADIO_OPTIONS,
    MarkerIcon, MatchPair, MeterStyle, MotionPath, NamedTextStyle, NavMode, NextRequirement,
    Object, ObjectKind, Orientation, Outline, PATH_MAX_POINTS, PATH_MIN_POINTS, PaletteColor,
    Paragraph, Path, PathPoint, PathStyle, PictureFillMode, PlayerSettings, PlayerTheme, Point,
    Question, QuestionBank, QuestionKind, QuizSettings, RadioOption, Rect, Resource,
    ResourceTarget, RevisitMode, SCHEMA_VERSION, Scene, Scoring, ScrollBar, SeekMode, Shadow,
    ShapeKind, ShapeText, Size, Slide, SlidePlayerOverrides, SlideTarget, StateDef, TABLE_MAX_AXIS,
    TABLE_MAX_CELLS, TABLE_MIN_FRACTION, TableBorders, TableCell, TableRow, TextRun, TextStyle,
    Theme, TimerFormat, Timing, Transition, TransitionEffect, Trigger, Trim, VALID_TRIGGER_KEYS,
    VAlign, Value, ValueKind, VarOp, VariableDef, WEB_DEFAULT_ENTRY, WEB_MAX_BYTES, WEB_MAX_FILES,
    WebSource, builtin_variable_kind, clamp_letter_spacing, clamp_lottie_speed, clamp_opacity,
    clamp_panel_offset, d500, d1000, default_accent_color, default_background_color,
    default_bar_color, default_bar_text_color, default_theme_font, default_theme_text_color, fill,
    format_timer_text, is_image_src, is_language_tag_valid, is_safe_media_path,
    is_timer_remaining_variable_on, is_valid_link_url, is_valid_trigger_key, is_valid_web_entry,
    is_valid_web_folder_path, is_valid_web_url, normalize_rotation, normalize_table_fractions,
    path_frame_and_points, script_unsupported, snap_to_step, sort_and_repair_cues,
    table_cell_is_visible, table_cell_owners, table_fraction_offsets,
    timer_id_for_remaining_variable, timer_remaining_seconds, timer_remaining_variable,
    web_folder_within_caps,
};
pub use outline::{
    CourseOutline, MAX_BODY_LINES, MAX_CHOICES, MAX_LINE_BYTES, MAX_OUTLINE_SLIDES, OutlineChoice,
    OutlineProblem, OutlineQuestion, OutlineScene, OutlineSlide, outline_from_markdown,
};
pub use shapes::{
    AdjustSpec, SHAPE_CATALOG, ShapeCategory, ShapeInfo, adjust_defaults, adjust_spec,
    clamp_adjust, shape_allows_text, shape_info, shape_is_adjustable, shape_kind_for_ooxml,
    shape_label,
};
pub use strings::{
    Hit, Location, MAX_HITS, ReplaceOutcome, StringItem, StringTarget, course_strings,
    find_in_course, find_matches, replace_in_target, snippet, string_mut, variable_reference_spans,
};
pub use text::{
    TextRunPatch, align_paragraphs, apply_link_to_range, apply_style_to_range, apply_text_edit,
    normalize_runs, paragraph_range_at, paragraphs_for,
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
    let mut out = Vec::new();
    for face in &course.fonts {
        if !face.src.is_empty() {
            let name = if face.family.trim().is_empty() {
                face.src.as_str()
            } else {
                face.family.as_str()
            };
            out.push(MediaReference {
                src: face.src.clone(),
                place: format!("Fonts ▸ {name}"),
                slide: None,
            });
        }
    }
    if let Some(audio) = &course.background_audio
        && !audio.src.is_empty()
    {
        out.push(MediaReference {
            src: audio.src.clone(),
            place: "Course ▸ Background audio".to_owned(),
            slide: None,
        });
    }
    if let Some(logo) = &course.player.logo
        && !logo.is_empty()
    {
        out.push(MediaReference {
            src: logo.clone(),
            place: "Course ▸ Player logo".to_owned(),
            slide: None,
        });
    }
    for resource in &course.resources {
        if let ResourceTarget::File { src } = &resource.target
            && !src.is_empty()
        {
            out.push(MediaReference {
                src: src.clone(),
                place: format!("Resources ▸ {}", resource.title),
                slide: None,
            });
        }
    }
    let mut number = 0usize;
    for scene in &course.scenes {
        for slide in &scene.slides {
            number = number.saturating_add(1);
            walk_slide_media_references(&mut out, slide, &format!("Slide {number}"));
        }
    }
    for master in &course.masters {
        let name = slide_place_name(&master.title, &master.id);
        walk_slide_media_references(&mut out, master, &format!("Master \"{name}\""));
    }
    for template in &course.templates {
        let name = slide_place_name(&template.title, &template.id);
        walk_slide_media_references(&mut out, template, &format!("Template \"{name}\""));
    }
    for bank in &course.banks {
        let name = slide_place_name(&bank.title, &bank.id);
        for slide in &bank.slides {
            walk_slide_media_references(&mut out, slide, &format!("Bank \"{name}\""));
        }
    }
    out
}

fn slide_place_name<'a>(title: &'a str, id: &'a str) -> &'a str {
    if title.is_empty() { id } else { title }
}

fn walk_slide_media_references(out: &mut Vec<MediaReference>, slide: &Slide, place: &str) {
    for object in &slide.objects {
        walk_object_media_references(out, slide, place, object);
    }
    for layer in &slide.layers {
        let name = slide_place_name(&layer.name, &layer.id);
        let layer_place = format!("{place} ▸ Layer \"{name}\"");
        for object in &layer.objects {
            walk_object_media_references(out, slide, &layer_place, object);
        }
    }
}

fn walk_object_media_references(
    out: &mut Vec<MediaReference>,
    slide: &Slide,
    place: &str,
    object: &Object,
) {
    let name = slide_place_name(&object.name, &object.id);
    let object_place = format!("{place} ▸ {name}");
    let mut push = |src: &str| {
        if !src.is_empty() {
            out.push(MediaReference {
                src: src.to_owned(),
                place: object_place.clone(),
                slide: Some(slide.id.clone()),
            });
        }
    };
    match &object.kind {
        ObjectKind::Image { src, .. } | ObjectKind::Lottie { src, .. } => push(src),
        ObjectKind::Audio { src, captions, .. } | ObjectKind::Video { src, captions, .. } => {
            push(src);
            if let Some(captions) = captions {
                push(captions);
            }
        }
        ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
            if let Some(src) = fill_style.as_ref().and_then(FillStyle::picture_src) {
                push(src);
            }
        }
        ObjectKind::Web {
            source: WebSource::Folder { path, .. },
        } => push(&format!("{path}/")),
        _ => {}
    }
    for state in &object.states {
        if let Some(src) = &state.src
            && !src.is_empty()
        {
            out.push(MediaReference {
                src: src.clone(),
                place: format!("{object_place} ▸ State \"{}\"", state.name),
                slide: Some(slide.id.clone()),
            });
        }
    }
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

    /// True when the course has at least one trigger that reports a result to
    /// the LMS: `submit_results` (score and completion) or `complete_course`.
    #[must_use]
    pub fn reports_result(&self) -> bool {
        self.slides().any(|slide| {
            slide
                .triggers
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
                .any(|trigger| {
                    matches!(
                        trigger.action,
                        Action::SubmitResults | Action::CompleteCourse { .. }
                    )
                })
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

    /// Every id used on this slide (objects and layers share one namespace).
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
    }
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
mod outline_tests;
#[cfg(test)]
mod tests;
