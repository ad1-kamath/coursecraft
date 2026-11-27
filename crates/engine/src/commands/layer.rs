//! Slide layer commands.

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{Layer, RevisitMode, SeekMode, Slide};
use serde_json::{Value as Json, json};

use super::{
    MAX_DURATION_MS, all_group_ids, bad, fresh_id, fresh_id_in, keep_valid, kind_prefix, layer_mut,
    opt, remap_object_glues, remap_trigger, remove_dangling_triggers, req, slide_mut, text,
    validate_id,
};
use crate::{EngineError, Result, Session};

pub(super) fn layer_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.add";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: Option<String> = opt(C, p, "id")?;
    let name: Option<String> = opt(C, p, "name")?;
    let slide = slide_mut(s, &sid)?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if slide.local_ids().any(|existing| existing == custom) {
                return Err(EngineError::Invalid(format!(
                    "layer id {custom:?} already exists on slide {sid:?}"
                )));
            }
            custom
        }
        None => fresh_id("layer", slide.local_ids()),
    };
    let name = text(
        C,
        name.unwrap_or_else(|| format!("Layer {}", slide.layers.len() + 1)),
        "name",
    )?;
    slide.layers.push(Layer {
        id: id.clone(),
        name,
        duration_ms: 5000,
        objects: Vec::new(),
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn layer_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let name: Option<String> = opt(C, p, "name")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let hide_other_layers: Option<bool> = opt(C, p, "hide_other_layers")?;
    let pause_base_timeline: Option<bool> = opt(C, p, "pause_base_timeline")?;
    let prevent_base_clicks: Option<bool> = opt(C, p, "prevent_base_clicks")?;
    let hide_base_objects: Option<bool> = opt(C, p, "hide_base_objects")?;
    let hide_when_timeline_ends: Option<bool> = opt(C, p, "hide_when_timeline_ends")?;
    let allow_seek: Option<SeekMode> = opt(C, p, "allow_seek")?;
    let revisit: Option<RevisitMode> = opt(C, p, "revisit")?;

    let slide = slide_mut(s, &sid)?;
    let layer = layer_mut(slide, &lid)?;
    if let Some(n) = name {
        let n = text(C, n, "name")?;
        if n.trim().is_empty() {
            return Err(bad(C, "name is empty"));
        }
        layer.name = n;
    }
    if let Some(ms) = duration_ms {
        if ms > MAX_DURATION_MS {
            return Err(bad(C, "duration is longer than 24 hours"));
        }
        layer.duration_ms = ms;
    }
    if let Some(v) = hide_other_layers {
        layer.hide_other_layers = v;
    }
    if let Some(v) = pause_base_timeline {
        layer.pause_base_timeline = v;
    }
    if let Some(v) = prevent_base_clicks {
        layer.prevent_base_clicks = v;
    }
    if let Some(v) = hide_base_objects {
        layer.hide_base_objects = v;
    }
    if let Some(v) = hide_when_timeline_ends {
        layer.hide_when_timeline_ends = v;
    }
    if let Some(v) = allow_seek {
        layer.allow_seek = v;
    }
    if let Some(v) = revisit {
        layer.revisit = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn layer_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.delete";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let slide = slide_mut(s, &sid)?;
    let (deleted_oids, deleted_timers, deleted_gids): (Vec<String>, Vec<String>, Vec<String>) =
        slide
            .layers
            .iter()
            .find(|l| l.id == lid)
            .map(|l| {
                (
                    l.objects.iter().map(|o| o.id.clone()).collect(),
                    l.objects
                        .iter()
                        .filter(|o| matches!(o.kind, coursecraft_course::ObjectKind::Timer { .. }))
                        .map(|o| o.id.clone())
                        .collect(),
                    l.groups.iter().map(|g| g.id.clone()).collect(),
                )
            })
            .unwrap_or_default();
    let before = slide.layers.len();
    slide.layers.retain(|l| l.id != lid);
    if slide.layers.len() == before {
        return Err(EngineError::NotFound(format!(
            "layer {lid:?} is not on slide {sid:?}"
        )));
    }
    // A drop target on the deleted layer is no longer a target, and a question
    // no longer shows the layer as its feedback.
    let slide = slide_mut(s, &sid)?;
    for object in slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
    {
        if let Some(drag) = object.drag.as_mut() {
            drag.targets.retain(|target| !deleted_oids.contains(target));
        }
    }
    if let Some(question) = slide.question.as_mut() {
        for feedback in [
            &mut question.correct_layer,
            &mut question.incorrect_layer,
            &mut question.try_again_layer,
        ] {
            if feedback.as_deref() == Some(lid.as_str()) {
                *feedback = None;
            }
        }
    }
    // Triggers that show, hide or start from the layer (or reference objects on it) go with it.
    let removed = if was_valid {
        let mut needles = vec![format!("layer {lid:?} is not on slide")];
        for oid in deleted_oids.iter().chain(deleted_gids.iter()) {
            needles.push(format!("object {oid:?} is not on slide"));
            needles.push(format!("target {oid:?} is not on slide"));
        }
        for timer in &deleted_timers {
            needles.push(format!(
                "variable {:?} does not exist",
                coursecraft_course::timer_remaining_variable(timer)
            ));
        }
        remove_dangling_triggers(s, &sid, &needles)
    } else {
        0
    };
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed_triggers": removed }))
}

pub(super) fn layer_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.move";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let to: usize = req(C, p, "to")?;

    let slide = slide_mut(s, &sid)?;
    let Some(pos) = slide.layers.iter().position(|l| l.id == lid) else {
        return Err(EngineError::NotFound(format!(
            "layer {lid:?} is not on slide {sid:?}"
        )));
    };
    // `slide.layers` runs back to front: index 0 is the backmost layer.
    let last = slide.layers.len().saturating_sub(1);
    let at = to.min(last);
    if at == pos {
        s.skip_undo = true;
        return Ok(Json::Null);
    }
    let layer = slide.layers.remove(pos);
    slide.layers.insert(at.min(slide.layers.len()), layer);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A name for a duplicated layer: `<name> copy`, then `<name> copy 2`, …
fn copied_layer_name(slide: &Slide, name: &str) -> String {
    let first = format!("{name} copy");
    if !slide.layers.iter().any(|layer| layer.name == first) {
        return first;
    }
    for n in 2u64..=u32::MAX as u64 {
        let candidate = format!("{name} copy {n}");
        if !slide.layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    first
}

pub(super) fn layer_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.duplicate";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;

    // Build the copy against the slide as it is now, then insert it directly in
    // front of its source (a higher index is closer to the front).
    let (mut copy, insert_at, name, object_map, copied_map, layer_map, slide_id) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let (mut copy, index) = slide
            .layers
            .iter()
            .enumerate()
            .find(|(_, layer)| layer.id == lid)
            .map(|(index, layer)| (layer.clone(), index))
            .ok_or_else(|| {
                EngineError::NotFound(format!("layer {lid:?} is not on slide {sid:?}"))
            })?;

        let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        taken.extend(all_group_ids(slide));
        let new_layer_id = fresh_id_in("layer", &taken);
        taken.insert(new_layer_id.clone());

        // References to anything outside the copy stay pointed at the same ids.
        let mut object_map: BTreeMap<String, String> = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            .map(|object| (object.id.clone(), object.id.clone()))
            .collect();
        let mut group_map: BTreeMap<String, String> = BTreeMap::new();
        let mut copied_map: BTreeMap<String, String> = BTreeMap::new();
        for object in &mut copy.objects {
            let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
            taken.insert(new_id.clone());
            object_map.insert(object.id.clone(), new_id.clone());
            copied_map.insert(object.id.clone(), new_id.clone());
            object.id = new_id;
            if let Some(old_group) = object.group.clone() {
                let new_group = group_map.entry(old_group).or_insert_with(|| {
                    let fresh = fresh_id_in("group", &taken);
                    taken.insert(fresh.clone());
                    fresh
                });
                object.group = Some(new_group.clone());
            }
        }
        // Group records carry the group's id too: give them the copy's ids, and
        // let triggers inside the copy that name an interactive group follow.
        for record in &mut copy.groups {
            let new_group = group_map
                .entry(record.id.clone())
                .or_insert_with(|| {
                    let fresh = fresh_id_in("group", &taken);
                    taken.insert(fresh.clone());
                    fresh
                })
                .clone();
            object_map.insert(record.id.clone(), new_group.clone());
            record.id = new_group;
        }

        let mut layer_map: BTreeMap<String, String> = slide
            .layers
            .iter()
            .map(|layer| (layer.id.clone(), layer.id.clone()))
            .collect();
        layer_map.insert(lid.clone(), new_layer_id.clone());

        let name = copied_layer_name(slide, &copy.name);
        (
            copy,
            index.saturating_add(1),
            name,
            object_map,
            copied_map,
            layer_map,
            slide.id.clone(),
        )
    };

    remap_object_glues(&mut copy.objects, &copied_map);
    let mut kept = Vec::with_capacity(copy.triggers.len());
    for mut trigger in std::mem::take(&mut copy.triggers) {
        if remap_trigger(
            &mut trigger,
            &object_map,
            &layer_map,
            &s.course,
            None,
            Some(&lid),
            false,
        ) {
            kept.push(trigger);
        }
    }
    copy.triggers = kept;
    let new_id = layer_map
        .get(&lid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("layer {lid:?} disappeared")))?;
    copy.id = new_id.clone();
    copy.name = name;

    let slide = slide_mut(s, &slide_id)?;
    let at = insert_at.min(slide.layers.len());
    slide.layers.insert(at, copy);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id }))
}
