//! Course-level, view/guides, resources, glossary, variable, and undo/redo commands.

use std::collections::HashSet;

use coursecraft_course::{
    Action, BackgroundAudio, Color, Condition, Event, FontFace, FontStyle, Guide, Location,
    MAX_FONT_FACES, MAX_FONT_FAMILY_CHARS, QuizSettings, QuizTracking, SlidesViewedTracking,
    Trigger, Value, VariableDef, course_string_locations, find_in_course, replace_in_target,
    string_mut,
};
use serde_json::{Value as Json, json};

use super::{
    MAX_TEXT, add_finish_button, bad, fresh_id, keep_valid, move_to, obj, opt, req, slide_mut,
    text, validate_color, validate_id,
};
use crate::{EngineError, Result, Session};

pub(super) fn course_info(s: &mut Session, _p: &Json) -> Result<Json> {
    Ok(json!({
        "title": s.course.title,
        "scenes": s.course.scenes.len(),
        "slides": s.course.slides().count(),
        "revision": s.revision,
        "can_undo": s.can_undo(),
        "can_redo": s.can_redo(),
    }))
}

pub(super) fn course_map(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(coursecraft_course::course_map(&s.course))
        .map_err(|e| EngineError::Invalid(e.to_string()))
}

pub(super) fn course_validate(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(s.validation_issues()).map_err(|e| EngineError::Invalid(e.to_string()))
}

pub(super) fn course_audit(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(s.course.audit()).map_err(|e| EngineError::Invalid(e.to_string()))
}

pub(super) fn course_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.update";
    let was_valid = s.is_playable();
    let map = obj(C, p)?;
    if let Some(title) = opt::<String>(C, p, "title")? {
        let title = text(C, title, "title")?;
        if title.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        s.course.title = title;
    }
    if let Some(id) = opt::<String>(C, p, "id")? {
        validate_id(C, &id)?;
        s.course.id = id;
    }
    if let Some(v) = map.get("start_slide") {
        match v {
            Json::Null => s.course.start_slide = None,
            Json::String(id) => {
                if id.trim().is_empty() {
                    return Err(bad(C, "start_slide cannot be empty"));
                }
                s.course.start_slide = Some(id.clone());
            }
            _ => return Err(bad(C, "`start_slide`: expected string or null")),
        }
    }
    if let Some(v) = map.get("language") {
        match v {
            Json::Null => s.course.language = None,
            Json::String(tag) => {
                let tag = tag.trim();
                if tag.is_empty() {
                    s.course.language = None;
                } else if coursecraft_course::is_language_tag_valid(tag) {
                    s.course.language = Some(tag.to_owned());
                } else {
                    return Err(bad(C, "language is not a simple BCP 47 tag"));
                }
            }
            _ => return Err(bad(C, "`language`: expected string or null")),
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn course_set_slide_size(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_slide_size";
    let was_valid = s.is_playable();
    let w: u32 = req(C, p, "w")?;
    let h: u32 = req(C, p, "h")?;
    let mode: String = req(C, p, "mode")?;
    let mode = match mode.as_str() {
        "scale" => crate::slide_size::ResizeMode::Scale,
        "fill" => crate::slide_size::ResizeMode::Fill,
        "keep" => crate::slide_size::ResizeMode::Keep,
        _ => return Err(bad(C, "`mode` must be \"scale\", \"fill\", or \"keep\"")),
    };
    if !crate::slide_size::size_in_bounds(w, h) {
        return Err(bad(C, "size must be 100..=7680 wide and 100..=4320 high"));
    }
    if !crate::slide_size::set_slide_size(&mut s.course, w, h, mode) {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "w": w, "h": h }))
}

pub(super) fn course_set_title(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_title";
    let was_valid = s.is_playable();
    let title = text(C, req(C, p, "title")?, "title")?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.title = title;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn course_set_player(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_player";
    let was_valid = s.is_playable();
    if let Some(v) = opt::<bool>(C, p, "show_bar")? {
        s.course.player.show_bar = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_nav")? {
        s.course.player.show_nav = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_menu")? {
        s.course.player.show_menu = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_seekbar")? {
        s.course.player.show_seekbar = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_speed")? {
        s.course.player.show_speed = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_fullscreen")? {
        s.course.player.show_fullscreen = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_transcript")? {
        s.course.player.show_transcript = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_title")? {
        s.course.player.show_title = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_volume")? {
        s.course.player.show_volume = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_captions")? {
        s.course.player.show_captions = v;
    }
    match obj(C, p)?.get("logo") {
        None => {}
        Some(Json::Null) => s.course.player.logo = None,
        Some(Json::String(src)) => {
            let src = text(C, src.clone(), "logo")?;
            if !coursecraft_course::is_image_src(&src) {
                return Err(bad(C, "`logo` must be a safe path to an image file"));
            }
            s.course.player.logo = Some(src);
        }
        Some(_) => return Err(bad(C, "`logo`: expected string or null")),
    }
    if let Some(v) = opt::<coursecraft_course::NavMode>(C, p, "navigation")? {
        s.course.player.navigation = v;
    }
    if let Some(v) = opt::<coursecraft_course::NextRequirement>(C, p, "next_requires")? {
        check_next_requirement(s, C, &v)?;
        s.course.player.next_requires = v;
    }
    if let Some(v) = opt::<coursecraft_course::SeekbarBehaviour>(C, p, "seekbar")? {
        s.course.player.seekbar = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A variable rule must name an existing true/false variable.
pub(super) fn check_next_requirement(
    s: &Session,
    cmd: &str,
    rule: &coursecraft_course::NextRequirement,
) -> Result<()> {
    let coursecraft_course::NextRequirement::Variable { name } = rule else {
        return Ok(());
    };
    let kind = coursecraft_course::builtin_variable_kind(name)
        .or_else(|| s.course.variable(name).map(|def| def.value.kind()));
    match kind {
        None => Err(bad(cmd, format!("variable {name:?} does not exist"))),
        Some(coursecraft_course::ValueKind::Bool) => Ok(()),
        Some(_) => Err(bad(cmd, format!("variable {name:?} is not true/false"))),
    }
}

pub(super) fn course_set_quiz(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_quiz";
    let was_valid = s.is_playable();
    let map = obj(C, p)?;
    match map.get("passing_percent") {
        None => return Err(bad(C, "missing `passing_percent`")),
        Some(Json::Null) => {
            s.course.quiz = None;
        }
        Some(Json::Number(n)) => {
            let Some(val) = n.as_u64() else {
                return Err(bad(C, "`passing_percent` must be an integer 0..=100"));
            };
            if val > 100 {
                return Err(bad(C, "`passing_percent` must be 0..=100"));
            }
            #[allow(clippy::cast_possible_truncation)]
            let passing_percent = val as u8;
            s.course.quiz = Some(QuizSettings { passing_percent });
        }
        _ => return Err(bad(C, "`passing_percent`: expected integer or null")),
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn course_set_publish_settings(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_publish_settings";
    let was_valid = s.is_playable();
    let _ = obj(C, p)?;
    let standard: Option<String> = opt(C, p, "standard")?;
    let lms_status: Option<coursecraft_course::LmsStatus> = opt(C, p, "lms_status")?;
    if standard.is_none() && lms_status.is_none() {
        return Err(bad(C, "provide `standard` and/or `lms_status`"));
    }
    let mut settings = s.course.editor.last_publish.clone().unwrap_or_else(|| {
        coursecraft_course::PublishSettings {
            standard: "scorm12".to_string(),
            lms_status: coursecraft_course::LmsStatus::Auto,
        }
    });
    if let Some(std) = standard {
        settings.standard = std;
    }
    if let Some(status) = lms_status {
        settings.lms_status = status;
    }
    s.course.editor.last_publish = Some(settings.clone());
    keep_valid(s, was_valid)?;
    Ok(json!({ "last_publish": settings }))
}

pub(super) fn course_set_tracking(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_tracking";
    let was_valid = s.is_playable();
    let map = obj(C, p)?;
    for key in map.keys() {
        if key != "slides_viewed" && key != "quiz" {
            return Err(bad(C, format!("unknown parameter `{key}`")));
        }
    }
    let has_slides = map.contains_key("slides_viewed");
    let has_quiz = map.contains_key("quiz");
    if !has_slides && !has_quiz {
        return Err(bad(C, "provide `slides_viewed` and/or `quiz`"));
    }
    if let Some(val) = map.get("slides_viewed") {
        if val.is_null() {
            s.course.tracking.slides_viewed = None;
        } else {
            let sv: SlidesViewedTracking = serde_json::from_value(val.clone())
                .map_err(|e| bad(C, format!("`slides_viewed`: {e}")))?;
            if matches!(sv, SlidesViewedTracking::All)
                && let Some(sv_map) = val.as_object()
                && let Some(extra) = sv_map.keys().find(|k| k.as_str() != "mode")
            {
                return Err(bad(
                    C,
                    format!("`slides_viewed`: unknown field `{extra}` for mode `all`"),
                ));
            }
            if matches!(sv, SlidesViewedTracking::AtLeast { count: 0 }) {
                return Err(bad(C, "`slides_viewed.count` must be at least 1"));
            }
            s.course.tracking.slides_viewed = Some(sv);
        }
    }
    if let Some(val) = map.get("quiz") {
        if val.is_null() {
            s.course.tracking.quiz = None;
        } else {
            let qz: QuizTracking =
                serde_json::from_value(val.clone()).map_err(|e| bad(C, format!("`quiz`: {e}")))?;
            if let Some(ref id) = qz.slide {
                validate_id(C, id)?;
            }
            s.course.tracking.quiz = Some(qz);
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "tracking": s.course.tracking }))
}

pub(super) fn course_set_background_audio(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_background_audio";
    let was_valid = s.is_playable();
    let map = obj(C, p)?;
    let volume: Option<f32> = opt(C, p, "volume")?;
    if let Some(volume) = volume
        && (!volume.is_finite() || !(0.0..=1.0).contains(&volume))
    {
        return Err(bad(C, "`volume` must be finite and within 0..=1"));
    }
    let looping: Option<bool> = opt(C, p, "loop")?;

    match map.get("src") {
        Some(Json::Null) => {
            s.course.background_audio = None;
            // Triggers that play, pause or stop the background audio have
            // nothing left to act on, so they go with it.
            if was_valid {
                let needles = vec!["the course has no background audio".to_owned()];
                let slide_ids: Vec<String> = s
                    .course
                    .slides()
                    .chain(s.course.banks.iter().flat_map(|b| b.slides.iter()))
                    .filter(|sl| {
                        !sl.triggers.is_empty() || sl.layers.iter().any(|l| !l.triggers.is_empty())
                    })
                    .map(|sl| sl.id.clone())
                    .collect();
                for slide_id in &slide_ids {
                    super::remove_dangling_triggers(s, slide_id, &needles);
                }
            }
        }
        Some(Json::String(src)) => {
            let src = text(C, src.clone(), "src")?;
            let audio = s
                .course
                .background_audio
                .get_or_insert_with(|| BackgroundAudio {
                    src: String::new(),
                    volume: 0.5,
                    looping: true,
                });
            audio.src = src;
        }
        Some(_) => return Err(bad(C, "`src`: expected string or null")),
        None => {
            if s.course.background_audio.is_none() && (volume.is_some() || looping.is_some()) {
                return Err(EngineError::NotFound(
                    "course has no background audio".to_string(),
                ));
            }
        }
    }

    if let Some(audio) = &mut s.course.background_audio {
        if let Some(volume) = volume {
            audio.volume = volume;
        }
        if let Some(looping) = looping {
            audio.looping = looping;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn course_add_finish_button(s: &mut Session, _p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let last_slide_id = s
        .course
        .scenes
        .iter()
        .rev()
        .find_map(|scene| scene.slides.last())
        .map(|slide| slide.id.clone())
        .ok_or_else(|| EngineError::Invalid("course has no slides".into()))?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &last_slide_id)?;
    let button = add_finish_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

pub(super) fn course_set_theme(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_theme";
    let was_valid = s.is_playable();
    if let Some(accent) = opt::<String>(C, p, "accent")? {
        let col = Color(accent);
        validate_color(C, &col)?;
        s.course.theme.accent = col;
    }
    if let Some(bg) = opt::<String>(C, p, "background")? {
        let col = Color(bg);
        validate_color(C, &col)?;
        s.course.theme.background = col;
    }
    if let Some(text_col) = opt::<String>(C, p, "text")? {
        let col = Color(text_col);
        validate_color(C, &col)?;
        s.course.theme.text = col;
    }
    if let Some(font) = opt::<String>(C, p, "font")? {
        let f = text(C, font, "font")?;
        s.course.theme.font = f;
    }
    // `heading_font` is a string, or null / empty to clear it (headings use `font`).
    if let Some(value) = p.get("heading_font") {
        s.course.theme.heading_font = match value {
            Json::Null => None,
            Json::String(raw) if raw.trim().is_empty() => None,
            Json::String(raw) => Some(text(C, raw.clone(), "heading_font")?),
            _ => {
                return Err(EngineError::Invalid(format!(
                    "{C}: heading_font must be a string or null"
                )));
            }
        };
    }
    if let Some(bar) = opt::<String>(C, p, "bar")? {
        let col = Color(bar);
        validate_color(C, &col)?;
        s.course.theme.player.bar = col;
    }
    if let Some(bar_text) = opt::<String>(C, p, "bar_text")? {
        let col = Color(bar_text);
        validate_color(C, &col)?;
        s.course.theme.player.bar_text = col;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Trim and strip control characters from a font family name.
fn clean_font_family(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_FONT_FAMILY_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn font_src_is_valid(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    !src.is_empty()
        && src.starts_with("fonts/")
        && !src.starts_with('/')
        && !src.contains('\\')
        && !src.contains(':')
        && !src.chars().any(char::is_control)
        && src
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && [".ttf", ".otf", ".ttc", ".otc"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

pub(super) fn course_add_font(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.add_font";
    let was_valid = s.is_playable();
    let raw_family: String = req(C, p, "family")?;
    let family = clean_font_family(&raw_family);
    if family.is_empty() {
        return Err(bad(C, "`family` is empty"));
    }
    let style: FontStyle = req(C, p, "style")?;
    let src: String = req(C, p, "src")?;
    if !font_src_is_valid(&src) {
        return Err(bad(
            C,
            "`src` must be a .ttf, .otf, .ttc or .otc media path under fonts/",
        ));
    }
    if s.course.fonts.len() >= MAX_FONT_FACES {
        return Err(bad(
            C,
            format!("a course may register at most {MAX_FONT_FACES} font faces"),
        ));
    }
    if s.course
        .fonts
        .iter()
        .any(|face| face.family.trim().eq_ignore_ascii_case(&family) && face.style == style)
    {
        return Err(bad(
            C,
            format!("font face for {family:?} {style:?} already exists"),
        ));
    }
    let face = FontFace { family, style, src };
    s.course.fonts.push(face.clone());
    keep_valid(s, was_valid)?;
    Ok(json!({ "font": face }))
}

pub(super) fn course_remove_font(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.remove_font";
    let was_valid = s.is_playable();
    let family = opt::<String>(C, p, "family")?.map(|f| clean_font_family(&f));
    let style = opt::<FontStyle>(C, p, "style")?;
    let src = opt::<String>(C, p, "src")?;
    if family.as_deref().unwrap_or_default().is_empty() && src.is_none() {
        return Err(bad(C, "pass `family` or `src`"));
    }
    let before = s.course.fonts.len();
    s.course.fonts.retain(|face| {
        let family_matches = family
            .as_deref()
            .is_some_and(|name| face.family.trim().eq_ignore_ascii_case(name));
        let style_matches = style.is_none_or(|wanted| face.style == wanted);
        let src_matches = src.as_deref().is_some_and(|path| face.src == path);
        if family.is_some() {
            !(family_matches && style_matches)
        } else {
            !src_matches
        }
    });
    let removed = before.saturating_sub(s.course.fonts.len());
    if removed == 0 {
        return Err(EngineError::NotFound("font face not found".to_string()));
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed": removed }))
}

pub(super) fn course_export_xliff(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.export_xliff";
    let target_language = req::<String>(C, p, "target_language")?;
    if !coursecraft_course::is_language_tag_valid(&target_language) {
        return Err(bad(C, "target_language is not a simple BCP 47 tag"));
    }
    let xliff = coursecraft_format::xliff::export_xliff(&s.course, &target_language)
        .map_err(|e| EngineError::Invalid(e.to_string()))?;
    Ok(Json::String(xliff))
}

pub(super) fn course_import_xliff(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.import_xliff";
    let xliff = req::<String>(C, p, "xliff")?;
    let doc = coursecraft_format::xliff::parse_xliff(xliff.as_bytes())
        .map_err(|e| EngineError::Invalid(e.to_string()))?;
    let (course, report) = coursecraft_format::xliff::apply_xliff(&s.course, &doc);
    if let Some(issue) = course
        .validate()
        .into_iter()
        .find(|i| i.severity == coursecraft_course::Severity::Error)
    {
        return Err(EngineError::Invalid(format!(
            "translated course is invalid: {} ({})",
            issue.message, issue.path
        )));
    }
    s.course = course;
    Ok(json!({
        "applied": report.applied,
        "missing_target": report.missing_target,
        "source_changed": report.source_changed,
        "unknown_ids": report.unknown_ids,
        "placeholder_mismatch": report.placeholder_mismatch,
        "unsupported_script": report.unsupported_script,
    }))
}

/// Read-only search over every author-visible string.
pub(super) fn course_find(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.find";
    let query = text(C, req::<String>(C, p, "query")?, "query")?;
    let case_sensitive = opt::<bool>(C, p, "case")?.unwrap_or(false);
    let whole_word = opt::<bool>(C, p, "whole_word")?.unwrap_or(false);
    let (hits, truncated) = find_in_course(&s.course, &query, case_sensitive, whole_word);
    let hits = serde_json::to_value(&hits).map_err(|e| EngineError::Invalid(e.to_string()))?;
    Ok(json!({ "hits": hits, "truncated": truncated }))
}

/// Replace in every author-visible string, or only in the `only` locations.
/// One undo step. Matches that overlap a `%name%` reference are skipped and
/// counted. A replace that changes nothing makes no undo step.
pub(super) fn course_replace(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.replace";
    let was_valid = s.is_playable();
    let query = text(C, req::<String>(C, p, "query")?, "query")?;
    let replacement = text(C, req::<String>(C, p, "replacement")?, "replacement")?;
    let case_sensitive = opt::<bool>(C, p, "case")?.unwrap_or(false);
    let whole_word = opt::<bool>(C, p, "whole_word")?.unwrap_or(false);
    let only: Option<HashSet<Location>> =
        opt::<Vec<Location>>(C, p, "only")?.map(|list| list.into_iter().collect());

    let mut replaced = 0usize;
    let mut skipped = 0usize;
    // Looking up a string can fill in a state's `text` from its runs, so keep
    // the course as it was to restore it when nothing matches.
    let before = s.course.clone();
    if !query.is_empty() {
        let locations: Vec<Location> = course_string_locations(&s.course)
            .into_iter()
            .filter(|location| only.as_ref().is_none_or(|set| set.contains(location)))
            .collect();
        for location in &locations {
            let Some(target) = string_mut(&mut s.course, location) else {
                continue;
            };
            let outcome = replace_in_target(
                target,
                &query,
                &replacement,
                case_sensitive,
                whole_word,
                MAX_TEXT,
            );
            if outcome.too_long {
                return Err(bad(
                    C,
                    format!("the replacement would make a text longer than {MAX_TEXT} bytes"),
                ));
            }
            replaced = replaced.saturating_add(outcome.replaced);
            skipped = skipped.saturating_add(outcome.skipped);
        }
    }
    if replaced == 0 {
        s.course = before;
        s.skip_undo = true;
        return Ok(json!({ "replaced": 0, "skipped_variable_refs": skipped }));
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "replaced": replaced, "skipped_variable_refs": skipped }))
}

pub(super) fn edit_undo(s: &mut Session, _p: &Json) -> Result<Json> {
    s.undo().map(|()| Json::Null)
}

pub(super) fn edit_redo(s: &mut Session, _p: &Json) -> Result<Json> {
    s.redo().map(|()| Json::Null)
}

pub(super) fn edit_history(s: &mut Session, _p: &Json) -> Result<Json> {
    Ok(json!({
        "undo": s.undo_names(),
        "redo": s.redo_names(),
    }))
}

const MAX_GUIDES: usize = 256;

fn check_guide_pos(cmd: &'static str, at: f32) -> Result<f32> {
    if !at.is_finite() || !(-100_000.0..=100_000.0).contains(&at) {
        return Err(bad(cmd, "`at` must be a finite coordinate"));
    }
    Ok(at)
}

pub(super) fn view_set(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "view.set";
    let was_valid = s.is_playable();
    let mut before = s.course.editor.clone();
    if let Some(v) = opt::<bool>(C, p, "show_rulers")? {
        s.course.editor.show_rulers = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_grid")? {
        s.course.editor.show_grid = v;
    }
    if let Some(v) = opt::<bool>(C, p, "snap_to_grid")? {
        s.course.editor.snap_to_grid = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_guides")? {
        s.course.editor.show_guides = v;
    }
    if let Some(v) = opt::<f32>(C, p, "grid_spacing")? {
        if !v.is_finite() || !coursecraft_course::EditorSettings::GRID_SPACING_RANGE.contains(&v) {
            return Err(bad(
                C,
                "`grid_spacing` must be between 2 and 400 slide pixels",
            ));
        }
        // The default spacing is stored as none, so the file stays as it was.
        s.course.editor.grid_spacing =
            ((v - coursecraft_course::EditorSettings::DEFAULT_GRID_SPACING).abs() > f32::EPSILON)
                .then_some(v);
    }
    if let Some(v) = opt::<coursecraft_course::BaseSlideView>(C, p, "base_slide_view")? {
        s.course.editor.base_slide_view = v;
    }
    if let Some(v) = opt::<coursecraft_course::PublishSettings>(C, p, "last_publish")? {
        s.course.editor.last_publish = Some(v);
    }
    // Remembering the last publish standard is not an author edit; skip undo
    // when nothing else in `editor` changed.
    before.last_publish = s.course.editor.last_publish.clone();
    if s.course.editor == before {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "editor": s.course.editor }))
}

pub(super) fn view_guides_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "view.guides.add";
    let was_valid = s.is_playable();
    let vertical: bool = req(C, p, "vertical")?;
    let at = check_guide_pos(C, req::<f32>(C, p, "at")?)?;
    if s.course.editor.guides.len() >= MAX_GUIDES {
        return Err(bad(C, "too many guides"));
    }
    s.course.editor.guides.push(Guide { vertical, at });
    s.course.editor.show_guides = true;
    let index = s.course.editor.guides.len().saturating_sub(1);
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": index }))
}

pub(super) fn view_guides_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "view.guides.move";
    let was_valid = s.is_playable();
    let index: usize = req(C, p, "index")?;
    let at = check_guide_pos(C, req::<f32>(C, p, "at")?)?;
    let Some(guide) = s.course.editor.guides.get_mut(index) else {
        return Err(bad(C, "guide index out of range"));
    };
    if (guide.at - at).abs() <= f32::EPSILON {
        s.skip_undo = true;
    } else {
        guide.at = at;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": index, "at": at }))
}

pub(super) fn view_guides_remove(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "view.guides.remove";
    let was_valid = s.is_playable();
    let index: usize = req(C, p, "index")?;
    if index >= s.course.editor.guides.len() {
        return Err(bad(C, "guide index out of range"));
    }
    s.course.editor.guides.remove(index);
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed": index }))
}

// ---------------------------------------------------------------- resources and glossary

fn resource_index(s: &Session, cmd: &str, id: &str) -> Result<usize> {
    let _ = cmd;
    s.course
        .resources
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("resource {id:?} does not exist")))
}

fn glossary_index(s: &Session, id: &str) -> Result<usize> {
    s.course
        .glossary
        .iter()
        .position(|g| g.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("glossary entry {id:?} does not exist")))
}

pub(super) fn resource_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.add";
    let was_valid = s.is_playable();
    let title = text(C, req(C, p, "title")?, "title")?;
    let target: coursecraft_course::ResourceTarget = req(C, p, "target")?;
    let id = fresh_id("resource", s.course.resources.iter().map(|r| r.id.as_str()));
    s.course.resources.push(coursecraft_course::Resource {
        id: id.clone(),
        title,
        target,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn resource_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.update";
    let was_valid = s.is_playable();
    let id: String = req(C, p, "id")?;
    let title: Option<String> = opt(C, p, "title")?;
    let target: Option<coursecraft_course::ResourceTarget> = opt(C, p, "target")?;
    let index = resource_index(s, C, &id)?;
    if let Some(resource) = s.course.resources.get_mut(index) {
        if let Some(title) = title {
            resource.title = text(C, title, "title")?;
        }
        if let Some(target) = target {
            resource.target = target;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn resource_remove(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.remove";
    let id: String = req(C, p, "id")?;
    let index = resource_index(s, C, &id)?;
    s.course.resources.remove(index);
    Ok(Json::Null)
}

pub(super) fn resource_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.move";
    let id: String = req(C, p, "id")?;
    let to: usize = req(C, p, "to")?;
    let index = resource_index(s, C, &id)?;
    if to >= s.course.resources.len() {
        return Err(bad(C, "`to` is past the end of the list"));
    }
    move_to(&mut s.course.resources, index, to);
    Ok(Json::Null)
}

pub(super) fn glossary_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.add";
    let was_valid = s.is_playable();
    let term = text(C, req(C, p, "term")?, "term")?;
    let definition: Option<String> = opt(C, p, "definition")?;
    let definition = text(C, definition.unwrap_or_default(), "definition")?;
    let id = fresh_id("term", s.course.glossary.iter().map(|g| g.id.as_str()));
    s.course.glossary.push(coursecraft_course::GlossaryEntry {
        id: id.clone(),
        term,
        definition,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn glossary_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.update";
    let was_valid = s.is_playable();
    let id: String = req(C, p, "id")?;
    let term: Option<String> = opt(C, p, "term")?;
    let definition: Option<String> = opt(C, p, "definition")?;
    let index = glossary_index(s, &id)?;
    if let Some(entry) = s.course.glossary.get_mut(index) {
        if let Some(term) = term {
            entry.term = text(C, term, "term")?;
        }
        if let Some(definition) = definition {
            entry.definition = text(C, definition, "definition")?;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn glossary_remove(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.remove";
    let id: String = req(C, p, "id")?;
    let index = glossary_index(s, &id)?;
    s.course.glossary.remove(index);
    Ok(Json::Null)
}

pub(super) fn glossary_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.move";
    let id: String = req(C, p, "id")?;
    let to: usize = req(C, p, "to")?;
    let index = glossary_index(s, &id)?;
    if to >= s.course.glossary.len() {
        return Err(bad(C, "`to` is past the end of the list"));
    }
    move_to(&mut s.course.glossary, index, to);
    Ok(Json::Null)
}

fn glossary_cmp(
    a: &coursecraft_course::GlossaryEntry,
    b: &coursecraft_course::GlossaryEntry,
) -> std::cmp::Ordering {
    a.term
        .to_lowercase()
        .cmp(&b.term.to_lowercase())
        .then_with(|| a.term.cmp(&b.term))
        .then_with(|| a.id.cmp(&b.id))
}

pub(super) fn glossary_sort(s: &mut Session, _p: &Json) -> Result<Json> {
    let already_sorted = s
        .course
        .glossary
        .windows(2)
        .all(|w| glossary_cmp(&w[0], &w[1]) != std::cmp::Ordering::Greater);
    if already_sorted {
        s.skip_undo = true;
        return Ok(Json::Null);
    }
    s.course.glossary.sort_by(glossary_cmp);
    Ok(Json::Null)
}

pub(super) fn variable_add(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let name: String = req("variable.add", p, "name")?;
    let value: Value = req("variable.add", p, "value")?;
    if s.course.variable(&name).is_some() {
        return Err(EngineError::Invalid(format!(
            "variable {name:?} already exists"
        )));
    }
    s.course.variables.push(VariableDef { name, value });
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn variable_set_value(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "variable.set_value";
    let was_valid = s.is_playable();
    let name: String = req(C, p, "name")?;
    let value: Value = req(C, p, "value")?;

    if let Value::Number(n) = &value
        && !n.is_finite()
    {
        return Err(bad(C, "variable value must be finite"));
    }

    let var = s
        .course
        .variables
        .iter_mut()
        .find(|v| v.name == name)
        .ok_or_else(|| EngineError::NotFound(format!("variable {name:?} does not exist")))?;

    if var.value.kind() != value.kind() {
        return Err(EngineError::Invalid(format!(
            "cannot change variable type from {:?} to {:?}",
            var.value.kind(),
            value.kind()
        )));
    }
    var.value = value;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn condition_references_var(c: &Condition, var_name: &str) -> bool {
    match c {
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(|c| condition_references_var(c, var_name))
        }
        Condition::Not(inner) => condition_references_var(inner, var_name),
        Condition::Compare {
            variable, value, ..
        } => variable == var_name || value.as_variable() == Some(var_name),
        Condition::ObjectState { .. } => false,
    }
}

fn trigger_references_var(t: &Trigger, var_name: &str) -> bool {
    let in_event = match &t.when {
        Event::VariableChanges { variable } => variable == var_name,
        _ => false,
    };
    let in_action = match &t.action {
        Action::SetVariable {
            variable, value, ..
        } => variable == var_name || value.as_ref().and_then(|v| v.as_variable()) == Some(var_name),
        _ => false,
    };
    let in_condition = t
        .condition
        .as_ref()
        .is_some_and(|c| condition_references_var(c, var_name));
    in_event || in_action || in_condition
}

pub(super) fn variable_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "variable.delete";
    let was_valid = s.is_playable();
    let name: String = req(C, p, "name")?;

    let pos = s
        .course
        .variables
        .iter()
        .position(|v| v.name == name)
        .ok_or_else(|| EngineError::NotFound(format!("variable {name:?} does not exist")))?;

    for sc in &s.course.scenes {
        for sl in &sc.slides {
            for (i, t) in sl.triggers.iter().enumerate() {
                if trigger_references_var(t, &name) {
                    return Err(EngineError::Invalid(format!(
                        "variable {name:?} is referenced by trigger {i} on slide {:?}",
                        sl.id
                    )));
                }
            }
            for l in &sl.layers {
                for (i, t) in l.triggers.iter().enumerate() {
                    if trigger_references_var(t, &name) {
                        return Err(EngineError::Invalid(format!(
                            "variable {name:?} is referenced by trigger {i} on layer {:?} of slide {:?}",
                            l.id, sl.id
                        )));
                    }
                }
            }
        }
    }

    s.course.variables.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}
