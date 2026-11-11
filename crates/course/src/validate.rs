//! Course validation. Never panics: every problem becomes an [`Issue`] with a
//! JSON-pointer-like path so the editor, CLI and agents can point at it.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    Action, Condition, Course, Event, Layer, Object, ObjectKind, QuestionKind, SCHEMA_VERSION,
    Scoring, Slide, SlideTarget, Trigger, ValueKind, VarOp,
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

struct Ctx<'a> {
    course: &'a Course,
    issues: Vec<Issue>,
}

impl Ctx<'_> {
    fn err(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        });
    }
    fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        });
    }
}

fn valid_name(s: &str) -> bool {
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
        let mut cx = Ctx {
            course: self,
            issues: Vec::new(),
        };
        if self.schema_version != SCHEMA_VERSION {
            cx.err(
                "/schema_version",
                format!(
                    "unsupported schema_version {} (this build reads {SCHEMA_VERSION})",
                    self.schema_version
                ),
            );
        }
        if !valid_name(&self.id) {
            cx.err("/id", "course id must be 1-128 chars of [A-Za-z0-9_-]");
        }
        if self.title.trim().is_empty() {
            cx.err("/title", "course title is empty");
        }
        if let Some(lang) = &self.language
            && !crate::is_language_tag_valid(lang)
        {
            cx.err(
                "/language",
                format!(
                    "language {lang:?} is not a simple BCP 47 tag (letters and digits in hyphen-separated subtags, first subtag 2-8 letters)"
                ),
            );
        }
        for key in self.strings.keys() {
            if !crate::CHROME_STRINGS.iter().any(|(k, _)| k == key) {
                cx.warn(
                    format!("/strings/{key}"),
                    format!("unknown player string key {key:?}"),
                );
            }
        }
        if self.slide_size.w == 0
            || self.slide_size.h == 0
            || self.slide_size.w > 8192
            || self.slide_size.h > 8192
        {
            cx.err("/slide_size", "slide size must be 1..=8192 px on each side");
        }
        if let Some(q) = &self.quiz
            && q.passing_percent > 100
        {
            cx.err("/quiz/passing_percent", "passing_percent must be 0..=100");
        }
        if !self.theme.accent.is_valid() {
            cx.err(
                "/theme/accent",
                format!(
                    "color {:?} is not #RRGGBB or #RRGGBBAA",
                    self.theme.accent.0
                ),
            );
        }
        if !self.theme.background.is_valid() {
            cx.err(
                "/theme/background",
                format!(
                    "color {:?} is not #RRGGBB or #RRGGBBAA",
                    self.theme.background.0
                ),
            );
        }
        if !self.theme.text.is_valid() {
            cx.err(
                "/theme/text",
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", self.theme.text.0),
            );
        }
        if !self.theme.player.bar.is_valid() {
            cx.err(
                "/theme/player/bar",
                format!(
                    "color {:?} is not #RRGGBB or #RRGGBBAA",
                    self.theme.player.bar.0
                ),
            );
        }
        if !self.theme.player.bar_text.is_valid() {
            cx.err(
                "/theme/player/bar_text",
                format!(
                    "color {:?} is not #RRGGBB or #RRGGBBAA",
                    self.theme.player.bar_text.0
                ),
            );
        }
        check_course_fonts(&mut cx);
        check_background_audio(&mut cx);
        check_next_rules(&mut cx);
        check_resources_and_glossary(&mut cx);
        check_logo(&mut cx);
        check_font_name(&mut cx, &self.theme.font, "/theme/font");
        check_palette(&mut cx);
        check_text_styles(&mut cx);

        let mut names = BTreeSet::new();
        for (i, v) in self.variables.iter().enumerate() {
            let p = format!("/variables/{i}");
            if !valid_name(&v.name) {
                cx.err(
                    &p,
                    format!(
                        "variable name {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                        v.name
                    ),
                );
            }
            if !names.insert(v.name.as_str()) {
                cx.err(&p, format!("duplicate variable {:?}", v.name));
            }
            if let crate::Value::Number(n) = v.value
                && !n.is_finite()
            {
                cx.err(&p, "variable value must be finite");
            }
        }

        let mut scene_ids = BTreeSet::new();
        let mut slide_ids = BTreeSet::new();
        let mut slide_count = 0usize;
        for (si, scene) in self.scenes.iter().enumerate() {
            if !scene_ids.insert(scene.id.as_str()) {
                cx.err(
                    format!("/scenes/{si}"),
                    format!("duplicate scene id {:?}", scene.id),
                );
            }
            for (li, slide) in scene.slides.iter().enumerate() {
                slide_count += 1;
                let p = format!("/scenes/{si}/slides/{li}");
                if !valid_name(&slide.id) {
                    cx.err(
                        &p,
                        format!(
                            "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                            slide.id
                        ),
                    );
                }
                if !slide_ids.insert(slide.id.as_str()) {
                    cx.err(&p, format!("duplicate slide id {:?}", slide.id));
                }
                check_slide(&mut cx, slide, &p);
            }
        }
        let scene_slide_ids = slide_ids.clone();
        let mut all_slide_ids = slide_ids;
        check_banks(&mut cx, &mut all_slide_ids);
        check_draws(&mut cx);
        check_masters_and_templates(&mut cx, &all_slide_ids);
        if slide_count == 0 {
            cx.err("/scenes", "course has no slides");
        }
        check_course_results(&mut cx);
        if let Some(start) = &self.start_slide
            && !scene_slide_ids.contains(start.as_str())
        {
            cx.err(
                "/start_slide",
                format!("start_slide {start:?} does not exist"),
            );
        }
        cx.issues
    }

    /// True when validation found no errors (warnings are allowed).
    #[must_use]
    pub fn is_playable(&self) -> bool {
        self.validate()
            .iter()
            .all(|i| i.severity != Severity::Error)
    }
}

/// Bounded, unique, clean font faces stored under `fonts/`.
fn check_course_fonts(cx: &mut Ctx<'_>) {
    if cx.course.fonts.len() > crate::MAX_FONT_FACES {
        cx.err(
            "/fonts",
            format!(
                "a course may register at most {} font faces, found {}",
                crate::MAX_FONT_FACES,
                cx.course.fonts.len()
            ),
        );
    }
    let mut seen = BTreeSet::new();
    for (i, face) in cx.course.fonts.iter().enumerate() {
        let p = format!("/fonts/{i}");
        let family = face.family.trim();
        if family.is_empty() {
            cx.err(format!("{p}/family"), "font family is empty");
        } else if family.chars().count() > crate::MAX_FONT_FAMILY_CHARS {
            cx.err(
                format!("{p}/family"),
                format!(
                    "font family may be at most {} characters",
                    crate::MAX_FONT_FAMILY_CHARS
                ),
            );
        } else if family.chars().any(char::is_control) {
            cx.err(
                format!("{p}/family"),
                "font family must not contain control characters",
            );
        }
        let key = (family.to_ascii_lowercase(), face.style);
        if !seen.insert(key) {
            cx.err(
                &p,
                format!(
                    "duplicate font face for family {family:?} and style {:?}",
                    face.style
                ),
            );
        }
        let src = face.src.trim();
        if src.is_empty() {
            cx.err(format!("{p}/src"), "font src is empty");
        } else if !is_safe_relative_path(src) {
            cx.err(
                format!("{p}/src"),
                format!("font src {src:?} must be a safe relative path under media/"),
            );
        } else if !src.starts_with("fonts/") {
            cx.err(
                format!("{p}/src"),
                format!("font src {src:?} must be under fonts/"),
            );
        } else if !(src.to_ascii_lowercase().ends_with(".ttf")
            || src.to_ascii_lowercase().ends_with(".otf"))
        {
            cx.err(
                format!("{p}/src"),
                format!("font src {src:?} must be a .ttf or .otf file"),
            );
        }
    }
}

/// The course-wide background track, when one is set.
fn check_background_audio(cx: &mut Ctx<'_>) {
    let Some(audio) = &cx.course.background_audio else {
        return;
    };
    let p = "/background_audio";
    let src = audio.src.trim();
    if src.is_empty() {
        cx.err(format!("{p}/src"), "background audio src is empty");
    } else if !is_safe_relative_path(src) {
        cx.err(
            format!("{p}/src"),
            format!("background audio src {src:?} must be a safe relative path under media/"),
        );
    }
    if !audio.volume.is_finite() || !(0.0..=1.0).contains(&audio.volume) {
        cx.err(
            format!("{p}/volume"),
            "background audio volume must be finite and within 0.0..=1.0",
        );
    }
}

/// Warn when a font name is neither Inter nor a registered course family.
fn check_font_name(cx: &mut Ctx<'_>, name: &str, p: &str) {
    let name = name.trim();
    if name.is_empty() {
        cx.warn(p, "font name is empty; Inter is used instead");
    } else if !cx.course.has_font_family(name) {
        cx.warn(
            p,
            format!("font {name:?} is not Inter or a course font; Inter is used instead"),
        );
    }
}

fn check_palette(cx: &mut Ctx<'_>) {
    let palette = &cx.course.theme.palette;
    if palette.len() > crate::MAX_PALETTE_COLORS {
        cx.err(
            "/theme/palette",
            format!(
                "a palette holds at most {} colours",
                crate::MAX_PALETTE_COLORS
            ),
        );
    }
    let mut ids = BTreeSet::new();
    for (i, entry) in palette.iter().enumerate() {
        let p = format!("/theme/palette/{i}");
        if !valid_name(&entry.id) {
            cx.err(
                &p,
                format!(
                    "palette id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    entry.id
                ),
            );
        }
        if !ids.insert(entry.id.as_str()) {
            cx.err(&p, format!("duplicate palette id {:?}", entry.id));
        }
        if entry.name.trim().is_empty() {
            cx.err(&p, "palette colour name is empty");
        }
        if !entry.color.is_valid() {
            cx.err(
                format!("{p}/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", entry.color.0),
            );
        }
    }
}

fn check_text_styles(cx: &mut Ctx<'_>) {
    let styles = cx.course.text_styles.clone();
    if styles.len() > crate::MAX_TEXT_STYLES {
        cx.err(
            "/text_styles",
            format!(
                "a course holds at most {} text styles",
                crate::MAX_TEXT_STYLES
            ),
        );
    }
    let mut ids = BTreeSet::new();
    for (i, named) in styles.iter().enumerate() {
        let p = format!("/text_styles/{i}");
        if !valid_name(&named.id) {
            cx.err(
                &p,
                format!(
                    "text style id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    named.id
                ),
            );
        }
        if !ids.insert(named.id.clone()) {
            cx.err(&p, format!("duplicate text style id {:?}", named.id));
        }
        if named.name.trim().is_empty() {
            cx.err(&p, "text style name is empty");
        }
        if !named.style.color.is_valid() {
            cx.err(
                format!("{p}/style/color"),
                format!(
                    "color {:?} is not #RRGGBB or #RRGGBBAA",
                    named.style.color.0
                ),
            );
        }
        check_font_style(cx, &named.style, &format!("{p}/style"));
        if let Some(paragraph) = &named.paragraphs {
            check_paragraphs(cx, std::slice::from_ref(paragraph), &p);
        }
    }
}

fn check_font_style(cx: &mut Ctx<'_>, style: &crate::TextStyle, p: &str) {
    check_font_name(cx, &style.font, &format!("{p}/font"));
}

fn check_masters_and_templates(cx: &mut Ctx<'_>, slide_ids: &BTreeSet<&str>) {
    let mut master_ids = BTreeSet::new();
    for mi in 0..cx.course.masters.len() {
        let Some(master) = cx.course.masters.get(mi).cloned() else {
            continue;
        };
        let p = format!("/masters/{mi}");
        if !valid_name(&master.id) {
            cx.err(
                &p,
                format!(
                    "master id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    master.id
                ),
            );
        }
        if !master_ids.insert(master.id.clone()) {
            cx.err(&p, format!("duplicate master id {:?}", master.id));
        }
        if slide_ids.contains(master.id.as_str()) {
            cx.err(
                &p,
                format!(
                    "master id {:?} is also a slide id; slide ids are unique across scenes and masters",
                    master.id
                ),
            );
        }
        check_master(cx, &master, &p);
    }
    let mut template_ids = BTreeSet::new();
    for ti in 0..cx.course.templates.len() {
        let Some(template_id) = cx
            .course
            .templates
            .get(ti)
            .map(|template| template.id.clone())
        else {
            continue;
        };
        let p = format!("/templates/{ti}");
        if !valid_name(&template_id) {
            cx.err(
                &p,
                format!(
                    "template id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    template_id
                ),
            );
        }
        if !template_ids.insert(template_id.clone()) {
            cx.err(&p, format!("duplicate template id {:?}", template_id));
        }
        if slide_ids.contains(template_id.as_str()) || master_ids.contains(&template_id) {
            cx.err(
                &p,
                format!(
                    "template id {:?} is also a slide or master id; slide ids are unique across scenes, masters and templates",
                    template_id
                ),
            );
        }
        let has_draw = cx
            .course
            .templates
            .get(ti)
            .is_some_and(|template| template.draw.is_some());
        if has_draw {
            cx.err(
                format!("{p}/draw"),
                format!("template {template_id:?} must not have draw"),
            );
        }
    }
    let mut missing_masters = Vec::new();
    for (si, scene) in cx.course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            if let Some(ref master) = slide.master {
                missing_masters.push((si, li, slide.id.clone(), master.clone()));
            }
        }
    }
    for (si, li, slide_id, master) in missing_masters {
        if !master_ids.contains(master.as_str()) {
            cx.warn(
                format!("/scenes/{si}/slides/{li}/master"),
                format!(
                    "slide {slide_id:?} names master {master:?}, which does not exist; it draws with no master"
                ),
            );
        }
    }
}

fn check_banks<'a>(cx: &mut Ctx<'a>, slide_ids: &mut BTreeSet<&'a str>) {
    let mut bank_ids = BTreeSet::new();
    for (bi, bank) in cx.course.banks.iter().enumerate() {
        let bp = format!("/banks/{bi}");
        if !valid_name(&bank.id) {
            cx.err(
                &bp,
                format!("bank id {:?} must be 1-128 chars of [A-Za-z0-9_-]", bank.id),
            );
        }
        if !bank_ids.insert(bank.id.as_str()) {
            cx.err(&bp, format!("duplicate bank id {:?}", bank.id));
        }
        if bank.title.trim().is_empty() {
            cx.err(format!("{bp}/title"), "bank title is empty");
        }
        for (si, slide) in bank.slides.iter().enumerate() {
            let p = format!("{bp}/slides/{si}");
            if !valid_name(&slide.id) {
                cx.err(
                    &p,
                    format!(
                        "slide id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                        slide.id
                    ),
                );
            }
            if !slide_ids.insert(slide.id.as_str()) {
                cx.err(&p, format!("duplicate slide id {:?}", slide.id));
            }
            check_slide(cx, slide, &p);
            check_bank_slide(cx, slide, &p);
        }
    }
}

fn check_bank_slide(cx: &mut Ctx<'_>, slide: &Slide, p: &str) {
    if slide.question.is_none() {
        cx.err(
            format!("{p}/question"),
            format!("bank slide {:?} must have a question", slide.id),
        );
    }
    if slide.draw.is_some() {
        cx.err(
            format!("{p}/draw"),
            format!("bank slide {:?} must not have draw", slide.id),
        );
    }
}

fn check_draws(cx: &mut Ctx<'_>) {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for (si, scene) in cx.course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            let Some(draw) = &slide.draw else { continue };
            let p = format!("/scenes/{si}/slides/{li}/draw");
            let bank = cx.course.bank(&draw.bank);
            if bank.is_none() {
                cx.err(&p, format!("draw bank {:?} does not exist", draw.bank));
            }
            if slide.question.is_some() {
                cx.err(
                    &p,
                    format!("slide {:?} with draw must not have a question", slide.id),
                );
            }
            if draw.count == 0 {
                cx.err(format!("{p}/count"), "draw count must be at least 1");
            } else if let Some(bank) = bank {
                let size = bank.slides.len() as u64;
                if u64::from(draw.count) > size {
                    cx.err(
                        format!("{p}/count"),
                        format!(
                            "draw count {} is more than bank {:?} size {size}",
                            draw.count, draw.bank
                        ),
                    );
                }
                let total = totals.entry(draw.bank.clone()).or_insert(0);
                *total = total.saturating_add(u64::from(draw.count));
            }
        }
    }
    check_draw_totals(cx, &totals);
}

fn check_draw_totals(cx: &mut Ctx<'_>, totals: &BTreeMap<String, u64>) {
    for (bi, bank) in cx.course.banks.iter().enumerate() {
        let Some(total) = totals.get(&bank.id) else {
            continue;
        };
        let size = bank.slides.len() as u64;
        if *total > size {
            cx.err(
                format!("/banks/{bi}"),
                format!(
                    "draw counts from bank {:?} add up to {total}, more than the bank size {size}",
                    bank.id
                ),
            );
        }
    }
}

fn check_course_results(cx: &mut Ctx<'_>) {
    if !cx.course.reports_result() {
        cx.warn(
            "/scenes",
            "this course never reports a result to the LMS: add a Finish button on the last slide",
        );
    }
}

fn check_slide(cx: &mut Ctx<'_>, slide: &Slide, p: &str) {
    if let Some(ref bg) = slide.background
        && !bg.is_valid()
    {
        cx.err(
            format!("{p}/background"),
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", bg.0),
        );
    }
    if let Some(ref tr) = slide.transition
        && (tr.duration_ms == 0 || tr.duration_ms > 60000)
    {
        cx.err(
            format!("{p}/transition/duration_ms"),
            "transition duration_ms must be 1..=60000",
        );
    }
    let mut ids = BTreeSet::new();
    for id in slide.local_ids() {
        if !valid_name(id) {
            cx.err(
                p,
                format!("object/layer id {id:?} must be 1-128 chars of [A-Za-z0-9_-]"),
            );
        }
        if !ids.insert(id) {
            cx.err(
                p,
                format!("duplicate object/layer id {id:?} on slide {:?}", slide.id),
            );
        }
    }
    for (i, o) in slide.objects.iter().enumerate() {
        check_object(cx, o, slide.duration_ms, &format!("{p}/objects/{i}"));
    }
    check_group_members(cx, &slide.objects, &format!("{p}/objects"));
    check_panel_members(cx, &slide.objects, &format!("{p}/objects"));
    check_slide_draggable_objects(cx, slide, p);
    for (i, t) in slide.triggers.iter().enumerate() {
        check_trigger(cx, slide, None, t, &format!("{p}/triggers/{i}"));
    }
    for (li, layer) in slide.layers.iter().enumerate() {
        let lp = format!("{p}/layers/{li}");
        for (i, o) in layer.objects.iter().enumerate() {
            check_object(cx, o, layer.duration_ms, &format!("{lp}/objects/{i}"));
        }
        check_group_members(cx, &layer.objects, &format!("{lp}/objects"));
        check_panel_members(cx, &layer.objects, &format!("{lp}/objects"));
        for (i, t) in layer.triggers.iter().enumerate() {
            check_trigger(cx, slide, Some(layer), t, &format!("{lp}/triggers/{i}"));
        }
    }
    check_question(cx, slide, p);
    if slide_has_unsupported_script(slide) {
        cx.warn(
            p,
            "text uses characters the bundled Inter font has no glyphs for; right-to-left and complex shaping are not supported",
        );
    }
}

/// Check a slide master. A master is a slide for authoring, but it must never
/// carry play-time behaviour: no triggers, layers or question, and no audio or
/// video. Its objects get the same checks as a slide's, named with the master.
fn check_master(cx: &mut Ctx<'_>, master: &Slide, p: &str) {
    let label = if master.title.trim().is_empty() {
        format!("master {:?}", master.id)
    } else {
        format!("master {:?} ({:?})", master.id, master.title)
    };
    if !master.triggers.is_empty() {
        cx.err(
            format!("{p}/triggers"),
            format!("{label} must not have triggers"),
        );
    }
    if !master.layers.is_empty() {
        cx.err(
            format!("{p}/layers"),
            format!("{label} must not have layers"),
        );
    }
    if master.question.is_some() {
        cx.err(
            format!("{p}/question"),
            format!("{label} must not have a question"),
        );
    }
    if master.draw.is_some() {
        cx.err(format!("{p}/draw"), format!("{label} must not have draw"));
    }
    for (i, object) in master.objects.iter().enumerate() {
        if matches!(
            object.kind,
            ObjectKind::Audio { .. } | ObjectKind::Video { .. } | ObjectKind::Lottie { .. }
        ) {
            cx.err(
                format!("{p}/objects/{i}/kind"),
                format!(
                    "{label} must not hold audio or video (or Lottie) object {:?}",
                    object.id
                ),
            );
        }
    }
    if let Some(ref bg) = master.background
        && !bg.is_valid()
    {
        cx.err(
            format!("{p}/background"),
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", bg.0),
        );
    }
    let mut ids = BTreeSet::new();
    for id in master.local_ids() {
        if !valid_name(id) {
            cx.err(
                p,
                format!("{label}: object/layer id {id:?} must be 1-128 chars of [A-Za-z0-9_-]"),
            );
        }
        if !ids.insert(id) {
            cx.err(p, format!("{label}: duplicate object/layer id {id:?}"));
        }
    }
    // Run the ordinary object checks, then name the master in every new message.
    let first_new = cx.issues.len();
    for (i, o) in master.objects.iter().enumerate() {
        check_object(cx, o, master.duration_ms, &format!("{p}/objects/{i}"));
    }
    check_group_members(cx, &master.objects, &format!("{p}/objects"));
    check_panel_members(cx, &master.objects, &format!("{p}/objects"));
    for issue in cx.issues.get_mut(first_new..).into_iter().flatten() {
        if !issue.message.starts_with(&label) {
            issue.message = format!("{label}: {}", issue.message);
        }
    }
    if slide_has_unsupported_script(master) {
        cx.warn(
            p,
            format!(
                "{label}: text uses characters the bundled Inter font has no glyphs for; right-to-left and complex shaping are not supported"
            ),
        );
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
        }) || o
            .states
            .iter()
            .any(|state| state.text.as_deref().is_some_and(bad))
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

/// Warn when a group id has only one member on this layer.
fn check_group_members(cx: &mut Ctx<'_>, objects: &[Object], prefix: &str) {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() {
        if let Some(group) = object.group.as_deref() {
            let entry = counts.entry(group).or_insert((0, index));
            entry.0 = entry.0.saturating_add(1);
        }
    }
    for (group, (count, index)) in counts {
        if count == 1 {
            cx.warn(
                format!("{prefix}/{index}"),
                format!("group {group:?} has only one member on this layer"),
            );
        }
    }
}

/// Check that every `panel` link names a scrolling panel on the same layer,
/// and that no panel sits inside another panel.
fn check_panel_members(cx: &mut Ctx<'_>, objects: &[Object], prefix: &str) {
    for (index, object) in objects.iter().enumerate() {
        let Some(panel_id) = object.panel.as_deref() else {
            continue;
        };
        let path = format!("{prefix}/{index}/panel");
        if matches!(object.kind, ObjectKind::ScrollPanel { .. }) {
            cx.err(
                path,
                "a scrolling panel cannot be inside another scrolling panel",
            );
            continue;
        }
        match objects.iter().find(|candidate| candidate.id == panel_id) {
            None => cx.err(
                path,
                format!("panel {panel_id:?} is not a scrolling panel on this layer"),
            ),
            Some(panel) if !matches!(panel.kind, ObjectKind::ScrollPanel { .. }) => cx.err(
                path,
                format!("object {panel_id:?} is not a scrolling panel"),
            ),
            Some(_) => {}
        }
    }
}

fn check_slide_draggable_objects(cx: &mut Ctx<'_>, slide: &Slide, p: &str) {
    let check_obj = |cx: &mut Ctx<'_>, o: &Object, obj_p: &str| {
        let Some(drag) = &o.drag else { return };
        if slide.question.as_ref().is_some_and(|q| match q.kind {
            QuestionKind::Hotspot => q.hotspot_image.as_deref() == Some(&o.id),
            QuestionKind::DragDrop => {
                q.drag_items.iter().any(|d| d.item == o.id)
                    || q.drop_targets.iter().any(|t| t == &o.id)
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
            cx.err(
                format!("{obj_p}/drag"),
                format!(
                    "draggable spec is not allowed on question widget {:?}",
                    o.id
                ),
            );
        }
        for (ti, target_id) in drag.targets.iter().enumerate() {
            if slide.object(target_id).is_none() {
                cx.err(
                    format!("{obj_p}/drag/targets/{ti}"),
                    format!("target {target_id:?} is not on slide {:?}", slide.id),
                );
            }
        }
    };

    for (i, o) in slide.objects.iter().enumerate() {
        check_obj(cx, o, &format!("{p}/objects/{i}"));
    }
    for (li, layer) in slide.layers.iter().enumerate() {
        for (i, o) in layer.objects.iter().enumerate() {
            check_obj(cx, o, &format!("{p}/layers/{li}/objects/{i}"));
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

fn check_text_runs(cx: &mut Ctx<'_>, text: &str, runs: &[crate::TextRun], p: &str) {
    if runs.len() > 1000 {
        cx.err(format!("{p}/runs"), "text object has more than 1000 runs");
    }
    for (ri, run) in runs.iter().enumerate() {
        if let Some(ref c) = run.color
            && !c.is_valid()
        {
            cx.err(
                format!("{p}/runs/{ri}/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", c.0),
            );
        }
        if let Some(font) = &run.font {
            check_font_name(cx, font, &format!("{p}/runs/{ri}/font"));
        }
        if let Some(link) = &run.link
            && !link.is_valid(cx.course)
        {
            let message = match link {
                crate::Link::Url { url } => {
                    format!("link {url:?} must start with http://, https:// or mailto:")
                }
                crate::Link::Slide { slide } => {
                    format!("link target slide {slide:?} does not exist")
                }
            };
            cx.err(format!("{p}/runs/{ri}/link"), message);
        }
    }
    if !runs.is_empty() {
        let concat: String = runs.iter().map(|r| r.text.as_str()).collect();
        if text != concat {
            cx.warn(
                format!("{p}/text"),
                "text does not match the concatenation of runs",
            );
        }
    }
}

/// Paragraph formatting is bounded; the runtime clamps hostile values, but a
/// loaded file should say what is wrong.
fn check_paragraphs(cx: &mut Ctx<'_>, paragraphs: &[crate::Paragraph], p: &str) {
    for (i, paragraph) in paragraphs.iter().enumerate() {
        if paragraph.level > 4 {
            cx.err(
                format!("{p}/paragraphs/{i}/level"),
                "paragraph level must be within 0..=4",
            );
        }
        if !paragraph.line_spacing.is_finite() || !(0.8..=3.0).contains(&paragraph.line_spacing) {
            cx.err(
                format!("{p}/paragraphs/{i}/line_spacing"),
                "line_spacing must be finite and within 0.8..=3",
            );
        }
        for (name, value) in [
            ("space_before", paragraph.space_before),
            ("space_after", paragraph.space_after),
        ] {
            if !value.is_finite() || !(0.0..=200.0).contains(&value) {
                cx.err(
                    format!("{p}/paragraphs/{i}/{name}"),
                    format!("{name} must be finite and within 0..=200"),
                );
            }
        }
    }
}

fn check_fill_style(cx: &mut Ctx<'_>, style: &crate::FillStyle, p: &str) {
    match style {
        crate::FillStyle::Linear { angle, stops } => {
            if !angle.is_finite() || !(0.0..360.0).contains(angle) {
                cx.err(
                    format!("{p}/fill_style/angle"),
                    "gradient angle must be a finite number of degrees in 0..360",
                );
            }
            check_gradient_stops(cx, stops, &format!("{p}/fill_style"));
        }
        crate::FillStyle::Radial { center, stops } => {
            if !center.iter().all(|value| value.is_finite())
                || !(0.0..=1.0).contains(&center[0])
                || !(0.0..=1.0).contains(&center[1])
            {
                cx.err(
                    format!("{p}/fill_style/center"),
                    "radial gradient centre must be finite fractions within 0..=1",
                );
            }
            check_gradient_stops(cx, stops, &format!("{p}/fill_style"));
        }
        crate::FillStyle::Picture { src, .. } => {
            if src.trim().is_empty() {
                cx.warn(
                    format!("{p}/fill_style/src"),
                    "picture fill has no image; the solid fill is drawn instead",
                );
            }
        }
    }
}

fn check_gradient_stops(cx: &mut Ctx<'_>, stops: &[crate::GradientStop], p: &str) {
    if !(2..=crate::MAX_GRADIENT_STOPS).contains(&stops.len()) {
        cx.err(
            format!("{p}/stops"),
            format!(
                "a gradient needs 2 to {} stops, found {}",
                crate::MAX_GRADIENT_STOPS,
                stops.len()
            ),
        );
    }
    for (i, stop) in stops.iter().enumerate() {
        if !stop.color.is_valid() {
            cx.err(
                format!("{p}/stops/{i}/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", stop.color.0),
            );
        }
        if !stop.position.is_finite() || !(0.0..=1.0).contains(&stop.position) {
            cx.err(
                format!("{p}/stops/{i}/position"),
                "gradient stop position must be finite and within 0..=1",
            );
        }
    }
}

fn check_shape_outline_and_text(
    cx: &mut Ctx<'_>,
    shape: crate::ShapeKind,
    outline: Option<&crate::Outline>,
    text: Option<&crate::ShapeText>,
    p: &str,
) {
    if let Some(outline) = outline {
        if !crate::shape_allows_text(shape) {
            cx.err(
                format!("{p}/kind/outline"),
                "line shapes cannot carry an outline",
            );
        }
        if !outline.color.is_valid() {
            cx.err(
                format!("{p}/kind/outline/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", outline.color.0),
            );
        }
        if !outline.width.is_finite() || !(0.0..=50.0).contains(&outline.width) {
            cx.err(
                format!("{p}/kind/outline/width"),
                "outline width must be finite and within 0..=50",
            );
        }
    }
    if let Some(text) = text {
        if !crate::shape_allows_text(shape) {
            cx.err(format!("{p}/kind/text"), "line shapes cannot carry text");
        }
        if !text.style.color.is_valid() {
            cx.err(
                format!("{p}/kind/text/style/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", text.style.color.0),
            );
        }
        check_font_style(cx, &text.style, &format!("{p}/kind/text/style"));
        check_text_runs(cx, &text.text, &text.runs, &format!("{p}/kind/text"));
        check_paragraphs(cx, &text.paragraphs, &format!("{p}/kind/text"));
    }
}

fn check_path(cx: &mut Ctx<'_>, path: &crate::Path, p: &str) {
    let count = path.points.len();
    if count < crate::PATH_MIN_POINTS {
        cx.err(
            format!("{p}/kind/path/points"),
            format!("a path needs at least {} points", crate::PATH_MIN_POINTS),
        );
    }
    if count > crate::PATH_MAX_POINTS {
        cx.err(
            format!("{p}/kind/path/points"),
            format!("a path may have at most {} points", crate::PATH_MAX_POINTS),
        );
    }
    for (i, point) in path.points.iter().enumerate() {
        if !point.is_valid() {
            cx.err(
                format!("{p}/kind/path/points/{i}"),
                "path points must be finite fractions within 0..=1",
            );
        }
    }
    if path.fill.is_some() && !path.closed {
        cx.err(
            format!("{p}/kind/path/fill"),
            "a path may only have a fill when it is closed",
        );
    }
    if let Some(fill) = &path.fill
        && !fill.is_valid()
    {
        cx.err(
            format!("{p}/kind/path/fill"),
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
        );
    }
    if !path.outline.color.is_valid() {
        cx.err(
            format!("{p}/kind/path/outline/color"),
            format!(
                "color {:?} is not #RRGGBB or #RRGGBBAA",
                path.outline.color.0
            ),
        );
    }
    if !path.outline.width.is_finite() || !(0.0..=50.0).contains(&path.outline.width) {
        cx.err(
            format!("{p}/kind/path/outline/width"),
            "outline width must be finite and within 0..=50",
        );
    }
    for (end, glue) in [
        ("start_glue", &path.start_glue),
        ("end_glue", &path.end_glue),
    ] {
        if let Some(glue) = glue
            && glue.object.is_empty()
        {
            cx.err(
                format!("{p}/kind/path/{end}/object"),
                "glue needs an object id",
            );
        }
    }
}

fn check_table(
    cx: &mut Ctx<'_>,
    columns: &[f32],
    rows: &[crate::TableRow],
    borders: &crate::TableBorders,
    p: &str,
) {
    if columns.is_empty() || columns.len() > crate::TABLE_MAX_AXIS {
        cx.err(
            format!("{p}/kind/columns"),
            format!("a table needs 1..={} columns", crate::TABLE_MAX_AXIS),
        );
    }
    if rows.is_empty() || rows.len() > crate::TABLE_MAX_AXIS {
        cx.err(
            format!("{p}/kind/rows"),
            format!("a table needs 1..={} rows", crate::TABLE_MAX_AXIS),
        );
    }
    if rows.len().saturating_mul(columns.len()) > crate::TABLE_MAX_CELLS {
        cx.err(
            format!("{p}/kind"),
            format!("a table may have at most {} cells", crate::TABLE_MAX_CELLS),
        );
    }
    let sum = |values: &[f32]| -> f32 { values.iter().copied().sum() };
    if !columns
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
        || (sum(columns) - 1.0).abs() > 0.01
    {
        cx.err(
            format!("{p}/kind/columns"),
            "column widths must be finite fractions summing to 1",
        );
    }
    for (index, row) in rows.iter().enumerate() {
        if !row.height.is_finite() || row.height <= 0.0 {
            cx.err(
                format!("{p}/kind/rows/{index}/height"),
                "row height must be a finite fraction above 0",
            );
        }
        if row.cells.len() != columns.len() {
            cx.err(
                format!("{p}/kind/rows/{index}/cells"),
                format!(
                    "row {index} has {} cells but the table has {} columns",
                    row.cells.len(),
                    columns.len()
                ),
            );
        }
        for (column, cell) in row.cells.iter().enumerate() {
            if cell.row_span == 0 || cell.col_span == 0 {
                cx.err(
                    format!("{p}/kind/rows/{index}/cells/{column}"),
                    "cell spans must be at least 1",
                );
            }
            if index.saturating_add(usize::from(cell.row_span)) > rows.len() {
                cx.err(
                    format!("{p}/kind/rows/{index}/cells/{column}/row_span"),
                    "row span runs past the last row",
                );
            }
            if column.saturating_add(usize::from(cell.col_span)) > columns.len() {
                cx.err(
                    format!("{p}/kind/rows/{index}/cells/{column}/col_span"),
                    "column span runs past the last column",
                );
            }
            if let Some(fill) = &cell.fill
                && !fill.is_valid()
            {
                cx.err(
                    format!("{p}/kind/rows/{index}/cells/{column}/fill"),
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
                );
            }
            if !cell.style.color.is_valid() {
                cx.err(
                    format!("{p}/kind/rows/{index}/cells/{column}/style/color"),
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", cell.style.color.0),
                );
            }
            check_font_style(
                cx,
                &cell.style,
                &format!("{p}/kind/rows/{index}/cells/{column}/style"),
            );
            check_text_runs(
                cx,
                &cell.text,
                &cell.runs,
                &format!("{p}/kind/rows/{index}/cells/{column}"),
            );
            check_paragraphs(
                cx,
                &cell.paragraphs,
                &format!("{p}/kind/rows/{index}/cells/{column}"),
            );
        }
    }
    if row_heights_sum(rows) && (rows.iter().map(|row| row.height).sum::<f32>() - 1.0).abs() > 0.01
    {
        cx.err(
            format!("{p}/kind/rows"),
            "row heights must be finite fractions summing to 1",
        );
    }
    for (name, outline) in [("outer", &borders.outer), ("inner", &borders.inner)] {
        let Some(outline) = outline else {
            continue;
        };
        if !outline.color.is_valid() {
            cx.err(
                format!("{p}/kind/borders/{name}/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", outline.color.0),
            );
        }
        if !outline.width.is_finite() || !(0.0..=50.0).contains(&outline.width) {
            cx.err(
                format!("{p}/kind/borders/{name}/width"),
                "border width must be finite and within 0..=50",
            );
        }
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

fn check_object_effects(cx: &mut Ctx<'_>, o: &Object, p: &str) {
    if !o.opacity.is_finite() || !(0.0..=1.0).contains(&o.opacity) {
        cx.err(
            format!("{p}/opacity"),
            "opacity must be a finite number in 0..=1",
        );
    }
    if let Some(shadow) = &o.shadow {
        if !shadow.color.is_valid() {
            cx.err(
                format!("{p}/shadow/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", shadow.color.0),
            );
        }
        if ![shadow.offset_x, shadow.offset_y]
            .iter()
            .all(|v| v.is_finite())
            || !(-200.0..=200.0).contains(&shadow.offset_x)
            || !(-200.0..=200.0).contains(&shadow.offset_y)
        {
            cx.err(
                format!("{p}/shadow"),
                "shadow offsets must be finite and within -200..=200",
            );
        }
        if !shadow.blur.is_finite() || !(0.0..=100.0).contains(&shadow.blur) {
            cx.err(
                format!("{p}/shadow/blur"),
                "shadow blur must be finite and within 0..=100",
            );
        }
    }
    if let Some(glow) = &o.glow {
        if !glow.color.is_valid() {
            cx.err(
                format!("{p}/glow/color"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", glow.color.0),
            );
        }
        if !glow.size.is_finite() || !(0.0..=100.0).contains(&glow.size) {
            cx.err(
                format!("{p}/glow/size"),
                "glow size must be finite and within 0..=100",
            );
        }
    }
}

fn check_input_variable(cx: &mut Ctx<'_>, variable: &str, kind: ValueKind, p: &str) {
    match cx.course.variable(variable) {
        None => cx.err(
            format!("{p}/kind/variable"),
            format!("input variable {variable:?} does not exist"),
        ),
        Some(def) if def.value.kind() != kind => cx.err(
            format!("{p}/kind/variable"),
            format!(
                "input variable {variable:?} is {:?}, but this input needs {kind:?}",
                def.value.kind()
            ),
        ),
        Some(_) => {}
    }
}

fn check_input_object(cx: &mut Ctx<'_>, kind: &ObjectKind, p: &str) {
    match kind {
        ObjectKind::TextEntry {
            variable,
            max_chars,
            ..
        } => {
            check_input_variable(cx, variable, ValueKind::Text, p);
            if *max_chars > crate::MAX_INPUT_CHARS {
                cx.err(
                    format!("{p}/kind/max_chars"),
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
            check_input_variable(cx, variable, ValueKind::Number, p);
            if !min.is_finite() || !max.is_finite() || min >= max {
                cx.err(
                    format!("{p}/kind"),
                    "slider/dial min and max must be finite and min must be less than max",
                );
            }
            if !step.is_finite() || *step < 0.0 {
                cx.err(
                    format!("{p}/kind/step"),
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
                cx.err(
                    format!("{p}/kind"),
                    "dial angles must be finite, within -360..=360, and start_angle must be less than end_angle",
                );
            }
        }
        ObjectKind::ProgressMeter {
            variable, min, max, ..
        } => {
            check_input_variable(cx, variable, ValueKind::Number, p);
            if !min.is_finite() || !max.is_finite() || min >= max {
                cx.err(
                    format!("{p}/kind"),
                    "progress meter min and max must be finite and min must be less than max",
                );
            }
        }
        ObjectKind::Checkbox { variable, .. } => {
            check_input_variable(cx, variable, ValueKind::Bool, p);
        }
        ObjectKind::RadioGroup { variable, options } => {
            check_input_variable(cx, variable, ValueKind::Text, p);
            if !(crate::MIN_RADIO_OPTIONS..=crate::MAX_RADIO_OPTIONS).contains(&options.len()) {
                cx.err(
                    format!("{p}/kind/options"),
                    format!(
                        "a radio group needs {} to {} options",
                        crate::MIN_RADIO_OPTIONS,
                        crate::MAX_RADIO_OPTIONS
                    ),
                );
            }
            let mut values = BTreeSet::new();
            for (i, option) in options.iter().enumerate() {
                if option.value.trim().is_empty() {
                    cx.err(
                        format!("{p}/kind/options/{i}/value"),
                        "radio option values must not be empty",
                    );
                } else if !values.insert(option.value.as_str()) {
                    cx.err(
                        format!("{p}/kind/options/{i}/value"),
                        format!("duplicate radio option value {:?}", option.value),
                    );
                }
            }
        }
        _ => {}
    }
}

fn check_timer_object(cx: &mut Ctx<'_>, duration_ms: u32, style: &crate::TextStyle, p: &str) {
    if !(1..=crate::MAX_TIMER_DURATION_MS).contains(&duration_ms) {
        cx.err(
            format!("{p}/kind/duration_ms"),
            format!(
                "timer duration_ms must be 1..={}",
                crate::MAX_TIMER_DURATION_MS
            ),
        );
    }
    if !style.color.is_valid() {
        cx.err(
            format!("{p}/kind/style/color"),
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
        );
    }
    if !style.size.is_finite() || style.size <= 0.0 || style.size > 400.0 {
        cx.err(
            format!("{p}/kind/style/size"),
            "timer text size must be finite and within 0..=400",
        );
    }
    check_font_style(cx, style, &format!("{p}/kind/style"));
}

fn check_marker_object(
    cx: &mut Ctx<'_>,
    body: &str,
    runs: &[crate::TextRun],
    paragraphs: &[crate::Paragraph],
    color: Option<&crate::Color>,
    p: &str,
) {
    if let Some(color) = color
        && !color.is_valid()
    {
        cx.err(
            format!("{p}/kind/color"),
            format!("color {:?} is not #RRGGBB or #RRGGBBAA", color.0),
        );
    }
    check_text_runs(cx, body, runs, &format!("{p}/kind"));
    check_paragraphs(cx, paragraphs, &format!("{p}/kind"));
}

fn check_scroll_panel_object(cx: &mut Ctx<'_>, content_height: f32, frame_height: f32, p: &str) {
    if !content_height.is_finite()
        || content_height < 0.0
        || content_height > crate::MAX_SCROLL_CONTENT_HEIGHT
    {
        cx.err(
            format!("{p}/kind/content_height"),
            format!(
                "scroll panel content_height must be finite and within 0..={}",
                crate::MAX_SCROLL_CONTENT_HEIGHT
            ),
        );
    } else if content_height + 0.5 < frame_height {
        cx.err(
            format!("{p}/kind/content_height"),
            "scroll panel content_height must be at least the panel's frame height",
        );
    }
}

fn check_web_source(cx: &mut Ctx<'_>, source: &crate::WebSource, p: &str) {
    match source {
        crate::WebSource::Url { url } => {
            if !crate::is_valid_web_url(url) {
                cx.err(
                    format!("{p}/kind/source/url"),
                    format!("web URL {url:?} must start with https:// and name a host"),
                );
            }
        }
        crate::WebSource::Folder { path, entry } => {
            if !crate::is_valid_web_folder_path(path) {
                cx.err(
                    format!("{p}/kind/source/path"),
                    format!(
                        "web folder path {path:?} must be a safe relative path under media/web/"
                    ),
                );
            }
            if !crate::is_valid_web_entry(entry) {
                cx.err(
                    format!("{p}/kind/source/entry"),
                    format!(
                        "web folder entry {entry:?} must be a safe relative file inside the folder"
                    ),
                );
            }
        }
    }
}

fn check_captions(cx: &mut Ctx<'_>, captions: Option<&str>, object_id: &str, p: &str) {
    let Some(path) = captions else {
        return;
    };
    if path.trim().is_empty() {
        cx.err(
            format!("{p}/kind/captions"),
            format!("captions path for object {object_id:?} is empty"),
        );
    } else if !is_safe_relative_path(path) {
        cx.err(
            format!("{p}/kind/captions"),
            format!(
                "captions path {path:?} for object {object_id:?} must be a safe relative path under media/"
            ),
        );
    } else if !path.to_ascii_lowercase().ends_with(".vtt") {
        cx.err(
            format!("{p}/kind/captions"),
            format!("captions path {path:?} for object {object_id:?} must end in .vtt"),
        );
    }
}

fn check_media_options(cx: &mut Ctx<'_>, volume: f32, trim: Option<&crate::Trim>, p: &str) {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        cx.err(
            format!("{p}/kind/volume"),
            "volume must be finite and within 0.0..=1.0",
        );
    }
    let Some(trim) = trim else {
        return;
    };
    if trim.start_ms > crate::MAX_TRIM_MS {
        cx.err(
            format!("{p}/kind/trim/start_ms"),
            format!("trim start must be at most {} ms", crate::MAX_TRIM_MS),
        );
    }
    if let Some(end_ms) = trim.end_ms {
        if end_ms > crate::MAX_TRIM_MS {
            cx.err(
                format!("{p}/kind/trim/end_ms"),
                format!("trim end must be at most {} ms", crate::MAX_TRIM_MS),
            );
        }
        if end_ms <= trim.start_ms {
            cx.err(
                format!("{p}/kind/trim/end_ms"),
                "trim end must be after its start",
            );
        }
    }
}

fn check_cues(cx: &mut Ctx<'_>, cues: &[crate::Cue], timeline_ms: u32, p: &str) {
    if cues.len() > crate::MAX_CUES {
        cx.err(
            format!("{p}/kind/cues"),
            format!("at most {} cues are allowed", crate::MAX_CUES),
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, cue) in cues.iter().enumerate() {
        let q = format!("{p}/kind/cues/{i}");
        if cue.id.is_empty() {
            cx.err(format!("{q}/id"), "cue id is empty");
        } else if !seen.insert(cue.id.as_str()) {
            cx.err(format!("{q}/id"), format!("duplicate cue id {:?}", cue.id));
        }
        if cue.name.trim().is_empty() {
            cx.err(format!("{q}/name"), "cue name is empty");
        }
        if !cue.at_ms.is_finite() || cue.at_ms < 0.0 {
            cx.err(
                format!("{q}/at_ms"),
                "cue at_ms must be finite and at least 0",
            );
        } else if cue.at_ms > f64::from(timeline_ms) {
            cx.err(
                format!("{q}/at_ms"),
                format!(
                    "cue at {} ms is past the video's known duration of {timeline_ms} ms",
                    cue.at_ms
                ),
            );
        }
    }
}

fn check_object(cx: &mut Ctx<'_>, o: &Object, timeline_ms: u32, p: &str) {
    let f = o.frame;
    if ![f.x, f.y, f.w, f.h].iter().all(|v| v.is_finite()) || f.w < 0.0 || f.h < 0.0 {
        cx.err(
            format!("{p}/frame"),
            "frame values must be finite and w/h non-negative",
        );
    }
    if !o.rotation.is_finite() || !(0.0..360.0).contains(&o.rotation) {
        cx.err(
            format!("{p}/rotation"),
            "rotation must be a finite number of degrees in 0..360",
        );
    }
    check_object_effects(cx, o, p);
    if let Some(end) = o.timing.end_ms
        && end <= o.timing.start_ms
    {
        cx.err(format!("{p}/timing"), "end_ms must be after start_ms");
    }
    if o.timing.start_ms > timeline_ms {
        cx.warn(
            format!("{p}/timing"),
            "object starts after its timeline ends and will never appear",
        );
    }
    match &o.kind {
        ObjectKind::Shape { fill, .. } | ObjectKind::Button { fill, .. } if !fill.is_valid() => {
            cx.err(
                format!("{p}/kind/fill"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", fill.0),
            );
        }
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            text_style,
            ..
        } => {
            if let Some(id) = text_style
                && !cx.course.text_styles.iter().any(|named| &named.id == id)
            {
                cx.warn(
                    format!("{p}/kind/text_style"),
                    format!("text style {id:?} does not exist; the box keeps its own look"),
                );
            }
            if !style.color.is_valid() {
                cx.err(
                    format!("{p}/kind/style/color"),
                    format!("color {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
                );
            }
            check_font_style(cx, style, &format!("{p}/kind/style"));
            check_text_runs(cx, text, runs, &format!("{p}/kind"));
            check_paragraphs(cx, paragraphs, &format!("{p}/kind"));
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
                check_fill_style(cx, fill_style, p);
            }
            check_shape_outline_and_text(cx, *shape, outline.as_ref(), text.as_ref(), p);
        }
        ObjectKind::Image { src, crop, .. } => {
            if let Some(crop) = crop
                && let Some(message) = check_crop_values(*crop)
            {
                cx.err(format!("{p}/kind/crop"), message);
            }
            if src.trim().is_empty() {
                cx.err(format!("{p}/kind/src"), "media src is empty");
            }
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
                cx.err(format!("{p}/kind/src"), "media src is empty");
            }
            check_cues(cx, cues, timeline_ms, p);
            check_captions(cx, captions.as_deref(), &o.id, p);
            check_media_options(cx, *volume, trim.as_ref(), p);
        }
        ObjectKind::Lottie { src, speed, .. } => {
            if src.trim().is_empty() {
                cx.err(format!("{p}/kind/src"), "media src is empty");
            }
            if !speed.is_finite()
                || !(crate::LOTTIE_MIN_SPEED..=crate::LOTTIE_MAX_SPEED).contains(speed)
            {
                cx.err(
                    format!("{p}/kind/speed"),
                    "Lottie speed must be finite and within 0.1..=4",
                );
            }
        }
        ObjectKind::Web { source } => check_web_source(cx, source, p),
        ObjectKind::TextEntry { .. }
        | ObjectKind::Slider { .. }
        | ObjectKind::Dial { .. }
        | ObjectKind::Checkbox { .. }
        | ObjectKind::RadioGroup { .. }
        | ObjectKind::ProgressMeter { .. } => check_input_object(cx, &o.kind, p),
        ObjectKind::Timer {
            duration_ms, style, ..
        } => check_timer_object(cx, *duration_ms, style, p),
        ObjectKind::Marker {
            body,
            runs,
            paragraphs,
            color,
            ..
        } => check_marker_object(cx, body, runs, paragraphs, color.as_ref(), p),
        ObjectKind::ScrollPanel { content_height, .. } => {
            check_scroll_panel_object(cx, *content_height, f.h, p)
        }
        ObjectKind::Path { path } => {
            check_path(cx, path, p);
        }
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row: _,
        } => {
            check_table(cx, columns, rows, borders, p);
        }
        ObjectKind::Button { fill_style, .. } => {
            if let Some(fill_style) = fill_style {
                check_fill_style(cx, fill_style, p);
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
                cx.err(format!("{p}/kind/src"), "media src is empty");
            }
            check_captions(cx, captions.as_deref(), &o.id, p);
            check_media_options(cx, *volume, trim.as_ref(), p);
        }
    }
    let mut seen = BTreeSet::new();
    for (i, s) in o.states.iter().enumerate() {
        if s.name == "normal" {
            cx.err(
                format!("{p}/states/{i}"),
                "`normal` is the object's own appearance; don't redefine it",
            );
        }
        if !seen.insert(s.name.as_str()) {
            cx.err(
                format!("{p}/states/{i}"),
                format!("duplicate state {:?}", s.name),
            );
        }
        if let Some(c) = &s.fill
            && !c.is_valid()
        {
            cx.err(
                format!("{p}/states/{i}/fill"),
                format!("color {:?} is not #RRGGBB or #RRGGBBAA", c.0),
            );
        }
    }
    if !o.has_state(&o.initial_state) {
        cx.err(
            format!("{p}/initial_state"),
            format!("state {:?} is not defined on this object", o.initial_state),
        );
    }
    check_object_animations(cx, o, p);
    check_object_motion_paths(cx, o, p);
}

fn check_object_animations(cx: &mut Ctx<'_>, o: &Object, p: &str) {
    if let Some(ref anim) = o.entrance
        && (anim.duration_ms == 0 || anim.duration_ms > 60000)
    {
        cx.err(
            format!("{p}/entrance/duration_ms"),
            "animation duration_ms must be 1..=60000",
        );
    }
    if let Some(ref anim) = o.exit
        && (anim.duration_ms == 0 || anim.duration_ms > 60000)
    {
        cx.err(
            format!("{p}/exit/duration_ms"),
            "animation duration_ms must be 1..=60000",
        );
    }
}

/// The same rule package paths use: no absolute paths, drive letters, URL
/// schemes, backslashes, control characters, or `.`/`..` segments.
fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
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

fn check_object_motion_paths(cx: &mut Ctx<'_>, o: &Object, p: &str) {
    if o.motion_paths.len() > 20 {
        cx.err(
            format!("{p}/motion_paths"),
            "object has more than 20 motion paths",
        );
    }
    let mut path_ids = BTreeSet::new();
    for (i, path) in o.motion_paths.iter().enumerate() {
        let pp = format!("{p}/motion_paths/{i}");
        if !valid_name(&path.id) {
            cx.err(
                format!("{pp}/id"),
                format!("path id {:?} must be 1-128 chars of [A-Za-z0-9_-]", path.id),
            );
        }
        if !path_ids.insert(path.id.as_str()) {
            cx.err(
                format!("{pp}/id"),
                format!(
                    "duplicate motion path id {:?} on object {:?}",
                    path.id, o.id
                ),
            );
        }
        if path.duration_ms == 0 || path.duration_ms > 60000 {
            cx.err(
                format!("{pp}/duration_ms"),
                "motion path duration_ms must be 1..=60000",
            );
        }
        if path.points.len() < 2 || path.points.len() > 100 {
            cx.err(
                format!("{pp}/points"),
                "motion path must have 2..=100 points",
            );
        }
        for (pi, pt) in path.points.iter().enumerate() {
            if !pt.x.is_finite()
                || !pt.y.is_finite()
                || pt.x < -100_000.0
                || pt.x > 100_000.0
                || pt.y < -100_000.0
                || pt.y > 100_000.0
            {
                cx.err(
                    format!("{pp}/points/{pi}"),
                    "path coordinates must be finite and within -100000..=100000",
                );
            }
        }
        if path.relative
            && let Some(first) = path.points.first()
            && (first.x != 0.0 || first.y != 0.0)
            && first.x.is_finite()
            && first.y.is_finite()
        {
            cx.warn(
                format!("{pp}/points/0"),
                "relative motion path start point should be {0,0}",
            );
        }
    }
}

fn check_object_ref(cx: &mut Ctx<'_>, slide: &Slide, id: &str, p: &str) -> Option<()> {
    if slide.object(id).is_none() {
        cx.err(p, format!("object {id:?} is not on slide {:?}", slide.id));
        return None;
    }
    Some(())
}

fn check_layer_ref(cx: &mut Ctx<'_>, slide: &Slide, id: &str, p: &str) {
    if slide.layer(id).is_none() {
        cx.err(p, format!("layer {id:?} is not on slide {:?}", slide.id));
    }
}

fn check_trigger(cx: &mut Ctx<'_>, slide: &Slide, owner: Option<&Layer>, t: &Trigger, p: &str) {
    check_event(cx, slide, &t.when, &format!("{p}/when"));
    check_action(cx, slide, owner, &t.action, &format!("{p}/action"));
    if let Some(c) = &t.condition {
        check_condition(cx, slide, c, &format!("{p}/condition"), 1);
    }
}

fn check_event(cx: &mut Ctx<'_>, slide: &Slide, e: &Event, p: &str) {
    match e {
        Event::TimelineStarts { layer } | Event::TimelineEnds { layer } => {
            if let Some(l) = layer {
                check_layer_ref(cx, slide, l, p);
            }
        }
        Event::TimelineReaches { ms, layer } => {
            let len = match layer {
                Some(l) => slide.layer(l).map(|l| l.duration_ms),
                None => Some(slide.duration_ms),
            };
            match len {
                None => {
                    if let Some(l) = layer {
                        check_layer_ref(cx, slide, l, p);
                    }
                }
                Some(len) if *ms > len => cx.warn(p, format!("timeline_reaches {ms} ms is past the timeline end ({len} ms) and never fires")),
                Some(_) => {}
            }
        }
        Event::MediaEnds { object } => {
            if object == crate::BACKGROUND_AUDIO_ID {
                if cx.course.background_audio.is_none() {
                    cx.err(p, "the course has no background audio");
                }
            } else {
                let _ = check_object_ref(cx, slide, object, p);
            }
        }
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object } => {
            let _ = check_object_ref(cx, slide, object, p);
        }
        Event::CueReached { object, cue } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let Some((o, _)) = slide.object(object)
            {
                match &o.kind {
                    ObjectKind::Video { cues, .. } => {
                        if !cues.iter().any(|c| c.id == *cue) {
                            cx.err(
                                p,
                                format!("cue {cue:?} is not defined on video object {object:?}"),
                            );
                        }
                    }
                    _ => cx.err(p, format!("object {object:?} is not a video")),
                }
            }
        }
        Event::StateChanges { object, to } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let (Some(to), Some((o, _))) = (to, slide.object(object))
                && !o.has_state(to)
                && !crate::BUILTIN_STATES.contains(&to.as_str())
            {
                cx.err(
                    p,
                    format!("state {to:?} is not defined on object {object:?}"),
                );
            }
        }
        Event::VariableChanges { variable } => {
            if crate::is_timer_remaining_variable_on(slide, variable) {
                // `Timer.<id>.Remaining` fires once per whole-second change.
            } else if crate::builtin_variable_kind(variable).is_some() {
                cx.err(p, "built-in variables do not fire variable_changes");
            } else if cx.course.variable(variable).is_none() {
                cx.err(p, format!("variable {variable:?} does not exist"));
            }
        }
        Event::TimerEnds { timer } => {
            if check_object_ref(cx, slide, timer, p).is_some()
                && let Some((o, _)) = slide.object(timer)
                && !matches!(o.kind, ObjectKind::Timer { .. })
            {
                cx.err(p, format!("object {timer:?} is not a timer"));
            }
        }
        Event::KeyPressed { key, object } => {
            if !crate::is_valid_trigger_key(key) {
                cx.err(
                    p,
                    format!("invalid key {key:?}; must be a-z, 0-9, arrow_up, arrow_down, arrow_left, arrow_right, enter, space, or escape"),
                );
            }
            if let Some(obj) = object {
                let _ = check_object_ref(cx, slide, obj, p);
            }
        }
        Event::DroppedOn { object, target } => {
            let _ = check_object_ref(cx, slide, object, p);
            let _ = check_object_ref(cx, slide, target, p);
        }
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            let _ = check_object_ref(cx, slide, a, p);
            let _ = check_object_ref(cx, slide, b, p);
        }
        Event::Submit => {
            if slide.question.is_none() {
                cx.warn(
                    p,
                    "submit trigger on a slide without a question never fires",
                );
            }
        }
        Event::AnimationCompletes { object, path } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let (Some(path_id), Some((obj, _))) = (path, slide.object(object))
                && !obj.motion_paths.iter().any(|mp| mp.id == *path_id)
            {
                cx.err(
                    p,
                    format!("motion path {path_id:?} is not defined on object {object:?}"),
                );
            }
        }
    }
}

fn check_action(cx: &mut Ctx<'_>, slide: &Slide, owner: Option<&Layer>, a: &Action, p: &str) {
    match a {
        Action::JumpToSlide {
            target: SlideTarget::Id(id),
        } => {
            if cx.course.play_slide(id).is_none() {
                cx.err(p, format!("slide {id:?} does not exist"));
            }
        }
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
                cx.err(p, "lightbox_slide cannot open the slide it is already on");
            } else if cx.course.play_slide(target).is_none() {
                cx.err(p, format!("slide {target:?} does not exist"));
            }
        }
        Action::ShowLayer { layer } => check_layer_ref(cx, slide, layer, p),
        Action::HideLayer { layer: Some(l) } => check_layer_ref(cx, slide, l, p),
        Action::HideLayer { layer: None } => {
            if owner.is_none() {
                cx.err(p, "hide_layer without `layer` hides the trigger's own layer, but this trigger is on the base layer");
            }
        }
        Action::ChangeState { object, state } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let Some((o, _)) = slide.object(object)
                && !o.has_state(state)
            {
                cx.err(
                    p,
                    format!("state {state:?} is not defined on object {object:?}"),
                );
            }
        }
        Action::SetVariable {
            variable,
            op,
            value,
        } => check_set_variable(cx, variable, *op, value.as_ref(), p),
        Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object } => {
            if object == crate::BACKGROUND_AUDIO_ID {
                if cx.course.background_audio.is_none() {
                    cx.err(p, "the course has no background audio");
                }
            } else if check_object_ref(cx, slide, object, p).is_some()
                && let Some((o, _)) = slide.object(object)
                && !matches!(
                    o.kind,
                    ObjectKind::Audio { .. } | ObjectKind::Video { .. } | ObjectKind::Lottie { .. }
                )
            {
                cx.err(p, format!("object {object:?} is not a media object"));
            }
        }
        Action::StartTimer { timer }
        | Action::PauseTimer { timer }
        | Action::ResetTimer { timer } => {
            if check_object_ref(cx, slide, timer, p).is_some()
                && let Some((o, _)) = slide.object(timer)
                && !matches!(o.kind, ObjectKind::Timer { .. })
            {
                cx.err(p, format!("object {timer:?} is not a timer"));
            }
        }
        Action::SubmitQuestion => {
            if slide.question.is_none() {
                cx.err(p, "submit_question on a slide without a question");
            }
        }
        Action::OpenUrl { url } => {
            if !(url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("mailto:"))
            {
                cx.err(p, "open_url only allows http(s):// and mailto: links");
            }
        }
        Action::RunJavaScript { code } => {
            if code.len() > crate::MAX_JAVASCRIPT_BYTES {
                cx.err(
                    p,
                    format!(
                        "run_javascript code must be at most {} bytes",
                        crate::MAX_JAVASCRIPT_BYTES
                    ),
                );
            }
            cx.warn(
                p,
                "Uses JavaScript, which runs only in the published course",
            );
        }
        Action::MoveOnPath { object, path } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let Some((obj, _)) = slide.object(object)
                && !obj.motion_paths.iter().any(|mp| mp.id == *path)
            {
                cx.err(
                    p,
                    format!("motion path {path:?} is not defined on object {object:?}"),
                );
            }
        }
    }
}

fn check_set_variable(
    cx: &mut Ctx<'_>,
    variable: &str,
    op: VarOp,
    value: Option<&crate::Value>,
    p: &str,
) {
    if crate::builtin_variable_kind(variable).is_some()
        || crate::timer_id_for_remaining_variable(variable).is_some()
    {
        cx.err(p, "read-only built-in variable");
        return;
    }
    let Some(def) = cx.course.variable(variable) else {
        cx.err(p, format!("variable {variable:?} does not exist"));
        return;
    };
    let kind = def.value.kind();
    match op {
        VarOp::Toggle => {
            if kind != ValueKind::Bool {
                cx.err(
                    p,
                    format!("toggle needs a true/false variable; {variable:?} is {kind:?}"),
                );
            }
        }
        VarOp::Assign => match value {
            None => cx.err(p, "assign needs a value"),
            Some(v) if v.kind() != kind => cx.err(
                p,
                format!("{variable:?} is {kind:?}, value is {:?}", v.kind()),
            ),
            Some(_) => {}
        },
        VarOp::Add | VarOp::Subtract | VarOp::Multiply | VarOp::Divide => {
            if kind != ValueKind::Number {
                cx.err(
                    p,
                    format!("{op:?} needs a number variable; {variable:?} is {kind:?}"),
                );
            }
            match value {
                Some(crate::Value::Number(n)) if n.is_finite() => {
                    if op == VarOp::Divide && *n == 0.0 {
                        cx.err(p, "divide by zero");
                    }
                }
                _ => cx.err(p, format!("{op:?} needs a finite number value")),
            }
        }
    }
}

/// Resources and glossary entries: unique valid ids, non-empty text, safe targets.
fn check_resources_and_glossary(cx: &mut Ctx<'_>) {
    let course = cx.course;
    let mut ids = BTreeSet::new();
    for (i, resource) in course.resources.iter().enumerate() {
        let p = format!("/resources/{i}");
        if !valid_name(&resource.id) || !ids.insert(resource.id.as_str()) {
            cx.err(
                &p,
                format!("resource id {:?} is invalid or duplicated", resource.id),
            );
        }
        if resource.title.trim().is_empty() {
            cx.err(&p, "resource title is empty");
        }
        match &resource.target {
            crate::ResourceTarget::Url { url } => {
                if !crate::is_valid_link_url(url) {
                    cx.err(
                        format!("{p}/target"),
                        "resource URL must be http, https or mailto",
                    );
                }
            }
            crate::ResourceTarget::File { src } => {
                if !is_safe_relative_path(src.trim()) {
                    cx.err(
                        format!("{p}/target"),
                        format!("resource file {src:?} must be a safe relative path"),
                    );
                }
            }
        }
    }
    let mut ids = BTreeSet::new();
    for (i, entry) in course.glossary.iter().enumerate() {
        let p = format!("/glossary/{i}");
        if !valid_name(&entry.id) || !ids.insert(entry.id.as_str()) {
            cx.err(
                &p,
                format!("glossary id {:?} is invalid or duplicated", entry.id),
            );
        }
        if entry.term.trim().is_empty() {
            cx.err(&p, "glossary term is empty");
        }
    }
}

/// The player logo must be a safe path to a picture.
fn check_logo(cx: &mut Ctx<'_>) {
    if let Some(logo) = &cx.course.player.logo
        && !crate::is_image_src(logo)
    {
        cx.err(
            "/player/logo",
            format!("player logo {logo:?} must be a safe relative path to an image"),
        );
    }
}

/// Every "Next unlocks" rule, course-wide and per slide.
fn check_next_rules(cx: &mut Ctx<'_>) {
    let course = cx.course;
    check_next_rule(cx, &course.player.next_requires, "/player/next_requires");
    for (si, scene) in course.scenes.iter().enumerate() {
        for (li, slide) in scene.slides.iter().enumerate() {
            if let Some(rule) = slide
                .player
                .as_ref()
                .and_then(|overrides| overrides.next_requires.as_ref())
            {
                let path = format!("/scenes/{si}/slides/{li}/player/next_requires");
                check_next_rule(cx, rule, &path);
            }
        }
    }
}

/// A "Next unlocks when a variable is true" rule needs a true/false variable.
fn check_next_rule(cx: &mut Ctx<'_>, rule: &crate::NextRequirement, path: &str) {
    let crate::NextRequirement::Variable { name } = rule else {
        return;
    };
    let kind = crate::builtin_variable_kind(name)
        .or_else(|| cx.course.variable(name).map(|def| def.value.kind()));
    match kind {
        None => cx.err(path, format!("variable {name:?} does not exist")),
        Some(ValueKind::Bool) => {}
        Some(other) => cx.err(
            path,
            format!("variable {name:?} is {other:?}, not true/false"),
        ),
    }
}

fn check_condition(cx: &mut Ctx<'_>, slide: &Slide, c: &Condition, p: &str, depth: usize) {
    if depth > MAX_CONDITION_DEPTH {
        cx.err(
            p,
            format!("condition nested deeper than {MAX_CONDITION_DEPTH}"),
        );
        return;
    }
    match c {
        Condition::All(list) | Condition::Any(list) => {
            if list.is_empty() {
                cx.warn(p, "empty condition group");
            }
            for (i, c) in list.iter().enumerate() {
                check_condition(cx, slide, c, &format!("{p}/{i}"), depth + 1);
            }
        }
        Condition::Not(inner) => check_condition(cx, slide, inner, &format!("{p}/not"), depth + 1),
        Condition::Compare {
            variable,
            op,
            value,
        } => {
            let kind = crate::builtin_variable_kind(variable)
                .or_else(|| {
                    if crate::is_timer_remaining_variable_on(slide, variable) {
                        Some(ValueKind::Number)
                    } else {
                        None
                    }
                })
                .or_else(|| cx.course.variable(variable).map(|def| def.value.kind()));
            match kind {
                None => cx.err(p, format!("variable {variable:?} does not exist")),
                Some(kind) => {
                    if value.kind() != kind {
                        cx.err(
                            p,
                            format!("{variable:?} is {kind:?}, compared with {:?}", value.kind()),
                        );
                    }
                    let ordered = matches!(
                        op,
                        crate::CmpOp::Lt | crate::CmpOp::Le | crate::CmpOp::Gt | crate::CmpOp::Ge
                    );
                    if ordered && kind != ValueKind::Number {
                        cx.err(p, format!("{op:?} only compares numbers"));
                    }
                    if *op == crate::CmpOp::Contains && kind != ValueKind::Text {
                        cx.err(p, "contains only applies to text");
                    }
                }
            }
        }
        Condition::ObjectState { object, state, .. } => {
            if check_object_ref(cx, slide, object, p).is_some()
                && let Some((o, _)) = slide.object(object)
                && !o.has_state(state)
            {
                cx.err(
                    p,
                    format!("state {state:?} is not defined on object {object:?}"),
                );
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

fn check_hotspot(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question, qp: &str) {
    if !q.choices.is_empty() {
        cx.err(qp, "hotspot must not have choices");
    }
    match q.hotspot_image.as_deref() {
        Some(id) => match base_object(slide, id) {
            Some(object) if matches!(object.kind, ObjectKind::Image { .. }) => {}
            Some(_) => cx.err(qp, format!("hotspot_image {id:?} is not an image object")),
            None => cx.err(
                qp,
                format!("hotspot_image {id:?} must name a base-layer image object"),
            ),
        },
        None => cx.err(qp, "hotspot_image must name a base-layer image object"),
    }
    let n = q.regions.len();
    if !(1..=20).contains(&n) {
        cx.err(qp, format!("hotspot needs 1 to 20 regions, found {n}"));
    }
    if !q.regions.iter().any(|r| r.correct) {
        cx.err(qp, "hotspot needs at least one correct region");
    }
    let mut ids = BTreeSet::new();
    for region in &q.regions {
        if !valid_name(&region.id) {
            cx.err(
                qp,
                format!(
                    "region id {:?} must be 1-128 chars of [A-Za-z0-9_-]",
                    region.id
                ),
            );
        }
        if !ids.insert(region.id.as_str()) {
            cx.err(qp, format!("duplicate region id {:?}", region.id));
        }
        if !coord_ok(region.x)
            || !coord_ok(region.y)
            || !extent_ok(region.w)
            || !extent_ok(region.h)
        {
            cx.err(
                qp,
                format!(
                    "region {:?} coordinates must be finite, x and y within ±100000, and w and h greater than 0 and at most 100000",
                    region.id
                ),
            );
        }
    }
    if !(1..=10).contains(&q.markers) {
        cx.err(qp, format!("markers must be 1 to 10, found {}", q.markers));
    }
}

fn check_drag_drop(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question, qp: &str) {
    if !q.choices.is_empty() {
        cx.err(qp, "drag_drop must not have choices");
    }
    let n = q.drag_items.len();
    if !(1..=20).contains(&n) {
        cx.err(qp, format!("drag_drop needs 1 to 20 drag items, found {n}"));
    }
    let mut items = BTreeSet::new();
    for drag in &q.drag_items {
        if !items.insert(drag.item.as_str()) {
            cx.err(qp, format!("duplicate drag item {:?}", drag.item));
        }
        if base_object(slide, &drag.item).is_none() {
            cx.err(
                qp,
                format!("drag item {:?} is not a base-layer object", drag.item),
            );
        }
        if base_object(slide, &drag.target).is_none() {
            cx.err(
                qp,
                format!("drag target {:?} is not a base-layer object", drag.target),
            );
        }
        if drag.item == drag.target || q.drag_items.iter().any(|other| other.item == drag.target) {
            cx.err(
                qp,
                format!(
                    "drag item {:?} cannot target itself or another drag item",
                    drag.item
                ),
            );
        }
        if !q.drop_targets.iter().any(|id| id == &drag.target) {
            cx.err(
                qp,
                format!(
                    "drag item {:?} target {:?} is not in drop_targets",
                    drag.item, drag.target
                ),
            );
        }
    }
    let t = q.drop_targets.len();
    if !(1..=20).contains(&t) {
        cx.err(qp, format!("drop_targets needs 1 to 20 ids, found {t}"));
    }
    let mut targets = BTreeSet::new();
    for id in &q.drop_targets {
        if !targets.insert(id.as_str()) {
            cx.err(qp, format!("duplicate drop target {id:?}"));
        }
        if base_object(slide, id).is_none() {
            cx.err(qp, format!("drop target {id:?} is not a base-layer object"));
        }
        if items.contains(id.as_str()) {
            cx.err(
                qp,
                format!("object {id:?} is both a drag item and a drop target"),
            );
        }
    }
}

fn check_scoring(cx: &mut Ctx<'_>, q: &crate::Question, qp: &str) {
    for (i, c) in q.choices.iter().enumerate() {
        if !(-1000..=1000).contains(&c.points) {
            cx.err(
                format!("{qp}/choices/{i}/points"),
                "choice points must be within -1000..=1000",
            );
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
                    qp,
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
                    qp,
                    "partial scoring is only allowed for multiple_response, sequence, matching, hotspot and drag_drop questions",
                );
            }
        }
        Scoring::AllOrNothing => {}
    }
}

fn check_likert(cx: &mut Ctx<'_>, q: &crate::Question, qp: &str) {
    let n = q.choices.len();
    if !(2..=11).contains(&n) {
        cx.err(qp, format!("likert needs 2 to 11 choices, found {n}"));
    }
    if q.choices.iter().any(|c| c.correct) {
        cx.err(qp, "likert choices must not be marked correct");
    }
}

fn check_text_answer(cx: &mut Ctx<'_>, q: &crate::Question, qp: &str) {
    if !q.choices.is_empty() {
        cx.err(qp, "short_answer and essay must not have choices");
    }
}

fn check_freeform(cx: &mut Ctx<'_>, slide: &Slide, q: &crate::Question, qp: &str) {
    match &q.correct_when {
        Some(condition) => check_condition(cx, slide, condition, &format!("{qp}/correct_when"), 1),
        None => cx.err(qp, "freeform needs a correct_when condition"),
    }
    if !q.choices.is_empty() {
        cx.err(qp, "freeform must not have choices");
    }
    if !q.accepted.is_empty() {
        cx.err(qp, "freeform must not have accepted answers");
    }
    if !q.pairs.is_empty() {
        cx.err(qp, "freeform must not have pairs");
    }
    if !q.regions.is_empty() {
        cx.err(qp, "freeform must not have regions");
    }
    if !q.drag_items.is_empty() {
        cx.err(qp, "freeform must not have drag items");
    }
    if !q.drop_targets.is_empty() {
        cx.err(qp, "freeform must not have drop targets");
    }
    if q.scoring != Scoring::AllOrNothing {
        cx.err(qp, "freeform scoring must be all_or_nothing");
    }
}

fn check_question(cx: &mut Ctx<'_>, slide: &Slide, p: &str) {
    let Some(q) = &slide.question else { return };
    let qp = format!("{p}/question");
    let mut ids = BTreeSet::new();
    for c in q.choices.iter() {
        if !ids.insert(c.id.as_str()) {
            cx.err(&qp, format!("duplicate choice id {:?}", c.id));
        }
    }
    check_scoring(cx, q, &qp);
    let correct = q.choices.iter().filter(|c| c.correct).count();
    match q.kind {
        QuestionKind::MultipleChoice | QuestionKind::MultipleResponse => {
            if q.choices.len() < 2 {
                cx.err(&qp, "a question needs at least 2 choices");
            }
            if q.kind == QuestionKind::MultipleChoice && correct != 1 {
                cx.err(
                    &qp,
                    format!("multiple_choice needs exactly 1 correct choice, found {correct}"),
                );
            }
            if q.kind == QuestionKind::MultipleResponse && correct == 0 {
                cx.err(&qp, "multiple_response needs at least 1 correct choice");
            }
        }
        QuestionKind::TrueFalse => {
            if q.choices.len() != 2 {
                cx.err(
                    &qp,
                    format!(
                        "true_false needs exactly 2 choices, found {}",
                        q.choices.len()
                    ),
                );
            }
            if correct != 1 {
                cx.err(
                    &qp,
                    format!("true_false needs exactly 1 correct choice, found {correct}"),
                );
            }
        }
        QuestionKind::FillIn => {
            if !q.choices.is_empty() {
                cx.err(&qp, "fill_in must not have choices");
            }
            let n = q.accepted.len();
            if !(1..=50).contains(&n) {
                cx.err(
                    &qp,
                    format!("fill_in needs 1 to 50 accepted answers, found {n}"),
                );
            }
            for (i, answer) in q.accepted.iter().enumerate() {
                let len = answer.trim().chars().count();
                if !(1..=500).contains(&len) {
                    cx.err(
                        format!("{qp}/accepted/{i}"),
                        "accepted answer must be 1 to 500 characters after trimming",
                    );
                }
            }
        }
        QuestionKind::Sequence => {
            let n = q.choices.len();
            if !(2..=20).contains(&n) {
                cx.err(&qp, format!("sequence needs 2 to 20 choices, found {n}"));
            }
            if q.choices.iter().any(|c| c.correct) {
                cx.warn(
                    &qp,
                    "sequence ignores correct flags; list choices in the correct order",
                );
            }
            if q.shuffle {
                cx.warn(
                    &qp,
                    "shuffle is ignored for sequence; items are shuffled every attempt",
                );
            }
        }
        QuestionKind::Matching => {
            if !q.choices.is_empty() {
                cx.err(&qp, "matching must not have choices");
            }
            let n = q.pairs.len();
            if !(2..=20).contains(&n) {
                cx.err(&qp, format!("matching needs 2 to 20 pairs, found {n}"));
            }
            let mut pair_ids = BTreeSet::new();
            for pair in &q.pairs {
                if !valid_name(&pair.id) {
                    cx.err(
                        &qp,
                        format!("pair id {:?} must be 1-128 chars of [A-Za-z0-9_-]", pair.id),
                    );
                }
                if !pair_ids.insert(pair.id.as_str()) {
                    cx.err(&qp, format!("duplicate pair id {:?}", pair.id));
                }
                if pair.left.is_empty() || pair.right.is_empty() {
                    cx.err(
                        &qp,
                        format!("pair {:?} needs non-empty left and right text", pair.id),
                    );
                }
            }
            if q.shuffle {
                cx.warn(
                    &qp,
                    "shuffle is ignored for matching; the right-hand column is shuffled every attempt",
                );
            }
        }
        QuestionKind::Hotspot => check_hotspot(cx, slide, q, &qp),
        QuestionKind::DragDrop => check_drag_drop(cx, slide, q, &qp),
        QuestionKind::Likert => check_likert(cx, q, &qp),
        QuestionKind::ShortAnswer | QuestionKind::Essay => check_text_answer(cx, q, &qp),
        QuestionKind::Freeform => check_freeform(cx, slide, q, &qp),
    }
    if q.kind != QuestionKind::Freeform && q.correct_when.is_some() {
        cx.err(&qp, "correct_when is only allowed on freeform questions");
    }
    for l in [&q.correct_layer, &q.incorrect_layer].into_iter().flatten() {
        check_layer_ref(cx, slide, l, &qp);
    }
    check_question_submit(cx, slide, &qp);
}

fn check_question_submit(cx: &mut Ctx<'_>, slide: &Slide, qp: &str) {
    let submit_triggers: Vec<&Trigger> = slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .filter(|t| t.action == Action::SubmitQuestion)
        .collect();
    if submit_triggers.is_empty() {
        cx.warn(
            qp,
            "no trigger runs submit_question, so this question can never be graded",
        );
    } else if submit_triggers
        .iter()
        .all(|trigger| trigger.when == Event::Submit)
    {
        cx.warn(
            qp,
            "no control on this slide submits the question: add a Submit button",
        );
    }
}
