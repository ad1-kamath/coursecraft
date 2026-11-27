//! Course validation. Never panics: every problem becomes an [`Issue`] with a
//! JSON-pointer-like path so the editor, CLI and agents can point at it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::{
    Action, Condition, Course, Event, Group, Layer, Object, ObjectKind, QuestionKind, QuizTracking,
    SCHEMA_VERSION, Scoring, Slide, SlideTarget, SlidesViewedTracking, StateExtraKind, ValueKind,
    ValueSource, VarOp,
};

/// Conditions nested deeper than this are rejected (and the player refuses them).
pub const MAX_CONDITION_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Copy)]
enum PathSeg<'a> {
    Field(&'static str),
    Index(usize),
    Key(&'a str),
}

struct Ctx<'a> {
    prefix: &'a str,
    path: Vec<PathSeg<'a>>,
    issues: Vec<Issue>,
    skip_warnings: bool,
    stop_on_first_error: bool,
    has_error: bool,
}

impl<'a> Ctx<'a> {
    fn new(prefix: &'a str, skip_warnings: bool, stop_on_first_error: bool) -> Self {
        Self {
            prefix,
            path: Vec::with_capacity(12),
            issues: Vec::new(),
            skip_warnings,
            stop_on_first_error,
            has_error: false,
        }
    }

    #[inline]
    fn should_stop(&self) -> bool {
        self.stop_on_first_error && self.has_error
    }

    #[inline]
    fn with_field<R>(&mut self, field: &'static str, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(PathSeg::Field(field));
        let r = f(self);
        self.path.pop();
        r
    }

    #[inline]
    fn with_index<R>(&mut self, index: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(PathSeg::Index(index));
        let r = f(self);
        self.path.pop();
        r
    }

    #[inline]
    fn with_field_index<R>(
        &mut self,
        field: &'static str,
        index: usize,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.path.push(PathSeg::Field(field));
        self.path.push(PathSeg::Index(index));
        let r = f(self);
        self.path.pop();
        self.path.pop();
        r
    }

    #[inline]
    fn with_key<R>(&mut self, key: &'a str, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(PathSeg::Key(key));
        let r = f(self);
        self.path.pop();
        r
    }

    fn materialize_path(&self, suffix: Option<&str>) -> String {
        let cap = self
            .prefix
            .len()
            .saturating_add(self.path.len().saturating_mul(8))
            .saturating_add(suffix.map_or(0, |s| s.len().saturating_add(1)));
        let mut out = String::with_capacity(cap);
        out.push_str(self.prefix);
        for seg in &self.path {
            out.push('/');
            match *seg {
                PathSeg::Field(f) => out.push_str(f),
                PathSeg::Index(i) => {
                    let _ = write!(out, "{i}");
                }
                PathSeg::Key(k) => out.push_str(k),
            }
        }
        if let Some(s) = suffix
            && !s.is_empty()
        {
            if !s.starts_with('/') {
                out.push('/');
            }
            out.push_str(s);
        }
        out
    }

    fn err(&mut self, message: impl Into<String>) {
        if self.should_stop() {
            return;
        }
        self.has_error = true;
        let path = self.materialize_path(None);
        self.issues.push(Issue {
            severity: Severity::Error,
            path,
            message: message.into(),
        });
    }

    fn err_at(&mut self, suffix: &str, message: impl Into<String>) {
        if self.should_stop() {
            return;
        }
        self.has_error = true;
        let path = self.materialize_path(Some(suffix));
        self.issues.push(Issue {
            severity: Severity::Error,
            path,
            message: message.into(),
        });
    }

    fn warn(&mut self, message: impl Into<String>) {
        if self.skip_warnings || self.should_stop() {
            return;
        }
        let path = self.materialize_path(None);
        self.issues.push(Issue {
            severity: Severity::Warning,
            path,
            message: message.into(),
        });
    }

    fn warn_at(&mut self, suffix: &str, message: impl Into<String>) {
        if self.skip_warnings || self.should_stop() {
            return;
        }
        let path = self.materialize_path(Some(suffix));
        self.issues.push(Issue {
            severity: Severity::Warning,
            path,
            message: message.into(),
        });
    }
}

fn ends_with_ignore_ascii_case(s: &str, suffix: &str) -> bool {
    let sb = s.as_bytes();
    let tb = suffix.as_bytes();
    sb.len()
        .checked_sub(tb.len())
        .and_then(|start| sb.get(start..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(tb))
}

pub(crate) fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Course {
    /// Check the whole course. An empty list (or warnings only) means the
    /// player can run it.
    #[must_use]
    pub fn validate(&self) -> Vec<Issue> {
        let mut cx = Ctx::new("", false, false);
        self.run_validate(&mut cx);
        cx.issues
    }

    /// Return the first validation error, skipping warning-only checks and
    /// stopping as soon as an error is found.
    #[must_use]
    pub fn first_error(&self) -> Option<Issue> {
        let mut cx = Ctx::new("", true, true);
        self.run_validate(&mut cx);
        cx.issues.into_iter().next()
    }

    /// Validate a single slide (scene slide, master, bank slide, or template)
    /// against `self` for variable names, slide targets, and bank lookups,
    /// returning the first error if any.
    #[must_use]
    pub fn validate_slide_errors(&self, slide: &Slide, path_prefix: &str) -> Option<Issue> {
        let prefix = path_prefix.trim_end_matches('/');
        let mut cx = Ctx::new(prefix, true, true);
        if prefix.starts_with("/masters/") || prefix == "/masters" {
            if !valid_name(&slide.id) {
                cx.err(format!(
                    "master id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    slide.id
                ));
            }
            if !cx.should_stop() {
                check_master(self, &mut cx, slide);
            }
            return cx.issues.into_iter().next();
        }
        if prefix.starts_with("/templates/") || prefix == "/templates" {
            if !valid_name(&slide.id) {
                cx.err(format!(
                    "template id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    slide.id
                ));
            }
            if !cx.should_stop() && slide.draw.is_some() {
                cx.err_at(
                    "draw",
                    format!("template {:?} must not have draw", slide.id),
                );
            }
            return cx.issues.into_iter().next();
        }
        if prefix.starts_with("/banks/") || prefix == "/banks" {
            if !valid_name(&slide.id) {
                cx.err(format!(
                    "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    slide.id
                ));
            }
            if !cx.should_stop() {
                check_slide(self, &mut cx, slide);
            }
            if !cx.should_stop() {
                check_bank_slide(&mut cx, slide);
            }
            return cx.issues.into_iter().next();
        }
        if let Some(rule) = slide
            .player
            .as_ref()
            .and_then(|overrides| overrides.next_requires.as_ref())
        {
            cx.with_field("player", |cx| {
                cx.with_field("next_requires", |cx| check_next_rule(self, cx, rule));
            });
        }
        if !cx.should_stop() && !valid_name(&slide.id) {
            cx.err(format!(
                "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                slide.id
            ));
        }
        if !cx.should_stop() {
            check_slide(self, &mut cx, slide);
        }
        if !cx.should_stop()
            && let Some(draw) = &slide.draw
        {
            cx.with_field("draw", |cx| check_single_draw(self, cx, slide, draw));
            if !cx.should_stop()
                && let Some(issue) = check_single_slide_draw_total(self, slide, draw)
            {
                return Some(issue);
            }
        }
        cx.issues.into_iter().next()
    }

    fn run_validate<'a>(&'a self, cx: &mut Ctx<'a>) {
        self.check_header(cx);
        if cx.should_stop() {
            return;
        }
        self.check_theme(cx);
        if cx.should_stop() {
            return;
        }
        check_course_fonts(self, cx);
        if cx.should_stop() {
            return;
        }
        check_background_audio(self, cx);
        if cx.should_stop() {
            return;
        }
        check_next_rules(self, cx);
        if cx.should_stop() {
            return;
        }
        if !cx.skip_warnings {
            check_bar_off_dead_ends(self, cx);
        }
        check_resources_and_glossary(self, cx);
        if cx.should_stop() {
            return;
        }
        check_logo(self, cx);
        if cx.should_stop() {
            return;
        }
        check_font_name(self, cx, &self.theme.font, "theme/font");
        if let Some(heading) = &self.theme.heading_font {
            check_font_name(self, cx, heading, "theme/heading_font");
        }
        check_palette(self, cx);
        if cx.should_stop() {
            return;
        }
        check_text_styles(self, cx);
        if cx.should_stop() {
            return;
        }
        self.check_variables(cx);
        if cx.should_stop() {
            return;
        }
        let (mut all_slide_ids, slide_count) = self.check_scenes(cx);
        if cx.should_stop() {
            return;
        }
        check_banks(self, cx, &mut all_slide_ids);
        if cx.should_stop() {
            return;
        }
        check_draws(self, cx);
        if cx.should_stop() {
            return;
        }
        check_masters_and_templates(self, cx, &all_slide_ids);
        if cx.should_stop() {
            return;
        }
        if slide_count == 0 {
            cx.err_at("scenes", "course has no slides");
            if cx.should_stop() {
                return;
            }
        }
        check_tracking(self, cx);
        if cx.should_stop() {
            return;
        }
        if !cx.skip_warnings {
            check_course_results(self, cx);
            check_unknown_variable_refs(self, cx);
        }
        if let Some(start) = &self.start_slide
            && self.slide_index(start).is_none()
        {
            cx.err_at(
                "start_slide",
                format!("start_slide {start:?} does not exist"),
            );
        }
    }

    fn check_header<'a>(&'a self, cx: &mut Ctx<'a>) {
        if self.schema_version != SCHEMA_VERSION {
            cx.err_at(
                "schema_version",
                format!(
                    "unsupported schema_version {} (this build reads {SCHEMA_VERSION})",
                    self.schema_version
                ),
            );
        }
        if !valid_name(&self.id) {
            cx.err_at("id", "course id must be 1-128 chars of [A-Za-z0-9_-]");
        }
        if self.title.trim().is_empty() {
            cx.err_at("title", "course title is empty");
        }
        if let Some(lang) = &self.language
            && !crate::is_language_tag_valid(lang)
        {
            cx.err_at(
                "language",
                format!(
                    "language {lang:?} is not a simple BCP 47 tag (letters and digits in hyphen-separated subtags, first subtag 2-8 letters)"
                ),
            );
        }
        if !cx.skip_warnings && !self.strings.is_empty() {
            cx.with_field("strings", |cx| {
                for key in self.strings.keys() {
                    if !crate::CHROME_STRINGS.iter().any(|(k, _)| k == key) {
                        cx.with_key(key.as_str(), |cx| {
                            cx.warn(format!("unknown player string key {key:?}"));
                        });
                    }
                }
            });
        }
        if self.slide_size.w == 0
            || self.slide_size.h == 0
            || self.slide_size.w > 8192
            || self.slide_size.h > 8192
        {
            cx.err_at("slide_size", "slide size must be 1..=8192 px on each side");
        }
        if let Some(q) = &self.quiz
            && q.passing_percent > 100
        {
            cx.err_at("quiz/passing_percent", "passing_percent must be 0..=100");
        }
    }

    fn check_theme(&self, cx: &mut Ctx<'_>) {
        for (suffix, color) in [
            ("theme/accent", &self.theme.accent),
            ("theme/background", &self.theme.background),
            ("theme/text", &self.theme.text),
            ("theme/player/bar", &self.theme.player.bar),
            ("theme/player/bar_text", &self.theme.player.bar_text),
        ] {
            if !color.is_valid() {
                cx.err_at(
                    suffix,
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", color.0),
                );
            }
        }
    }

    fn check_variables(&self, cx: &mut Ctx<'_>) {
        if self.variables.is_empty() {
            return;
        }
        let mut names = BTreeSet::new();
        cx.with_field("variables", |cx| {
            for (i, v) in self.variables.iter().enumerate() {
                cx.with_index(i, |cx| {
                    if !valid_name(&v.name) {
                        cx.err(format!(
                            "variable name {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            v.name
                        ));
                    }
                    if !names.insert(v.name.as_str()) {
                        cx.err(format!("duplicate variable {:?}", v.name));
                    }
                    if let crate::Value::Number(n) = v.value
                        && !n.is_finite()
                    {
                        cx.err("variable value must be finite");
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    }

    fn check_scenes<'a>(&'a self, cx: &mut Ctx<'a>) -> (BTreeSet<&'a str>, usize) {
        let mut scene_ids = BTreeSet::new();
        let mut slide_ids = BTreeSet::new();
        let mut slide_count = 0usize;
        cx.with_field("scenes", |cx| {
            for (si, scene) in self.scenes.iter().enumerate() {
                cx.with_index(si, |cx| {
                    if !scene_ids.insert(scene.id.as_str()) {
                        cx.err(format!("duplicate scene id {:?}", scene.id));
                        if cx.should_stop() {
                            return;
                        }
                    }
                    // Old files may hold other ids or titles, so these are warnings.
                    if !cx.skip_warnings {
                        if !valid_name(&scene.id) {
                            cx.warn_at(
                                "id",
                                format!(
                                    "scene id {:?} should be 1-128 chars of [A-Za-z0-9_-]",
                                    scene.id
                                ),
                            );
                        }
                        if scene.title.trim().is_empty() {
                            cx.warn_at("title", "scene title is empty");
                        }
                    }
                    cx.with_field("slides", |cx| {
                        for (li, slide) in scene.slides.iter().enumerate() {
                            slide_count += 1;
                            cx.with_index(li, |cx| {
                                if !valid_name(&slide.id) {
                                    cx.err(format!(
                                        "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                                        slide.id
                                    ));
                                }
                                if !slide_ids.insert(slide.id.as_str()) {
                                    cx.err(format!("duplicate slide id {:?}", slide.id));
                                }
                                if !cx.should_stop() {
                                    check_slide(self, cx, slide);
                                }
                            });
                            if cx.should_stop() {
                                return;
                            }
                        }
                    });
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
        (slide_ids, slide_count)
    }

    /// True when validation found no errors (warnings are allowed).
    #[must_use]
    pub fn is_playable(&self) -> bool {
        self.first_error().is_none()
    }
}

/// Bounded, unique, clean font faces stored under `fonts/`.
fn check_course_fonts(course: &Course, cx: &mut Ctx<'_>) {
    if course.fonts.len() > crate::MAX_FONT_FACES {
        cx.err_at(
            "fonts",
            format!(
                "a course may register at most {} font faces, found {}",
                crate::MAX_FONT_FACES,
                course.fonts.len()
            ),
        );
    }
    if course.fonts.is_empty() || cx.should_stop() {
        return;
    }
    let mut seen = BTreeSet::new();
    cx.with_field("fonts", |cx| {
        for (i, face) in course.fonts.iter().enumerate() {
            cx.with_index(i, |cx| {
                let family = face.family.trim();
                if family.is_empty() {
                    cx.err_at("family", "font family is empty");
                } else if family.chars().count() > crate::MAX_FONT_FAMILY_CHARS {
                    cx.err_at(
                        "family",
                        format!(
                            "font family may be at most {} characters",
                            crate::MAX_FONT_FAMILY_CHARS
                        ),
                    );
                } else if family.chars().any(char::is_control) {
                    cx.err_at("family", "font family must not contain control characters");
                }
                let key = (family.to_ascii_lowercase(), face.style);
                if !seen.insert(key) {
                    cx.err(format!(
                        "duplicate font face for family {family:?} and style {:?}",
                        face.style
                    ));
                }
                let src = face.src.trim();
                if src.is_empty() {
                    cx.err_at("src", "font src is empty");
                } else if !is_safe_relative_path(src) {
                    cx.err_at(
                        "src",
                        format!("font src {src:?} must be a safe relative path under media/"),
                    );
                } else if !src.starts_with("fonts/") {
                    cx.err_at("src", format!("font src {src:?} must be under fonts/"));
                } else if !(ends_with_ignore_ascii_case(src, ".ttf")
                    || ends_with_ignore_ascii_case(src, ".otf")
                    || ends_with_ignore_ascii_case(src, ".ttc")
                    || ends_with_ignore_ascii_case(src, ".otc"))
                {
                    cx.err_at(
                        "src",
                        format!("font src {src:?} must be a .ttf or .otf file (or .ttc/.otc)"),
                    );
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

/// The course-wide background track, when one is set.
fn check_background_audio(course: &Course, cx: &mut Ctx<'_>) {
    let Some(audio) = &course.background_audio else {
        return;
    };
    let src = audio.src.trim();
    if src.is_empty() {
        cx.err_at("background_audio/src", "background audio src is empty");
    } else if !is_safe_relative_path(src) {
        cx.err_at(
            "background_audio/src",
            format!("background audio src {src:?} must be a safe relative path under media/"),
        );
    }
    if !audio.volume.is_finite() || !(0.0..=1.0).contains(&audio.volume) {
        cx.err_at(
            "background_audio/volume",
            "background audio volume must be finite and within 0.0..=1.0",
        );
    }
}

/// Warn when a font name is neither Inter nor a registered course family.
fn check_font_name(course: &Course, cx: &mut Ctx<'_>, name: &str, suffix: &str) {
    if cx.skip_warnings {
        return;
    }
    let name = name.trim();
    if name.is_empty() {
        cx.warn_at(suffix, "font name is empty; Inter is used instead");
    } else if !course.has_font_family(name) {
        cx.warn_at(
            suffix,
            format!("font {name:?} is not Inter or a course font; Inter is used instead"),
        );
    }
}

fn check_palette(course: &Course, cx: &mut Ctx<'_>) {
    let palette = &course.theme.palette;
    if palette.len() > crate::MAX_PALETTE_COLORS {
        cx.err_at(
            "theme/palette",
            format!(
                "a palette holds at most {} colours",
                crate::MAX_PALETTE_COLORS
            ),
        );
    }
    if palette.is_empty() || cx.should_stop() {
        return;
    }
    let mut ids = BTreeSet::new();
    cx.with_field("theme", |cx| {
        cx.with_field("palette", |cx| {
            for (i, entry) in palette.iter().enumerate() {
                cx.with_index(i, |cx| {
                    if !valid_name(&entry.id) {
                        cx.err(format!(
                            "palette id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            entry.id
                        ));
                    }
                    if !ids.insert(entry.id.as_str()) {
                        cx.err(format!("duplicate palette id {:?}", entry.id));
                    }
                    if entry.name.trim().is_empty() {
                        cx.err("palette colour name is empty");
                    }
                    if !entry.color.is_valid() {
                        cx.err_at(
                            "color",
                            format!("color {:?} is not #RRGGBB or #RRGGBBAA", entry.color.0),
                        );
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    });
}

fn check_text_styles(course: &Course, cx: &mut Ctx<'_>) {
    let styles = &course.text_styles;
    if styles.len() > crate::MAX_TEXT_STYLES {
        cx.err_at(
            "text_styles",
            format!(
                "a course holds at most {} text styles",
                crate::MAX_TEXT_STYLES
            ),
        );
    }
    if styles.is_empty() || cx.should_stop() {
        return;
    }
    let mut ids = BTreeSet::new();
    cx.with_field("text_styles", |cx| {
        for (i, named) in styles.iter().enumerate() {
            cx.with_index(i, |cx| {
                if !valid_name(&named.id) {
                    cx.err(format!(
                        "text style id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                        named.id
                    ));
                }
                if !ids.insert(named.id.as_str()) {
                    cx.err(format!("duplicate text style id {:?}", named.id));
                }
                if named.name.trim().is_empty() {
                    cx.err("text style name is empty");
                }
                if !named.style.color.is_valid() {
                    cx.err_at(
                        "style/color",
                        format!(
                            "color {:?} is not #RRGGBB or #RRGGBBAA",
                            named.style.color.0
                        ),
                    );
                }
                check_font_name(course, cx, &named.style.font, "style/font");
                if let Some(paragraph) = &named.paragraphs {
                    check_paragraphs(cx, std::slice::from_ref(paragraph));
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn check_masters_and_templates(course: &Course, cx: &mut Ctx<'_>, slide_ids: &BTreeSet<&str>) {
    let mut master_ids = BTreeSet::new();
    if !course.masters.is_empty() {
        cx.with_field("masters", |cx| {
            for (mi, master) in course.masters.iter().enumerate() {
                cx.with_index(mi, |cx| {
                    if !valid_name(&master.id) {
                        cx.err(format!(
                            "master id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            master.id
                        ));
                    }
                    if !master_ids.insert(master.id.as_str()) {
                        cx.err(format!("duplicate master id {:?}", master.id));
                    }
                    if slide_ids.contains(master.id.as_str()) {
                        cx.err(format!(
                            "master id {:?} is also a slide id; slide ids are unique across scenes and masters",
                            master.id
                        ));
                    }
                    if !cx.should_stop() {
                        check_master(course, cx, master);
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    }
    if cx.should_stop() {
        return;
    }
    if !course.templates.is_empty() {
        let mut template_ids = BTreeSet::new();
        cx.with_field("templates", |cx| {
            for (ti, template) in course.templates.iter().enumerate() {
                cx.with_index(ti, |cx| {
                    if !valid_name(&template.id) {
                        cx.err(format!(
                            "template id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            template.id
                        ));
                    }
                    if !template_ids.insert(template.id.as_str()) {
                        cx.err(format!("duplicate template id {:?}", template.id));
                    }
                    if slide_ids.contains(template.id.as_str())
                        || master_ids.contains(template.id.as_str())
                    {
                        cx.err(format!(
                            "template id {:?} is also a slide or master id; slide ids are unique across scenes, masters and templates",
                            template.id
                        ));
                    }
                    if template.draw.is_some() {
                        cx.err_at(
                            "draw",
                            format!("template {:?} must not have draw", template.id),
                        );
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    }
    if cx.skip_warnings || cx.should_stop() {
        return;
    }
    for (si, scene) in course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            if let Some(ref master) = slide.master
                && !master_ids.contains(master.as_str())
            {
                cx.with_field_index("scenes", si, |cx| {
                    cx.with_field_index("slides", li, |cx| {
                        cx.warn_at(
                            "master",
                            format!(
                                "slide {:?} names master {master:?}, which does not exist; it draws with no master",
                                slide.id
                            ),
                        );
                    });
                });
            }
        }
    }
    for (bi, bank) in course.banks.iter().enumerate() {
        for (li, slide) in bank.slides.iter().enumerate() {
            if let Some(ref master) = slide.master
                && !master_ids.contains(master.as_str())
            {
                cx.with_field_index("banks", bi, |cx| {
                    cx.with_field_index("slides", li, |cx| {
                        cx.warn_at(
                            "master",
                            format!(
                                "question-bank slide {:?} names master {master:?}, which does not exist; it draws with no master",
                                slide.id
                            ),
                        );
                    });
                });
            }
        }
    }
}

fn check_banks<'a>(course: &'a Course, cx: &mut Ctx<'a>, slide_ids: &mut BTreeSet<&'a str>) {
    if course.banks.is_empty() {
        return;
    }
    let mut bank_ids = BTreeSet::new();
    cx.with_field("banks", |cx| {
        for (bi, bank) in course.banks.iter().enumerate() {
            cx.with_index(bi, |cx| {
                if !valid_name(&bank.id) {
                    cx.err(format!(
                        "bank id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                        bank.id
                    ));
                }
                if !bank_ids.insert(bank.id.as_str()) {
                    cx.err(format!("duplicate bank id {:?}", bank.id));
                }
                if bank.title.trim().is_empty() {
                    cx.err_at("title", "bank title is empty");
                }
                if cx.should_stop() {
                    return;
                }
                cx.with_field("slides", |cx| {
                    for (si, slide) in bank.slides.iter().enumerate() {
                        cx.with_index(si, |cx| {
                            if !valid_name(&slide.id) {
                                cx.err(format!(
                                    "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                                    slide.id
                                ));
                            }
                            if !slide_ids.insert(slide.id.as_str()) {
                                cx.err(format!("duplicate slide id {:?}", slide.id));
                            }
                            if !cx.should_stop() {
                                check_slide(course, cx, slide);
                            }
                            if !cx.should_stop() {
                                check_bank_slide(cx, slide);
                            }
                        });
                        if cx.should_stop() {
                            return;
                        }
                    }
                });
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn check_bank_slide(cx: &mut Ctx<'_>, slide: &Slide) {
    if slide.question.is_none() {
        cx.err_at(
            "question",
            format!("bank slide {:?} must have a question", slide.id),
        );
    }
    if slide.draw.is_some() {
        cx.err_at(
            "draw",
            format!("bank slide {:?} must not have draw", slide.id),
        );
    }
}

fn check_single_draw(course: &Course, cx: &mut Ctx<'_>, slide: &Slide, draw: &crate::BankDraw) {
    let bank = course.bank(&draw.bank);
    if bank.is_none() {
        cx.err(format!("draw bank {:?} does not exist", draw.bank));
    }
    if slide.question.is_some() {
        cx.err(format!(
            "slide {:?} with draw must not have a question",
            slide.id
        ));
    }
    if draw.count == 0 {
        cx.err_at("count", "draw count must be at least 1");
    } else if let Some(bank) = bank {
        let size = bank.slides.len() as u64;
        if u64::from(draw.count) > size {
            cx.err_at(
                "count",
                format!(
                    "draw count {} is more than bank {:?} size {size}",
                    draw.count, draw.bank
                ),
            );
        }
    }
}

fn check_single_slide_draw_total(
    course: &Course,
    slide: &Slide,
    draw: &crate::BankDraw,
) -> Option<Issue> {
    let (bi, bank) = course
        .banks
        .iter()
        .enumerate()
        .find(|(_, b)| b.id == draw.bank)?;
    let mut total = 0u64;
    let mut saw_slide = false;
    for scene in &course.scenes {
        for s in &scene.slides {
            let d = if s.id == slide.id {
                saw_slide = true;
                slide.draw.as_ref()
            } else {
                s.draw.as_ref()
            };
            if let Some(d) = d
                && d.bank == draw.bank
            {
                total = total.saturating_add(u64::from(d.count));
            }
        }
    }
    if !saw_slide {
        total = total.saturating_add(u64::from(draw.count));
    }
    let size = bank.slides.len() as u64;
    if total > size {
        Some(Issue {
            severity: Severity::Error,
            path: format!("/banks/{bi}"),
            message: format!(
                "draw counts from bank {:?} add up to {total}, more than the bank size {size}",
                bank.id
            ),
        })
    } else {
        None
    }
}

fn check_draws(course: &Course, cx: &mut Ctx<'_>) {
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    for (si, scene) in course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            let Some(draw) = &slide.draw else { continue };
            cx.with_field_index("scenes", si, |cx| {
                cx.with_field_index("slides", li, |cx| {
                    cx.with_field("draw", |cx| check_single_draw(course, cx, slide, draw));
                });
            });
            if cx.should_stop() {
                return;
            }
            if draw.count != 0 && course.bank(&draw.bank).is_some() {
                let total = totals.entry(draw.bank.as_str()).or_insert(0);
                *total = total.saturating_add(u64::from(draw.count));
            }
        }
    }
    if !totals.is_empty() {
        check_draw_totals(course, cx, &totals);
    }
}

fn check_draw_totals(course: &Course, cx: &mut Ctx<'_>, totals: &BTreeMap<&str, u64>) {
    for (bi, bank) in course.banks.iter().enumerate() {
        let Some(total) = totals.get(bank.id.as_str()) else {
            continue;
        };
        let size = bank.slides.len() as u64;
        if *total > size {
            cx.with_field_index("banks", bi, |cx| {
                cx.err(format!(
                    "draw counts from bank {:?} add up to {total}, more than the bank size {size}",
                    bank.id
                ));
            });
            if cx.should_stop() {
                return;
            }
        }
    }
}

fn check_tracking(course: &Course, cx: &mut Ctx<'_>) {
    if let Some(sv) = course.tracking.slides_viewed {
        match sv {
            SlidesViewedTracking::AtLeast { count: 0 } => {
                cx.err_at(
                    "tracking/slides_viewed/count",
                    "slides_viewed count must be at least 1",
                );
            }
            SlidesViewedTracking::AtLeast { count } => {
                let total = course.play_slide_count();
                if total > 0 && (count as usize) > total {
                    cx.warn_at(
                        "tracking/slides_viewed/count",
                        format!(
                            "slides_viewed count ({count}) exceeds the course slide count ({total}); completion will trigger after viewing all {total} slides"
                        ),
                    );
                }
            }
            SlidesViewedTracking::All => {}
        }
    }

    if let Some(qt) = &course.tracking.quiz {
        match qt {
            QuizTracking { slide: Some(id) } => {
                if course.slide_index(id).is_none() {
                    cx.warn_at(
                        "tracking/quiz/slide",
                        format!("quiz tracking slide {id:?} does not exist in play order"),
                    );
                } else if let Some(slide) = course.play_slide(id)
                    && !slide.has_submit_results_trigger()
                    && slide.question.is_none()
                {
                    cx.warn_at(
                        "tracking/quiz/slide",
                        format!(
                            "quiz tracking slide {id:?} has no Submit results trigger or question; quiz score will be reported on slide entry"
                        ),
                    );
                }
            }
            QuizTracking { slide: None } => {
                if !course.has_graded_questions() && course.results_slides().next().is_none() {
                    cx.warn_at(
                        "tracking/quiz",
                        "quiz result tracking is enabled, but the course has no graded questions or results slide",
                    );
                }
            }
        }
        if course.quiz.is_none() {
            cx.warn_at(
                "tracking/quiz",
                "quiz result tracking is enabled, but no passing score is set in Quiz settings",
            );
        }
    }
}

fn check_course_results(course: &Course, cx: &mut Ctx<'_>) {
    if !course.reports_result() {
        cx.warn_at(
            "scenes",
            "this course never reports a result to the LMS: add a Finish button on the last slide",
        );
    }
}

fn check_slide(course: &Course, cx: &mut Ctx<'_>, slide: &Slide) {
    if slide.duration_ms > crate::MAX_TIMELINE_DURATION_MS {
        cx.err_at(
            "duration_ms",
            format!(
                "slide duration_ms must be at most {}",
                crate::MAX_TIMELINE_DURATION_MS
            ),
        );
    }
    if let Some(ref bg) = slide.background
        && !bg.is_valid()
    {
        cx.err_at(
            "background",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", bg.0),
        );
    }
    if let Some(ref tr) = slide.transition
        && (tr.duration_ms == 0 || tr.duration_ms > 60000)
    {
        cx.err_at(
            "transition/duration_ms",
            "transition duration_ms must be 1..=60000",
        );
    }
    if cx.should_stop() {
        return;
    }
    let mut ids = BTreeSet::new();
    for id in slide
        .objects
        .iter()
        .map(|o| o.id.as_str())
        .chain(slide.layers.iter().map(|l| l.id.as_str()))
        .chain(
            slide
                .layers
                .iter()
                .flat_map(|l| l.objects.iter().map(|o| o.id.as_str())),
        )
    {
        if !valid_name(id) {
            cx.err(format!(
                "object/layer id {id:?} must be 1-128 chars of [A-Za-z0-9_-]"
            ));
        }
        if !ids.insert(id) {
            cx.err(format!(
                "duplicate object/layer id {id:?} on slide {:?}",
                slide.id
            ));
        }
        if cx.should_stop() {
            return;
        }
    }
    if !slide.objects.is_empty() {
        cx.with_field("objects", |cx| {
            for (i, o) in slide.objects.iter().enumerate() {
                cx.with_index(i, |cx| {
                    check_object(course, cx, o, slide.duration_ms, &slide.id)
                });
                if cx.should_stop() {
                    return;
                }
            }
            if !cx.skip_warnings {
                check_group_members(cx, &slide.objects);
            }
            check_panel_members(cx, &slide.objects);
        });
    }
    if cx.should_stop() {
        return;
    }
    if !slide.groups.is_empty() {
        cx.with_field("groups", |cx| {
            check_groups_list(cx, slide, &slide.objects, &slide.groups, &mut ids, false);
        });
    }
    if cx.should_stop() {
        return;
    }
    check_slide_draggable_objects(cx, slide);
    if cx.should_stop() {
        return;
    }
    if !slide.triggers.is_empty() {
        cx.with_field("triggers", |cx| {
            for (i, t) in slide.triggers.iter().enumerate() {
                cx.with_index(i, |cx| check_trigger(course, cx, slide, None, t));
                if cx.should_stop() {
                    return;
                }
            }
        });
    }
    if cx.should_stop() {
        return;
    }
    check_slide_layers(course, cx, slide, &mut ids);
    if cx.should_stop() {
        return;
    }
    check_question(course, cx, slide);
    if cx.skip_warnings || cx.should_stop() {
        return;
    }
    if slide.revisit == crate::RevisitMode::Auto
        && slide_changes_states_from_timeline_or_media(slide)
    {
        cx.warn_at(
            "revisit",
            format!(
                "Slide \u{201C}{}\u{201D} changes object states from its timeline or media, but revisiting keeps old states while the timeline restarts. Set When revisiting to Reset to start fresh.",
                slide.title
            ),
        );
    }
    if slide_has_unsupported_script(slide) {
        cx.warn(
            "text uses characters the bundled Inter font has no glyphs for; right-to-left and complex shaping are not supported",
        );
    }
}

fn check_slide_layers<'a>(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &'a Slide,
    ids: &mut BTreeSet<&'a str>,
) {
    if slide.layers.is_empty() {
        return;
    }
    cx.with_field("layers", |cx| {
        for (li, layer) in slide.layers.iter().enumerate() {
            cx.with_index(li, |cx| {
                if layer.duration_ms > crate::MAX_TIMELINE_DURATION_MS {
                    cx.err_at(
                        "duration_ms",
                        format!(
                            "layer duration_ms must be at most {}",
                            crate::MAX_TIMELINE_DURATION_MS
                        ),
                    );
                }
                if !cx.should_stop() && !layer.objects.is_empty() {
                    cx.with_field("objects", |cx| {
                        for (i, o) in layer.objects.iter().enumerate() {
                            cx.with_index(i, |cx| {
                                check_object(course, cx, o, layer.duration_ms, &slide.id)
                            });
                            if cx.should_stop() {
                                return;
                            }
                        }
                        if !cx.skip_warnings {
                            check_group_members(cx, &layer.objects);
                        }
                        check_panel_members(cx, &layer.objects);
                    });
                }
                if !cx.should_stop() && !layer.groups.is_empty() {
                    cx.with_field("groups", |cx| {
                        check_groups_list(cx, slide, &layer.objects, &layer.groups, ids, false);
                    });
                }
                if !cx.should_stop() && !layer.triggers.is_empty() {
                    cx.with_field("triggers", |cx| {
                        for (i, t) in layer.triggers.iter().enumerate() {
                            cx.with_index(i, |cx| check_trigger(course, cx, slide, Some(layer), t));
                            if cx.should_stop() {
                                return;
                            }
                        }
                    });
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn slide_changes_states_from_timeline_or_media(slide: &Slide) -> bool {
    slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .any(|t| {
            !t.disabled
                && matches!(
                    t.when,
                    Event::TimelineStarts { .. }
                        | Event::TimelineEnds { .. }
                        | Event::TimelineReaches { .. }
                        | Event::CueReached { .. }
                        | Event::MediaEnds { .. }
                )
                && matches!(t.action, Action::ChangeState { .. })
        })
}

fn master_label(master: &Slide) -> String {
    if master.title.trim().is_empty() {
        format!("master {:?}", master.id)
    } else {
        format!("master {:?} ({:?})", master.id, master.title)
    }
}

/// Check a slide master. A master is a slide for authoring, but it must never
/// carry play-time behaviour: no triggers, layers or question, and no audio or
/// video. Its objects get the same checks as a slide's, named with the master.
fn check_master(course: &Course, cx: &mut Ctx<'_>, master: &Slide) {
    if !master.triggers.is_empty() {
        let label = master_label(master);
        cx.err_at("triggers", format!("{label} must not have triggers"));
    }
    if !master.layers.is_empty() {
        let label = master_label(master);
        cx.err_at("layers", format!("{label} must not have layers"));
    }
    if master.question.is_some() {
        let label = master_label(master);
        cx.err_at("question", format!("{label} must not have a question"));
    }
    if master.draw.is_some() {
        let label = master_label(master);
        cx.err_at("draw", format!("{label} must not have draw"));
    }
    if cx.should_stop() {
        return;
    }
    if !master.objects.is_empty() {
        cx.with_field("objects", |cx| {
            for (i, object) in master.objects.iter().enumerate() {
                if matches!(
                    object.kind,
                    ObjectKind::Audio { .. } | ObjectKind::Video { .. } | ObjectKind::Lottie { .. }
                ) {
                    let label = master_label(master);
                    cx.with_index(i, |cx| {
                        cx.err_at(
                            "kind",
                            format!(
                                "{label} must not hold audio or video (or Lottie) object {:?}",
                                object.id
                            ),
                        );
                    });
                    if cx.should_stop() {
                        return;
                    }
                }
                if !cx.skip_warnings && !object.states.is_empty() {
                    let label = master_label(master);
                    cx.with_index(i, |cx| {
                        cx.warn_at(
                            "states",
                            format!(
                                "{label}: master objects do not have states (object {:?})",
                                object.id
                            ),
                        );
                    });
                }
            }
        });
    }
    if cx.should_stop() {
        return;
    }
    if let Some(ref bg) = master.background
        && !bg.is_valid()
    {
        cx.err_at(
            "background",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", bg.0),
        );
    }
    if cx.should_stop() {
        return;
    }
    let mut ids = BTreeSet::new();
    for id in master
        .objects
        .iter()
        .map(|o| o.id.as_str())
        .chain(master.layers.iter().map(|l| l.id.as_str()))
        .chain(
            master
                .layers
                .iter()
                .flat_map(|l| l.objects.iter().map(|o| o.id.as_str())),
        )
    {
        if !valid_name(id) {
            let label = master_label(master);
            cx.err(format!(
                "{label}: object/layer id {id:?} must be 1-128 chars of [A-Za-z0-9_-]"
            ));
        }
        if !ids.insert(id) {
            let label = master_label(master);
            cx.err(format!("{label}: duplicate object/layer id {id:?}"));
        }
        if cx.should_stop() {
            return;
        }
    }
    // Run the ordinary object and group checks, then name the master in every new message.
    let first_new = cx.issues.len();
    if !master.objects.is_empty() {
        cx.with_field("objects", |cx| {
            for (i, o) in master.objects.iter().enumerate() {
                cx.with_index(i, |cx| check_object(course, cx, o, master.duration_ms, ""));
                if cx.should_stop() {
                    return;
                }
            }
            if !cx.skip_warnings {
                check_group_members(cx, &master.objects);
            }
            check_panel_members(cx, &master.objects);
        });
    }
    if !cx.should_stop() && !master.groups.is_empty() {
        cx.with_field("groups", |cx| {
            check_groups_list(cx, master, &master.objects, &master.groups, &mut ids, true);
        });
    }
    if cx.issues.len() > first_new {
        let label = master_label(master);
        for issue in cx.issues.get_mut(first_new..).into_iter().flatten() {
            if !issue.message.starts_with(&label) {
                issue.message = format!("{label}: {}", issue.message);
            }
        }
    }
    if !cx.skip_warnings && !cx.should_stop() && slide_has_unsupported_script(master) {
        let label = master_label(master);
        cx.warn(format!(
            "{label}: text uses characters the bundled Inter font has no glyphs for; right-to-left and complex shaping are not supported"
        ));
    }
}

/// True when any learner-visible string on `slide` uses a script the bundled
/// font cannot draw.
fn slide_has_unsupported_script(slide: &Slide) -> bool {
    let bad = |s: &str| s.chars().any(crate::script_unsupported);
    if bad(&slide.title) || bad(&slide.notes) {
        return true;
    }
    let object_bad = |o: &Object| {
        if let Some(description) = &o.description
            && bad(description)
        {
            return true;
        }
        (match &o.kind {
            ObjectKind::Text { text, runs, .. } => {
                bad(text) || runs.iter().any(|run| bad(&run.text))
            }
            ObjectKind::Button { label, .. } => bad(label),
            ObjectKind::Image { alt, .. } => bad(alt),
            ObjectKind::Shape { text, .. } => text
                .as_ref()
                .is_some_and(|text| bad(&text.text) || text.runs.iter().any(|run| bad(&run.text))),
            ObjectKind::Table { rows, .. } => rows.iter().any(|row| {
                row.cells
                    .iter()
                    .any(|cell| bad(&cell.text) || cell.runs.iter().any(|run| bad(&run.text)))
            }),
            ObjectKind::TextEntry { placeholder, .. } => bad(placeholder),
            ObjectKind::Checkbox { label, .. } => bad(label),
            ObjectKind::RadioGroup { options, .. } => {
                options.iter().any(|option| bad(&option.label))
            }
            ObjectKind::Marker {
                title, body, runs, ..
            } => bad(title) || bad(body) || runs.iter().any(|run| bad(&run.text)),
            ObjectKind::Audio { .. }
            | ObjectKind::Video { .. }
            | ObjectKind::Lottie { .. }
            | ObjectKind::Web { .. }
            | ObjectKind::Slider { .. }
            | ObjectKind::Dial { .. }
            | ObjectKind::ScrollPanel { .. }
            | ObjectKind::Timer { .. }
            | ObjectKind::ProgressMeter { .. }
            | ObjectKind::Path { .. } => false,
        }) || o.states.iter().any(|state| {
            state.text.as_deref().is_some_and(bad)
                || state
                    .runs
                    .as_deref()
                    .is_some_and(|runs| runs.iter().any(|run| bad(&run.text)))
                || state.extras.iter().any(|extra| match &extra.kind {
                    StateExtraKind::Text { text, runs, .. } => {
                        bad(text) || runs.iter().any(|run| bad(&run.text))
                    }
                    StateExtraKind::Shape { text: Some(t), .. } => {
                        bad(&t.text) || t.runs.iter().any(|run| bad(&run.text))
                    }
                    StateExtraKind::Image { alt, .. } => bad(alt),
                    StateExtraKind::Shape { text: None, .. } | StateExtraKind::Path { .. } => false,
                })
        })
    };
    if slide.objects.iter().any(object_bad)
        || slide
            .layers
            .iter()
            .flat_map(|layer| layer.objects.iter())
            .any(object_bad)
    {
        return true;
    }
    slide.question.as_ref().is_some_and(|question| {
        bad(&question.prompt)
            || question.choices.iter().any(|choice| bad(&choice.text))
            || question.accepted.iter().any(|answer| bad(answer))
            || question
                .pairs
                .iter()
                .any(|pair| bad(&pair.left) || bad(&pair.right))
    })
}

/// Validate explicit [`Group`] records on a slide base layer or slide layer.
fn check_groups_list<'a>(
    cx: &mut Ctx<'_>,
    slide: &'a Slide,
    owner_objects: &[Object],
    groups: &'a [Group],
    ids: &mut BTreeSet<&'a str>,
    is_master: bool,
) {
    for (gi, g) in groups.iter().enumerate() {
        cx.with_index(gi, |cx| {
            if !valid_name(&g.id) {
                cx.err_at(
                    "id",
                    format!("group id {:?} must be 1-128 chars of [A-Za-z0-9_-]", g.id),
                );
            }
            if !ids.insert(g.id.as_str()) {
                cx.err_at(
                    "id",
                    format!(
                        "group id {:?} is duplicated or collides with an object or layer id on slide {:?}",
                        g.id, slide.id
                    ),
                );
            }
            if cx.should_stop() {
                return;
            }
            let members: Vec<&Object> = owner_objects
                .iter()
                .filter(|o| o.group.as_deref() == Some(g.id.as_str()))
                .collect();
            if members.is_empty() {
                cx.err(format!(
                    "group {:?} has no member objects on this layer",
                    g.id
                ));
                if cx.should_stop() {
                    return;
                }
            }
            if !g.interactive {
                return;
            }
            if is_master {
                let label = master_label(slide);
                cx.err_at(
                    "interactive",
                    format!("{label} must not have interactive group {:?}", g.id),
                );
                if cx.should_stop() {
                    return;
                }
            }
            for member in &members {
                if crate::is_disallowed_interactive_group_member(member, slide.question.as_ref()) {
                    cx.err(format!(
                        "interactive group {:?} cannot contain object {:?} (inputs, scrolling panels, web, media, markers, tables, draggable objects, and question items are not allowed)",
                        g.id, member.id
                    ));
                    if cx.should_stop() {
                        return;
                    }
                }
            }
            if !slide.group_has_state(&g.id, &g.initial_state) {
                cx.err_at(
                    "initial_state",
                    format!(
                        "initial_state {:?} is not defined on any member of interactive group {:?}",
                        g.initial_state, g.id
                    ),
                );
            }
        });
        if cx.should_stop() {
            return;
        }
    }
}

/// Warn when a group id has only one member on this layer.
fn check_group_members(cx: &mut Ctx<'_>, objects: &[Object]) {
    if cx.skip_warnings {
        return;
    }
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() {
        if let Some(group) = object.group.as_deref() {
            let entry = counts.entry(group).or_insert((0, index));
            entry.0 = entry.0.saturating_add(1);
        }
    }
    for (group, (count, index)) in counts {
        if count == 1 {
            cx.with_index(index, |cx| {
                cx.warn(format!("group {group:?} has only one member on this layer"));
            });
        }
    }
}

/// Check that every `panel` link names a scrolling panel on the same layer,
/// and that no panel sits inside another panel.
fn check_panel_members(cx: &mut Ctx<'_>, objects: &[Object]) {
    for (index, object) in objects.iter().enumerate() {
        let Some(panel_id) = object.panel.as_deref() else {
            continue;
        };
        cx.with_index(index, |cx| {
            if matches!(object.kind, ObjectKind::ScrollPanel { .. }) {
                cx.err_at(
                    "panel",
                    "a scrolling panel cannot be inside another scrolling panel",
                );
                return;
            }
            match objects.iter().find(|candidate| candidate.id == panel_id) {
                None => cx.err_at(
                    "panel",
                    format!("panel {panel_id:?} is not a scrolling panel on this layer"),
                ),
                Some(panel) if !matches!(panel.kind, ObjectKind::ScrollPanel { .. }) => cx.err_at(
                    "panel",
                    format!("object {panel_id:?} is not a scrolling panel"),
                ),
                Some(_) => {}
            }
        });
        if cx.should_stop() {
            return;
        }
    }
}

fn check_draggable_object(cx: &mut Ctx<'_>, slide: &Slide, o: &Object) {
    let Some(drag) = &o.drag else { return };
    if slide.question.as_ref().is_some_and(|q| match q.kind {
        QuestionKind::Hotspot => q.hotspot_image.as_deref() == Some(&o.id),
        QuestionKind::DragDrop => {
            q.drag_items.iter().any(|d| d.item == o.id) || q.drop_targets.iter().any(|t| t == &o.id)
        }
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::FillIn
        | QuestionKind::Sequence
        | QuestionKind::Matching
        | QuestionKind::Likert
        | QuestionKind::ShortAnswer
        | QuestionKind::Essay
        | QuestionKind::Freeform => false,
    }) {
        cx.err_at(
            "drag",
            format!(
                "draggable spec is not allowed on question widget {:?}",
                o.id
            ),
        );
    }
    if cx.should_stop() || drag.targets.is_empty() {
        return;
    }
    for (ti, target_id) in drag.targets.iter().enumerate() {
        if slide.object(target_id).is_none() {
            cx.with_field("drag", |cx| {
                cx.with_field_index("targets", ti, |cx| {
                    cx.err(format!(
                        "target {target_id:?} is not on slide {:?}",
                        slide.id
                    ));
                });
            });
            if cx.should_stop() {
                return;
            }
        }
    }
}

fn check_slide_draggable_objects(cx: &mut Ctx<'_>, slide: &Slide) {
    for (i, o) in slide.objects.iter().enumerate() {
        if o.drag.is_some() {
            cx.with_field_index("objects", i, |cx| check_draggable_object(cx, slide, o));
            if cx.should_stop() {
                return;
            }
        }
    }
    for (li, layer) in slide.layers.iter().enumerate() {
        for (i, o) in layer.objects.iter().enumerate() {
            if o.drag.is_some() {
                cx.with_field_index("layers", li, |cx| {
                    cx.with_field_index("objects", i, |cx| check_draggable_object(cx, slide, o));
                });
                if cx.should_stop() {
                    return;
                }
            }
        }
    }
}

/// Describe what is wrong with an image crop, or `None` when it is a positive
/// window inside the source picture.
fn check_crop_values(crop: crate::CropRect) -> Option<&'static str> {
    let (x, y, w, h) = (crop.x, crop.y, crop.w, crop.h);
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return Some("crop values must be finite");
    }
    if x < 0.0 || y < 0.0 || x > 1.0 || y > 1.0 || w > 1.0 || h > 1.0 {
        return Some(
            "crop must be a window inside the source: x, y, w and h are fractions in 0..=1",
        );
    }
    if w <= 0.0 || h <= 0.0 {
        return Some("crop w and h must be greater than 0");
    }
    if x + w > 1.000_001 || y + h > 1.000_001 {
        return Some("crop runs past the source: x + w and y + h must be at most 1");
    }
    None
}

fn text_matches_runs(text: &str, runs: &[crate::TextRun]) -> bool {
    let mut rest = text;
    for run in runs {
        let Some(after) = rest.strip_prefix(run.text.as_str()) else {
            return false;
        };
        rest = after;
    }
    rest.is_empty()
}

fn check_text_runs(
    course: &Course,
    cx: &mut Ctx<'_>,
    expected_text: Option<&str>,
    runs: &[crate::TextRun],
) {
    if runs.is_empty() {
        return;
    }
    if runs.len() > 1000 {
        cx.err_at("runs", "text object has more than 1000 runs");
        if cx.should_stop() {
            return;
        }
    }
    cx.with_field("runs", |cx| {
        for (ri, run) in runs.iter().enumerate() {
            cx.with_index(ri, |cx| {
                if let Some(ref c) = run.color
                    && !c.is_valid()
                {
                    cx.err_at(
                        "color",
                        format!("color {:?} is not #RRGGBB or #RRGGBBAA", c.0),
                    );
                }
                if let Some(ref c) = run.highlight
                    && !c.is_valid()
                {
                    cx.err_at(
                        "highlight",
                        format!("highlight {:?} is not #RRGGBB or #RRGGBBAA", c.0),
                    );
                }
                if !cx.skip_warnings
                    && let Some(font) = &run.font
                {
                    check_font_name(course, cx, font, "font");
                }
                if let Some(link) = &run.link
                    && !link.is_valid(course)
                {
                    let message = match link {
                        crate::Link::Url { url } => {
                            format!("link {url:?} must start with http://, https:// or mailto:")
                        }
                        crate::Link::Slide { slide } => {
                            format!("link target slide {slide:?} does not exist")
                        }
                    };
                    cx.err_at("link", message);
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
    if !cx.skip_warnings
        && !cx.should_stop()
        && let Some(text) = expected_text
        && !text_matches_runs(text, runs)
    {
        cx.warn_at("text", "text does not match the concatenation of runs");
    }
}

/// Paragraph formatting is bounded; the runtime clamps hostile values, but a
/// loaded file should say what is wrong.
fn check_paragraphs(cx: &mut Ctx<'_>, paragraphs: &[crate::Paragraph]) {
    if paragraphs.is_empty() {
        return;
    }
    cx.with_field("paragraphs", |cx| {
        for (i, paragraph) in paragraphs.iter().enumerate() {
            cx.with_index(i, |cx| {
                if paragraph.level > 4 {
                    cx.err_at("level", "paragraph level must be within 0..=4");
                }
                if !paragraph.line_spacing.is_finite()
                    || !(0.8..=3.0).contains(&paragraph.line_spacing)
                {
                    cx.err_at(
                        "line_spacing",
                        "line_spacing must be finite and within 0.8..=3",
                    );
                }
                for (name, value) in [
                    ("space_before", paragraph.space_before),
                    ("space_after", paragraph.space_after),
                ] {
                    if !value.is_finite() || !(0.0..=200.0).contains(&value) {
                        cx.err_at(name, format!("{name} must be finite and within 0..=200"));
                    }
                }
                if let Some(color) = &paragraph.bullet_color
                    && !color.is_valid()
                {
                    cx.err_at("bullet_color", "bullet_color must be a valid hex colour");
                }
                if let Some(size) = paragraph.bullet_size
                    && (!size.is_finite() || !(1.0..=400.0).contains(&size))
                {
                    cx.err_at(
                        "bullet_size",
                        "bullet_size must be finite and within 1..=400",
                    );
                }
                if paragraph.start_at == Some(0) {
                    cx.err_at("start_at", "start_at must be at least 1");
                }
                if paragraph.start_at.is_some_and(|n| n > 99_999) {
                    cx.warn_at(
                        "start_at",
                        "start_at above 99999 is outside the editor range; Roman numbering falls back to digits above 3999",
                    );
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn check_fill_style(cx: &mut Ctx<'_>, style: &crate::FillStyle) {
    cx.with_field("fill_style", |cx| match style {
        crate::FillStyle::Linear { angle, stops } => {
            if !angle.is_finite() || !(0.0..360.0).contains(angle) {
                cx.err_at(
                    "angle",
                    "gradient angle must be a finite number of degrees in 0..360",
                );
            }
            if !cx.should_stop() {
                check_gradient_stops(cx, stops);
            }
        }
        crate::FillStyle::Radial { center, stops } => {
            if !center.iter().all(|value| value.is_finite())
                || !(0.0..=1.0).contains(&center[0])
                || !(0.0..=1.0).contains(&center[1])
            {
                cx.err_at(
                    "center",
                    "radial gradient centre must be finite fractions within 0..=1",
                );
            }
            if !cx.should_stop() {
                check_gradient_stops(cx, stops);
            }
        }
        crate::FillStyle::Picture { src, .. } => {
            if src.trim().is_empty() {
                cx.warn_at(
                    "src",
                    "picture fill has no image; the solid fill is drawn instead",
                );
            } else if !is_safe_relative_path(src) {
                cx.err_at(
                    "src",
                    format!("picture fill path {src:?} must be a safe relative path"),
                );
            }
        }
        crate::FillStyle::Pattern { preset, fg, bg } => {
            if preset.trim().is_empty() {
                cx.warn_at(
                    "preset",
                    "pattern fill has no preset name; falling back to default pattern",
                );
            }
            if !fg.is_valid() {
                cx.err_at(
                    "fg",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", fg.0),
                );
            }
            if !bg.is_valid() {
                cx.err_at(
                    "bg",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", bg.0),
                );
            }
        }
    });
}

fn check_gradient_stops(cx: &mut Ctx<'_>, stops: &[crate::GradientStop]) {
    if !(2..=crate::MAX_GRADIENT_STOPS).contains(&stops.len()) {
        cx.err_at(
            "stops",
            format!(
                "a gradient needs 2 to {} stops, found {}",
                crate::MAX_GRADIENT_STOPS,
                stops.len()
            ),
        );
    }
    if stops.is_empty() || cx.should_stop() {
        return;
    }
    cx.with_field("stops", |cx| {
        for (i, stop) in stops.iter().enumerate() {
            cx.with_index(i, |cx| {
                if !stop.color.is_valid() {
                    cx.err_at(
                        "color",
                        format!("color {:?} is not #RRGGBB or #RRGGBBAA", stop.color.0),
                    );
                }
                if !stop.position.is_finite() || !(0.0..=1.0).contains(&stop.position) {
                    cx.err_at(
                        "position",
                        "gradient stop position must be finite and within 0..=1",
                    );
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn check_outline(cx: &mut Ctx<'_>, outline: &crate::Outline) {
    if !outline.color.is_valid() {
        cx.err_at(
            "color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", outline.color.0),
        );
    }
    if !outline.width.is_finite() || !(0.0..=50.0).contains(&outline.width) {
        cx.err_at("width", "outline width must be finite and within 0..=50");
    }
    check_dash_pattern(cx, outline.dash_pattern.as_deref());
    check_miter_limit(cx, outline.miter_limit);
}

fn check_shape_outline_and_text(
    course: &Course,
    cx: &mut Ctx<'_>,
    shape: crate::ShapeKind,
    outline: Option<&crate::Outline>,
    text: Option<&crate::ShapeText>,
) {
    if outline.is_none() && text.is_none() {
        return;
    }
    cx.with_field("kind", |cx| {
        if let Some(outline) = outline {
            cx.with_field("outline", |cx| {
                if !crate::shape_allows_text(shape) {
                    cx.err("line shapes cannot carry an outline");
                }
                check_outline(cx, outline);
            });
            if cx.should_stop() {
                return;
            }
        }
        if let Some(text) = text {
            cx.with_field("text", |cx| {
                if !crate::shape_allows_text(shape) {
                    cx.err("line shapes cannot carry text");
                }
                check_shape_text(course, cx, text);
            });
        }
    });
}

/// The text inside a shape or path: style colour, font, insets, runs and paragraphs.
fn check_shape_text(course: &Course, cx: &mut Ctx<'_>, text: &crate::ShapeText) {
    if !text.style.color.is_valid() {
        cx.err_at(
            "style/color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", text.style.color.0),
        );
    }
    check_font_name(course, cx, &text.style.font, "style/font");
    check_insets(cx, text.style.insets.as_ref());
    if !cx.should_stop() {
        check_text_runs(course, cx, Some(&text.text), &text.runs);
    }
    if !cx.should_stop() {
        check_paragraphs(cx, &text.paragraphs);
    }
}

/// Insets are clamped when drawn, so a stray value is a warning; a value that
/// is not a number is an error.
fn check_insets(cx: &mut Ctx<'_>, insets: Option<&crate::TextInsets>) {
    let Some(insets) = insets else {
        return;
    };
    for (name, value) in [
        ("left", insets.left),
        ("top", insets.top),
        ("right", insets.right),
        ("bottom", insets.bottom),
    ] {
        if !value.is_finite() {
            cx.err_at(
                &format!("style/insets/{name}"),
                format!("inset {name} must be a finite number"),
            );
        } else if !cx.skip_warnings && !(0.0..=100_000.0).contains(&value) {
            cx.warn_at(
                &format!("style/insets/{name}"),
                format!("inset {name} is outside 0..=100000 and is clamped when drawn"),
            );
        }
    }
}

fn check_path(course: &Course, cx: &mut Ctx<'_>, path: &crate::Path) {
    cx.with_field("kind", |cx| {
        cx.with_field("path", |cx| {
            let count = path.points.len();
            if count < crate::PATH_MIN_POINTS {
                cx.err_at(
                    "points",
                    format!("a path needs at least {} points", crate::PATH_MIN_POINTS),
                );
            }
            if count > crate::PATH_MAX_POINTS {
                cx.err_at(
                    "points",
                    format!("a path may have at most {} points", crate::PATH_MAX_POINTS),
                );
            }
            if !cx.should_stop() && !path.points.is_empty() {
                cx.with_field("points", |cx| {
                    for (i, point) in path.points.iter().enumerate() {
                        if !point.is_valid() {
                            cx.with_index(i, |cx| {
                                cx.err("path points must be finite fractions within 0..=1");
                            });
                            if cx.should_stop() {
                                return;
                            }
                        }
                    }
                });
            }
            if cx.should_stop() {
                return;
            }
            if path.fill.is_some() && !path.closed {
                cx.err_at("fill", "a path may only have a fill when it is closed");
            }
            if let Some(fill) = &path.fill
                && !fill.is_valid()
            {
                cx.err_at(
                    "fill",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
                );
            }
            if let Some(fill_style) = &path.fill_style {
                check_fill_style(cx, fill_style);
            }
            cx.with_field("subpaths", |cx| {
                for (i, sub) in path.subpaths.iter().enumerate() {
                    cx.with_index(i, |cx| {
                        if !(crate::PATH_MIN_POINTS..=crate::PATH_MAX_POINTS)
                            .contains(&sub.points.len())
                        {
                            cx.err_at(
                                "points",
                                format!(
                                    "a subpath needs {}..={} points",
                                    crate::PATH_MIN_POINTS,
                                    crate::PATH_MAX_POINTS
                                ),
                            );
                        } else if sub.points.iter().any(|point| !point.is_valid()) {
                            cx.err_at(
                                "points",
                                "path points must be finite fractions within 0..=1",
                            );
                        }
                    });
                    if cx.should_stop() {
                        return;
                    }
                }
            });
            if let Some(text) = &path.text {
                cx.with_field("text", |cx| check_shape_text(course, cx, text));
            }
            cx.with_field("outline", |cx| {
                check_outline(cx, &path.outline);
            });
            for (end, glue) in [
                ("start_glue", &path.start_glue),
                ("end_glue", &path.end_glue),
            ] {
                if let Some(glue) = glue
                    && glue.object.is_empty()
                {
                    cx.with_field(end, |cx| cx.err_at("object", "glue needs an object id"));
                }
            }
        });
    });
}

/// The first merged cell (row, column) whose span covers a cell that another
/// merge owns. Spans past the table edge are reported elsewhere.
fn overlapping_merge(rows: &[crate::TableRow], columns: usize) -> Option<(usize, usize)> {
    let owners = crate::table_cell_owners(rows, columns);
    for (row, line) in rows.iter().enumerate() {
        for (column, cell) in line.cells.iter().enumerate() {
            if owners
                .get(row)
                .and_then(|o| o.get(column))
                .copied()
                .flatten()
                != Some((row, column))
            {
                continue;
            }
            let row_end = rows
                .len()
                .min(row.saturating_add(usize::from(cell.row_span.max(1))));
            let column_end = columns.min(column.saturating_add(usize::from(cell.col_span.max(1))));
            for covered_row in row..row_end {
                for covered_column in column..column_end {
                    let owner = owners
                        .get(covered_row)
                        .and_then(|o| o.get(covered_column))
                        .copied()
                        .flatten();
                    if owner != Some((row, column)) {
                        return Some((row, column));
                    }
                }
            }
        }
    }
    None
}

fn check_table_cell(
    course: &Course,
    cx: &mut Ctx<'_>,
    index: usize,
    column: usize,
    rows_len: usize,
    cols_len: usize,
    cell: &crate::TableCell,
) {
    if cell.row_span == 0 || cell.col_span == 0 {
        cx.err("cell spans must be at least 1");
    }
    if index.saturating_add(usize::from(cell.row_span)) > rows_len {
        cx.err_at("row_span", "row span runs past the last row");
    }
    if column.saturating_add(usize::from(cell.col_span)) > cols_len {
        cx.err_at("col_span", "column span runs past the last column");
    }
    if let Some(fill) = &cell.fill
        && !fill.is_valid()
    {
        cx.err_at(
            "fill",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
        );
    }
    if !cell.style.color.is_valid() {
        cx.err_at(
            "style/color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", cell.style.color.0),
        );
    }
    check_font_name(course, cx, &cell.style.font, "style/font");
    if !cx.should_stop() {
        check_text_runs(course, cx, Some(&cell.text), &cell.runs);
    }
    if !cx.should_stop() {
        check_paragraphs(cx, &cell.paragraphs);
    }
}

fn check_table(
    course: &Course,
    cx: &mut Ctx<'_>,
    columns: &[f32],
    rows: &[crate::TableRow],
    borders: &crate::TableBorders,
) {
    cx.with_field("kind", |cx| {
        if columns.is_empty() || columns.len() > crate::TABLE_MAX_AXIS {
            cx.err_at(
                "columns",
                format!("a table needs 1..={} columns", crate::TABLE_MAX_AXIS),
            );
        }
        if rows.is_empty() || rows.len() > crate::TABLE_MAX_AXIS {
            cx.err_at(
                "rows",
                format!("a table needs 1..={} rows", crate::TABLE_MAX_AXIS),
            );
        }
        if rows.len().saturating_mul(columns.len()) > crate::TABLE_MAX_CELLS {
            cx.err(format!(
                "a table may have at most {} cells",
                crate::TABLE_MAX_CELLS
            ));
        }
        let sum = |values: &[f32]| -> f32 { values.iter().copied().sum() };
        if !columns
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
            || (sum(columns) - 1.0).abs() > 0.01
        {
            cx.err_at(
                "columns",
                "column widths must be finite fractions summing to 1",
            );
        }
        if cx.should_stop() {
            return;
        }
        if !cx.skip_warnings
            && rows.iter().all(|row| row.cells.len() == columns.len())
            && let Some((row, column)) = overlapping_merge(rows, columns.len())
        {
            cx.warn_at(
                "rows",
                format!(
                    "the merged cell at row {row}, column {column} overlaps another merged cell"
                ),
            );
        }
        cx.with_field("rows", |cx| {
            for (index, row) in rows.iter().enumerate() {
                cx.with_index(index, |cx| {
                    if !row.height.is_finite() || row.height <= 0.0 {
                        cx.err_at("height", "row height must be a finite fraction above 0");
                    }
                    if row.cells.len() != columns.len() {
                        cx.err_at(
                            "cells",
                            format!(
                                "row {index} has {} cells but the table has {} columns",
                                row.cells.len(),
                                columns.len()
                            ),
                        );
                    }
                    if cx.should_stop() {
                        return;
                    }
                    cx.with_field("cells", |cx| {
                        for (column, cell) in row.cells.iter().enumerate() {
                            cx.with_index(column, |cx| {
                                check_table_cell(
                                    course,
                                    cx,
                                    index,
                                    column,
                                    rows.len(),
                                    columns.len(),
                                    cell,
                                );
                            });
                            if cx.should_stop() {
                                return;
                            }
                        }
                    });
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
        if cx.should_stop() {
            return;
        }
        if row_heights_sum(rows)
            && (rows.iter().map(|row| row.height).sum::<f32>() - 1.0).abs() > 0.01
        {
            cx.err_at("rows", "row heights must be finite fractions summing to 1");
        }
        if borders.outer.is_some() || borders.inner.is_some() {
            cx.with_field("borders", |cx| {
                for (name, outline) in [("outer", &borders.outer), ("inner", &borders.inner)] {
                    let Some(outline) = outline else {
                        continue;
                    };
                    cx.with_field(name, |cx| {
                        if !outline.color.is_valid() {
                            cx.err_at(
                                "color",
                                format!("color {:?} is not #RRGGBB or #RRGGBBAA", outline.color.0),
                            );
                        }
                        if !outline.width.is_finite() || !(0.0..=50.0).contains(&outline.width) {
                            cx.err_at("width", "border width must be finite and within 0..=50");
                        }
                        check_dash_pattern(cx, outline.dash_pattern.as_deref());
                        check_miter_limit(cx, outline.miter_limit);
                    });
                    if cx.should_stop() {
                        return;
                    }
                }
            });
        }
    });
}

fn check_miter_limit(cx: &mut Ctx<'_>, limit: Option<f32>) {
    let Some(limit) = limit else {
        return;
    };
    if !limit.is_finite() || !(1.0..=100.0).contains(&limit) {
        cx.err_at(
            "miter_limit",
            "miter limit must be finite and within 1..=100",
        );
    }
}

fn check_dash_pattern(cx: &mut Ctx<'_>, pattern: Option<&[f32]>) {
    let Some(pattern) = pattern else {
        return;
    };
    if pattern.is_empty() || pattern.len() > 32 || pattern.len() % 2 != 0 {
        cx.err_at(
            "dash_pattern",
            "dash_pattern must have an even number of entries between 2 and 32",
        );
        return;
    }
    if pattern
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0 || *v > 100.0)
    {
        cx.err_at(
            "dash_pattern",
            "dash_pattern entries must be finite and within (0, 100]",
        );
    }
}

/// True when every row height is finite and positive, so the sum warning is
/// only useful when each row can be measured.
fn row_heights_sum(rows: &[crate::TableRow]) -> bool {
    !rows.is_empty()
        && rows
            .iter()
            .all(|row| row.height.is_finite() && row.height > 0.0)
}

fn check_shadow(cx: &mut Ctx<'_>, shadow: &crate::Shadow) {
    if !shadow.color.is_valid() {
        cx.err_at(
            "color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", shadow.color.0),
        );
    }
    if ![shadow.offset_x, shadow.offset_y]
        .iter()
        .all(|v| v.is_finite())
        || !(-200.0..=200.0).contains(&shadow.offset_x)
        || !(-200.0..=200.0).contains(&shadow.offset_y)
    {
        cx.err("shadow offsets must be finite and within -200..=200");
    }
    if !shadow.blur.is_finite() || !(0.0..=100.0).contains(&shadow.blur) {
        cx.err_at("blur", "shadow blur must be finite and within 0..=100");
    }
}

fn check_glow(cx: &mut Ctx<'_>, glow: &crate::Glow) {
    if !glow.color.is_valid() {
        cx.err_at(
            "color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", glow.color.0),
        );
    }
    if !glow.size.is_finite() || !(0.0..=100.0).contains(&glow.size) {
        cx.err_at("size", "glow size must be finite and within 0..=100");
    }
}

fn check_object_effects(cx: &mut Ctx<'_>, o: &Object) {
    if !o.opacity.is_finite() || !(0.0..=1.0).contains(&o.opacity) {
        cx.err_at("opacity", "opacity must be a finite number in 0..=1");
    }
    if let Some(shadow) = &o.shadow {
        cx.with_field("shadow", |cx| check_shadow(cx, shadow));
    }
    if let Some(glow) = &o.glow {
        cx.with_field("glow", |cx| check_glow(cx, glow));
    }
}

fn check_input_variable(course: &Course, cx: &mut Ctx<'_>, variable: &str, kind: ValueKind) {
    match course.variable(variable) {
        None => cx.err_at(
            "kind/variable",
            format!("input variable {variable:?} does not exist"),
        ),
        Some(def) if def.value.kind() != kind => cx.err_at(
            "kind/variable",
            format!(
                "input variable {variable:?} is {:?}, but this input needs {kind:?}",
                def.value.kind()
            ),
        ),
        Some(_) => {}
    }
}

fn check_input_object(course: &Course, cx: &mut Ctx<'_>, kind: &ObjectKind) {
    match kind {
        ObjectKind::TextEntry {
            variable,
            max_chars,
            ..
        } => {
            check_input_variable(course, cx, variable, ValueKind::Text);
            if *max_chars > crate::MAX_INPUT_CHARS {
                cx.err_at(
                    "kind/max_chars",
                    format!(
                        "text entry max_chars must be at most {}",
                        crate::MAX_INPUT_CHARS
                    ),
                );
            }
        }
        ObjectKind::Slider {
            variable,
            min,
            max,
            step,
            ..
        }
        | ObjectKind::Dial {
            variable,
            min,
            max,
            step,
            ..
        } => {
            check_input_variable(course, cx, variable, ValueKind::Number);
            if !min.is_finite() || !max.is_finite() || min >= max {
                cx.err_at(
                    "kind",
                    "slider/dial min and max must be finite and min must be less than max",
                );
            }
            if !step.is_finite() || *step < 0.0 {
                cx.err_at(
                    "kind/step",
                    "slider/dial step must be finite and at least 0",
                );
            }
            if let ObjectKind::Dial {
                start_angle,
                end_angle,
                ..
            } = kind
                && (!start_angle.is_finite()
                    || !end_angle.is_finite()
                    || !(-360.0..=360.0).contains(start_angle)
                    || !(-360.0..=360.0).contains(end_angle)
                    || start_angle >= end_angle)
            {
                cx.err_at(
                    "kind",
                    "dial angles must be finite, within -360..=360, and start_angle must be less than end_angle",
                );
            }
        }
        ObjectKind::ProgressMeter {
            variable, min, max, ..
        } => {
            check_input_variable(course, cx, variable, ValueKind::Number);
            if !min.is_finite() || !max.is_finite() || min >= max {
                cx.err_at(
                    "kind",
                    "progress meter min and max must be finite and min must be less than max",
                );
            }
        }
        ObjectKind::Checkbox { variable, .. } => {
            check_input_variable(course, cx, variable, ValueKind::Bool);
        }
        ObjectKind::RadioGroup { variable, options } => {
            check_input_variable(course, cx, variable, ValueKind::Text);
            if !(crate::MIN_RADIO_OPTIONS..=crate::MAX_RADIO_OPTIONS).contains(&options.len()) {
                cx.err_at(
                    "kind/options",
                    format!(
                        "a radio group needs {} to {} options",
                        crate::MIN_RADIO_OPTIONS,
                        crate::MAX_RADIO_OPTIONS
                    ),
                );
            }
            if cx.should_stop() {
                return;
            }
            let mut values = BTreeSet::new();
            cx.with_field("kind", |cx| {
                cx.with_field("options", |cx| {
                    for (i, option) in options.iter().enumerate() {
                        if option.value.trim().is_empty() {
                            cx.with_index(i, |cx| {
                                cx.err_at("value", "radio option values must not be empty");
                            });
                        } else if !values.insert(option.value.as_str()) {
                            cx.with_index(i, |cx| {
                                cx.err_at(
                                    "value",
                                    format!("duplicate radio option value {:?}", option.value),
                                );
                            });
                        }
                        if cx.should_stop() {
                            return;
                        }
                    }
                });
            });
        }
        _ => {}
    }
}

fn format_secs(ms: u32) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}", ms / 1000)
    } else {
        let s = format!("{:.3}", f64::from(ms) / 1000.0);
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn check_timer_object(
    course: &Course,
    cx: &mut Ctx<'_>,
    id: &str,
    duration_ms: u32,
    timeline_ms: u32,
    style: &crate::TextStyle,
) {
    if !(1..=crate::MAX_TIMER_DURATION_MS).contains(&duration_ms) {
        cx.err_at(
            "kind/duration_ms",
            format!(
                "timer duration_ms must be 1..={}",
                crate::MAX_TIMER_DURATION_MS
            ),
        );
    } else if !cx.skip_warnings && duration_ms > timeline_ms {
        cx.warn_at(
            "kind/duration_ms",
            format!(
                "Timer '{id}' runs {} s but its timeline ends at {} s, so it will stop early and never reach its end.",
                format_secs(duration_ms),
                format_secs(timeline_ms),
            ),
        );
    }
    if !style.color.is_valid() {
        cx.err_at(
            "kind/style/color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
        );
    }
    if !style.size.is_finite() || style.size <= 0.0 || style.size > 400.0 {
        cx.err_at(
            "kind/style/size",
            "timer text size must be finite and within 0..=400",
        );
    }
    check_font_name(course, cx, &style.font, "kind/style/font");
}

fn check_marker_object(
    course: &Course,
    cx: &mut Ctx<'_>,
    body: &str,
    runs: &[crate::TextRun],
    paragraphs: &[crate::Paragraph],
    color: Option<&crate::Color>,
) {
    if let Some(color) = color
        && !color.is_valid()
    {
        cx.err_at(
            "kind/color",
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", color.0),
        );
    }
    if !cx.should_stop() && (!runs.is_empty() || !paragraphs.is_empty()) {
        cx.with_field("kind", |cx| {
            check_text_runs(course, cx, Some(body), runs);
            if !cx.should_stop() {
                check_paragraphs(cx, paragraphs);
            }
        });
    }
}

fn check_scroll_panel_object(cx: &mut Ctx<'_>, content_height: f32, frame_height: f32) {
    if !content_height.is_finite()
        || content_height < 0.0
        || content_height > crate::MAX_SCROLL_CONTENT_HEIGHT
    {
        cx.err_at(
            "kind/content_height",
            format!(
                "scroll panel content_height must be finite and within 0..={}",
                crate::MAX_SCROLL_CONTENT_HEIGHT
            ),
        );
    } else if content_height + 0.5 < frame_height {
        cx.err_at(
            "kind/content_height",
            "scroll panel content_height must be at least the panel's frame height",
        );
    }
}

fn check_web_source(cx: &mut Ctx<'_>, source: &crate::WebSource) {
    match source {
        crate::WebSource::Url { url } => {
            if !crate::is_valid_web_url(url) {
                cx.err_at(
                    "kind/source/url",
                    format!("web URL {url:?} must start with https:// and name a host"),
                );
            }
        }
        crate::WebSource::Folder { path, entry } => {
            if !crate::is_valid_web_folder_path(path) {
                cx.err_at(
                    "kind/source/path",
                    format!(
                        "web folder path {path:?} must be a safe relative path under media/web/"
                    ),
                );
            }
            if !crate::is_valid_web_entry(entry) {
                cx.err_at(
                    "kind/source/entry",
                    format!(
                        "web folder entry {entry:?} must be a safe relative file inside the folder"
                    ),
                );
            }
        }
    }
}

fn check_captions(cx: &mut Ctx<'_>, captions: Option<&str>, object_id: &str) {
    let Some(path) = captions else {
        return;
    };
    if path.trim().is_empty() {
        cx.err_at(
            "kind/captions",
            format!("captions path for object {object_id:?} is empty"),
        );
    } else if !is_safe_relative_path(path) {
        cx.err_at(
            "kind/captions",
            format!(
                "captions path {path:?} for object {object_id:?} must be a safe relative path under media/"
            ),
        );
    } else if !ends_with_ignore_ascii_case(path, ".vtt") {
        cx.err_at(
            "kind/captions",
            format!("captions path {path:?} for object {object_id:?} must end in .vtt"),
        );
    }
}

fn check_media_options(cx: &mut Ctx<'_>, volume: f32, trim: Option<&crate::Trim>) {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        cx.err_at("kind/volume", "volume must be finite and within 0.0..=1.0");
    }
    let Some(trim) = trim else {
        return;
    };
    if trim.start_ms > crate::MAX_TRIM_MS {
        cx.err_at(
            "kind/trim/start_ms",
            format!("trim start must be at most {} ms", crate::MAX_TRIM_MS),
        );
    }
    if let Some(end_ms) = trim.end_ms {
        if end_ms > crate::MAX_TRIM_MS {
            cx.err_at(
                "kind/trim/end_ms",
                format!("trim end must be at most {} ms", crate::MAX_TRIM_MS),
            );
        }
        if end_ms <= trim.start_ms {
            cx.err_at("kind/trim/end_ms", "trim end must be after its start");
        }
    }
}

fn check_cues(cx: &mut Ctx<'_>, cues: &[crate::Cue], timeline_ms: u32) {
    if cues.is_empty() {
        return;
    }
    if cues.len() > crate::MAX_CUES {
        cx.err_at(
            "kind/cues",
            format!("at most {} cues are allowed", crate::MAX_CUES),
        );
        if cx.should_stop() {
            return;
        }
    }
    let mut seen = BTreeSet::new();
    cx.with_field("kind", |cx| {
        cx.with_field("cues", |cx| {
            for (i, cue) in cues.iter().enumerate() {
                cx.with_index(i, |cx| {
                    if cue.id.is_empty() {
                        cx.err_at("id", "cue id is empty");
                    } else if !seen.insert(cue.id.as_str()) {
                        cx.err_at("id", format!("duplicate cue id {:?}", cue.id));
                    }
                    if cue.name.trim().is_empty() {
                        cx.err_at("name", "cue name is empty");
                    }
                    if !cue.at_ms.is_finite() || cue.at_ms < 0.0 {
                        cx.err_at("at_ms", "cue at_ms must be finite and at least 0");
                    } else if cue.at_ms > f64::from(timeline_ms) {
                        cx.err_at(
                            "at_ms",
                            format!(
                                "cue at {} ms is past the video's known duration of {timeline_ms} ms",
                                cue.at_ms
                            ),
                        );
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    });
}

fn object_completely_outside_slide(frame: crate::Rect, rotation: f32, slide: crate::Size) -> bool {
    if ![frame.x, frame.y, frame.w, frame.h, rotation]
        .iter()
        .all(|v| v.is_finite())
        || frame.w < 0.0
        || frame.h < 0.0
    {
        return false;
    }
    let sw = slide.w as f32;
    let sh = slide.h as f32;
    let cx = frame.x + frame.w * 0.5;
    let cy = frame.y + frame.h * 0.5;
    let hw = frame.w * 0.5;
    let hh = frame.h * 0.5;
    let (sin, cos) = rotation.to_radians().sin_cos();
    let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (lx, ly) in corners {
        let x = cx + lx * cos - ly * sin;
        let y = cy + lx * sin + ly * cos;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    max_x <= 0.0 || min_x >= sw || max_y <= 0.0 || min_y >= sh
}

fn check_object(course: &Course, cx: &mut Ctx<'_>, o: &Object, timeline_ms: u32, slide_id: &str) {
    let f = o.frame;
    if ![f.x, f.y, f.w, f.h].iter().all(|v| v.is_finite()) || f.w < 0.0 || f.h < 0.0 {
        cx.err_at("frame", "frame values must be finite and w/h non-negative");
    }
    if f.w > crate::MAX_OBJECT_FRAME_SIZE || f.h > crate::MAX_OBJECT_FRAME_SIZE {
        cx.err_at(
            "frame",
            format!(
                "frame width and height must be at most {} px",
                crate::MAX_OBJECT_FRAME_SIZE
            ),
        );
    }
    if !o.rotation.is_finite() || !(0.0..360.0).contains(&o.rotation) {
        cx.err_at(
            "rotation",
            "rotation must be a finite number of degrees in 0..360",
        );
    }
    if !cx.skip_warnings
        && !matches!(o.kind, ObjectKind::Audio { .. })
        && o.panel.is_none()
        && object_completely_outside_slide(f, o.rotation, course.slide_size)
    {
        let label = if o.name.trim().is_empty() {
            o.id.as_str()
        } else {
            o.name.as_str()
        };
        let sw = course.slide_size.w;
        let sh = course.slide_size.h;
        cx.warn_at(
            "frame",
            format!("object {label:?} lies outside the {sw}×{sh} slide"),
        );
    }
    check_object_effects(cx, o);
    if let Some(end) = o.timing.end_ms
        && end <= o.timing.start_ms
    {
        cx.err_at("timing", "end_ms must be after start_ms");
    }
    if !cx.skip_warnings && o.timing.start_ms > timeline_ms {
        cx.warn_at(
            "timing",
            "object starts after its timeline ends and will never appear",
        );
    }
    if cx.should_stop() {
        return;
    }
    check_object_kind(course, cx, o, timeline_ms);
    if cx.should_stop() {
        return;
    }
    check_object_states(course, cx, o);
    if cx.should_stop() {
        return;
    }
    check_object_animations(cx, o);
    check_animation_windows(cx, o, timeline_ms, slide_id);
    if cx.should_stop() {
        return;
    }
    check_object_motion_paths(cx, o);
}

/// A media path must stay inside the course package. A bad path is a warning,
/// because old files may hold one and still open.
fn check_media_src(cx: &mut Ctx<'_>, src: &str) {
    if !cx.skip_warnings && !src.trim().is_empty() && !is_safe_relative_path(src) {
        cx.warn_at(
            "kind/src",
            format!("media path {src:?} must be a safe relative path inside the course"),
        );
    }
}

fn check_object_kind(course: &Course, cx: &mut Ctx<'_>, o: &Object, timeline_ms: u32) {
    match &o.kind {
        ObjectKind::Shape { fill, .. } | ObjectKind::Button { fill, .. } if !fill.is_valid() => {
            cx.err_at(
                "kind/fill",
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
            );
        }
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            text_style,
            outline,
            ..
        } => {
            if let Some(outline) = outline {
                cx.with_field("kind", |cx| {
                    cx.with_field("outline", |cx| check_outline(cx, outline));
                });
            }
            cx.with_field("kind", |cx| check_insets(cx, style.insets.as_ref()));
            if !cx.skip_warnings
                && let Some(id) = text_style
                && !course.text_styles.iter().any(|named| &named.id == id)
            {
                cx.warn_at(
                    "kind/text_style",
                    format!("text style {id:?} does not exist; the box keeps its own look"),
                );
            }
            if !style.color.is_valid() {
                cx.err_at(
                    "kind/style/color",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
                );
            }
            check_font_name(course, cx, &style.font, "kind/style/font");
            if !cx.should_stop() && (!runs.is_empty() || !paragraphs.is_empty()) {
                cx.with_field("kind", |cx| {
                    check_text_runs(course, cx, Some(text), runs);
                    if !cx.should_stop() {
                        check_paragraphs(cx, paragraphs);
                    }
                });
            }
        }
        ObjectKind::Shape {
            shape,
            fill: _,
            fill_style,
            outline,
            text,
            adjust: _,
        } => {
            if let Some(fill_style) = fill_style {
                check_fill_style(cx, fill_style);
            }
            if !cx.should_stop() {
                check_shape_outline_and_text(course, cx, *shape, outline.as_ref(), text.as_ref());
            }
        }
        ObjectKind::Image {
            src, crop, outline, ..
        } => {
            if let Some(outline) = outline {
                cx.with_field("kind", |cx| {
                    cx.with_field("outline", |cx| check_outline(cx, outline));
                });
            }
            if let Some(crop) = crop
                && let Some(message) = check_crop_values(*crop)
            {
                cx.err_at("kind/crop", message);
            }
            if src.trim().is_empty() {
                cx.err_at("kind/src", "media src is empty");
            }
            check_media_src(cx, src);
        }
        ObjectKind::Video {
            src,
            cues,
            captions,
            volume,
            trim,
            ..
        } => {
            if src.trim().is_empty() {
                cx.err_at("kind/src", "media src is empty");
            }
            check_media_src(cx, src);
            if !cx.should_stop() {
                check_cues(cx, cues, timeline_ms);
            }
            if !cx.should_stop() {
                check_captions(cx, captions.as_deref(), &o.id);
            }
            if !cx.should_stop() {
                check_media_options(cx, *volume, trim.as_ref());
            }
        }
        ObjectKind::Lottie { src, speed, .. } => {
            if src.trim().is_empty() {
                cx.err_at("kind/src", "media src is empty");
            }
            check_media_src(cx, src);
            if !speed.is_finite()
                || !(crate::LOTTIE_MIN_SPEED..=crate::LOTTIE_MAX_SPEED).contains(speed)
            {
                cx.err_at(
                    "kind/speed",
                    "Lottie speed must be finite and within 0.1..=4",
                );
            }
        }
        ObjectKind::Web { source } => check_web_source(cx, source),
        ObjectKind::TextEntry { .. }
        | ObjectKind::Slider { .. }
        | ObjectKind::Dial { .. }
        | ObjectKind::Checkbox { .. }
        | ObjectKind::RadioGroup { .. }
        | ObjectKind::ProgressMeter { .. } => check_input_object(course, cx, &o.kind),
        ObjectKind::Timer {
            duration_ms, style, ..
        } => check_timer_object(course, cx, &o.id, *duration_ms, timeline_ms, style),
        ObjectKind::Marker {
            body,
            runs,
            paragraphs,
            color,
            ..
        } => check_marker_object(course, cx, body, runs, paragraphs, color.as_ref()),
        ObjectKind::ScrollPanel { content_height, .. } => {
            check_scroll_panel_object(cx, *content_height, o.frame.h)
        }
        ObjectKind::Path { path } => {
            check_path(course, cx, path);
        }
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row: _,
        } => {
            check_table(course, cx, columns, rows, borders);
        }
        ObjectKind::Button { fill_style, .. } => {
            if let Some(fill_style) = fill_style {
                check_fill_style(cx, fill_style);
            }
        }
        ObjectKind::Audio {
            src,
            captions,
            volume,
            trim,
            ..
        } => {
            if src.trim().is_empty() {
                cx.err_at("kind/src", "media src is empty");
            }
            check_media_src(cx, src);
            if !cx.should_stop() {
                check_captions(cx, captions.as_deref(), &o.id);
            }
            if !cx.should_stop() {
                check_media_options(cx, *volume, trim.as_ref());
            }
        }
    }
}

fn check_object_states(course: &Course, cx: &mut Ctx<'_>, o: &Object) {
    if !cx.skip_warnings && o.states.len() > crate::MAX_STATES_PER_OBJECT {
        cx.warn_at(
            "states",
            format!(
                "object has {} states; at most {} are recommended",
                o.states.len(),
                crate::MAX_STATES_PER_OBJECT
            ),
        );
    }
    if !o.states.is_empty() {
        let mut seen = BTreeSet::new();
        cx.with_field("states", |cx| {
            for (i, s) in o.states.iter().enumerate() {
                cx.with_index(i, |cx| {
                    if s.name == "normal" {
                        cx.err("`normal` is the object's own appearance; don't redefine it");
                    } else if !cx.skip_warnings {
                        if s.name == "hidden" {
                            cx.warn("`hidden` is built in and needs no state definition");
                        } else if s.name.trim().is_empty() {
                            cx.warn("state name is empty");
                        } else if s.name.chars().count() > crate::MAX_STATE_NAME_CHARS {
                            cx.warn(format!(
                                "state name {:?} is longer than {} characters",
                                s.name,
                                crate::MAX_STATE_NAME_CHARS
                            ));
                        }
                    }
                    if !seen.insert(s.name.as_str()) {
                        cx.err(format!("duplicate state {:?}", s.name));
                    }
                    if let Some(c) = &s.fill
                        && !c.is_valid()
                    {
                        cx.err_at(
                            "fill",
                            format!("color {:?} is not #RRGGBB or #RRGGBBAA", c.0),
                        );
                    }
                    if !cx.skip_warnings
                        && let Some(src) = &s.src
                    {
                        let obj_label = if o.name.trim().is_empty() {
                            o.id.as_str()
                        } else {
                            o.name.as_str()
                        };
                        if src.trim().is_empty() {
                            cx.warn_at(
                                "src",
                                format!(
                                    "State \u{201C}{}\u{201D} on \u{201C}{obj_label}\u{201D} has an empty image path; the base image will be used.",
                                    s.name
                                ),
                            );
                        } else {
                            if !matches!(
                                o.kind,
                                ObjectKind::Image { .. }
                                    | ObjectKind::Video { .. }
                                    | ObjectKind::Audio { .. }
                                    | ObjectKind::Lottie { .. }
                            ) {
                                cx.warn_at(
                                    "src",
                                    format!(
                                        "State \u{201C}{}\u{201D} on \u{201C}{obj_label}\u{201D} sets a file, which only applies to image, video, audio and Lottie objects.",
                                        s.name
                                    ),
                                );
                            }
                            if !is_safe_relative_path(src) {
                                cx.warn_at(
                                    "src",
                                    format!(
                                        "State \u{201C}{}\u{201D} on \u{201C}{obj_label}\u{201D} points at a file outside the course. Choose the image again from the state card.",
                                        s.name
                                    ),
                                );
                            }
                        }
                    }
                    if !cx.should_stop() {
                        check_state_widened_overrides(course, cx, s);
                    }
                    if !cx.should_stop() {
                        check_state_extras(course, cx, &s.extras);
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    }
    if !cx.should_stop() && !o.has_state(&o.initial_state) {
        cx.err_at(
            "initial_state",
            format!("state {:?} is not defined on this object", o.initial_state),
        );
    }
}

fn check_state_widened_overrides(course: &Course, cx: &mut Ctx<'_>, s: &crate::StateDef) {
    for (field, is_set, is_clear) in [
        ("fill_style", s.fill_style.is_some(), s.clear_fill_style),
        ("outline", s.outline.is_some(), s.clear_outline),
        ("shadow", s.shadow.is_some(), s.clear_shadow),
        ("glow", s.glow.is_some(), s.clear_glow),
        ("crop", s.crop.is_some(), s.clear_crop),
        ("image_shape", s.image_shape.is_some(), s.clear_image_shape),
    ] {
        if is_set && is_clear {
            cx.err_at(
                field,
                format!("state cannot both set {field} and clear_{field}"),
            );
            if cx.should_stop() {
                return;
            }
        }
    }
    if let Some(fill_style) = &s.fill_style {
        check_fill_style(cx, fill_style);
        if cx.should_stop() {
            return;
        }
    }
    if let Some(outline) = &s.outline {
        cx.with_field("outline", |cx| check_outline(cx, outline));
        if cx.should_stop() {
            return;
        }
    }
    if let Some(ts) = &s.text_style {
        if !ts.color.is_valid() {
            cx.err_at(
                "text_style/color",
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", ts.color.0),
            );
        }
        check_font_name(course, cx, &ts.font, "text_style/font");
        if cx.should_stop() {
            return;
        }
    }
    if let Some(runs) = &s.runs {
        check_text_runs(course, cx, s.text.as_deref(), runs);
        if cx.should_stop() {
            return;
        }
    }
    if let Some(paragraphs) = &s.paragraphs {
        check_paragraphs(cx, paragraphs);
        if cx.should_stop() {
            return;
        }
    }
    if let Some(opacity) = s.opacity
        && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
    {
        cx.err_at("opacity", "opacity must be a finite number in 0..=1");
    }
    if let Some(shadow) = &s.shadow {
        cx.with_field("shadow", |cx| check_shadow(cx, shadow));
    }
    if let Some(glow) = &s.glow {
        cx.with_field("glow", |cx| check_glow(cx, glow));
    }
    if let Some(crop) = s.crop
        && let Some(msg) = check_crop_values(crop)
    {
        cx.err_at("crop", msg);
    }
    if let Some(rotation) = s.rotation
        && (!rotation.is_finite() || !(0.0..360.0).contains(&rotation))
    {
        cx.err_at(
            "rotation",
            "rotation must be a finite number of degrees in 0..360",
        );
    }
}

fn check_state_extra_kind(course: &Course, cx: &mut Ctx<'_>, kind: &crate::StateExtraKind) {
    match kind {
        crate::StateExtraKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            text,
            adjust: _,
        } => {
            if !fill.is_valid() {
                cx.err_at(
                    "kind/fill",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
                );
            }
            if !cx.should_stop()
                && let Some(fs) = fill_style
            {
                check_fill_style(cx, fs);
            }
            if !cx.should_stop() {
                check_shape_outline_and_text(course, cx, *shape, outline.as_ref(), text.as_ref());
            }
        }
        crate::StateExtraKind::Image {
            src,
            alt: _,
            crop,
            outline,
            shape: _,
        } => {
            if src.trim().is_empty() {
                cx.err_at("kind/src", "media src is empty");
            } else if !is_safe_relative_path(src) {
                cx.err_at(
                    "kind/src",
                    format!("media src {src:?} must be a safe relative path"),
                );
            }
            if let Some(crop) = crop
                && let Some(message) = check_crop_values(*crop)
            {
                cx.err_at("kind/crop", message);
            }
            if !cx.should_stop()
                && let Some(outline) = outline
            {
                cx.with_field("kind", |cx| {
                    cx.with_field("outline", |cx| check_outline(cx, outline));
                });
            }
        }
        crate::StateExtraKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit: _,
            outline,
            point_text: _,
        } => {
            if !style.color.is_valid() {
                cx.err_at(
                    "kind/style/color",
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
                );
            }
            check_font_name(course, cx, &style.font, "kind/style/font");
            if !cx.should_stop()
                && (!runs.is_empty() || !paragraphs.is_empty() || outline.is_some())
            {
                cx.with_field("kind", |cx| {
                    check_text_runs(course, cx, Some(text), runs);
                    if !cx.should_stop() {
                        check_paragraphs(cx, paragraphs);
                    }
                    if !cx.should_stop()
                        && let Some(outline) = outline
                    {
                        cx.with_field("outline", |cx| check_outline(cx, outline));
                    }
                });
            }
        }
        crate::StateExtraKind::Path { path } => {
            check_path(course, cx, path);
            if path.connector.is_some() {
                cx.err_at(
                    "kind/path/connector",
                    "state extra paths cannot be connectors",
                );
            }
            if path.start_glue.is_some() || path.end_glue.is_some() {
                cx.err_at(
                    "kind/path/glue",
                    "state extra paths cannot have connector glue",
                );
            }
            if !cx.should_stop()
                && let Some(fs) = &path.fill_style
            {
                check_fill_style(cx, fs);
            }
        }
    }
}

fn check_state_extras(course: &Course, cx: &mut Ctx<'_>, extras: &[crate::StateExtra]) {
    if extras.is_empty() {
        return;
    }
    if extras.len() > crate::MAX_STATE_EXTRAS {
        cx.err_at(
            "extras",
            format!(
                "a state may have at most {} extras (found {})",
                crate::MAX_STATE_EXTRAS,
                extras.len()
            ),
        );
        if cx.should_stop() {
            return;
        }
    }
    let mut seen_ids = BTreeSet::new();
    cx.with_field("extras", |cx| {
        for (ei, extra) in extras.iter().enumerate() {
            cx.with_index(ei, |cx| {
                if !valid_name(&extra.id) {
                    cx.err_at(
                        "id",
                        format!(
                            "extra id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            extra.id
                        ),
                    );
                }
                if !seen_ids.insert(extra.id.as_str()) {
                    cx.err_at("id", format!("duplicate extra id {:?}", extra.id));
                }
                if !coord_ok(extra.frame.x)
                    || !coord_ok(extra.frame.y)
                    || !extent_ok(extra.frame.w)
                    || !extent_ok(extra.frame.h)
                {
                    cx.err_at(
                        "frame",
                        "extra frame coordinates must be finite, x and y within ±100000, and w and h greater than 0 and at most 100000",
                    );
                }
                if !extra.opacity.is_finite() || !(0.0..=1.0).contains(&extra.opacity) {
                    cx.err_at("opacity", "opacity must be a finite number in 0..=1");
                }
                if !extra.rotation.is_finite() || !(0.0..360.0).contains(&extra.rotation) {
                    cx.err_at(
                        "rotation",
                        "rotation must be a finite number of degrees in 0..360",
                    );
                }
                if let Some(shadow) = &extra.shadow {
                    cx.with_field("shadow", |cx| check_shadow(cx, shadow));
                }
                if let Some(glow) = &extra.glow {
                    cx.with_field("glow", |cx| check_glow(cx, glow));
                }
                if !cx.should_stop() {
                    check_state_extra_kind(course, cx, &extra.kind);
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

fn check_object_animations(cx: &mut Ctx<'_>, o: &Object) {
    for (which, anim) in [("entrance", &o.entrance), ("exit", &o.exit)] {
        if let Some(anim) = anim {
            check_anim_effect(cx, which, &anim.effect);
        }
    }
    if let Some(ref anim) = o.entrance
        && (anim.duration_ms == 0 || anim.duration_ms > 60000)
    {
        cx.err_at(
            "entrance/duration_ms",
            "animation duration_ms must be 1..=60000",
        );
    }
    if let Some(ref anim) = o.exit
        && (anim.duration_ms == 0 || anim.duration_ms > 60000)
    {
        cx.err_at(
            "exit/duration_ms",
            "animation duration_ms must be 1..=60000",
        );
    }
    if o.emphasis.len() > crate::MAX_EMPHASIS_PER_OBJECT {
        cx.err_at(
            "emphasis",
            format!(
                "an object has at most {} emphasis animations",
                crate::MAX_EMPHASIS_PER_OBJECT
            ),
        );
    }
    for (i, e) in o.emphasis.iter().enumerate() {
        if e.effect.def().is_none() {
            cx.warn_at(
                &format!("emphasis/{i}/effect/type"),
                format!(
                    "unknown emphasis effect `{}`; it plays as `{}`",
                    e.effect.id,
                    crate::FALLBACK_EMPHASIS
                ),
            );
        }
        if e.duration_ms == 0 || e.duration_ms > 60000 {
            cx.err_at(
                &format!("emphasis/{i}/duration_ms"),
                "emphasis duration_ms must be 1..=60000",
            );
        }
        if e.repeat == 0 || e.repeat > crate::MAX_EMPHASIS_REPEAT {
            cx.err_at(
                &format!("emphasis/{i}/repeat"),
                format!("emphasis repeat must be 1..={}", crate::MAX_EMPHASIS_REPEAT),
            );
        }
    }
}

/// Warn about animations that the object's time window or the timeline cuts off. The window
/// runs from `timing.start_ms` to `timing.end_ms`, or to the timeline's end. An entrance plays
/// from the start, an exit over `[end - duration, end)`, an emphasis over its own span and only
/// while the object is on screen. The messages name the object and slide so a UI can jump to it.
fn check_animation_windows(cx: &mut Ctx<'_>, o: &Object, timeline_ms: u32, slide_id: &str) {
    if cx.skip_warnings {
        return;
    }
    let start = o.timing.start_ms;
    let end = o.timing.end_ms.map_or(timeline_ms, |e| e.min(timeline_ms));
    if start >= end {
        return;
    }
    let who = if slide_id.is_empty() {
        format!("object {:?}", o.id)
    } else {
        format!("object {:?} on slide {slide_id:?}", o.id)
    };
    if let Some(a) = &o.entrance
        && start.saturating_add(a.duration_ms) > end
    {
        cx.warn_at(
            "entrance",
            format!(
                "{who}: the {} ms entrance animation does not finish before the object's window ends at {end} ms",
                a.duration_ms
            ),
        );
    }
    if let Some(a) = &o.exit
        && end.saturating_sub(a.duration_ms) < start
    {
        cx.warn_at(
            "exit",
            format!(
                "{who}: the {} ms exit animation does not fit in the object's window of {start} to {end} ms",
                a.duration_ms
            ),
        );
    }
    for (i, e) in o.emphasis.iter().enumerate() {
        if e.start_ms < start || e.end_ms() > end {
            cx.warn_at(
                &format!("emphasis/{i}"),
                format!(
                    "{who}: emphasis {i} plays from {} to {} ms, outside the object's window of {start} to {end} ms",
                    e.start_ms,
                    e.end_ms()
                ),
            );
        }
    }
}

/// Unknown effect ids and options the effect does not take are warnings: the file still
/// plays (an unknown effect as a fade, an extra option ignored).
fn check_anim_effect(cx: &mut Ctx<'_>, which: &str, effect: &crate::AnimEffect) {
    let Some(def) = effect.def() else {
        cx.warn_at(
            &format!("{which}/effect/type"),
            format!(
                "unknown animation effect `{}`; it plays as `{}`",
                effect.id,
                crate::FALLBACK_EFFECT
            ),
        );
        return;
    };
    for (field, message) in def.option_problems(effect) {
        cx.warn_at(
            &format!("{which}/effect/{field}"),
            format!("{message}; it plays with the default"),
        );
    }
}

/// The same rule package paths use: no absolute paths, drive letters, URL
/// schemes, backslashes, control characters, or `.`/`..` segments.
fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains('"')
    {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

fn check_object_motion_paths(cx: &mut Ctx<'_>, o: &Object) {
    if o.motion_paths.is_empty() {
        return;
    }
    cx.with_field("motion_paths", |cx| {
        if o.motion_paths.len() > 20 {
            cx.err("object has more than 20 motion paths");
            if cx.should_stop() {
                return;
            }
        }
        let mut path_ids = BTreeSet::new();
        for (i, path) in o.motion_paths.iter().enumerate() {
            cx.with_index(i, |cx| {
                if !valid_name(&path.id) {
                    cx.err_at(
                        "id",
                        format!("path id {:?} must be 1-128 chars of [A-Za-z0-9_-]", path.id),
                    );
                }
                if !path_ids.insert(path.id.as_str()) {
                    cx.err_at(
                        "id",
                        format!(
                            "duplicate motion path id {:?} on object {:?}",
                            path.id, o.id
                        ),
                    );
                }
                if path.duration_ms == 0 || path.duration_ms > 60000 {
                    cx.err_at("duration_ms", "motion path duration_ms must be 1..=60000");
                }
                if path.points.len() < 2 || path.points.len() > 100 {
                    cx.err_at("points", "motion path must have 2..=100 points");
                }
                if !cx.should_stop() && !path.points.is_empty() {
                    cx.with_field("points", |cx| {
                        for (pi, pt) in path.points.iter().enumerate() {
                            if !pt.x.is_finite()
                                || !pt.y.is_finite()
                                || pt.x < -100_000.0
                                || pt.x > 100_000.0
                                || pt.y < -100_000.0
                                || pt.y > 100_000.0
                            {
                                cx.with_index(pi, |cx| {
                                    cx.err(
                                        "path coordinates must be finite and within -100000..=100000",
                                    );
                                });
                                if cx.should_stop() {
                                    return;
                                }
                            }
                        }
                    });
                }
                if !cx.skip_warnings
                    && !cx.should_stop()
                    && path.relative
                    && let Some(first) = path.points.first()
                    && (first.x != 0.0 || first.y != 0.0)
                    && first.x.is_finite()
                    && first.y.is_finite()
                {
                    cx.warn_at("points/0", "relative motion path start point should be {0,0}");
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

enum TargetRef<'a> {
    Object(&'a Object),
    InteractiveGroup(&'a Group),
}

fn check_object_ref(cx: &mut Ctx<'_>, slide: &Slide, id: &str) -> Option<()> {
    if slide.object(id).is_none() {
        cx.err(format!("object {id:?} is not on slide {:?}", slide.id));
        return None;
    }
    Some(())
}

fn check_object_or_interactive_group_ref<'a>(
    cx: &mut Ctx<'_>,
    slide: &'a Slide,
    id: &str,
) -> Option<TargetRef<'a>> {
    if let Some((o, _)) = slide.object(id) {
        return Some(TargetRef::Object(o));
    }
    if let Some((g, _)) = slide.interactive_group(id) {
        return Some(TargetRef::InteractiveGroup(g));
    }
    if slide.group(id).is_some() || !slide.group_members(id).is_empty() {
        cx.err(format!(
            "group {id:?} on slide {:?} is not an interactive group",
            slide.id
        ));
    } else {
        cx.err(format!("object {id:?} is not on slide {:?}", slide.id));
    }
    None
}

fn check_layer_ref(cx: &mut Ctx<'_>, slide: &Slide, id: &str) {
    if slide.layer(id).is_none() {
        cx.err(format!("layer {id:?} is not on slide {:?}", slide.id));
    }
}

fn check_trigger(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    owner: Option<&Layer>,
    t: &crate::Trigger,
) {
    cx.with_field("when", |cx| check_event(course, cx, slide, &t.when));
    if cx.should_stop() {
        return;
    }
    cx.with_field("action", |cx| {
        check_action(course, cx, slide, owner, &t.action)
    });
    if cx.should_stop() {
        return;
    }
    if let Some(c) = &t.condition {
        cx.with_field("condition", |cx| check_condition(course, cx, slide, c, 1));
    }
}

fn check_timeline_reaches_event(
    cx: &mut Ctx<'_>,
    slide: &Slide,
    ms: u32,
    layer: Option<&str>,
    cue: Option<&str>,
) {
    let target_layer = layer.and_then(|l| slide.layer(l));
    if let Some(l) = layer
        && target_layer.is_none()
    {
        check_layer_ref(cx, slide, l);
        return;
    }
    let (len, cues) = match target_layer {
        Some(l) => (l.duration_ms, l.cue_points.as_slice()),
        None => (slide.duration_ms, slide.cue_points.as_slice()),
    };
    if let Some(cue_id) = cue {
        match cues.iter().find(|c| c.id == cue_id) {
            Some(c) if c.at_ms > len => {
                if !cx.skip_warnings {
                    cx.warn(format!(
                        "timeline_reaches cue {cue_id:?} ({} ms) is past the timeline end ({len} ms) and never fires",
                        c.at_ms
                    ));
                }
            }
            Some(_) => {}
            None => cx.err(format!("cue {cue_id:?} is not defined on this timeline")),
        }
    } else if !cx.skip_warnings && ms > len {
        cx.warn(format!(
            "timeline_reaches {ms} ms is past the timeline end ({len} ms) and never fires"
        ));
    }
}

fn check_event(course: &Course, cx: &mut Ctx<'_>, slide: &Slide, e: &Event) {
    match e {
        Event::TimelineStarts { layer } | Event::TimelineEnds { layer } => {
            if let Some(l) = layer {
                check_layer_ref(cx, slide, l);
            }
        }
        Event::TimelineReaches { ms, layer, cue } => {
            check_timeline_reaches_event(cx, slide, *ms, layer.as_deref(), cue.as_deref());
        }
        Event::MediaEnds { object } => {
            if object == crate::BACKGROUND_AUDIO_ID {
                if course.background_audio.is_none() {
                    cx.err("the course has no background audio");
                }
            } else {
                let _ = check_object_ref(cx, slide, object);
            }
        }
        Event::Click { object } | Event::HoverStart { object } | Event::HoverEnd { object } => {
            let _ = check_object_or_interactive_group_ref(cx, slide, object);
        }
        Event::MarkerOpened { object } | Event::MarkerClosed { object } => {
            let _ = check_object_ref(cx, slide, object);
        }
        Event::CueReached { object, cue } => {
            if check_object_ref(cx, slide, object).is_some()
                && let Some((o, _)) = slide.object(object)
            {
                match &o.kind {
                    ObjectKind::Video { cues, .. } => {
                        if !cues.iter().any(|c| c.id == *cue) {
                            cx.err(format!(
                                "cue {cue:?} is not defined on video object {object:?}"
                            ));
                        }
                    }
                    _ => cx.err(format!("object {object:?} is not a video")),
                }
            }
        }
        Event::StateChanges { object, to } => {
            match check_object_or_interactive_group_ref(cx, slide, object) {
                Some(TargetRef::Object(o)) => {
                    if let Some(to) = to
                        && !o.has_state(to)
                        && !crate::BUILTIN_STATES.contains(&to.as_str())
                    {
                        cx.err(format!("state {to:?} is not defined on object {object:?}"));
                    }
                }
                Some(TargetRef::InteractiveGroup(g)) => {
                    if let Some(to) = to
                        && !slide.group_has_state(&g.id, to)
                        && !crate::BUILTIN_STATES.contains(&to.as_str())
                    {
                        cx.err(format!(
                            "state {to:?} is not defined on interactive group {:?}",
                            g.id
                        ));
                    }
                }
                None => {}
            }
        }
        Event::VariableChanges { variable } => {
            if crate::is_timer_remaining_variable_on(slide, variable) {
                // `Timer.<id>.Remaining` fires once per whole-second change.
            } else if crate::builtin_variable_kind(variable).is_some() {
                cx.err("built-in variables do not fire variable_changes");
            } else if course.variable(variable).is_none() {
                cx.err(format!("variable {variable:?} does not exist"));
            }
        }
        Event::TimerEnds { timer } => {
            if check_object_ref(cx, slide, timer).is_some()
                && let Some((o, _)) = slide.object(timer)
                && !matches!(o.kind, ObjectKind::Timer { .. })
            {
                cx.err(format!("object {timer:?} is not a timer"));
            }
        }
        Event::KeyPressed { key, object } => {
            if !crate::is_valid_trigger_key(key) {
                cx.err(format!(
                    "invalid key {key:?}; must be a-z, 0-9, arrow_up, arrow_down, arrow_left, arrow_right, enter, space, or escape"
                ));
            }
            if !cx.should_stop()
                && let Some(obj) = object
            {
                let _ = check_object_or_interactive_group_ref(cx, slide, obj);
            }
        }
        Event::DroppedOn { object, target } => {
            let _ = check_object_ref(cx, slide, object);
            if !cx.should_stop() {
                let _ = check_object_ref(cx, slide, target);
            }
        }
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            let _ = check_object_ref(cx, slide, a);
            if !cx.should_stop() {
                let _ = check_object_ref(cx, slide, b);
            }
        }
        Event::Submit => {
            if !cx.skip_warnings && slide.question.is_none() {
                cx.warn("submit trigger on a slide without a question never fires");
            }
        }
        Event::AnimationCompletes { object, path } => {
            if check_object_ref(cx, slide, object).is_some()
                && let (Some(path_id), Some((obj, _))) = (path, slide.object(object))
                && !obj.motion_paths.iter().any(|mp| mp.id == *path_id)
            {
                cx.err(format!(
                    "motion path {path_id:?} is not defined on object {object:?}"
                ));
            }
        }
    }
}

fn check_action(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    owner: Option<&Layer>,
    a: &Action,
) {
    match a {
        Action::JumpToSlide {
            target: SlideTarget::Id(id),
        } => {
            if course.play_slide(id).is_none() {
                cx.err(format!("slide {id:?} does not exist"));
            }
        }
        Action::JumpToSlide {
            target: SlideTarget::Scene(id),
        } => match course.scenes.iter().find(|s| s.id == *id) {
            None => cx.err(format!("scene {id:?} does not exist")),
            Some(sc) if sc.slides.is_empty() => {
                if !cx.skip_warnings {
                    cx.warn(format!("scene {id:?} has no slides"));
                }
            }
            Some(_) => {}
        },
        Action::JumpToSlide { .. }
        | Action::PauseTimeline
        | Action::ResumeTimeline
        | Action::SubmitResults
        | Action::RetryQuiz
        | Action::ReviewQuiz
        | Action::ExitReview
        | Action::PrintSlide
        | Action::CompleteCourse { .. }
        | Action::ExitCourse
        | Action::CloseLightbox => {}
        Action::LightboxSlide { slide: target } => {
            if target == &slide.id {
                cx.err("lightbox_slide cannot open the slide it is already on");
            } else if course.play_slide(target).is_none() {
                cx.err(format!("slide {target:?} does not exist"));
            }
        }
        Action::ShowLayer { layer } => check_layer_ref(cx, slide, layer),
        Action::HideLayer { layer: Some(l) } => check_layer_ref(cx, slide, l),
        Action::HideLayer { layer: None } => {
            if owner.is_none() {
                cx.err("hide_layer without `layer` hides the trigger's own layer, but this trigger is on the base layer");
            }
        }
        Action::ChangeState { object, state } => {
            match check_object_or_interactive_group_ref(cx, slide, object) {
                Some(TargetRef::Object(o)) if !o.has_state(state) => {
                    cx.err(format!(
                        "state {state:?} is not defined on object {object:?}"
                    ));
                }
                Some(TargetRef::InteractiveGroup(g)) if !slide.group_has_state(&g.id, state) => {
                    cx.err(format!(
                        "state {state:?} is not defined on interactive group {:?}",
                        g.id
                    ));
                }
                _ => {}
            }
        }
        Action::SetVariable {
            variable,
            op,
            value,
        } => check_set_variable(course, cx, slide, variable, *op, value.as_ref()),
        Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object } => {
            if object == crate::BACKGROUND_AUDIO_ID {
                if course.background_audio.is_none() {
                    cx.err("the course has no background audio");
                }
            } else if check_object_ref(cx, slide, object).is_some()
                && let Some((o, _)) = slide.object(object)
                && !matches!(
                    o.kind,
                    ObjectKind::Audio { .. } | ObjectKind::Video { .. } | ObjectKind::Lottie { .. }
                )
            {
                cx.err(format!("object {object:?} is not a media object"));
            }
        }
        Action::StartTimer { timer }
        | Action::PauseTimer { timer }
        | Action::ResetTimer { timer } => {
            if check_object_ref(cx, slide, timer).is_some()
                && let Some((o, _)) = slide.object(timer)
                && !matches!(o.kind, ObjectKind::Timer { .. })
            {
                cx.err(format!("object {timer:?} is not a timer"));
            }
        }
        Action::SubmitQuestion => {
            if slide.question.is_none() {
                cx.err("submit_question on a slide without a question");
            }
        }
        Action::OpenUrl { url } => {
            if !(url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("mailto:"))
            {
                cx.err("open_url only allows http(s):// and mailto: links");
            }
        }
        Action::RunJavaScript { code } => {
            if code.len() > crate::MAX_JAVASCRIPT_BYTES {
                cx.err(format!(
                    "run_javascript code must be at most {} bytes",
                    crate::MAX_JAVASCRIPT_BYTES
                ));
            }
            cx.warn("Uses JavaScript, which runs only in the published course");
        }
        Action::MoveOnPath { object, path } => {
            if check_object_ref(cx, slide, object).is_some()
                && let Some((obj, _)) = slide.object(object)
                && !obj.motion_paths.iter().any(|mp| mp.id == *path)
            {
                cx.err(format!(
                    "motion path {path:?} is not defined on object {object:?}"
                ));
            }
        }
    }
}

fn resolve_var_kind(course: &Course, slide: &Slide, name: &str) -> Option<ValueKind> {
    crate::builtin_variable_kind(name)
        .or_else(|| {
            if crate::is_timer_remaining_variable_on(slide, name) {
                Some(ValueKind::Number)
            } else {
                None
            }
        })
        .or_else(|| course.variable(name).map(|def| def.value.kind()))
}

fn check_set_variable(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    variable: &str,
    op: VarOp,
    value: Option<&ValueSource>,
) {
    if crate::builtin_variable_kind(variable).is_some()
        || crate::timer_id_for_remaining_variable(variable).is_some()
    {
        cx.err("read-only built-in variable");
        return;
    }
    let Some(def) = course.variable(variable) else {
        cx.err(format!("variable {variable:?} does not exist"));
        return;
    };
    let kind = def.value.kind();
    match op {
        VarOp::Toggle => {
            if kind != ValueKind::Bool {
                cx.err(format!(
                    "toggle needs a true/false variable; {variable:?} is {kind:?}"
                ));
            }
        }
        VarOp::Assign => match value {
            None => cx.err("assign needs a value"),
            Some(ValueSource::Constant(v)) => {
                if v.kind() != kind {
                    cx.err(format!("{variable:?} is {kind:?}, value is {:?}", v.kind()));
                }
            }
            Some(ValueSource::Variable { variable: src }) => {
                match resolve_var_kind(course, slide, src) {
                    None => cx.err(format!("variable {src:?} does not exist")),
                    Some(src_kind) if src_kind != kind => cx.err(format!(
                        "{variable:?} is {kind:?}, source variable {src:?} is {src_kind:?}"
                    )),
                    Some(_) => {}
                }
            }
        },
        VarOp::Add | VarOp::Subtract | VarOp::Multiply | VarOp::Divide => {
            if kind != ValueKind::Number {
                cx.err(format!(
                    "{op:?} needs a number variable; {variable:?} is {kind:?}"
                ));
            }
            match value {
                Some(ValueSource::Constant(crate::Value::Number(n))) if n.is_finite() => {
                    if op == VarOp::Divide && *n == 0.0 {
                        cx.err("divide by zero");
                    }
                }
                Some(ValueSource::Variable { variable: src }) => {
                    match resolve_var_kind(course, slide, src) {
                        None => cx.err(format!("variable {src:?} does not exist")),
                        Some(src_kind) if src_kind != ValueKind::Number => cx.err(format!(
                            "{op:?} needs a number variable; {src:?} is {src_kind:?}"
                        )),
                        Some(_) => {}
                    }
                }
                _ => cx.err(format!("{op:?} needs a finite number value")),
            }
        }
    }
}

/// Resources and glossary entries: unique valid ids, non-empty text, safe targets.
fn check_resources_and_glossary(course: &Course, cx: &mut Ctx<'_>) {
    if !course.resources.is_empty() {
        let mut ids = BTreeSet::new();
        cx.with_field("resources", |cx| {
            for (i, resource) in course.resources.iter().enumerate() {
                cx.with_index(i, |cx| {
                    if !valid_name(&resource.id) || !ids.insert(resource.id.as_str()) {
                        cx.err(format!(
                            "resource id {:?} is invalid or duplicated",
                            resource.id
                        ));
                    }
                    if resource.title.trim().is_empty() {
                        cx.err("resource title is empty");
                    }
                    match &resource.target {
                        crate::ResourceTarget::Url { url } => {
                            if !crate::is_valid_link_url(url) {
                                cx.err_at("target", "resource URL must be http, https or mailto");
                            }
                        }
                        crate::ResourceTarget::File { src } => {
                            if !is_safe_relative_path(src.trim()) {
                                cx.err_at(
                                    "target",
                                    format!("resource file {src:?} must be a safe relative path"),
                                );
                            }
                        }
                    }
                });
                if cx.should_stop() {
                    return;
                }
            }
        });
    }
    if cx.should_stop() || course.glossary.is_empty() {
        return;
    }
    let mut ids = BTreeSet::new();
    cx.with_field("glossary", |cx| {
        for (i, entry) in course.glossary.iter().enumerate() {
            cx.with_index(i, |cx| {
                if !valid_name(&entry.id) || !ids.insert(entry.id.as_str()) {
                    cx.err(format!(
                        "glossary id {:?} is invalid or duplicated",
                        entry.id
                    ));
                }
                if entry.term.trim().is_empty() {
                    cx.err("glossary term is empty");
                }
            });
            if cx.should_stop() {
                return;
            }
        }
    });
}

/// The player logo must be a safe path to a picture.
fn check_logo(course: &Course, cx: &mut Ctx<'_>) {
    if let Some(logo) = &course.player.logo
        && !crate::is_image_src(logo)
    {
        cx.err_at(
            "player/logo",
            format!("player logo {logo:?} must be a safe relative path to an image"),
        );
    }
}

/// Warn on any non-last slide whose player bar is off and that has no Next,
/// Jump, or Exit trigger on the slide or its layers.
fn check_bar_off_dead_ends(course: &Course, cx: &mut Ctx<'_>) {
    if cx.skip_warnings {
        return;
    }
    let total_slides: usize = course.scenes.iter().map(|s| s.slides.len()).sum();
    let mut seen = 0usize;
    for (si, scene) in course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            seen += 1;
            if seen == total_slides {
                continue;
            }
            let show_bar = course.player_for(slide).show_bar;
            if show_bar || slide_has_forward_trigger(slide) {
                continue;
            }
            let name = if slide.title.is_empty() {
                slide.id.as_str()
            } else {
                slide.title.as_str()
            };
            cx.with_field_index("scenes", si, |cx| {
                cx.with_field_index("slides", li, |cx| {
                    cx.warn_at(
                        "player/show_bar",
                        format!(
                            "Slide \u{201C}{name}\u{201D} has the player bar turned off and no Next, Jump, or Exit trigger to move forward."
                        ),
                    );
                });
            });
        }
    }
}

fn slide_has_forward_trigger(slide: &Slide) -> bool {
    slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .any(|t| {
            !t.disabled
                && matches!(
                    &t.action,
                    crate::Action::JumpToSlide {
                        target: crate::SlideTarget::Next
                            | crate::SlideTarget::Id(_)
                            | crate::SlideTarget::Scene(_),
                    } | crate::Action::ExitCourse
                )
        })
}

/// Every "Next unlocks" rule, course-wide and per slide.
fn check_next_rules(course: &Course, cx: &mut Ctx<'_>) {
    cx.with_field("player", |cx| {
        cx.with_field("next_requires", |cx| {
            check_next_rule(course, cx, &course.player.next_requires);
        });
    });
    if cx.should_stop() {
        return;
    }
    for (si, scene) in course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            if let Some(rule) = slide
                .player
                .as_ref()
                .and_then(|overrides| overrides.next_requires.as_ref())
            {
                cx.with_field_index("scenes", si, |cx| {
                    cx.with_field_index("slides", li, |cx| {
                        cx.with_field("player", |cx| {
                            cx.with_field("next_requires", |cx| check_next_rule(course, cx, rule));
                        });
                    });
                });
                if cx.should_stop() {
                    return;
                }
            }
        }
    }
}

/// A "Next unlocks when a variable is true" rule needs a true/false variable.
fn check_next_rule(course: &Course, cx: &mut Ctx<'_>, rule: &crate::NextRequirement) {
    let crate::NextRequirement::Variable { name } = rule else {
        return;
    };
    let kind = crate::builtin_variable_kind(name)
        .or_else(|| course.variable(name).map(|def| def.value.kind()));
    match kind {
        None => cx.err(format!("variable {name:?} does not exist")),
        Some(ValueKind::Bool) => {}
        Some(other) => cx.err(format!("variable {name:?} is {other:?}, not true/false")),
    }
}

fn check_condition(course: &Course, cx: &mut Ctx<'_>, slide: &Slide, c: &Condition, depth: usize) {
    if depth > MAX_CONDITION_DEPTH {
        cx.err(format!(
            "condition nested deeper than {MAX_CONDITION_DEPTH}"
        ));
        return;
    }
    match c {
        Condition::All(list) | Condition::Any(list) => {
            if list.is_empty() {
                cx.warn("empty condition group");
            }
            for (i, c) in list.iter().enumerate() {
                cx.with_index(i, |cx| check_condition(course, cx, slide, c, depth + 1));
                if cx.should_stop() {
                    return;
                }
            }
        }
        Condition::Not(inner) => {
            cx.with_field("not", |cx| {
                check_condition(course, cx, slide, inner, depth + 1)
            });
        }
        Condition::Compare {
            variable,
            op,
            value,
        } => {
            let kind = resolve_var_kind(course, slide, variable);
            match kind {
                None => cx.err(format!("variable {variable:?} does not exist")),
                Some(kind) => {
                    match value {
                        ValueSource::Constant(v) => {
                            if v.kind() != kind {
                                cx.err(format!(
                                    "{variable:?} is {kind:?}, compared with {:?}",
                                    v.kind()
                                ));
                            }
                        }
                        ValueSource::Variable { variable: rhs } => {
                            match resolve_var_kind(course, slide, rhs) {
                                None => cx.err(format!("variable {rhs:?} does not exist")),
                                Some(rhs_kind) if rhs_kind != kind => {
                                    if !cx.skip_warnings {
                                        cx.warn(format!(
                                            "{variable:?} is {kind:?}, compared with {rhs:?} ({rhs_kind:?})"
                                        ));
                                    }
                                }
                                Some(_) => {}
                            }
                        }
                    }
                    let ordered = matches!(
                        op,
                        crate::CmpOp::Lt | crate::CmpOp::Le | crate::CmpOp::Gt | crate::CmpOp::Ge
                    );
                    if ordered && kind != ValueKind::Number {
                        cx.err(format!("{op:?} only compares numbers"));
                    }
                    if matches!(
                        op,
                        crate::CmpOp::Contains
                            | crate::CmpOp::EqIgnoreCase
                            | crate::CmpOp::ContainsIgnoreCase
                    ) && kind != ValueKind::Text
                    {
                        let msg = if *op == crate::CmpOp::Contains {
                            "contains only applies to text".to_owned()
                        } else {
                            format!("{op:?} only applies to text")
                        };
                        cx.err(msg);
                    }
                }
            }
        }
        Condition::ObjectState { object, state, .. } => {
            match check_object_or_interactive_group_ref(cx, slide, object) {
                Some(TargetRef::Object(o)) if !o.has_state(state) => {
                    cx.err(format!(
                        "state {state:?} is not defined on object {object:?}"
                    ));
                }
                Some(TargetRef::InteractiveGroup(g)) if !slide.group_has_state(&g.id, state) => {
                    cx.err(format!(
                        "state {state:?} is not defined on interactive group {:?}",
                        g.id
                    ));
                }
                _ => {}
            }
        }
    }
}

fn coord_ok(v: f32) -> bool {
    v.is_finite() && (-100_000.0..=100_000.0).contains(&v)
}

fn extent_ok(v: f32) -> bool {
    v.is_finite() && v > 0.0 && v <= 100_000.0
}

fn base_object<'a>(slide: &'a Slide, id: &str) -> Option<&'a Object> {
    match slide.object(id) {
        Some((object, None)) => Some(object),
        _ => None,
    }
}

fn check_hotspot(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question) {
    if !q.choices.is_empty() {
        cx.err("hotspot must not have choices");
    }
    match q.hotspot_image.as_deref() {
        Some(id) => match base_object(slide, id) {
            Some(object) if matches!(object.kind, ObjectKind::Image { .. }) => {}
            Some(_) => cx.err(format!("hotspot_image {id:?} is not an image object")),
            None => cx.err(format!(
                "hotspot_image {id:?} must name a base-layer image object"
            )),
        },
        None => cx.err("hotspot_image must name a base-layer image object"),
    }
    let n = q.regions.len();
    if !(1..=20).contains(&n) {
        cx.err(format!("hotspot needs 1 to 20 regions, found {n}"));
    }
    if !q.regions.iter().any(|r| r.correct) {
        cx.err("hotspot needs at least one correct region");
    }
    if cx.should_stop() {
        return;
    }
    let mut ids = BTreeSet::new();
    for region in &q.regions {
        if !valid_name(&region.id) {
            cx.err(format!(
                "region id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                region.id
            ));
        }
        if !ids.insert(region.id.as_str()) {
            cx.err(format!("duplicate region id {:?}", region.id));
        }
        if !coord_ok(region.x)
            || !coord_ok(region.y)
            || !extent_ok(region.w)
            || !extent_ok(region.h)
        {
            cx.err(format!(
                "region {:?} coordinates must be finite, x and y within ±100000, and w and h greater than 0 and at most 100000",
                region.id
            ));
        }
        if cx.should_stop() {
            return;
        }
    }
    if !(1..=10).contains(&q.markers) {
        cx.err(format!("markers must be 1 to 10, found {}", q.markers));
    }
}

fn check_drag_drop(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question) {
    if !q.choices.is_empty() {
        cx.err("drag_drop must not have choices");
    }
    let n = q.drag_items.len();
    if !(1..=20).contains(&n) {
        cx.err(format!("drag_drop needs 1 to 20 drag items, found {n}"));
    }
    if cx.should_stop() {
        return;
    }
    let mut items = BTreeSet::new();
    for drag in &q.drag_items {
        if !items.insert(drag.item.as_str()) {
            cx.err(format!("duplicate drag item {:?}", drag.item));
        }
        if base_object(slide, &drag.item).is_none() {
            cx.err(format!(
                "drag item {:?} is not a base-layer object",
                drag.item
            ));
        }
        if base_object(slide, &drag.target).is_none() {
            cx.err(format!(
                "drag target {:?} is not a base-layer object",
                drag.target
            ));
        }
        if drag.item == drag.target || q.drag_items.iter().any(|other| other.item == drag.target) {
            cx.err(format!(
                "drag item {:?} cannot target itself or another drag item",
                drag.item
            ));
        }
        if !q.drop_targets.iter().any(|id| id == &drag.target) {
            cx.err(format!(
                "drag item {:?} target {:?} is not in drop_targets",
                drag.item, drag.target
            ));
        }
        if cx.should_stop() {
            return;
        }
    }
    let t = q.drop_targets.len();
    if !(1..=20).contains(&t) {
        cx.err(format!("drop_targets needs 1 to 20 ids, found {t}"));
    }
    if cx.should_stop() {
        return;
    }
    let mut targets = BTreeSet::new();
    for id in &q.drop_targets {
        if !targets.insert(id.as_str()) {
            cx.err(format!("duplicate drop target {id:?}"));
        }
        if base_object(slide, id).is_none() {
            cx.err(format!("drop target {id:?} is not a base-layer object"));
        }
        if items.contains(id.as_str()) {
            cx.err(format!(
                "object {id:?} is both a drag item and a drop target"
            ));
        }
        if cx.should_stop() {
            return;
        }
    }
}

fn check_scoring(cx: &mut Ctx<'_>, q: &crate::Question) {
    for (i, c) in q.choices.iter().enumerate() {
        if !(-1000..=1000).contains(&c.points) {
            cx.with_field_index("choices", i, |cx| {
                cx.err_at("points", "choice points must be within -1000..=1000");
            });
            if cx.should_stop() {
                return;
            }
        }
    }
    match q.scoring {
        Scoring::PerAnswer => {
            if !matches!(
                q.kind,
                QuestionKind::MultipleChoice
                    | QuestionKind::MultipleResponse
                    | QuestionKind::TrueFalse
            ) {
                cx.err(
                    "per_answer scoring is only allowed for multiple_choice, multiple_response and true_false questions",
                );
            }
        }
        Scoring::Partial => {
            if !matches!(
                q.kind,
                QuestionKind::MultipleResponse
                    | QuestionKind::Sequence
                    | QuestionKind::Matching
                    | QuestionKind::Hotspot
                    | QuestionKind::DragDrop
            ) {
                cx.err(
                    "partial scoring is only allowed for multiple_response, sequence, matching, hotspot and drag_drop questions",
                );
            }
        }
        Scoring::AllOrNothing => {}
    }
}

fn check_likert(cx: &mut Ctx<'_>, q: &crate::Question) {
    let n = q.choices.len();
    if !(2..=11).contains(&n) {
        cx.err(format!("likert needs 2 to 11 choices, found {n}"));
    }
    if q.choices.iter().any(|c| c.correct) {
        cx.err("likert choices must not be marked correct");
    }
}

fn check_text_answer(cx: &mut Ctx<'_>, q: &crate::Question) {
    if !q.choices.is_empty() {
        cx.err("short_answer and essay must not have choices");
    }
}

fn check_freeform(course: &Course, cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question) {
    match &q.correct_when {
        Some(condition) => {
            cx.with_field("correct_when", |cx| {
                check_condition(course, cx, slide, condition, 1);
            });
        }
        None => cx.err("freeform needs a correct_when condition"),
    }
    if cx.should_stop() {
        return;
    }
    if !q.choices.is_empty() {
        cx.err("freeform must not have choices");
    }
    if !q.accepted.is_empty() {
        cx.err("freeform must not have accepted answers");
    }
    if !q.pairs.is_empty() {
        cx.err("freeform must not have pairs");
    }
    if !q.regions.is_empty() {
        cx.err("freeform must not have regions");
    }
    if !q.drag_items.is_empty() {
        cx.err("freeform must not have drag items");
    }
    if !q.drop_targets.is_empty() {
        cx.err("freeform must not have drop targets");
    }
    if q.scoring != Scoring::AllOrNothing {
        cx.err("freeform scoring must be all_or_nothing");
    }
}

fn check_question(course: &Course, cx: &mut Ctx<'_>, slide: &Slide) {
    let Some(q) = &slide.question else { return };
    cx.with_field("question", |cx| {
        let mut ids = BTreeSet::new();
        for c in &q.choices {
            if !ids.insert(c.id.as_str()) {
                cx.err(format!("duplicate choice id {:?}", c.id));
                if cx.should_stop() {
                    return;
                }
            }
        }
        check_scoring(cx, q);
        if cx.should_stop() {
            return;
        }
        let correct = q.choices.iter().filter(|c| c.correct).count();
        match q.kind {
            QuestionKind::MultipleChoice | QuestionKind::MultipleResponse => {
                if q.choices.len() < 2 {
                    cx.err("a question needs at least 2 choices");
                }
                if q.kind == QuestionKind::MultipleChoice && correct != 1 {
                    cx.err(format!(
                        "multiple_choice needs exactly 1 correct choice, found {correct}"
                    ));
                }
                if q.kind == QuestionKind::MultipleResponse && correct == 0 {
                    cx.err("multiple_response needs at least 1 correct choice");
                }
            }
            QuestionKind::TrueFalse => {
                if q.choices.len() != 2 {
                    cx.err(format!(
                        "true_false needs exactly 2 choices, found {}",
                        q.choices.len()
                    ));
                }
                if correct != 1 {
                    cx.err(format!(
                        "true_false needs exactly 1 correct choice, found {correct}"
                    ));
                }
            }
            QuestionKind::FillIn => {
                if !q.choices.is_empty() {
                    cx.err("fill_in must not have choices");
                }
                let n = q.accepted.len();
                if !(1..=50).contains(&n) {
                    cx.err(format!("fill_in needs 1 to 50 accepted answers, found {n}"));
                }
                if !cx.should_stop() {
                    for (i, answer) in q.accepted.iter().enumerate() {
                        let len = answer.trim().chars().count();
                        if !(1..=500).contains(&len) {
                            cx.with_field_index("accepted", i, |cx| {
                                cx.err(
                                    "accepted answer must be 1 to 500 characters after trimming",
                                );
                            });
                            if cx.should_stop() {
                                return;
                            }
                        }
                    }
                }
            }
            QuestionKind::Sequence => {
                let n = q.choices.len();
                if !(2..=20).contains(&n) {
                    cx.err(format!("sequence needs 2 to 20 choices, found {n}"));
                }
                if !cx.skip_warnings {
                    if q.choices.iter().any(|c| c.correct) {
                        cx.warn(
                            "sequence ignores correct flags; list choices in the correct order",
                        );
                    }
                    if q.shuffle {
                        cx.warn(
                            "shuffle is ignored for sequence; items are shuffled every attempt",
                        );
                    }
                }
            }
            QuestionKind::Matching => check_matching(cx, q),
            QuestionKind::Hotspot => check_hotspot(cx, slide, q),
            QuestionKind::DragDrop => check_drag_drop(cx, slide, q),
            QuestionKind::Likert => check_likert(cx, q),
            QuestionKind::ShortAnswer | QuestionKind::Essay => check_text_answer(cx, q),
            QuestionKind::Freeform => check_freeform(course, cx, slide, q),
        }
        if cx.should_stop() {
            return;
        }
        if q.kind != QuestionKind::Freeform && q.correct_when.is_some() {
            cx.err("correct_when is only allowed on freeform questions");
        }
        if !cx.should_stop() {
            check_question_layers(cx, slide, q);
        }
        if !cx.skip_warnings && !cx.should_stop() {
            check_question_submit(cx, slide);
        }
    });
}

fn check_question_layers(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question) {
    for l in [&q.correct_layer, &q.incorrect_layer].into_iter().flatten() {
        check_layer_ref(cx, slide, l);
        if cx.should_stop() {
            return;
        }
    }
    if !cx.skip_warnings
        && let Some(l) = &q.try_again_layer
        && slide.layer(l).is_none()
    {
        cx.warn(format!(
            "try_again_layer {l:?} is not on slide {:?}",
            slide.id
        ));
    }
}

fn check_matching(cx: &mut Ctx<'_>, q: &crate::Question) {
    if !q.choices.is_empty() {
        cx.err("matching must not have choices");
    }
    let n = q.pairs.len();
    if !(2..=20).contains(&n) {
        cx.err(format!("matching needs 2 to 20 pairs, found {n}"));
    }
    if cx.should_stop() {
        return;
    }
    let mut pair_ids = BTreeSet::new();
    for pair in &q.pairs {
        if !valid_name(&pair.id) {
            cx.err(format!(
                "pair id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                pair.id
            ));
        }
        if !pair_ids.insert(pair.id.as_str()) {
            cx.err(format!("duplicate pair id {:?}", pair.id));
        }
        if pair.left.is_empty() || pair.right.is_empty() {
            cx.err(format!(
                "pair {:?} needs non-empty left and right text",
                pair.id
            ));
        }
        if cx.should_stop() {
            return;
        }
    }
    if !cx.skip_warnings && q.shuffle {
        cx.warn("shuffle is ignored for matching; the right-hand column is shuffled every attempt");
    }
}

fn check_question_submit(cx: &mut Ctx<'_>, slide: &Slide) {
    if cx.skip_warnings || cx.should_stop() {
        return;
    }
    let mut has_submit = false;
    let mut all_from_submit_event = true;
    for trigger in slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
    {
        if trigger.action == Action::SubmitQuestion && !trigger.disabled {
            has_submit = true;
            if trigger.when != Event::Submit {
                all_from_submit_event = false;
            }
        }
    }
    if !has_submit {
        cx.warn("no trigger runs submit_question, so this question can never be graded");
    } else if all_from_submit_event {
        cx.warn("no control on this slide submits the question: add a Submit button");
    }
}

fn check_unknown_variable_refs(course: &Course, cx: &mut Ctx<'_>) {
    if cx.skip_warnings || cx.should_stop() {
        return;
    }
    if !course.scenes.is_empty() {
        cx.with_field("scenes", |cx| {
            for (si, scene) in course.scenes.iter().enumerate() {
                cx.with_index(si, |cx| {
                    cx.with_field("slides", |cx| {
                        for (li, slide) in scene.slides.iter().enumerate() {
                            cx.with_index(li, |cx| {
                                check_slide_unknown_variable_refs(course, cx, slide);
                            });
                        }
                    });
                });
            }
        });
    }
    if !course.banks.is_empty() {
        cx.with_field("banks", |cx| {
            for (bi, bank) in course.banks.iter().enumerate() {
                cx.with_index(bi, |cx| {
                    cx.with_field("slides", |cx| {
                        for (si, slide) in bank.slides.iter().enumerate() {
                            cx.with_index(si, |cx| {
                                check_slide_unknown_variable_refs(course, cx, slide);
                            });
                        }
                    });
                });
            }
        });
    }
    if !course.masters.is_empty() {
        cx.with_field("masters", |cx| {
            for (mi, master) in course.masters.iter().enumerate() {
                cx.with_index(mi, |cx| {
                    check_slide_unknown_variable_refs(course, cx, master);
                });
            }
        });
    }
    if !course.templates.is_empty() {
        cx.with_field("templates", |cx| {
            for (ti, template) in course.templates.iter().enumerate() {
                cx.with_index(ti, |cx| {
                    check_slide_unknown_variable_refs(course, cx, template);
                });
            }
        });
    }
}

fn check_slide_unknown_variable_refs(course: &Course, cx: &mut Ctx<'_>, slide: &Slide) {
    if !slide.objects.is_empty() {
        cx.with_field("objects", |cx| {
            for (oi, obj) in slide.objects.iter().enumerate() {
                cx.with_index(oi, |cx| {
                    check_object_unknown_variable_refs(course, cx, slide, obj);
                });
            }
        });
    }
    if !slide.layers.is_empty() {
        cx.with_field("layers", |cx| {
            for (ki, layer) in slide.layers.iter().enumerate() {
                if !layer.objects.is_empty() {
                    cx.with_index(ki, |cx| {
                        cx.with_field("objects", |cx| {
                            for (oi, obj) in layer.objects.iter().enumerate() {
                                cx.with_index(oi, |cx| {
                                    check_object_unknown_variable_refs(course, cx, slide, obj);
                                });
                            }
                        });
                    });
                }
            }
        });
    }
    if let Some(q) = &slide.question {
        cx.with_field("question", |cx| {
            check_text_unknown_variable_refs(course, cx, slide, &q.prompt, Some("prompt"));
            if !q.choices.is_empty() {
                cx.with_field("choices", |cx| {
                    for (ci, choice) in q.choices.iter().enumerate() {
                        cx.with_index(ci, |cx| {
                            check_text_unknown_variable_refs(course, cx, slide, &choice.text, None);
                        });
                    }
                });
            }
            if !q.pairs.is_empty() {
                cx.with_field("pairs", |cx| {
                    for (pi, pair) in q.pairs.iter().enumerate() {
                        cx.with_index(pi, |cx| {
                            check_text_unknown_variable_refs(course, cx, slide, &pair.left, None);
                            check_text_unknown_variable_refs(course, cx, slide, &pair.right, None);
                        });
                    }
                });
            }
        });
    }
}

fn check_object_unknown_variable_refs(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    obj: &Object,
) {
    match &obj.kind {
        ObjectKind::Text { text, runs, .. } => {
            check_rich_text_unknown_variable_refs(course, cx, slide, text, runs);
        }
        ObjectKind::Shape {
            text: Some(shape_text),
            ..
        } => {
            check_rich_text_unknown_variable_refs(
                course,
                cx,
                slide,
                &shape_text.text,
                &shape_text.runs,
            );
        }
        ObjectKind::Button { label, .. } => {
            check_text_unknown_variable_refs(course, cx, slide, label, None);
        }
        ObjectKind::Marker {
            title, body, runs, ..
        } => {
            check_text_unknown_variable_refs(course, cx, slide, title, None);
            check_rich_text_unknown_variable_refs(course, cx, slide, body, runs);
        }
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    check_rich_text_unknown_variable_refs(
                        course, cx, slide, &cell.text, &cell.runs,
                    );
                }
            }
        }
        ObjectKind::Path { path } => {
            if let Some(shape_text) = &path.text {
                check_rich_text_unknown_variable_refs(
                    course,
                    cx,
                    slide,
                    &shape_text.text,
                    &shape_text.runs,
                );
            }
        }
        _ => {}
    }

    for state in &obj.states {
        if let Some(runs) = &state.runs
            && !runs.is_empty()
        {
            for run in runs {
                check_text_unknown_variable_refs(course, cx, slide, &run.text, None);
            }
        } else if let Some(text) = &state.text {
            check_text_unknown_variable_refs(course, cx, slide, text, None);
        }
        for extra in &state.extras {
            match &extra.kind {
                StateExtraKind::Text { text, runs, .. } => {
                    check_rich_text_unknown_variable_refs(course, cx, slide, text, runs);
                }
                StateExtraKind::Shape {
                    text: Some(shape_text),
                    ..
                } => {
                    check_rich_text_unknown_variable_refs(
                        course,
                        cx,
                        slide,
                        &shape_text.text,
                        &shape_text.runs,
                    );
                }
                StateExtraKind::Path { path } => {
                    if let Some(shape_text) = &path.text {
                        check_rich_text_unknown_variable_refs(
                            course,
                            cx,
                            slide,
                            &shape_text.text,
                            &shape_text.runs,
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn check_rich_text_unknown_variable_refs(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    text: &str,
    runs: &[crate::TextRun],
) {
    if !runs.is_empty() {
        for run in runs {
            check_text_unknown_variable_refs(course, cx, slide, &run.text, None);
        }
    } else {
        check_text_unknown_variable_refs(course, cx, slide, text, None);
    }
}

fn check_text_unknown_variable_refs(
    course: &Course,
    cx: &mut Ctx<'_>,
    slide: &Slide,
    text: &str,
    suffix: Option<&str>,
) {
    for span in crate::variable_reference_spans(text) {
        let Some(name) = text.get(span.start.saturating_add(1)..span.end.saturating_sub(1)) else {
            continue;
        };
        let known = course.variable(name).is_some()
            || crate::builtin_variable_kind(name).is_some()
            || crate::is_timer_remaining_variable_on(slide, name);
        if !known {
            let message =
                format!("Text uses %{name}%, but the course has no variable called {name}.");
            if let Some(s) = suffix {
                cx.warn_at(s, message);
            } else {
                cx.warn(message);
            }
        }
    }
}
