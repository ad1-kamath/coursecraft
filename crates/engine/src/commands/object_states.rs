//! Object state, state extra, delete, animation, and motion-path commands.

use std::collections::BTreeSet;

use coursecraft_course::{
    Action, Animation, Condition, Easing, Event, MotionPath, ObjectKind, Point, Slide, StateDef,
    Trigger,
};
use serde_json::{Value as Json, json};

use super::{bad, find_object_mut, keep_valid, obj, opt, req, slide_mut};
use crate::{EngineError, Result, Session};

fn validate_state_name(cmd: &str, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(bad(cmd, "state name cannot be empty"));
    }
    if trimmed.chars().count() > coursecraft_course::MAX_STATE_NAME_CHARS {
        return Err(bad(
            cmd,
            format!(
                "state name cannot exceed {} characters",
                coursecraft_course::MAX_STATE_NAME_CHARS
            ),
        ));
    }
    if trimmed == "normal" {
        return Err(bad(cmd, "`normal` is built in and cannot be redefined"));
    }
    if trimmed == "hidden" {
        return Err(bad(
            cmd,
            "`hidden` is built in and needs no state definition",
        ));
    }
    Ok(trimmed.to_owned())
}

fn refuse_master_states(cmd: &str, s: &Session, sid: &str) -> Result<()> {
    if s.course.master(sid).is_some() {
        return Err(bad(cmd, "master objects cannot have states"));
    }
    Ok(())
}

fn next_copy_state_name(base: &str, existing: &[StateDef]) -> String {
    let taken: BTreeSet<&str> = existing.iter().map(|s| s.name.as_str()).collect();
    let max_chars = coursecraft_course::MAX_STATE_NAME_CHARS;
    for n in 1usize.. {
        let suffix = if n == 1 {
            " copy".to_owned()
        } else {
            format!(" copy {n}")
        };
        let suffix_chars = suffix.chars().count();
        let max_base = max_chars.saturating_sub(suffix_chars);
        let prefix: String = base.chars().take(max_base).collect();
        let candidate = format!("{prefix}{suffix}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{base} copy")
}

fn rewrite_state_in_condition(cond: &mut Condition, oid: &str, from: &str, to: &str) {
    match cond {
        Condition::All(list) | Condition::Any(list) => {
            for item in list {
                rewrite_state_in_condition(item, oid, from, to);
            }
        }
        Condition::Not(inner) => rewrite_state_in_condition(inner, oid, from, to),
        Condition::ObjectState { object, state, .. } if object == oid && state == from => {
            *state = to.to_owned();
        }
        Condition::ObjectState { .. } | Condition::Compare { .. } => {}
    }
}

fn rewrite_state_in_triggers(triggers: &mut [Trigger], oid: &str, from: &str, to: &str) {
    for trigger in triggers {
        if let Event::StateChanges {
            object,
            to: Some(state),
        } = &mut trigger.when
            && object == oid
            && state == from
        {
            *state = to.to_owned();
        }
        if let Action::ChangeState { object, state } = &mut trigger.action
            && object == oid
            && state == from
        {
            *state = to.to_owned();
        }
        if let Some(cond) = &mut trigger.condition {
            rewrite_state_in_condition(cond, oid, from, to);
        }
    }
}

pub(super) fn object_add_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let mut state: StateDef = req(C, p, "state")?;
    refuse_master_states(C, s, &sid)?;
    state.name = validate_state_name(C, &state.name)?;
    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    if obj.states.len() >= coursecraft_course::MAX_STATES_PER_OBJECT {
        return Err(bad(
            C,
            format!(
                "an object may have at most {} states",
                coursecraft_course::MAX_STATES_PER_OBJECT
            ),
        ));
    }
    if obj
        .states
        .iter()
        .any(|existing| existing.name == state.name)
    {
        return Err(bad(C, format!("state {:?} already exists", state.name)));
    }
    obj.states.push(state);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_update_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state: StateDef = req(C, p, "state")?;
    refuse_master_states(C, s, &sid)?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    let target = obj
        .states
        .iter_mut()
        .find(|s| s.name == state.name)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "state {:?} not found on object {oid:?}",
                state.name
            ))
        })?;
    *target = state;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn patch_opt<T: serde::de::DeserializeOwned>(
    cmd: &str,
    map: &serde_json::Map<String, Json>,
    key: &str,
    dst: &mut Option<T>,
) -> Result<()> {
    let Some(val) = map.get(key) else {
        return Ok(());
    };
    if val.is_null() {
        *dst = None;
    } else {
        *dst = Some(
            serde_json::from_value(val.clone())
                .map_err(|e| EngineError::Invalid(format!("{cmd}: invalid {key}: {e}")))?,
        );
    }
    Ok(())
}

fn patch_nullable<T: serde::de::DeserializeOwned>(
    cmd: &str,
    map: &serde_json::Map<String, Json>,
    key: &str,
    clear_key: &str,
    dst: &mut Option<T>,
    clear_dst: &mut bool,
) -> Result<()> {
    if let Some(val) = map.get(key) {
        if val.is_null() {
            *dst = None;
            *clear_dst = false;
        } else {
            *dst = Some(
                serde_json::from_value(val.clone())
                    .map_err(|e| EngineError::Invalid(format!("{cmd}: invalid {key}: {e}")))?,
            );
            *clear_dst = false;
        }
    }
    if let Some(clear_val) = map.get(clear_key) {
        let flag = clear_val
            .as_bool()
            .ok_or_else(|| EngineError::Invalid(format!("{cmd}: {clear_key} must be a boolean")))?;
        *clear_dst = flag;
        if flag {
            *dst = None;
        }
    }
    Ok(())
}

pub(super) fn object_patch_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.patch_state";
    const ALLOWED_KEYS: &[&str] = &[
        "fill",
        "text",
        "src",
        "fill_style",
        "clear_fill_style",
        "outline",
        "clear_outline",
        "text_style",
        "runs",
        "paragraphs",
        "autofit",
        "opacity",
        "shadow",
        "clear_shadow",
        "glow",
        "clear_glow",
        "crop",
        "clear_crop",
        "image_shape",
        "clear_image_shape",
        "rotation",
        "flip_h",
        "flip_v",
    ];
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: String = req(C, p, "name")?;
    let reset_all: bool = opt(C, p, "reset_all")?.unwrap_or(false);
    let patch_val: Option<Json> = opt(C, p, "patch")?;
    let empty_map = serde_json::Map::new();
    let patch_map = match &patch_val {
        Some(v) => v
            .as_object()
            .ok_or_else(|| EngineError::Invalid(format!("{C}: patch must be a JSON object")))?,
        None => &empty_map,
    };

    for k in patch_map.keys() {
        if !ALLOWED_KEYS.contains(&k.as_str()) {
            return Err(EngineError::Invalid(format!(
                "{C}: unknown patch field {k:?}"
            )));
        }
    }
    for (field_key, clear_key) in [
        ("fill_style", "clear_fill_style"),
        ("outline", "clear_outline"),
        ("shadow", "clear_shadow"),
        ("glow", "clear_glow"),
        ("crop", "clear_crop"),
        ("image_shape", "clear_image_shape"),
    ] {
        let sets_val = patch_map.get(field_key).is_some_and(|v| !v.is_null());
        let sets_clear = patch_map.get(clear_key).and_then(Json::as_bool) == Some(true);
        if sets_val && sets_clear {
            return Err(EngineError::Invalid(format!(
                "{C}: cannot set both {field_key} and {clear_key}: true"
            )));
        }
    }

    refuse_master_states(C, s, &sid)?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    let target = obj
        .states
        .iter_mut()
        .find(|st| st.name == name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {name:?} not found on object {oid:?}"))
        })?;

    if reset_all {
        let extras = std::mem::take(&mut target.extras);
        *target = StateDef {
            name: target.name.clone(),
            extras,
            ..Default::default()
        };
    }

    patch_opt(C, patch_map, "fill", &mut target.fill)?;
    patch_opt(C, patch_map, "text", &mut target.text)?;
    patch_opt(C, patch_map, "src", &mut target.src)?;
    patch_nullable(
        C,
        patch_map,
        "fill_style",
        "clear_fill_style",
        &mut target.fill_style,
        &mut target.clear_fill_style,
    )?;
    patch_nullable(
        C,
        patch_map,
        "outline",
        "clear_outline",
        &mut target.outline,
        &mut target.clear_outline,
    )?;
    patch_opt(C, patch_map, "text_style", &mut target.text_style)?;
    patch_opt(C, patch_map, "runs", &mut target.runs)?;
    patch_opt(C, patch_map, "paragraphs", &mut target.paragraphs)?;
    patch_opt(C, patch_map, "autofit", &mut target.autofit)?;
    patch_opt(C, patch_map, "opacity", &mut target.opacity)?;
    if let Some(op) = target.opacity {
        target.opacity = Some(coursecraft_course::clamp_opacity(op));
    }
    patch_nullable(
        C,
        patch_map,
        "shadow",
        "clear_shadow",
        &mut target.shadow,
        &mut target.clear_shadow,
    )?;
    patch_nullable(
        C,
        patch_map,
        "glow",
        "clear_glow",
        &mut target.glow,
        &mut target.clear_glow,
    )?;
    patch_nullable(
        C,
        patch_map,
        "crop",
        "clear_crop",
        &mut target.crop,
        &mut target.clear_crop,
    )?;
    patch_nullable(
        C,
        patch_map,
        "image_shape",
        "clear_image_shape",
        &mut target.image_shape,
        &mut target.clear_image_shape,
    )?;
    patch_opt(C, patch_map, "rotation", &mut target.rotation)?;
    if let Some(rot) = target.rotation {
        target.rotation = Some(coursecraft_course::normalize_rotation(rot));
    }
    patch_opt(C, patch_map, "flip_h", &mut target.flip_h)?;
    patch_opt(C, patch_map, "flip_v", &mut target.flip_v)?;

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_delete_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.delete_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: String = req(C, p, "name")?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    let before = obj.states.len();
    obj.states.retain(|s| s.name != name);
    if obj.states.len() == before {
        return Err(EngineError::NotFound(format!(
            "state {name:?} not found on object {oid:?}"
        )));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_duplicate_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.duplicate_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: String = req(C, p, "name")?;
    let new_name: Option<String> = opt(C, p, "new_name")?;
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    if obj.states.len() >= coursecraft_course::MAX_STATES_PER_OBJECT {
        return Err(bad(
            C,
            format!(
                "an object may have at most {} states",
                coursecraft_course::MAX_STATES_PER_OBJECT
            ),
        ));
    }
    let idx = obj
        .states
        .iter()
        .position(|st| st.name == name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {name:?} not found on object {oid:?}"))
        })?;
    let created_name = match new_name {
        Some(raw) => {
            let validated = validate_state_name(C, &raw)?;
            if obj.states.iter().any(|st| st.name == validated) {
                return Err(bad(C, format!("state {validated:?} already exists")));
            }
            validated
        }
        None => next_copy_state_name(&name, &obj.states),
    };
    let mut dup = obj.states[idx].clone();
    dup.name = created_name.clone();
    obj.states.insert(idx + 1, dup);
    keep_valid(s, was_valid)?;
    Ok(json!({ "name": created_name }))
}

pub(super) fn object_rename_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.rename_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let from: String = req(C, p, "from")?;
    let to_raw: String = req(C, p, "to")?;
    refuse_master_states(C, s, &sid)?;
    let to = validate_state_name(C, &to_raw)?;

    let slide = slide_mut(s, &sid)?;
    {
        let obj = find_object_mut(slide, &oid)?;
        if obj.states.iter().any(|st| st.name == to) {
            return Err(bad(C, format!("state {to:?} already exists")));
        }
        let target = obj
            .states
            .iter_mut()
            .find(|st| st.name == from)
            .ok_or_else(|| {
                EngineError::NotFound(format!("state {from:?} not found on object {oid:?}"))
            })?;
        target.name = to.clone();
        if obj.initial_state == from {
            obj.initial_state = to.clone();
        }
    }
    rewrite_state_in_triggers(&mut slide.triggers, &oid, &from, &to);
    for layer in &mut slide.layers {
        rewrite_state_in_triggers(&mut layer.triggers, &oid, &from, &to);
    }
    if let Some(question) = &mut slide.question
        && let Some(cond) = &mut question.correct_when
    {
        rewrite_state_in_condition(cond, &oid, &from, &to);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_reorder_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.reorder_state";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: String = req(C, p, "name")?;
    let index: usize = req(C, p, "index")?;
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let pos = obj
        .states
        .iter()
        .position(|st| st.name == name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {name:?} not found on object {oid:?}"))
        })?;
    if index >= obj.states.len() {
        return Err(bad(
            C,
            format!("index {index} out of range (0..{})", obj.states.len()),
        ));
    }
    let item = obj.states.remove(pos);
    obj.states.insert(index, item);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn next_extra_id(existing: &[coursecraft_course::StateExtra]) -> String {
    let taken: BTreeSet<&str> = existing.iter().map(|e| e.id.as_str()).collect();
    for n in 1usize.. {
        let candidate = format!("extra_{n}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    "extra_1".to_owned()
}

fn patch_req<T: serde::de::DeserializeOwned>(
    cmd: &str,
    map: &serde_json::Map<String, Json>,
    key: &str,
    dst: &mut T,
) -> Result<()> {
    let Some(val) = map.get(key) else {
        return Ok(());
    };
    *dst = serde_json::from_value(val.clone())
        .map_err(|e| EngineError::Invalid(format!("{cmd}: invalid {key}: {e}")))?;
    Ok(())
}

pub(super) fn object_state_add_extra(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.state_add_extra";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state_name: String = req(C, p, "state")?;
    let mut extra: coursecraft_course::StateExtra = req(C, p, "extra")?;
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let st = obj
        .states
        .iter_mut()
        .find(|st| st.name == state_name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {state_name:?} not found on object {oid:?}"))
        })?;
    if st.extras.len() >= coursecraft_course::MAX_STATE_EXTRAS {
        return Err(bad(
            C,
            format!(
                "a state may have at most {} extras",
                coursecraft_course::MAX_STATE_EXTRAS
            ),
        ));
    }
    if extra.id.trim().is_empty() {
        extra.id = next_extra_id(&st.extras);
    }
    if let coursecraft_course::StateExtraKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &mut extra.kind
    {
        if *point_text {
            style.wrap = false;
            *autofit = coursecraft_course::Autofit::None;
            super::text::fit_point_text_frame(
                &mut extra.frame,
                text,
                runs,
                style,
                paragraphs,
                super::text::parse_measured_size(p),
                true,
            );
        } else if *autofit == coursecraft_course::Autofit::Grow {
            super::text::fit_grow_text_frame(
                &mut extra.frame,
                text,
                runs,
                style,
                paragraphs,
                super::text::parse_measured_size(p),
                true,
            );
        }
    }
    let id = extra.id.clone();
    st.extras.push(extra);
    keep_valid(s, was_valid)?;
    Ok(serde_json::json!({ "id": id }))
}

pub(super) fn object_state_update_extra(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.state_update_extra";
    const ALLOWED_KEYS: &[&str] = &[
        "name", "frame", "kind", "rotation", "flip_h", "flip_v", "opacity", "shadow", "glow",
    ];
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state_name: String = req(C, p, "state")?;
    let extra_id: String = req(C, p, "extra")?;
    let patch_val: Json = req(C, p, "patch")?;
    let patch_map = patch_val
        .as_object()
        .ok_or_else(|| EngineError::Invalid(format!("{C}: patch must be a JSON object")))?;
    for k in patch_map.keys() {
        if !ALLOWED_KEYS.contains(&k.as_str()) {
            return Err(EngineError::Invalid(format!(
                "{C}: unknown patch field {k:?}"
            )));
        }
    }
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let st = obj
        .states
        .iter_mut()
        .find(|st| st.name == state_name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {state_name:?} not found on object {oid:?}"))
        })?;
    let target = st
        .extras
        .iter_mut()
        .find(|e| e.id == extra_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "extra {extra_id:?} not found in state {state_name:?}"
            ))
        })?;

    let old_w = target.frame.w;
    let frame_supplied = patch_map.contains_key("frame");
    let kind_supplied = patch_map.contains_key("kind");
    let measured = super::text::parse_measured_size(p);

    patch_req(C, patch_map, "name", &mut target.name)?;
    patch_req(C, patch_map, "frame", &mut target.frame)?;
    patch_req(C, patch_map, "kind", &mut target.kind)?;
    if patch_map.contains_key("rotation") {
        patch_req(C, patch_map, "rotation", &mut target.rotation)?;
        if !target.rotation.is_finite() {
            return Err(EngineError::Invalid(format!(
                "{C}: rotation must be finite"
            )));
        }
        target.rotation = coursecraft_course::normalize_rotation(target.rotation);
    }
    patch_req(C, patch_map, "flip_h", &mut target.flip_h)?;
    patch_req(C, patch_map, "flip_v", &mut target.flip_v)?;
    if patch_map.contains_key("opacity") {
        patch_req(C, patch_map, "opacity", &mut target.opacity)?;
        if !target.opacity.is_finite() {
            return Err(EngineError::Invalid(format!("{C}: opacity must be finite")));
        }
        target.opacity = coursecraft_course::clamp_opacity(target.opacity);
    }
    patch_opt(C, patch_map, "shadow", &mut target.shadow)?;
    patch_opt(C, patch_map, "glow", &mut target.glow)?;

    let width_changed = frame_supplied
        && target.frame.w.is_finite()
        && old_w.is_finite()
        && (target.frame.w - old_w).abs() > 0.5;
    let mut refit_point = false;
    let mut refit_grow = false;
    if let coursecraft_course::StateExtraKind::Text {
        style,
        autofit,
        point_text,
        ..
    } = &mut target.kind
    {
        if *point_text {
            if style.wrap || width_changed || *autofit != coursecraft_course::Autofit::None {
                *point_text = false;
                if width_changed {
                    style.wrap = true;
                }
                if *autofit == coursecraft_course::Autofit::Grow {
                    refit_grow = kind_supplied || measured.is_some();
                }
            } else {
                style.wrap = false;
                *autofit = coursecraft_course::Autofit::None;
                refit_point = kind_supplied || measured.is_some();
            }
        } else if *autofit == coursecraft_course::Autofit::Grow {
            refit_grow = kind_supplied || measured.is_some();
        }
    }
    if (refit_point || refit_grow)
        && let coursecraft_course::StateExtraKind::Text {
            text,
            style,
            runs,
            paragraphs,
            ..
        } = &target.kind
    {
        if refit_point {
            super::text::fit_point_text_frame(
                &mut target.frame,
                text,
                runs,
                style,
                paragraphs,
                measured,
                false,
            );
        } else {
            super::text::fit_grow_text_frame(
                &mut target.frame,
                text,
                runs,
                style,
                paragraphs,
                measured,
                false,
            );
        }
    }

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_state_delete_extra(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.state_delete_extra";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state_name: String = req(C, p, "state")?;
    let extra_id: String = req(C, p, "extra")?;
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let st = obj
        .states
        .iter_mut()
        .find(|st| st.name == state_name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {state_name:?} not found on object {oid:?}"))
        })?;
    let before = st.extras.len();
    st.extras.retain(|e| e.id != extra_id);
    if st.extras.len() == before {
        return Err(EngineError::NotFound(format!(
            "extra {extra_id:?} not found in state {state_name:?}"
        )));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_state_reorder_extra(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.state_reorder_extra";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state_name: String = req(C, p, "state")?;
    let extra_id: String = req(C, p, "extra")?;
    let index: usize = req(C, p, "index")?;
    refuse_master_states(C, s, &sid)?;

    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let st = obj
        .states
        .iter_mut()
        .find(|st| st.name == state_name)
        .ok_or_else(|| {
            EngineError::NotFound(format!("state {state_name:?} not found on object {oid:?}"))
        })?;
    let pos = st
        .extras
        .iter()
        .position(|e| e.id == extra_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "extra {extra_id:?} not found in state {state_name:?}"
            ))
        })?;
    if index >= st.extras.len() {
        return Err(bad(
            C,
            format!("index {index} out of range (0..{})", st.extras.len()),
        ));
    }
    let item = st.extras.remove(pos);
    st.extras.insert(index, item);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let sid: String = req("object.delete", p, "slide")?;
    let oid: String = req("object.delete", p, "object")?;
    let slide = slide_mut(s, &sid)?;
    let is_panel = slide
        .object(&oid)
        .is_some_and(|(object, _)| matches!(object.kind, ObjectKind::ScrollPanel { .. }));
    let is_timer = slide
        .object(&oid)
        .is_some_and(|(object, _)| matches!(object.kind, ObjectKind::Timer { .. }));
    let before = slide.local_ids().count();
    slide.objects.retain(|o| o.id != oid);
    for l in &mut slide.layers {
        l.objects.retain(|o| o.id != oid);
    }
    if slide.local_ids().count() == before {
        return Err(EngineError::NotFound(format!(
            "object {oid:?} is not on slide {sid:?}"
        )));
    }
    let removed_groups = super::remove_empty_group_records(slide);
    // A draggable object no longer lists the deleted object as a drop target.
    for object in slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
    {
        if let Some(drag) = object.drag.as_mut() {
            drag.targets.retain(|target| target != &oid);
        }
    }
    // Deleting a panel releases every child in the same undo step.
    if is_panel {
        for object in &mut slide.objects {
            if object.panel.as_deref() == Some(oid.as_str()) {
                object.panel = None;
            }
        }
        for layer in &mut slide.layers {
            for object in &mut layer.objects {
                if object.panel.as_deref() == Some(oid.as_str()) {
                    object.panel = None;
                }
            }
        }
    }
    // Triggers that start from the object or act on it (or on any group record
    // left empty by this delete) go with it, in the same undo step. Anything
    // else that still needs it (a question) makes keep_valid refuse the delete
    // and name it.
    let removed = if was_valid || !removed_groups.is_empty() {
        let mut needles = Vec::new();
        if was_valid {
            needles.push(format!("object {oid:?} is not on slide"));
            needles.push(format!("target {oid:?} is not on slide"));
            if is_timer {
                needles.push(format!(
                    "variable {:?} does not exist",
                    coursecraft_course::timer_remaining_variable(&oid)
                ));
            }
        }
        for gid in &removed_groups {
            needles.push(format!("object {gid:?} is not on slide"));
            needles.push(format!("target {gid:?} is not on slide"));
            needles.push(format!("group {gid:?} on slide"));
        }
        remove_dangling_triggers(s, &sid, &needles)
    } else {
        0
    };
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed_triggers": removed }))
}

/// Remove the triggers on slide `sid` (base or layers) that validation
/// reports with a message starting with one of `needles`, one at a time
/// until none is left. Returns how many were removed.
///
/// Validation is the single source of truth for what a trigger refers to,
/// so the event, the action and the condition are all covered.
pub(super) fn remove_dangling_triggers(s: &mut Session, sid: &str, needles: &[String]) -> usize {
    let Some(slide_path) = s
        .course
        .scenes
        .iter()
        .enumerate()
        .find_map(|(si, scene)| {
            scene
                .slides
                .iter()
                .position(|slide| slide.id == sid)
                .map(|li| format!("/scenes/{si}/slides/{li}/"))
        })
        .or_else(|| {
            s.course.banks.iter().enumerate().find_map(|(bi, bank)| {
                bank.slides
                    .iter()
                    .position(|slide| slide.id == sid)
                    .map(|si| format!("/banks/{bi}/slides/{si}/"))
            })
        })
    else {
        return 0;
    };
    let targets: BTreeSet<(Option<usize>, usize)> = s
        .course
        .validate()
        .into_iter()
        .filter_map(|issue| {
            if issue.severity != coursecraft_course::Severity::Error
                || !needles
                    .iter()
                    .any(|n| issue.message.starts_with(n.as_str()))
            {
                return None;
            }
            let rest = issue.path.strip_prefix(slide_path.as_str())?;
            let mut parts = rest.split('/');
            match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some("triggers"), Some(t), _, _) => Some((None, t.parse::<usize>().ok()?)),
                (Some("layers"), Some(l), Some("triggers"), Some(t)) => {
                    Some((Some(l.parse::<usize>().ok()?), t.parse::<usize>().ok()?))
                }
                _ => None,
            }
        })
        .collect();
    if targets.is_empty() {
        return 0;
    }
    let Some(slide) = s.course.slide_mut(sid) else {
        return 0;
    };
    let mut removed = 0;
    // Remove in descending index order so earlier removals do not shift later indices.
    for (layer, index) in targets.into_iter().rev() {
        let triggers = match layer {
            None => &mut slide.triggers,
            Some(l) => match slide.layers.get_mut(l) {
                Some(layer) => &mut layer.triggers,
                None => continue,
            },
        };
        if index < triggers.len() {
            triggers.remove(index);
            removed += 1;
        }
    }
    removed
}

/// Resolve `id` on `slide` as either an object or a group (returning `Some(owner)`
/// where `owner` is `None` for the base layer or `Some(layer_id)` for a slide layer).
fn resolve_group_owner(slide: &Slide, id: &str) -> Option<Option<String>> {
    if let Some((_, owner)) = slide.group(id) {
        return Some(owner.map(str::to_owned));
    }
    if slide.objects.iter().any(|o| o.group.as_deref() == Some(id)) {
        return Some(None);
    }
    slide
        .layers
        .iter()
        .find(|l| l.objects.iter().any(|o| o.group.as_deref() == Some(id)))
        .map(|l| Some(l.id.clone()))
}

fn groups_list_mut<'a>(
    slide: &'a mut Slide,
    owner: Option<&str>,
) -> Option<&'a mut Vec<coursecraft_course::Group>> {
    match owner {
        None => Some(&mut slide.groups),
        Some(lid) => slide
            .layers
            .iter_mut()
            .find(|l| l.id == lid)
            .map(|l| &mut l.groups),
    }
}

fn ensure_group_record_mut<'a>(
    slide: &'a mut Slide,
    id: &str,
    owner: Option<&str>,
) -> Result<&'a mut coursecraft_course::Group> {
    let sid = slide.id.clone();
    let groups = groups_list_mut(slide, owner).ok_or_else(|| {
        crate::EngineError::NotFound(format!("layer {owner:?} is not on slide {sid:?}"))
    })?;
    if let Some(pos) = groups.iter().position(|g| g.id == id) {
        return Ok(&mut groups[pos]);
    }
    groups.push(coursecraft_course::Group {
        id: id.to_owned(),
        name: String::new(),
        interactive: false,
        initial_state: "normal".to_owned(),
        focus_order: None,
        description: None,
        entrance: None,
        exit: None,
        emphasis: Vec::new(),
    });
    let last = groups.len() - 1;
    Ok(&mut groups[last])
}

fn prune_plain_empty_group(slide: &mut Slide, id: &str, owner: Option<&str>) {
    if let Some(groups) = groups_list_mut(slide, owner) {
        groups.retain(|g| g.id != id || !g.is_plain_empty());
    }
}

pub(super) fn object_set_animation(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_animation";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let which: String = req(C, p, "which")?;
    if which != "entrance" && which != "exit" {
        return Err(bad(
            C,
            format!("which must be 'entrance' or 'exit', got {which:?}"),
        ));
    }
    let map = obj(C, p)?;
    let anim_val = map
        .get("animation")
        .ok_or_else(|| bad(C, "missing `animation`"))?;
    let anim: Option<Animation> = match anim_val {
        Json::Null => None,
        val => {
            let a: Animation = serde_json::from_value(val.clone())
                .map_err(|e| bad(C, format!("invalid animation: {e}")))?;
            if a.duration_ms < 1 || a.duration_ms > 60000 {
                return Err(bad(C, "animation duration_ms must be 1..=60000"));
            }
            check_effect(C, &a.effect)?;
            Some(a)
        }
    };
    let slide = slide_mut(s, &sid)?;
    if slide.object(&oid).is_some() {
        let target = find_object_mut(slide, &oid)?;
        if which == "entrance" {
            target.entrance = anim;
        } else {
            target.exit = anim;
        }
    } else if let Some(owner) = resolve_group_owner(slide, &oid) {
        if let Some(a) = anim {
            let g = ensure_group_record_mut(slide, &oid, owner.as_deref())?;
            if which == "entrance" {
                g.entrance = Some(a);
            } else {
                g.exit = Some(a);
            }
        } else {
            if let Some(groups) = groups_list_mut(slide, owner.as_deref())
                && let Some(g) = groups.iter_mut().find(|g| g.id == oid)
            {
                if which == "entrance" {
                    g.entrance = None;
                } else {
                    g.exit = None;
                }
            }
            prune_plain_empty_group(slide, &oid, owner.as_deref());
        }
    } else {
        find_object_mut(slide, &oid)?;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Refuse an effect id the registry doesn't know, or an option the effect doesn't take, so
/// authoring never stores what the runtime would ignore. (Files from newer versions still open:
/// the parser and validator only warn.)
fn check_effect(cmd: &str, effect: &coursecraft_course::AnimEffect) -> Result<()> {
    let Some(def) = effect.def() else {
        return Err(bad(
            cmd,
            format!(
                "unknown effect {:?}; `animation.effects` lists them",
                effect.id
            ),
        ));
    };
    if let Some((_, message)) = def.option_problems(effect).into_iter().next() {
        return Err(bad(
            cmd,
            format!("{message}; `animation.effects` lists its options"),
        ));
    }
    Ok(())
}

/// `animation.effects`: the effect registries (entrance/exit and emphasis), for agents and
/// tools.
pub(super) fn animation_effects(_s: &mut Session, _p: &Json) -> Result<Json> {
    Ok(serde_json::json!({
        "effects": coursecraft_course::EFFECTS,
        "fallback": coursecraft_course::FALLBACK_EFFECT,
        "emphasis": coursecraft_course::EMPHASIS,
        "emphasis_fallback": coursecraft_course::FALLBACK_EMPHASIS,
    }))
}

/// Refuse an emphasis the registry doesn't know or whose numbers are out of range.
fn check_emphasis(cmd: &str, e: &coursecraft_course::Emphasis) -> Result<()> {
    if e.effect.def().is_none() {
        return Err(bad(
            cmd,
            format!(
                "unknown emphasis {:?}; `animation.effects` lists them under `emphasis`",
                e.effect.id
            ),
        ));
    }
    if e.duration_ms < 1 || e.duration_ms > 60_000 {
        return Err(bad(cmd, "emphasis duration_ms must be 1..=60000"));
    }
    if e.repeat == 0 || e.repeat > coursecraft_course::MAX_EMPHASIS_REPEAT {
        return Err(bad(
            cmd,
            format!(
                "emphasis repeat must be 1..={}",
                coursecraft_course::MAX_EMPHASIS_REPEAT
            ),
        ));
    }
    Ok(())
}

/// `object.add_emphasis`: append an emphasis (registry defaults for what is left out).
/// Returns its index.
pub(super) fn object_add_emphasis(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add_emphasis";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let effect: String = req(C, p, "effect")?;
    let start_ms: Option<u32> = opt(C, p, "start_ms")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let easing: Option<Easing> = opt(C, p, "easing")?;
    let repeat: Option<u8> = opt(C, p, "repeat")?;
    let slide = slide_mut(s, &sid)?;
    let index = if slide.object(&oid).is_some() {
        let target = find_object_mut(slide, &oid)?;
        if target.emphasis.len() >= coursecraft_course::MAX_EMPHASIS_PER_OBJECT {
            return Err(bad(
                C,
                format!(
                    "an object has at most {} emphasis animations",
                    coursecraft_course::MAX_EMPHASIS_PER_OBJECT
                ),
            ));
        }
        let mut e =
            coursecraft_course::Emphasis::new(&effect, start_ms.unwrap_or(target.timing.start_ms));
        if let Some(d) = duration_ms {
            e.duration_ms = d;
        }
        if let Some(easing) = easing {
            e.easing = easing;
        }
        if let Some(r) = repeat {
            e.repeat = r;
        }
        check_emphasis(C, &e)?;
        target.emphasis.push(e);
        target.emphasis.len() - 1
    } else if let Some(owner) = resolve_group_owner(slide, &oid) {
        let existing_len = slide.group(&oid).map_or(0, |(g, _)| g.emphasis.len());
        if existing_len >= coursecraft_course::MAX_EMPHASIS_PER_OBJECT {
            return Err(bad(
                C,
                format!(
                    "a group has at most {} emphasis animations",
                    coursecraft_course::MAX_EMPHASIS_PER_OBJECT
                ),
            ));
        }
        let default_start = slide
            .group_timing(&oid, owner.as_deref())
            .map_or(0, |t| t.start_ms);
        let mut e = coursecraft_course::Emphasis::new(&effect, start_ms.unwrap_or(default_start));
        if let Some(d) = duration_ms {
            e.duration_ms = d;
        }
        if let Some(easing) = easing {
            e.easing = easing;
        }
        if let Some(r) = repeat {
            e.repeat = r;
        }
        check_emphasis(C, &e)?;
        let g = ensure_group_record_mut(slide, &oid, owner.as_deref())?;
        g.emphasis.push(e);
        g.emphasis.len() - 1
    } else {
        find_object_mut(slide, &oid)?;
        0
    };
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": index }))
}

/// `object.set_emphasis`: replace emphasis `index` of an object or group.
pub(super) fn object_set_emphasis(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_emphasis";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let e: coursecraft_course::Emphasis = req(C, p, "emphasis")?;
    check_emphasis(C, &e)?;
    let slide = slide_mut(s, &sid)?;
    if slide.object(&oid).is_some() {
        let target = find_object_mut(slide, &oid)?;
        let slot = target
            .emphasis
            .get_mut(index)
            .ok_or_else(|| bad(C, format!("object {oid:?} has no emphasis {index}")))?;
        *slot = e;
    } else if let Some(owner) = resolve_group_owner(slide, &oid) {
        let slot = groups_list_mut(slide, owner.as_deref())
            .and_then(|groups| groups.iter_mut().find(|g| g.id == oid))
            .and_then(|g| g.emphasis.get_mut(index))
            .ok_or_else(|| bad(C, format!("group {oid:?} has no emphasis {index}")))?;
        *slot = e;
    } else {
        find_object_mut(slide, &oid)?;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// `object.remove_emphasis`: remove emphasis `index` of an object or group.
pub(super) fn object_remove_emphasis(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.remove_emphasis";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let slide = slide_mut(s, &sid)?;
    if slide.object(&oid).is_some() {
        let target = find_object_mut(slide, &oid)?;
        if index >= target.emphasis.len() {
            return Err(bad(C, format!("object {oid:?} has no emphasis {index}")));
        }
        target.emphasis.remove(index);
    } else if let Some(owner) = resolve_group_owner(slide, &oid) {
        let g = groups_list_mut(slide, owner.as_deref())
            .and_then(|groups| groups.iter_mut().find(|g| g.id == oid))
            .ok_or_else(|| bad(C, format!("group {oid:?} has no emphasis {index}")))?;
        if index >= g.emphasis.len() {
            return Err(bad(C, format!("group {oid:?} has no emphasis {index}")));
        }
        g.emphasis.remove(index);
        prune_plain_empty_group(slide, &oid, owner.as_deref());
    } else {
        find_object_mut(slide, &oid)?;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Start of a copied emphasis on the target. It keeps its offset from the source object's start
/// (an old file may hold one before that start; it counts as offset 0). The whole pass then
/// clamps into the target's window: not before its start, and, when the target has an end, not
/// so late that the pass runs past it.
fn copied_emphasis_start(
    e: &coursecraft_course::Emphasis,
    source_start: u32,
    target_start: u32,
    target_end: Option<u32>,
) -> u32 {
    let start = target_start.saturating_add(e.start_ms.saturating_sub(source_start));
    match target_end {
        Some(end) => {
            let pass = e.end_ms().saturating_sub(e.start_ms);
            start.min(end.saturating_sub(pass).max(target_start))
        }
        None => start,
    }
}

/// `object.copy_animation` (Animation Painter): give `objects` the entrance, exit and emphasis
/// of `from`. Emphasis keeps its offset from the object's start, so it lands at the same moment
/// of each target's timing. Motion paths stay where they are.
pub(super) fn object_copy_animation(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.copy_animation";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let from: String = req(C, p, "from")?;
    let from_slide: Option<String> = opt(C, p, "from_slide")?;
    let targets: Vec<String> = req(C, p, "objects")?;
    if targets.is_empty() {
        return Err(bad(C, "`objects` lists no object"));
    }
    let source_slide = from_slide.unwrap_or_else(|| sid.clone());
    let (entrance, exit, emphasis, start) = {
        let slide = slide_mut(s, &source_slide)?;
        if slide.object(&from).is_some() {
            let o = find_object_mut(slide, &from)?;
            (
                o.entrance.clone(),
                o.exit.clone(),
                o.emphasis.clone(),
                o.timing.start_ms,
            )
        } else if let Some(owner) = resolve_group_owner(slide, &from) {
            let start = slide
                .group_timing(&from, owner.as_deref())
                .map_or(0, |t| t.start_ms);
            if let Some((g, _)) = slide.group(&from) {
                (
                    g.entrance.clone(),
                    g.exit.clone(),
                    g.emphasis.clone(),
                    start,
                )
            } else {
                (None, None, Vec::new(), start)
            }
        } else {
            find_object_mut(slide, &from)?;
            (None, None, Vec::new(), 0)
        }
    };
    let slide = slide_mut(s, &sid)?;
    let mut copied = 0;
    for id in &targets {
        if *id == from && source_slide == sid {
            continue;
        }
        if slide.object(id).is_some() {
            let o = find_object_mut(slide, id)?;
            o.entrance = entrance.clone();
            o.exit = exit.clone();
            let (t_start, t_end) = (o.timing.start_ms, o.timing.end_ms);
            o.emphasis = emphasis
                .iter()
                .map(|e| coursecraft_course::Emphasis {
                    start_ms: copied_emphasis_start(e, start, t_start, t_end),
                    ..e.clone()
                })
                .collect();
        } else if let Some(owner) = resolve_group_owner(slide, id) {
            let timing =
                slide
                    .group_timing(id, owner.as_deref())
                    .unwrap_or(coursecraft_course::Timing {
                        start_ms: 0,
                        end_ms: None,
                    });
            let new_emphasis: Vec<coursecraft_course::Emphasis> = emphasis
                .iter()
                .map(|e| coursecraft_course::Emphasis {
                    start_ms: copied_emphasis_start(e, start, timing.start_ms, timing.end_ms),
                    ..e.clone()
                })
                .collect();
            if entrance.is_some() || exit.is_some() || !new_emphasis.is_empty() {
                let g = ensure_group_record_mut(slide, id, owner.as_deref())?;
                g.entrance = entrance.clone();
                g.exit = exit.clone();
                g.emphasis = new_emphasis;
            } else {
                if let Some(groups) = groups_list_mut(slide, owner.as_deref())
                    && let Some(g) = groups.iter_mut().find(|g| g.id == *id)
                {
                    g.entrance = None;
                    g.exit = None;
                    g.emphasis.clear();
                }
                prune_plain_empty_group(slide, id, owner.as_deref());
            }
        } else {
            find_object_mut(slide, id)?;
        }
        copied += 1;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "copied": copied }))
}

pub(super) fn object_add_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add_motion_path";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let points: Option<Vec<Point>> = opt(C, p, "points")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let easing: Option<Easing> = opt(C, p, "easing")?;
    let relative: Option<bool> = opt(C, p, "relative")?;

    let points =
        points.unwrap_or_else(|| vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 0.0 }]);
    if points.len() < 2 || points.len() > 100 {
        return Err(bad(C, "motion path points must be 2..=100"));
    }
    for pt in &points {
        if !pt.x.is_finite() || !pt.y.is_finite() || pt.x.abs() > 100000.0 || pt.y.abs() > 100000.0
        {
            return Err(bad(
                C,
                "motion path points must be finite and within ±100000",
            ));
        }
    }
    let duration_ms = duration_ms.unwrap_or(1000);
    if !(1..=60000).contains(&duration_ms) {
        return Err(bad(C, "motion path duration_ms must be 1..=60000"));
    }
    let easing = easing.unwrap_or_default();
    let relative = relative.unwrap_or(true);

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    if target.motion_paths.len() >= 20 {
        return Err(bad(C, "object cannot have more than 20 motion paths"));
    }

    let next_num = (1..)
        .find(|n| {
            let candidate = format!("path{n}");
            !target.motion_paths.iter().any(|mp| mp.id == candidate)
        })
        .unwrap_or(1);
    let id = format!("path{next_num}");

    target.motion_paths.push(MotionPath {
        id: id.clone(),
        points,
        duration_ms,
        easing,
        relative,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn object_update_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update_motion_path";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let path_id: String = req(C, p, "path")?;
    let points: Option<Vec<Point>> = opt(C, p, "points")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let easing: Option<Easing> = opt(C, p, "easing")?;
    let relative: Option<bool> = opt(C, p, "relative")?;

    if let Some(ref pts) = points {
        if pts.len() < 2 || pts.len() > 100 {
            return Err(bad(C, "motion path points must be 2..=100"));
        }
        for pt in pts {
            if !pt.x.is_finite()
                || !pt.y.is_finite()
                || pt.x.abs() > 100000.0
                || pt.y.abs() > 100000.0
            {
                return Err(bad(
                    C,
                    "motion path points must be finite and within ±100000",
                ));
            }
        }
    }
    if let Some(d) = duration_ms
        && !(1..=60000).contains(&d)
    {
        return Err(bad(C, "motion path duration_ms must be 1..=60000"));
    }

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    let path = target
        .motion_paths
        .iter_mut()
        .find(|mp| mp.id == path_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "motion path {path_id:?} not found on object {oid:?}"
            ))
        })?;

    if let Some(pts) = points {
        path.points = pts;
    }
    if let Some(d) = duration_ms {
        path.duration_ms = d;
    }
    if let Some(e) = easing {
        path.easing = e;
    }
    if let Some(r) = relative {
        path.relative = r;
    }

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_remove_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.remove_motion_path";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let path_id: String = req(C, p, "path")?;

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    let pos = target
        .motion_paths
        .iter()
        .position(|mp| mp.id == path_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "motion path {path_id:?} not found on object {oid:?}"
            ))
        })?;
    target.motion_paths.remove(pos);

    // Triggers naming the removed path cannot run again: remove them in the
    // same undo step so the course stays valid.
    slide
        .triggers
        .retain(|t| !trigger_references_path(t, &oid, &path_id));
    for l in &mut slide.layers {
        l.triggers
            .retain(|t| !trigger_references_path(t, &oid, &path_id));
    }

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn trigger_references_path(t: &Trigger, obj_id: &str, path_id: &str) -> bool {
    if let Action::MoveOnPath {
        ref object,
        ref path,
    } = t.action
        && object == obj_id
        && path == path_id
    {
        return true;
    }
    if let Event::AnimationCompletes {
        ref object,
        path: Some(ref path),
    } = t.when
        && object == obj_id
        && path == path_id
    {
        return true;
    }
    false
}
