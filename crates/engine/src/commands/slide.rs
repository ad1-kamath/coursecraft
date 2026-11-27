//! Scene, slide, master, bank, draw, layout, outline, template, copy/remap, results, and certificate commands.

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{
    Action, Align, Autofit, BankDraw, Choice, CmpOp, Color, Condition, Course, CourseOutline,
    DashStyle, Event, Glue, Layer, Link, Object, ObjectKind, Outline, OutlineProblem,
    OutlineQuestion, OutlineSlide, Question, QuestionBank, QuestionKind, QuizSettings, Rect,
    RevisitMode, Scene, SeekMode, ShapeKind, Size, Slide, SlideTarget, TextRun, TextStyle, Theme,
    Timing, Transition, Trigger, VAlign, Value,
};
use serde_json::{Value as Json, json};

use super::{
    MAX_DURATION_MS, add_feedback_layers, add_submit_button, apply_question_kind, bad, course,
    default_multiple_choice_question, ensure_question_support, fresh_id, keep_valid, obj, opt,
    question_insert, remove_dangling_triggers, req, slide_mut, text, validate_color, validate_id,
};
use crate::{EngineError, Result, Session};

pub(super) fn scene_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.add";
    let was_valid = s.is_playable();
    let id: Option<String> = opt(C, p, "id")?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if s.course.scenes.iter().any(|sc| sc.id == custom) {
                return Err(EngineError::Invalid(format!(
                    "scene id {custom:?} already exists"
                )));
            }
            custom
        }
        None => fresh_id("scene", s.course.scenes.iter().map(|sc| sc.id.as_str())),
    };
    let title =
        opt(C, p, "title")?.unwrap_or_else(|| format!("Scene {}", s.course.scenes.len() + 1));
    let title = text(C, title, "title")?;
    s.course.scenes.push(Scene {
        id: id.clone(),
        title,
        slides: Vec::new(),
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn scene_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "scene")?;
    let title: Option<String> = opt(C, p, "title")?;
    let scene = s
        .course
        .scenes
        .iter_mut()
        .find(|sc| sc.id == sid)
        .ok_or_else(|| EngineError::NotFound(format!("scene {sid:?} does not exist")))?;
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        scene.title = t;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn scene_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.delete";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "scene")?;
    if s.course.scenes.len() <= 1 {
        return Err(EngineError::Invalid(
            "a course needs at least one scene".into(),
        ));
    }
    let pos = s
        .course
        .scenes
        .iter()
        .position(|sc| sc.id == sid)
        .ok_or_else(|| EngineError::NotFound(format!("scene {sid:?} does not exist")))?;
    let deleted_sids: Vec<String> = s.course.scenes[pos]
        .slides
        .iter()
        .map(|sl| sl.id.clone())
        .collect();
    s.course.scenes.remove(pos);
    let removed = clean_after_slides_removed(s, was_valid, &deleted_sids, Some(&sid));
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed_triggers": removed }))
}

/// Remove everything that pointed at slides (or a scene) that are gone, in the
/// same undo step: jump and lightbox triggers, text links, `start_slide` and
/// the quiz tracking slide. Returns how many triggers were removed.
fn clean_after_slides_removed(
    s: &mut Session,
    was_valid: bool,
    deleted_sids: &[String],
    deleted_scene: Option<&str>,
) -> usize {
    let gone: BTreeSet<&str> = deleted_sids.iter().map(String::as_str).collect();
    if s.course
        .start_slide
        .as_deref()
        .is_some_and(|id| gone.contains(id))
    {
        s.course.start_slide = None;
    }
    if let Some(quiz) = s.course.tracking.quiz.as_mut()
        && quiz.slide.as_deref().is_some_and(|id| gone.contains(id))
    {
        // No slide means the course's graded quiz is evaluated on submit.
        quiz.slide = None;
    }
    clear_links_to_slides(&mut s.course, &gone);
    if !was_valid {
        return 0;
    }
    let mut needles: Vec<String> = deleted_sids
        .iter()
        .map(|deleted_sid| format!("slide {deleted_sid:?} does not exist"))
        .collect();
    if let Some(scene) = deleted_scene {
        needles.push(format!("scene {scene:?} does not exist"));
    }
    let remaining_sids: Vec<String> = s
        .course
        .slides()
        .chain(s.course.banks.iter().flat_map(|b| b.slides.iter()))
        .filter(|sl| !sl.triggers.is_empty() || sl.layers.iter().any(|l| !l.triggers.is_empty()))
        .map(|sl| sl.id.clone())
        .collect();
    let mut total = 0;
    for other_sid in &remaining_sids {
        total += remove_dangling_triggers(s, other_sid, &needles);
    }
    total
}

/// Drop every `Link::Slide` text link whose slide is in `gone`.
fn clear_links_to_slides(course: &mut Course, gone: &BTreeSet<&str>) {
    fn clear(runs: &mut [TextRun], gone: &BTreeSet<&str>) {
        for run in runs {
            if matches!(&run.link, Some(Link::Slide { slide }) if gone.contains(slide.as_str())) {
                run.link = None;
            }
        }
    }
    fn clear_shape_text(text: &mut Option<coursecraft_course::ShapeText>, gone: &BTreeSet<&str>) {
        if let Some(text) = text {
            clear(&mut text.runs, gone);
        }
    }
    fn clear_object(object: &mut Object, gone: &BTreeSet<&str>) {
        match &mut object.kind {
            ObjectKind::Text { runs, .. } | ObjectKind::Marker { runs, .. } => clear(runs, gone),
            ObjectKind::Shape { text, .. } => clear_shape_text(text, gone),
            ObjectKind::Path { path } => clear_shape_text(&mut path.text, gone),
            ObjectKind::Table { rows, .. } => {
                for cell in rows.iter_mut().flat_map(|row| row.cells.iter_mut()) {
                    clear(&mut cell.runs, gone);
                }
            }
            _ => {}
        }
        for state in &mut object.states {
            if let Some(runs) = &mut state.runs {
                clear(runs, gone);
            }
            for extra in &mut state.extras {
                match &mut extra.kind {
                    coursecraft_course::StateExtraKind::Text { runs, .. } => clear(runs, gone),
                    coursecraft_course::StateExtraKind::Shape { text, .. } => {
                        clear_shape_text(text, gone);
                    }
                    coursecraft_course::StateExtraKind::Path { path } => {
                        clear_shape_text(&mut path.text, gone);
                    }
                    coursecraft_course::StateExtraKind::Image { .. } => {}
                }
            }
        }
    }
    fn clear_slide(slide: &mut Slide, gone: &BTreeSet<&str>) {
        for object in &mut slide.objects {
            clear_object(object, gone);
        }
        for layer in &mut slide.layers {
            for object in &mut layer.objects {
                clear_object(object, gone);
            }
        }
    }
    for slide in course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .chain(course.masters.iter_mut())
        .chain(course.templates.iter_mut())
        .chain(
            course
                .banks
                .iter_mut()
                .flat_map(|bank| bank.slides.iter_mut()),
        )
    {
        clear_slide(slide, gone);
    }
}

pub(super) fn slide_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.add";
    let was_valid = s.is_playable();
    let scene: Option<String> = opt(C, p, "scene")?;
    let after: Option<String> = opt(C, p, "after")?;
    let id: Option<String> = opt(C, p, "id")?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if s.course.slides().any(|sl| sl.id == custom) {
                return Err(EngineError::Invalid(format!(
                    "slide id {custom:?} already exists"
                )));
            }
            custom
        }
        None => fresh_id("slide", s.course.all_slide_ids()),
    };
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| "Untitled".to_owned()),
        "title",
    )?;
    let new = Slide {
        id: id.clone(),
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: after
            .as_deref()
            .and_then(|after| s.course.play_slide(after))
            .and_then(|slide| slide.master.clone()),
        revisit: RevisitMode::Auto,
        player: None,
    };
    // Where: after a given slide, else at the end of the given scene, else of the last scene.
    let (si, pos) = if let Some(after) = &after {
        s.course
            .scenes
            .iter()
            .enumerate()
            .find_map(|(si, sc)| {
                sc.slides
                    .iter()
                    .position(|sl| sl.id == *after)
                    .map(|i| (si, i + 1))
            })
            .ok_or_else(|| EngineError::NotFound(format!("slide {after:?} does not exist")))?
    } else {
        let si = match &scene {
            Some(sc) => s
                .course
                .scenes
                .iter()
                .position(|x| x.id == *sc)
                .ok_or_else(|| EngineError::NotFound(format!("scene {sc:?} does not exist")))?,
            None => s
                .course
                .scenes
                .len()
                .checked_sub(1)
                .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
        };
        (si, s.course.scenes.get(si).map_or(0, |sc| sc.slides.len()))
    };
    let scene = s
        .course
        .scenes
        .get_mut(si)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    scene.slides.insert(pos.min(scene.slides.len()), new);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn slide_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let title: Option<String> = opt(C, p, "title")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let revisit: Option<RevisitMode> = opt(C, p, "revisit")?;
    let notes: Option<String> = opt(C, p, "notes")?;
    let notes = notes.map(|n| text(C, n, "notes")).transpose()?;
    let slide = slide_mut(s, &sid)?;
    if let Some(n) = notes {
        slide.notes = n;
    }
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        slide.title = t;
    }
    if let Some(ms) = duration_ms {
        if ms > MAX_DURATION_MS {
            return Err(bad(C, "duration is longer than 24 hours"));
        }
        slide.duration_ms = ms;
    }
    if let Some(mode) = revisit {
        slide.revisit = mode;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn slide_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let id: String = req("slide.delete", p, "slide")?;
    if s.course.slides().count() <= 1 {
        return Err(EngineError::Invalid(
            "a course needs at least one slide".into(),
        ));
    }
    let mut found = false;
    for sc in &mut s.course.scenes {
        let before = sc.slides.len();
        sc.slides.retain(|sl| sl.id != id);
        found |= sc.slides.len() != before;
    }
    if !found {
        return Err(EngineError::NotFound(format!(
            "slide {id:?} does not exist"
        )));
    }
    let removed = clean_after_slides_removed(s, was_valid, std::slice::from_ref(&id), None);
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed_triggers": removed }))
}

pub(super) fn slide_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.move";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let target_scene_id: Option<String> = opt(C, p, "scene")?;
    let index: usize = req(C, p, "index")?;

    if let Some(tid) = target_scene_id.as_deref()
        && !s.course.scenes.iter().any(|sc| sc.id == tid)
    {
        return Err(EngineError::NotFound(format!(
            "scene {tid:?} does not exist"
        )));
    }

    let mut found = None;
    for sc in &mut s.course.scenes {
        if let Some(pos) = sc.slides.iter().position(|sl| sl.id == sid) {
            let slide = sc.slides.remove(pos);
            found = Some((sc.id.clone(), slide));
            break;
        }
    }
    let (source_scene_id, slide) =
        found.ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;

    let target_id = target_scene_id.as_deref().unwrap_or(&source_scene_id);
    let target_scene = s
        .course
        .scenes
        .iter_mut()
        .find(|sc| sc.id == target_id)
        .ok_or_else(|| EngineError::NotFound(format!("scene {target_id:?} does not exist")))?;

    let at = index.min(target_scene.slides.len());
    target_scene.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn slide_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.duplicate";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let after: Option<String> = opt(C, p, "after")?;
    let source = s
        .course
        .slide(&sid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    let copy = copy_slide(&s.course, source, &sid, false);
    let new_id = copy.slide.id.clone();
    let after = after.as_deref().or(Some(sid.as_str()));
    insert_slide_after(s, copy.slide, None, after)?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

pub(super) fn slide_paste(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.paste";
    let was_valid = s.is_playable();
    let scene: Option<String> = opt(C, p, "scene")?;
    let after: Option<String> = opt(C, p, "after")?;
    let source: Slide = req(C, p, "slide")?;
    let old_id = source.id.clone();
    let copy = copy_slide(&s.course, source, &old_id, false);
    let new_id = copy.slide.id.clone();
    insert_slide_after(s, copy.slide, scene.as_deref(), after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

pub(super) fn slide_set_duration(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let id: String = req("slide.set_duration", p, "slide")?;
    let ms: u32 = req("slide.set_duration", p, "ms")?;
    if ms > MAX_DURATION_MS {
        return Err(bad(
            "slide.set_duration",
            "duration is longer than 24 hours",
        ));
    }
    slide_mut(s, &id)?.duration_ms = ms;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn slide_set_background(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_background";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let map = obj(C, p)?;
    let bg_val = map
        .get("background")
        .ok_or_else(|| bad(C, "missing `background`"))?;
    let bg = match bg_val {
        Json::Null => None,
        Json::String(hex) => {
            let col = Color(hex.clone());
            validate_color(C, &col)?;
            Some(col)
        }
        _ => return Err(bad(C, "`background` must be string or null")),
    };
    let sl = slide_mut(s, &sid)?;
    sl.background = bg;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn slide_set_transition(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_transition";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let transition = transition_param(C, p)?;
    let sl = slide_mut(s, &sid)?;
    sl.transition = transition;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Give every slide of every scene the same transition (Apply To All).
pub(super) fn slide_set_transition_all(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_transition_all";
    let was_valid = s.is_playable();
    let transition = transition_param(C, p)?;
    let mut count = 0usize;
    for slide in s
        .course
        .scenes
        .iter_mut()
        .flat_map(|sc| sc.slides.iter_mut())
    {
        slide.transition = transition.clone();
        count += 1;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "slides": count }))
}

/// The `transition` param: `null` for none, or a checked [`Transition`].
fn transition_param(cmd: &str, p: &Json) -> Result<Option<Transition>> {
    let map = obj(cmd, p)?;
    let trans_val = map
        .get("transition")
        .ok_or_else(|| bad(cmd, "missing `transition`"))?;
    let transition: Option<Transition> = match trans_val {
        Json::Null => None,
        val => {
            let t: Transition = serde_json::from_value(val.clone())
                .map_err(|e| bad(cmd, format!("invalid transition: {e}")))?;
            if t.duration_ms < 1 || t.duration_ms > 60000 {
                return Err(bad(cmd, "transition duration_ms must be 1..=60000"));
            }
            Some(t)
        }
    };
    Ok(transition)
}

/// An optional true/false override in a command's params: absent leaves it
/// alone, `null` clears it, a bool sets it.
fn flag_override(
    cmd: &str,
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<Option<bool>>> {
    match map.get(key) {
        None => Ok(None),
        Some(Json::Null) => Ok(Some(None)),
        Some(Json::Bool(value)) => Ok(Some(Some(*value))),
        Some(_) => Err(bad(cmd, format!("`{key}`: expected true, false or null"))),
    }
}

pub(super) fn slide_set_player_overrides(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_player_overrides";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let map = obj(C, p)?;
    let next_requires: Option<Option<coursecraft_course::NextRequirement>> = match map
        .get("next_requires")
    {
        None => None,
        Some(Json::Null) => Some(None),
        Some(v) => {
            let rule: coursecraft_course::NextRequirement = serde_json::from_value(v.clone())
                .map_err(|e| bad(C, format!("`next_requires`: {e}")))?;
            course::check_next_requirement(s, C, &rule)?;
            Some(Some(rule))
        }
    };
    let flags = [
        flag_override(C, map, "show_bar")?,
        flag_override(C, map, "show_nav")?,
        flag_override(C, map, "show_seekbar")?,
        flag_override(C, map, "show_menu")?,
    ];
    let sl = slide_mut(s, &sid)?;
    let mut overrides = sl.player.take().unwrap_or_default();
    if let Some(rule) = next_requires {
        overrides.next_requires = rule;
    }
    if let Some(value) = flags[0] {
        overrides.show_bar = value;
    }
    if let Some(value) = flags[1] {
        overrides.show_nav = value;
    }
    if let Some(value) = flags[2] {
        overrides.show_seekbar = value;
    }
    if let Some(value) = flags[3] {
        overrides.show_menu = value;
    }
    sl.player = if overrides == coursecraft_course::SlidePlayerOverrides::default() {
        None
    } else {
        Some(overrides)
    };
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- masters

pub(super) fn master_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "master.add";
    let was_valid = s.is_playable();
    let id = fresh_id("master", s.course.all_slide_ids());
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| format!("Master {}", s.course.masters.len() + 1)),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.masters.push(Slide {
        id: id.clone(),
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn master_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "master.delete";
    let was_valid = s.is_playable();
    let id: String = req(C, p, "master")?;
    let pos = s
        .course
        .masters
        .iter()
        .position(|master| master.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("master {id:?} does not exist")))?;
    s.course.masters.remove(pos);
    for scene in &mut s.course.scenes {
        for slide in &mut scene.slides {
            if slide.master.as_deref() == Some(id.as_str()) {
                slide.master = None;
            }
        }
    }
    for template in &mut s.course.templates {
        if template.master.as_deref() == Some(id.as_str()) {
            template.master = None;
        }
    }
    for bank in &mut s.course.banks {
        for slide in &mut bank.slides {
            if slide.master.as_deref() == Some(id.as_str()) {
                slide.master = None;
            }
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn slide_set_master(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_master";
    let was_valid = s.is_playable();
    let slides: Vec<String> = req(C, p, "slides")?;
    let master: Option<String> = opt(C, p, "master")?;
    if slides.is_empty() {
        return Err(bad(C, "`slides` is empty"));
    }
    if let Some(ref master) = master
        && s.course.master(master).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "master {master:?} does not exist"
        )));
    }
    // Scene slides and question-bank slides take a master; masters and templates do not.
    for id in &slides {
        if s.course.play_slide(id).is_none() {
            return Err(EngineError::NotFound(format!(
                "slide {id:?} does not exist"
            )));
        }
    }
    let banks = s
        .course
        .banks
        .iter_mut()
        .flat_map(|bank| bank.slides.iter_mut());
    for slide in s
        .course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .chain(banks)
    {
        if slides.iter().any(|id| id == &slide.id) {
            slide.master = master.clone();
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- banks and draws

fn bank_mut<'a>(s: &'a mut Session, id: &str) -> Result<&'a mut QuestionBank> {
    s.course
        .banks
        .iter_mut()
        .find(|bank| bank.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {id:?} does not exist")))
}

/// Total number of questions drawn from `bank_id` across all scene slides.
fn draw_total_for(s: &Session, bank_id: &str) -> usize {
    s.course
        .slides()
        .filter_map(|slide| slide.draw.as_ref())
        .filter(|draw| draw.bank == bank_id)
        .map(|draw| draw.count as usize)
        .sum()
}

pub(super) fn bank_create(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.create";
    let was_valid = s.is_playable();
    let id = fresh_id("bank", s.course.banks.iter().map(|bank| bank.id.as_str()));
    let title = text(
        C,
        opt(C, p, "title")?
            .unwrap_or_else(|| format!("Question Bank {}", s.course.banks.len() + 1)),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.banks.push(QuestionBank {
        id: id.clone(),
        title,
        slides: Vec::new(),
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn bank_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.update";
    let was_valid = s.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let title: Option<String> = opt(C, p, "title")?;
    let bank = bank_mut(s, &bank_id)?;
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        bank.title = t;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn bank_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.delete";
    let was_valid = s.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let pos = s
        .course
        .banks
        .iter()
        .position(|bank| bank.id == bank_id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    if s.course
        .slides()
        .any(|slide| slide.draw.as_ref().is_some_and(|draw| draw.bank == bank_id))
    {
        return Err(EngineError::Invalid(format!(
            "bank {bank_id:?} is drawn from; remove those draws first"
        )));
    }
    s.course.banks.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn next_fresh_id(prefix: &str, taken: &mut BTreeSet<String>, next: &mut u64) -> String {
    loop {
        let id = format!("{prefix}{}", *next);
        *next = next.saturating_add(1);
        if taken.insert(id.clone()) {
            return id;
        }
    }
}

/// A question slide as `question.insert` builds it, ready to put in a bank.
/// `customise` edits the question before the feedback layers are laid out.
fn new_bank_slide(
    course: &Course,
    kind: QuestionKind,
    title: String,
    customise: &dyn Fn(&mut Question),
) -> Result<Slide> {
    let id = fresh_id("slide", course.all_slide_ids());
    new_bank_slide_with_id(&course.theme, course.slide_size, id, kind, title, customise)
}

pub(super) fn new_bank_slide_with_id(
    theme: &Theme,
    slide_size: Size,
    id: String,
    kind: QuestionKind,
    title: String,
    customise: &dyn Fn(&mut Question),
) -> Result<Slide> {
    let mut slide = Slide {
        id,
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };
    ensure_question_support(&mut slide, kind, theme);
    let mut question = default_multiple_choice_question();
    apply_question_kind(&slide.objects, &mut question, kind)?;
    customise(&mut question);
    slide.question = Some(question);
    add_feedback_layers(&mut slide, theme, slide_size);
    add_submit_button(&mut slide, theme, slide_size);
    Ok(slide)
}

pub(super) fn bank_add_question(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.add_question";
    let was_valid = s.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let kind: QuestionKind = opt(C, p, "kind")?.unwrap_or(QuestionKind::MultipleChoice);
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| "Untitled".to_owned()),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    let bank_pos = s
        .course
        .banks
        .iter()
        .position(|bank| bank.id == bank_id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    let slide = new_bank_slide(&s.course, kind, title, &|_| {})?;
    let id = slide.id.clone();
    let bank = s
        .course
        .banks
        .get_mut(bank_pos)
        .ok_or_else(|| EngineError::Invalid("bank index out of range".into()))?;
    bank.slides.push(slide);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn bank_move_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.move_slide";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id: String = req(C, p, "bank")?;
    let index: Option<usize> = opt(C, p, "index")?;
    if s.course.bank(&bank_id).is_none() {
        return Err(EngineError::NotFound(format!(
            "bank {bank_id:?} does not exist"
        )));
    }
    let mut found = None;
    for scene in &mut s.course.scenes {
        if let Some(pos) = scene.slides.iter().position(|slide| slide.id == sid) {
            let slide = scene.slides.remove(pos);
            found = Some(slide);
            break;
        }
    }
    let slide =
        found.ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    if slide.question.is_none() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} has no question; only question slides can move into a bank"
        )));
    }
    if slide.draw.is_some() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} is a draw slide; it cannot move into a bank"
        )));
    }
    let bank = bank_mut(s, &bank_id)?;
    let at = index.unwrap_or(bank.slides.len()).min(bank.slides.len());
    bank.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn bank_remove_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.remove_slide";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id = s
        .course
        .banks
        .iter()
        .find(|bank| bank.slides.iter().any(|slide| slide.id == sid))
        .map(|bank| bank.id.clone())
        .ok_or_else(|| EngineError::NotFound(format!("bank slide {sid:?} does not exist")))?;
    let bank_size = s.course.bank(&bank_id).map_or(0, |bank| bank.slides.len());
    let remaining = bank_size.saturating_sub(1);
    let drawn = draw_total_for(s, &bank_id);
    if drawn > remaining {
        return Err(EngineError::Invalid(format!(
            "removing bank slide {sid:?} would leave draws asking for {drawn} questions from a bank of {remaining}"
        )));
    }
    let bank = bank_mut(s, &bank_id)?;
    bank.slides.retain(|slide| slide.id != sid);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn draw_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "draw.insert";
    let was_valid = s.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let count: u32 = req(C, p, "count")?;
    let scene_id: Option<String> = opt(C, p, "scene")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let bank_title = s
        .course
        .bank(&bank_id)
        .map(|bank| bank.title.clone())
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    if count == 0 {
        return Err(bad(C, "`count` must be at least 1"));
    }
    let id = fresh_id("slide", s.course.all_slide_ids());
    let slide = Slide {
        id: id.clone(),
        title: bank_title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: Some(BankDraw {
            bank: bank_id.clone(),
            count,
        }),
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };
    let scene_pos = match &scene_id {
        Some(scene) => s
            .course
            .scenes
            .iter()
            .position(|x| x.id == *scene)
            .ok_or_else(|| EngineError::NotFound(format!("scene {scene:?} does not exist")))?,
        None => s
            .course
            .scenes
            .len()
            .checked_sub(1)
            .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
    };
    let scene = s
        .course
        .scenes
        .get_mut(scene_pos)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    let at = index.unwrap_or(scene.slides.len()).min(scene.slides.len());
    scene.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn draw_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "draw.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id: Option<String> = opt(C, p, "bank")?;
    let count: Option<u32> = opt(C, p, "count")?;
    if let Some(bank) = &bank_id
        && s.course.bank(bank).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "bank {bank:?} does not exist"
        )));
    }
    if let Some(count) = count
        && count == 0
    {
        return Err(bad(C, "`count` must be at least 1"));
    }
    let slide = slide_mut(s, &sid)?;
    let draw = slide
        .draw
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} is not a draw slide")))?;
    if let Some(bank) = bank_id {
        draw.bank = bank;
    }
    if let Some(count) = count {
        draw.count = count;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- layouts

const LAYOUT_MARGIN: f32 = 40.0;

fn layout_rect(x: f32, y: f32, w: f32, h: f32, size: Size) -> Rect {
    let slide_w = size.w.max(1) as f32;
    let slide_h = size.h.max(1) as f32;
    let x = if x.is_finite() {
        x.clamp(0.0, (slide_w - 1.0).max(0.0))
    } else {
        0.0
    };
    let y = if y.is_finite() {
        y.clamp(0.0, (slide_h - 1.0).max(0.0))
    } else {
        0.0
    };
    let w = if w.is_finite() { w } else { 1.0 };
    let h = if h.is_finite() { h } else { 1.0 };
    Rect {
        x,
        y,
        w: w.max(1.0).min((slide_w - x).max(1.0)),
        h: h.max(1.0).min((slide_h - y).max(1.0)),
    }
}

fn layout_text(
    id: String,
    name: &str,
    text: String,
    size: f32,
    frame: Rect,
    theme: &Theme,
    heading: bool,
) -> Object {
    Object {
        id,
        name: name.to_owned(),
        kind: ObjectKind::Text {
            text,
            style: coursecraft_course::TextStyle {
                font: if heading {
                    theme.heading_font_or_body().to_owned()
                } else {
                    theme.font.clone()
                },
                size,
                color: theme.text.clone(),
                ..coursecraft_course::TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
            outline: None,
            point_text: false,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    }
}

/// What a layout placeholder is for. The layout chooses frames; this says which
/// placeholder the importer fills with an outline slide's title and body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutRole {
    Title,
    Content,
    /// Decorative or media placeholder, never filled with text.
    Media,
}

/// One placeholder of a starting layout: an object id prefix, the object name,
/// the role, and the frame.
struct Placeholder {
    prefix: &'static str,
    name: &'static str,
    role: LayoutRole,
    frame: Rect,
}

const PLACEHOLDER_TITLE: &str = "Click to add a title";
const PLACEHOLDER_TEXT: &str = "Click to add text";

/// The full list of layout names `slide.add_from_layout` accepts.
const LAYOUTS: [&str; 6] = [
    "blank",
    "title",
    "title_content",
    "two_columns",
    "picture_text",
    "question",
];

/// Placeholders of a starting layout, all inside the slide.
fn layout_placeholders(layout: &str, size: Size) -> Vec<Placeholder> {
    let slide_w = size.w.max(1) as f32;
    let slide_h = size.h.max(1) as f32;
    let left = LAYOUT_MARGIN.min(slide_w * 0.4);
    let top = LAYOUT_MARGIN.min(slide_h * 0.4);
    let inner_w = (slide_w - left * 2.0).max(1.0);
    let inner_h = (slide_h - top * 2.0).max(1.0);
    let gap = (inner_h * 0.04).clamp(8.0, 24.0).min(inner_h * 0.25);
    let title_h = (inner_h * 0.22).clamp(24.0, 90.0).min(inner_h);
    let body_y = top + title_h + gap;
    let body_h = (top + inner_h - body_y).max(1.0);
    let title = |y: f32| Placeholder {
        prefix: "text",
        name: "Title",
        role: LayoutRole::Title,
        frame: layout_rect(left, y, inner_w, title_h, size),
    };
    let content = |x: f32, w: f32, name: &'static str| Placeholder {
        prefix: "text",
        name,
        role: LayoutRole::Content,
        frame: layout_rect(x, body_y, w, body_h, size),
    };

    match layout {
        "title" => {
            let title_y = top + (inner_h - title_h).max(0.0) * 0.5;
            vec![title(title_y)]
        }
        "title_content" => vec![title(top), content(left, inner_w, "Text")],
        "two_columns" => {
            let col_gap = (inner_w * 0.04).clamp(8.0, 32.0).min(inner_w * 0.2);
            let col_w = ((inner_w - col_gap) * 0.5).max(1.0);
            vec![
                title(top),
                content(left, col_w, "Text"),
                content(left + col_w + col_gap, col_w, "Text 2"),
            ]
        }
        "picture_text" => {
            let col_gap = (inner_w * 0.04).clamp(8.0, 32.0).min(inner_w * 0.2);
            let image_w = ((inner_w - col_gap) * 0.45).max(1.0);
            let text_w = (inner_w - image_w - col_gap).max(1.0);
            vec![
                title(top),
                Placeholder {
                    prefix: "image",
                    name: "Picture",
                    role: LayoutRole::Media,
                    frame: layout_rect(left, body_y, image_w, body_h, size),
                },
                content(left + image_w + col_gap, text_w, "Text"),
            ]
        }
        _ => Vec::new(),
    }
}

/// Font size a layout placeholder starts with, by role.
fn layout_font_size(role: LayoutRole, frame: Rect) -> f32 {
    match role {
        LayoutRole::Title => 40.0_f32.min(frame.h.max(8.0)),
        _ => 20.0_f32,
    }
}

/// Objects for one of the six starting layouts, all inside the slide.
fn layout_objects(layout: &str, slide: &Slide, size: Size, theme: &Theme) -> Vec<Object> {
    let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
    let mut next = |prefix: &str| {
        let id = fresh_id_in(prefix, &taken);
        taken.insert(id.clone());
        id
    };
    layout_placeholders(layout, size)
        .into_iter()
        .map(|p| match p.role {
            LayoutRole::Media => Object {
                id: next(p.prefix),
                name: p.name.to_owned(),
                kind: ObjectKind::Image {
                    src: "placeholder.png".to_owned(),
                    alt: "Choose an image".to_owned(),
                    crop: None,
                    outline: None,
                    shape: None,
                },
                frame: p.frame,
                timing: Timing::default(),
                states: Vec::new(),
                initial_state: "normal".into(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                emphasis: Vec::new(),
                focus_order: None,
                decorative: false,
                description: None,
                group: None,
                panel: None,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
                drag: None,
                locked: false,
                editor_hidden: false,
            },
            role => layout_text(
                next(p.prefix),
                p.name,
                if role == LayoutRole::Title {
                    PLACEHOLDER_TITLE
                } else {
                    PLACEHOLDER_TEXT
                }
                .to_owned(),
                layout_font_size(role, p.frame),
                p.frame,
                theme,
                role == LayoutRole::Title,
            ),
        })
        .collect()
}

pub(super) fn slide_add_from_layout(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.add_from_layout";
    let was_valid = s.is_playable();
    let layout: String = req(C, p, "layout")?;
    if !LAYOUTS.contains(&layout.as_str()) {
        return Err(bad(
            C,
            "`layout` must be blank, title, title_content, two_columns, picture_text or question",
        ));
    }
    let after: Option<String> = opt(C, p, "after")?;
    let scene: Option<String> = opt(C, p, "scene")?;
    let master: Option<String> = opt(C, p, "master")?;
    if let Some(ref master) = master
        && s.course.master(master).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "master {master:?} does not exist"
        )));
    }
    let title = format!("Slide {}", s.course.slides().count().saturating_add(1));
    let mut params = json!({ "title": title });
    if let Some(ref after) = after {
        params["after"] = json!(after);
    }
    if let Some(ref scene) = scene {
        params["scene"] = json!(scene);
    }
    let added = slide_add(s, &params)?;
    let id = added
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| EngineError::Invalid("new slide has no id".into()))?
        .to_owned();
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    let new_id = {
        let slide = s
            .course
            .slide_mut(&id)
            .ok_or_else(|| EngineError::Invalid("new slide is missing".into()))?;
        if let Some(ref master) = master {
            slide.master = Some(master.clone());
        }
        let objects = layout_objects(&layout, slide, size, &theme);
        slide.objects.extend(objects);
        id.clone()
    };
    if layout == "question" {
        question_insert(s, &json!({ "slide": new_id, "kind": "multiple_choice" }))?;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

// ---------------------------------------------------------------- outline import

/// Turn an outline into slides in one undo step.
///
/// Everything happens here rather than by calling `scene.add` / `slide.add` /
/// `question.insert` so the whole import is a single `Session::execute` call:
/// one undo step, and any failure leaves the course exactly as it was (the
/// session also rolls back, but nothing is per-command undoable here).
pub(super) fn course_import_outline(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.import_outline";
    let was_valid = s.is_playable();
    let outline: CourseOutline = req(C, p, "outline")?;
    let after: Option<String> = opt(C, p, "after")?;
    if let Some(ref after) = after
        && s.course.play_slide(after).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "slide {after:?} does not exist"
        )));
    }
    let problems = outline.check();
    if !problems.is_empty() {
        return Err(EngineError::Invalid(describe_problems(&problems)));
    }
    // A checked outline with no slides is the dialog's empty field: nothing to
    // add, so no undo step rather than an empty one.
    if outline.slides().next().is_none() {
        s.skip_undo = true;
        return Ok(json!({ "scenes": 0, "slides": 0, "first_slide": null }));
    }

    let scene_at = insertion_point(s, after.as_deref())?;
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    // New slides take the master of the slide they follow: the last slide of
    // the scene just before the insertion point. With no `after`, that is the
    // course's last slide.
    let anchor_master = s
        .course
        .scenes
        .iter()
        .take(scene_at)
        .rev()
        .find_map(|scene| scene.slides.last())
        .and_then(|slide| slide.master.clone());

    let mut imported_scenes = 0usize;
    let mut imported_slides = 0usize;
    let mut first_slide: Option<String> = None;
    let mut taken: BTreeSet<String> = s.course.all_slide_ids().map(str::to_owned).collect();
    let mut taken_scenes: BTreeSet<String> =
        s.course.scenes.iter().map(|sc| sc.id.clone()).collect();
    let mut new_scenes = Vec::with_capacity(outline.scenes.len());
    for scene in &outline.scenes {
        let mut slides = Vec::with_capacity(scene.slides.len());
        for os in &scene.slides {
            let id = fresh_id_in("slide", &taken);
            taken.insert(id.clone());
            if first_slide.is_none() {
                first_slide = Some(id.clone());
            }
            slides.push(outline_slide(os, &id, &theme, size, anchor_master.clone()));
            imported_slides += 1;
        }
        if slides.is_empty() {
            continue;
        }
        imported_scenes += 1;
        let scene_id = fresh_id_in("scene", &taken_scenes);
        taken_scenes.insert(scene_id.clone());
        new_scenes.push(Scene {
            id: scene_id,
            title: scene.title.clone(),
            slides,
        });
    }
    let pos = scene_at.min(s.course.scenes.len());
    s.course.scenes.splice(pos..pos, new_scenes);
    keep_valid(s, was_valid)?;
    Ok(json!({
        "scenes": imported_scenes,
        "slides": imported_slides,
        "first_slide": first_slide,
    }))
}

/// One sentence listing the first few outline problems, then a count of the
/// rest. Outline problems carry no line numbers once parsed from JSON, so the
/// slide's own title says where to look.
fn describe_problems(problems: &[OutlineProblem]) -> String {
    const SHOWN: usize = 5;
    let mut message = match problems.len() {
        1 => "The outline has a problem: ".to_owned(),
        n => format!("The outline has {n} problems: "),
    };
    let shown: Vec<String> = problems
        .iter()
        .take(SHOWN)
        .map(|p| p.message.clone())
        .collect();
    message.push_str(&shown.join("; "));
    if problems.len() > SHOWN {
        message.push_str(&format!("; and {} more", problems.len() - SHOWN));
    }
    message
}

/// Where the import goes: the scene index the new scenes start at.
///
/// With `after`, the new scenes follow the scene that contains that slide, so
/// the imported slides stay together under their outline scene headings. With
/// no `after`, they follow every existing scene.
fn insertion_point(s: &Session, after: Option<&str>) -> Result<usize> {
    if let Some(after) = after {
        for (si, scene) in s.course.scenes.iter().enumerate() {
            if scene.slides.iter().any(|sl| sl.id == after) {
                return Ok(si + 1);
            }
        }
        return Err(EngineError::NotFound(format!(
            "slide {after:?} does not exist"
        )));
    }
    Ok(s.course.scenes.len())
}

/// Build one slide from an outline slide: the title layout or the
/// title-and-content layout, with notes and an optional graded question.
fn outline_slide(
    os: &OutlineSlide,
    id: &str,
    theme: &Theme,
    size: Size,
    master: Option<String>,
) -> Slide {
    // A question slide uses the question layout: the visible prompt is the
    // question, its body is ignored, and `question.insert`'s button and
    // feedback layers are added below.
    let layout = if os.question.is_some() {
        "question"
    } else if os.body.is_empty() {
        "title"
    } else {
        "title_content"
    };
    let mut slide = Slide {
        id: id.to_owned(),
        title: os.title.clone(),
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: os.notes.clone().unwrap_or_default(),
        master,
        revisit: RevisitMode::Auto,
        player: None,
    };
    // Fill the layout's own placeholders so the slide keeps one text object per
    // role (the title and, when there is body text, the content box).
    let placeholders = layout_placeholders(layout, size);
    let mut body_text = String::new();
    if os.question.is_none() {
        if os.body.len() >= 2 {
            for (i, line) in os.body.iter().enumerate() {
                if i > 0 {
                    body_text.push('\n');
                }
                body_text.push_str("• ");
                body_text.push_str(line);
            }
        } else if let Some(line) = os.body.first() {
            body_text.push_str(line);
        }
    }
    slide.objects = layout_objects(layout, &slide, size, theme);
    for (object, placeholder) in slide.objects.iter_mut().zip(placeholders) {
        let fill = match placeholder.role {
            LayoutRole::Title => &os.title,
            LayoutRole::Content => &body_text,
            LayoutRole::Media => continue,
        };
        if let ObjectKind::Text { text, runs, .. } = &mut object.kind {
            text.clone_from(fill);
            runs.clear();
        }
    }
    if let Some(q) = &os.question {
        slide.question = Some(outline_question(q));
        add_feedback_layers(&mut slide, theme, size);
        add_submit_button(&mut slide, theme, size);
    }
    slide
}

/// A multiple-choice question from an outline question. Choices get ids `a`,
/// `b`, `c`, … which are unique because ids only need to be unique per slide.
fn outline_question(q: &OutlineQuestion) -> Question {
    let mut base = default_multiple_choice_question();
    base.prompt = q.prompt.clone();
    base.choices = q
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| Choice {
            id: choice_id(i),
            text: c.text.clone(),
            correct: c.correct,
            points: 0,
        })
        .collect();
    base
}

/// `a`..`z`, then `a1`, `b1`, … — short, readable, unique.
fn choice_id(i: usize) -> String {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    let letter = LETTERS.get(i % LETTERS.len()).copied().unwrap_or(b'x');
    let round = i / LETTERS.len();
    let mut id = String::with_capacity(2);
    id.push(char::from(letter));
    if round > 0 {
        id.push_str(&round.to_string());
    }
    id
}

// ---------------------------------------------------------------- templates

pub(super) fn template_save(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.save";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    if s.course.master(&sid).is_some() {
        return Err(bad(C, "a master cannot be saved as a template"));
    }
    let source = s
        .course
        .play_slide(&sid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| source.title.clone()),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    let id = fresh_id("template", s.course.all_slide_ids());
    let mut template = source;
    template.id = id.clone();
    template.title = title;
    s.course.templates.push(template);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn template_add_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.add_slide";
    let was_valid = s.is_playable();
    let tid: String = req(C, p, "template")?;
    let after: Option<String> = opt(C, p, "after")?;
    let scene: Option<String> = opt(C, p, "scene")?;
    let source = s
        .course
        .template(&tid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("template {tid:?} does not exist")))?;
    let mut copy = copy_slide(&s.course, source, &tid, true);
    // A slide made from a template is a new slide, not a copy of one.
    copy.slide.title = format!("Slide {}", s.course.slides().count().saturating_add(1));
    let new_id = copy.slide.id.clone();
    insert_slide_after(s, copy.slide, scene.as_deref(), after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

pub(super) fn template_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.delete";
    let was_valid = s.is_playable();
    let tid: String = req(C, p, "template")?;
    let pos = s
        .course
        .templates
        .iter()
        .position(|template| template.id == tid)
        .ok_or_else(|| EngineError::NotFound(format!("template {tid:?} does not exist")))?;
    s.course.templates.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- copy/paste helpers

pub(super) fn kind_prefix(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Text { .. } => "text",
        ObjectKind::Shape { .. } => "shape",
        ObjectKind::Image { .. } => "image",
        ObjectKind::Audio { .. } => "audio",
        ObjectKind::Video { .. } => "video",
        ObjectKind::Web { .. } => "web",
        ObjectKind::TextEntry { .. } => "text_entry",
        ObjectKind::Slider { .. } => "slider",
        ObjectKind::Dial { .. } => "dial",
        ObjectKind::Checkbox { .. } => "checkbox",
        ObjectKind::RadioGroup { .. } => "radio",
        ObjectKind::Marker { .. } => "marker",
        ObjectKind::ScrollPanel { .. } => "panel",
        ObjectKind::Timer { .. } => "timer",
        ObjectKind::ProgressMeter { .. } => "meter",
        ObjectKind::Lottie { .. } => "lottie",
        ObjectKind::Button { .. } => "button",
        ObjectKind::Path { .. } => "path",
        ObjectKind::Table { .. } => "table",
    }
}

pub(super) fn fresh_id_in(prefix: &str, taken: &BTreeSet<String>) -> String {
    (1u64..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id))
        .unwrap_or_else(|| prefix.to_owned())
}

/// Keep at least one pixel of a pasted frame on the slide.
pub(super) fn clamp_frame(frame: &mut Rect, size: Size) {
    let slide_w = size.w as f32;
    let slide_h = size.h as f32;
    if !frame.x.is_finite() {
        frame.x = 0.0;
    }
    if !frame.y.is_finite() {
        frame.y = 0.0;
    }
    if !frame.w.is_finite() || frame.w <= 0.0 {
        frame.w = 4.0;
    }
    if !frame.h.is_finite() || frame.h <= 0.0 {
        frame.h = 4.0;
    }
    if frame.w > slide_w.max(4.0) {
        frame.w = slide_w.max(4.0);
    }
    if frame.h > slide_h.max(4.0) {
        frame.h = slide_h.max(4.0);
    }
    let min_x = 1.0 - frame.w;
    let max_x = (slide_w - 1.0).max(min_x);
    let min_y = 1.0 - frame.h;
    let max_y = (slide_h - 1.0).max(min_y);
    frame.x = frame.x.clamp(min_x, max_x);
    frame.y = frame.y.clamp(min_y, max_y);
}

struct Remap<'a> {
    objects: &'a BTreeMap<String, String>,
    layers: &'a BTreeMap<String, String>,
    course: &'a Course,
    /// `(old, new)` when duplicating or pasting a whole slide.
    self_slide: Option<(&'a str, &'a str)>,
    /// Layer that owns the trigger being rewritten.
    owner_layer: Option<&'a str>,
    /// Keep a jump target that no longer exists so validation reports it.
    allow_missing_jump: bool,
}

fn remap_object(id: &mut String, r: &Remap<'_>) -> bool {
    match r.objects.get(id) {
        Some(new_id) => {
            *id = new_id.clone();
            true
        }
        None => false,
    }
}

fn remap_layer(id: &mut String, r: &Remap<'_>) -> bool {
    match r.layers.get(id) {
        Some(new_id) => {
            *id = new_id.clone();
            true
        }
        None => false,
    }
}

/// Keep a connector's glue only when its target was copied too, remapped to the
/// new id. Otherwise clear it; the end stays where it is.
fn remap_glue(glue: &mut Option<Glue>, objects: &BTreeMap<String, String>) {
    if let Some(current) = glue.as_mut() {
        match objects.get(&current.object) {
            Some(new_id) => current.object = new_id.clone(),
            None => *glue = None,
        }
    }
}

pub(super) fn remap_object_glue(object: &mut Object, map: &BTreeMap<String, String>) {
    if let ObjectKind::Path { path } = &mut object.kind {
        remap_glue(&mut path.start_glue, map);
        remap_glue(&mut path.end_glue, map);
    }
    object.panel = object.panel.as_deref().and_then(|id| map.get(id).cloned());
    if let Some(drag) = object.drag.as_mut() {
        drag.targets = drag
            .targets
            .iter()
            .filter_map(|id| map.get(id).cloned())
            .collect();
    }
}

pub(super) fn remap_object_glues(objects: &mut [Object], map: &BTreeMap<String, String>) {
    for object in objects {
        remap_object_glue(object, map);
    }
}

fn variable_exists(course: &Course, name: &str) -> bool {
    coursecraft_course::builtin_variable_kind(name).is_some() || course.variable(name).is_some()
}

/// A variable name inside a copy. A `Timer.<id>.Remaining` variable belongs to
/// a timer on the slide, so it follows that timer to its new id (and the
/// trigger goes when the timer was not copied). Any other name must exist.
fn remap_variable_name(
    name: &mut String,
    objects: &BTreeMap<String, String>,
    course: &Course,
) -> bool {
    if let Some(timer) = coursecraft_course::timer_id_for_remaining_variable(name) {
        let Some(new_id) = objects.get(timer) else {
            return false;
        };
        *name = coursecraft_course::timer_remaining_variable(new_id);
        return true;
    }
    variable_exists(course, name)
}

fn remap_value_source(
    value: &mut coursecraft_course::ValueSource,
    objects: &BTreeMap<String, String>,
    course: &Course,
) -> bool {
    match value {
        coursecraft_course::ValueSource::Variable { variable } => {
            remap_variable_name(variable, objects, course)
        }
        coursecraft_course::ValueSource::Constant(_) => true,
    }
}

/// A media target: the course's background audio is not on any slide, so it
/// stays as it is while the course still has it.
fn remap_media_object(id: &mut String, r: &Remap<'_>) -> bool {
    if id == coursecraft_course::BACKGROUND_AUDIO_ID {
        return r.course.background_audio.is_some();
    }
    remap_object(id, r)
}

fn remap_event(event: &mut Event, r: &Remap<'_>) -> bool {
    match event {
        Event::TimelineStarts { layer }
        | Event::TimelineEnds { layer }
        | Event::TimelineReaches { layer, .. } => match layer {
            Some(id) => remap_layer(id, r),
            None => true,
        },
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object } => remap_object(object, r),
        Event::MediaEnds { object } => remap_media_object(object, r),
        Event::CueReached { object, .. } => remap_object(object, r),
        Event::StateChanges { object, .. } | Event::AnimationCompletes { object, .. } => {
            remap_object(object, r)
        }
        Event::VariableChanges { variable } => remap_variable_name(variable, r.objects, r.course),
        Event::KeyPressed { object, .. } => match object {
            Some(obj) => remap_object(obj, r),
            None => true,
        },
        Event::DroppedOn { object, target } => remap_object(object, r) && remap_object(target, r),
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            remap_object(a, r) && remap_object(b, r)
        }
        Event::TimerEnds { timer } => remap_object(timer, r),
        Event::Submit => true,
    }
}

fn remap_action(action: &mut Action, r: &Remap<'_>) -> bool {
    match action {
        Action::JumpToSlide { target } => match target {
            SlideTarget::Next | SlideTarget::Previous | SlideTarget::LastVisited => true,
            SlideTarget::Id(id) => {
                if let Some((old, new)) = r.self_slide
                    && id == old
                {
                    *id = new.to_owned();
                    return true;
                }
                r.allow_missing_jump || r.course.play_slide(id).is_some()
            }
            SlideTarget::Scene(id) => {
                r.allow_missing_jump || r.course.scenes.iter().any(|s| s.id == *id)
            }
        },
        Action::ShowLayer { layer } => remap_layer(layer, r),
        Action::HideLayer { layer } => match layer {
            Some(id) => remap_layer(id, r),
            None => r.owner_layer.is_some(),
        },
        Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object } => remap_media_object(object, r),
        Action::ChangeState { object, .. }
        | Action::StartTimer { timer: object }
        | Action::PauseTimer { timer: object }
        | Action::ResetTimer { timer: object }
        | Action::MoveOnPath { object, .. } => remap_object(object, r),
        Action::SetVariable {
            variable, value, ..
        } => {
            remap_variable_name(variable, r.objects, r.course)
                && value
                    .as_mut()
                    .is_none_or(|v| remap_value_source(v, r.objects, r.course))
        }
        Action::PauseTimeline
        | Action::ResumeTimeline
        | Action::SubmitQuestion
        | Action::SubmitResults
        | Action::RetryQuiz
        | Action::ReviewQuiz
        | Action::ExitReview
        | Action::PrintSlide
        | Action::CompleteCourse { .. }
        | Action::ExitCourse
        | Action::OpenUrl { .. }
        | Action::RunJavaScript { .. }
        | Action::CloseLightbox => true,
        Action::LightboxSlide { slide } => {
            if let Some((old, new)) = r.self_slide
                && slide == old
            {
                *slide = new.to_owned();
                return true;
            }
            r.allow_missing_jump || r.course.play_slide(slide).is_some()
        }
    }
}

fn remap_condition(condition: &mut Condition, r: &Remap<'_>) -> bool {
    match condition {
        Condition::All(list) | Condition::Any(list) => {
            list.iter_mut().all(|inner| remap_condition(inner, r))
        }
        Condition::Not(inner) => remap_condition(inner, r),
        Condition::Compare {
            variable, value, ..
        } => {
            remap_variable_name(variable, r.objects, r.course)
                && remap_value_source(value, r.objects, r.course)
        }
        Condition::ObjectState { object, .. } => remap_object(object, r),
    }
}

pub(super) fn remap_trigger(
    trigger: &mut Trigger,
    objects: &BTreeMap<String, String>,
    layers: &BTreeMap<String, String>,
    course: &Course,
    self_slide: Option<(&str, &str)>,
    owner_layer: Option<&str>,
    allow_missing_jump: bool,
) -> bool {
    let r = Remap {
        objects,
        layers,
        course,
        self_slide,
        owner_layer,
        allow_missing_jump,
    };
    if !remap_event(&mut trigger.when, &r) || !remap_action(&mut trigger.action, &r) {
        return false;
    }
    match &mut trigger.condition {
        Some(condition) => remap_condition(condition, &r),
        None => true,
    }
}

struct SlideCopy {
    slide: Slide,
    skipped: usize,
}

/// Remap self-referential `Link::Slide` links and, when `allow_missing_jump` is
/// `false`, drop `Link::Slide` targets that do not exist in `course`.
fn sanitize_copied_runs(
    runs: &mut [TextRun],
    course: &Course,
    self_pair: (&str, &str),
    allow_missing_jump: bool,
) {
    for run in runs {
        if let Some(Link::Slide { slide }) = &mut run.link {
            if slide == self_pair.0 {
                *slide = self_pair.1.to_owned();
            } else if !allow_missing_jump && course.play_slide(slide).is_none() {
                run.link = None;
            }
        }
    }
}

fn sanitize_copied_object_links(
    obj: &mut Object,
    course: &Course,
    self_pair: (&str, &str),
    allow_missing_jump: bool,
) {
    match &mut obj.kind {
        ObjectKind::Text { runs, .. } | ObjectKind::Marker { runs, .. } => {
            sanitize_copied_runs(runs, course, self_pair, allow_missing_jump);
        }
        ObjectKind::Shape {
            text: Some(shape_text),
            ..
        } => {
            sanitize_copied_runs(&mut shape_text.runs, course, self_pair, allow_missing_jump);
        }
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &mut row.cells {
                    sanitize_copied_runs(&mut cell.runs, course, self_pair, allow_missing_jump);
                }
            }
        }
        _ => {}
    }
    for state in &mut obj.states {
        if let Some(runs) = &mut state.runs {
            sanitize_copied_runs(runs, course, self_pair, allow_missing_jump);
        }
        for extra in &mut state.extras {
            match &mut extra.kind {
                coursecraft_course::StateExtraKind::Text { runs, .. } => {
                    sanitize_copied_runs(runs, course, self_pair, allow_missing_jump);
                }
                coursecraft_course::StateExtraKind::Shape {
                    text: Some(shape_text),
                    ..
                } => {
                    sanitize_copied_runs(
                        &mut shape_text.runs,
                        course,
                        self_pair,
                        allow_missing_jump,
                    );
                }
                _ => {}
            }
        }
    }
}

fn sanitize_copied_slide_links_and_player(
    slide: &mut Slide,
    course: &Course,
    self_pair: (&str, &str),
    allow_missing_jump: bool,
) {
    for obj in &mut slide.objects {
        sanitize_copied_object_links(obj, course, self_pair, allow_missing_jump);
    }
    for layer in &mut slide.layers {
        for obj in &mut layer.objects {
            sanitize_copied_object_links(obj, course, self_pair, allow_missing_jump);
        }
    }
    if !allow_missing_jump
        && let Some(overrides) = &mut slide.player
        && let Some(coursecraft_course::NextRequirement::Variable { name }) =
            &overrides.next_requires
        && coursecraft_course::builtin_variable_kind(name).is_none()
        && course.variable(name).is_none()
    {
        overrides.next_requires = None;
    }
}

fn remap_condition_targets(condition: &mut Condition, targets: &BTreeMap<String, String>) {
    match condition {
        Condition::All(list) | Condition::Any(list) => {
            for inner in list {
                remap_condition_targets(inner, targets);
            }
        }
        Condition::Not(inner) => remap_condition_targets(inner, targets),
        Condition::ObjectState { object, .. } => {
            if let Some(new_id) = targets.get(object) {
                *object = new_id.clone();
            }
        }
        Condition::Compare {
            variable, value, ..
        } => {
            // A timer variable follows its timer. A name with no copied timer
            // stays as it is, and validation reports it.
            if let Some(new_id) = coursecraft_course::timer_id_for_remaining_variable(variable)
                .and_then(|timer| targets.get(timer))
            {
                *variable = coursecraft_course::timer_remaining_variable(new_id);
            }
            if let coursecraft_course::ValueSource::Variable { variable } = value
                && let Some(new_id) = coursecraft_course::timer_id_for_remaining_variable(variable)
                    .and_then(|timer| targets.get(timer))
            {
                *variable = coursecraft_course::timer_remaining_variable(new_id);
            }
        }
    }
}

fn remap_copied_group_list(
    groups: &mut [coursecraft_course::Group],
    objects: &mut [Object],
    taken: &mut BTreeSet<String>,
    object_map: &BTreeMap<String, String>,
    trigger_targets: &mut BTreeMap<String, String>,
) {
    if groups.is_empty() {
        return;
    }
    let mut group_map: BTreeMap<String, String> = BTreeMap::new();
    for group in groups {
        let old_id = group.id.clone();
        let new_id = fresh_id_in("grp-", taken);
        taken.insert(new_id.clone());
        group.id = new_id.clone();
        group_map.insert(old_id.clone(), new_id.clone());
        if group.interactive && !object_map.contains_key(&old_id) {
            trigger_targets.insert(old_id, new_id);
        }
    }
    for object in objects {
        if let Some(new_group) = object.group.as_deref().and_then(|old| group_map.get(old)) {
            object.group = Some(new_group.clone());
        }
    }
}

fn remap_copied_slide_groups(
    slide: &mut Slide,
    taken: &mut BTreeSet<String>,
    object_map: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut trigger_targets = object_map.clone();
    if slide.groups.is_empty() && slide.layers.iter().all(|l| l.groups.is_empty()) {
        return trigger_targets;
    }
    taken.extend(super::all_group_ids(slide));
    remap_copied_group_list(
        &mut slide.groups,
        &mut slide.objects,
        taken,
        object_map,
        &mut trigger_targets,
    );
    for layer in &mut slide.layers {
        remap_copied_group_list(
            &mut layer.groups,
            &mut layer.objects,
            taken,
            object_map,
            &mut trigger_targets,
        );
    }
    trigger_targets
}

/// Give a slide, its layers and objects fresh ids, then rewrite every internal
/// reference. `self_slide` is the slide id the triggers originally used.
fn copy_slide(
    course: &Course,
    mut slide: Slide,
    self_slide: &str,
    allow_missing_jump: bool,
) -> SlideCopy {
    // Seed with the source ids so the copy gets genuinely fresh ids too.
    let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
    let mut layer_map: BTreeMap<String, String> = BTreeMap::new();
    let mut object_map: BTreeMap<String, String> = BTreeMap::new();

    for layer in &mut slide.layers {
        let new_id = fresh_id_in("layer", &taken);
        taken.insert(new_id.clone());
        layer_map.insert(layer.id.clone(), new_id.clone());
        layer.id = new_id;
    }
    for object in &mut slide.objects {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        object_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id;
    }
    for layer in &mut slide.layers {
        for object in &mut layer.objects {
            let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
            taken.insert(new_id.clone());
            object_map.insert(object.id.clone(), new_id.clone());
            object.id = new_id;
        }
    }

    let trigger_targets = remap_copied_slide_groups(&mut slide, &mut taken, &object_map);

    remap_object_glues(&mut slide.objects, &object_map);
    for layer in &mut slide.layers {
        remap_object_glues(&mut layer.objects, &object_map);
    }

    let new_slide_id = fresh_id("slide", course.all_slide_ids());
    let self_pair = (self_slide, new_slide_id.as_str());
    sanitize_copied_slide_links_and_player(&mut slide, course, self_pair, allow_missing_jump);
    let mut skipped = 0;

    let mut kept = Vec::with_capacity(slide.triggers.len());
    for mut trigger in std::mem::take(&mut slide.triggers) {
        if remap_trigger(
            &mut trigger,
            &trigger_targets,
            &layer_map,
            course,
            Some(self_pair),
            None,
            allow_missing_jump,
        ) {
            kept.push(trigger);
        } else {
            skipped += 1;
        }
    }
    slide.triggers = kept;

    for layer in &mut slide.layers {
        let owner = layer.id.clone();
        let mut kept = Vec::with_capacity(layer.triggers.len());
        for mut trigger in std::mem::take(&mut layer.triggers) {
            if remap_trigger(
                &mut trigger,
                &trigger_targets,
                &layer_map,
                course,
                Some(self_pair),
                Some(&owner),
                allow_missing_jump,
            ) {
                kept.push(trigger);
            } else {
                skipped += 1;
            }
        }
        layer.triggers = kept;
    }

    if let Some(question) = &mut slide.question {
        if let Some(layer) = &mut question.correct_layer {
            *layer = layer_map.get(layer).cloned().unwrap_or_default();
            if layer.is_empty() {
                question.correct_layer = None;
            }
        }
        if let Some(layer) = &mut question.incorrect_layer {
            *layer = layer_map.get(layer).cloned().unwrap_or_default();
            if layer.is_empty() {
                question.incorrect_layer = None;
            }
        }
        if let Some(layer) = &mut question.try_again_layer {
            *layer = layer_map.get(layer).cloned().unwrap_or_default();
            if layer.is_empty() {
                question.try_again_layer = None;
            }
        }
        if let Some(image) = &mut question.hotspot_image {
            *image = object_map.get(image).cloned().unwrap_or_default();
            if image.is_empty() {
                question.hotspot_image = None;
            }
        }
        question.drag_items.retain_mut(|drag| {
            match (object_map.get(&drag.item), object_map.get(&drag.target)) {
                (Some(item), Some(target)) => {
                    drag.item = item.clone();
                    drag.target = target.clone();
                    true
                }
                _ => false,
            }
        });
        let mut targets = Vec::with_capacity(question.drop_targets.len());
        for target in std::mem::take(&mut question.drop_targets) {
            if let Some(new_target) = object_map.get(&target)
                && !targets.contains(new_target)
            {
                targets.push(new_target.clone());
            }
        }
        question.drop_targets = targets;
        if let Some(condition) = &mut question.correct_when {
            remap_condition_targets(condition, &trigger_targets);
        }
    }

    slide.id = new_slide_id;
    slide.title = format!("{} copy", slide.title);

    SlideCopy { slide, skipped }
}

fn insert_slide_after(
    s: &mut Session,
    slide: Slide,
    scene: Option<&str>,
    after: Option<&str>,
) -> Result<()> {
    let (si, pos) = if let Some(after) = after {
        s.course
            .scenes
            .iter()
            .enumerate()
            .find_map(|(si, sc)| {
                sc.slides
                    .iter()
                    .position(|sl| sl.id == after)
                    .map(|i| (si, i + 1))
            })
            .ok_or_else(|| EngineError::NotFound(format!("slide {after:?} does not exist")))?
    } else {
        let si = match scene {
            Some(sc) => s
                .course
                .scenes
                .iter()
                .position(|x| x.id == sc)
                .ok_or_else(|| EngineError::NotFound(format!("scene {sc:?} does not exist")))?,
            None => s
                .course
                .scenes
                .len()
                .checked_sub(1)
                .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
        };
        (si, s.course.scenes.get(si).map_or(0, |sc| sc.slides.len()))
    };
    let target_scene = s
        .course
        .scenes
        .get_mut(si)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    let at = pos.min(target_scene.slides.len());
    target_scene.slides.insert(at, slide);
    Ok(())
}

/// A button with `label`, filled with the theme accent.
pub(super) fn results_button(id: String, label: &str, fill: Color, frame: Rect) -> Object {
    Object {
        id,
        name: label.to_owned(),
        kind: ObjectKind::Button {
            label: label.to_owned(),
            fill,
            fill_style: None,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    }
}

/// One outcome layer of a results slide: a message and two action buttons.
#[allow(clippy::too_many_arguments)]
fn results_layer(
    slide: &Slide,
    layer_id: &str,
    name: &str,
    message: &str,
    message_frame: Rect,
    buttons: [(&str, &str, Rect, Action); 2],
    theme: &Theme,
) -> Layer {
    let message_id = fresh_id("message", slide.local_ids());
    let mut objects = vec![layout_text(
        message_id,
        &format!("{name} message"),
        message.to_owned(),
        28.0,
        message_frame,
        theme,
        false,
    )];
    let mut triggers = Vec::new();
    for (prefix, label, frame, action) in buttons {
        let object_id = fresh_id(prefix, slide.local_ids());
        objects.push(results_button(
            object_id.clone(),
            label,
            theme.accent.clone(),
            frame,
        ));
        triggers.push(Trigger {
            action,
            when: Event::Click { object: object_id },
            condition: None,
            disabled: false,
        });
    }
    Layer {
        id: layer_id.to_owned(),
        name: name.to_owned(),
        duration_ms: 5000,
        objects,
        triggers,
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    }
}

/// A slide with the score, a Passed layer and a Failed layer.
fn build_results_slide(id: &str, theme: &Theme, size: Size, passing_percent: u8) -> Slide {
    let w = size.w.max(1) as f32;
    let h = size.h.max(1) as f32;
    let frame = |x: f32, y: f32, fw: f32, fh: f32| Rect {
        x: x * w,
        y: y * h,
        w: fw * w,
        h: fh * h,
    };
    let button_width = 160.0;
    let button_height = 48.0;
    let gap = 12.0;
    let margin = 40.0;
    let button_y = (h - margin - button_height).max(0.0);
    let first_x = (w - margin - button_width * 2.0 - gap).max(0.0);
    let second_x = first_x + button_width + gap;

    let mut slide = Slide {
        id: id.to_owned(),
        title: "Results".to_owned(),
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let mut push =
        |prefix: &str, name: &str, text: String, text_size: f32, bold: bool, frame: Rect| {
            let object_id = fresh_id(prefix, slide.local_ids());
            let mut object = layout_text(
                object_id,
                name,
                text,
                text_size,
                frame,
                theme,
                prefix == "title",
            );
            if let ObjectKind::Text { style, .. } = &mut object.kind {
                style.bold = bold;
            }
            slide.objects.push(object);
        };
    push(
        "title",
        "Results title",
        "Results".to_owned(),
        44.0,
        true,
        frame(0.08, 0.10, 0.84, 0.12),
    );
    push(
        "score",
        "Score",
        "Your score: %Course.ScorePercent%%".to_owned(),
        32.0,
        false,
        frame(0.08, 0.30, 0.84, 0.10),
    );
    push(
        "points",
        "Points",
        "Points: %Course.ScorePoints%".to_owned(),
        24.0,
        false,
        frame(0.08, 0.42, 0.84, 0.08),
    );
    push(
        "passing",
        "Passing score",
        format!("Passing score: {passing_percent}%"),
        24.0,
        false,
        frame(0.08, 0.50, 0.84, 0.08),
    );

    let message_frame = frame(0.08, 0.62, 0.84, 0.08);
    let first = Rect {
        x: first_x,
        y: button_y,
        w: button_width,
        h: button_height,
    };
    let second = Rect {
        x: second_x,
        y: button_y,
        w: button_width,
        h: button_height,
    };
    slide.layers.push(results_layer(
        &slide,
        "passed",
        "Passed",
        "Congratulations, you passed.",
        message_frame,
        [
            ("passed_review", "Review quiz", first, Action::ReviewQuiz),
            ("passed_finish", "Finish", second, Action::ExitCourse),
        ],
        theme,
    ));
    slide.layers.push(results_layer(
        &slide,
        "failed",
        "Failed",
        "You did not pass this time.",
        message_frame,
        [
            ("failed_retry", "Retry quiz", first, Action::RetryQuiz),
            ("failed_review", "Review quiz", second, Action::ReviewQuiz),
        ],
        theme,
    ));

    slide.triggers.push(Trigger {
        action: Action::SubmitResults,
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "passed".to_owned(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: Some(Condition::Compare {
            variable: "Quiz.Passed".to_owned(),
            op: CmpOp::Eq,
            value: Value::Bool(true).into(),
        }),
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "failed".to_owned(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: Some(Condition::Compare {
            variable: "Quiz.Passed".to_owned(),
            op: CmpOp::Eq,
            value: Value::Bool(false).into(),
        }),
        disabled: false,
    });
    slide
}

pub(super) fn results_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "results.insert";
    let was_valid = s.is_playable();
    let after: Option<String> = opt(C, p, "after")?;
    if !s.course.slides().any(|slide| slide.question.is_some()) {
        return Err(EngineError::Invalid(
            "the course has no question to report".into(),
        ));
    }
    let passing_percent = match s.course.quiz {
        Some(quiz) => quiz.passing_percent,
        None => {
            s.course.quiz = Some(QuizSettings {
                passing_percent: 80,
            });
            80
        }
    };
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let id = fresh_id("results", s.course.all_slide_ids());
    let slide = build_results_slide(&id, &theme, slide_size, passing_percent);
    insert_slide_after(s, slide, None, after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({
        "slide": id,
        "passed_layer": "passed",
        "failed_layer": "failed",
    }))
}

/// A centred text object for the certificate.
fn certificate_text(
    id: String,
    name: &str,
    text: String,
    size: f32,
    bold: bool,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name: name.to_owned(),
        kind: ObjectKind::Text {
            text,
            style: TextStyle {
                font: theme.font.clone(),
                size,
                color: theme.text.clone(),
                bold,
                align: Align::Center,
                valign: VAlign::Middle,
                ..TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
            outline: None,
            point_text: false,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    }
}

/// A certificate slide: a border, the learner's name, the course and a Print button.
fn build_certificate_slide(id: &str, theme: &Theme, size: Size) -> Slide {
    let w = size.w.max(1) as f32;
    let h = size.h.max(1) as f32;
    let frame = |x: f32, y: f32, fw: f32, fh: f32| Rect {
        x: x * w,
        y: y * h,
        w: fw * w,
        h: fh * h,
    };

    let mut slide = Slide {
        id: id.to_owned(),
        title: "Certificate".to_owned(),
        duration_ms: 5000,
        background: Some(theme.background.clone()),
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let border_id = fresh_id("border", slide.local_ids());
    slide.objects.push(Object {
        id: border_id,
        name: "Border".to_owned(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::RoundedRectangle,
            fill: Color("#00000000".to_owned()),
            fill_style: None,
            outline: Some(Outline {
                color: theme.accent.clone(),
                width: 6.0,
                dash: DashStyle::Solid,
                dash_pattern: None,
                join: Default::default(),
                miter_limit: None,
            }),
            text: None,
            adjust: Vec::new(),
        },
        frame: frame(0.05, 0.05, 0.90, 0.90),
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: true,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    });

    let mut push = |prefix: &str, name: &str, text: String, size: f32, bold: bool, frame: Rect| {
        let text_id = fresh_id(prefix, slide.local_ids());
        slide.objects.push(certificate_text(
            text_id, name, text, size, bold, frame, theme,
        ));
    };
    push(
        "title",
        "Certificate title",
        "Certificate of Completion".to_owned(),
        44.0,
        true,
        frame(0.10, 0.14, 0.80, 0.12),
    );
    push(
        "certifies",
        "Certifies",
        "This certifies that".to_owned(),
        22.0,
        false,
        frame(0.10, 0.30, 0.80, 0.06),
    );
    push(
        "learner",
        "Learner name",
        "%Learner.Name%".to_owned(),
        36.0,
        true,
        frame(0.10, 0.38, 0.80, 0.07),
    );
    push(
        "completed",
        "Has completed",
        "has completed".to_owned(),
        22.0,
        false,
        frame(0.10, 0.50, 0.80, 0.06),
    );
    push(
        "course",
        "Course title",
        "%Course.Title%".to_owned(),
        30.0,
        true,
        frame(0.10, 0.57, 0.80, 0.07),
    );
    push(
        "score",
        "Score",
        "Score: %Course.ScorePercent%%".to_owned(),
        22.0,
        false,
        frame(0.10, 0.68, 0.80, 0.06),
    );
    push(
        "date",
        "Date",
        "%Course.Date%".to_owned(),
        20.0,
        false,
        frame(0.10, 0.76, 0.80, 0.05),
    );

    let margin = 40.0;
    let button_id = fresh_id("print", slide.local_ids());
    slide.objects.push(results_button(
        button_id.clone(),
        "Print",
        theme.accent.clone(),
        Rect {
            x: (w - margin - 160.0).max(0.0),
            y: (h - margin - 48.0).max(0.0),
            w: 160.0,
            h: 48.0,
        },
    ));
    slide.triggers.push(Trigger {
        action: Action::PrintSlide,
        when: Event::Click { object: button_id },
        condition: None,
        disabled: false,
    });
    slide
}

pub(super) fn certificate_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "certificate.insert";
    let was_valid = s.is_playable();
    let after: Option<String> = opt(C, p, "after")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let id = fresh_id("certificate", s.course.all_slide_ids());
    let slide = build_certificate_slide(&id, &theme, slide_size);
    insert_slide_after(s, slide, None, after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "slide": id }))
}
