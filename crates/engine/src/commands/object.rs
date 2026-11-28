//! Object geometry, layout, grouping, duplication/paste, transforms, look,
//! visual styling, specialized kinds, crop, captions, and media commands.

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{
    Action, Autofit, Color, Condition, CropRect, Event, FillStyle, Glow, Glue, Group, Object,
    ObjectKind, Outline, RadioOption, Rect, Shadow, Slide, TextRun, TextStyle, Timing, Trigger,
    Trim, WebSource, shape_allows_text,
};
use serde_json::{Value as Json, json};

use super::{
    bad, clamp_frame, fresh_id, fresh_id_in, keep_valid, kind_prefix, layer_mut, obj, opt,
    remap_object_glue, remap_object_glues, remap_trigger, remove_dangling_triggers, req, slide_mut,
    text, validate_id,
};
use crate::{EngineError, Result, Session};

pub(super) fn object_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let mut kind: ObjectKind = req(C, p, "kind")?;
    clamp_shape_adjust(&mut kind);
    let mut frame: Rect = req(C, p, "frame")?;
    if let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &mut kind
    {
        if *point_text {
            style.wrap = false;
            *autofit = Autofit::None;
            super::text::fit_point_text_frame(
                &mut frame,
                text,
                runs,
                style,
                paragraphs,
                super::text::parse_measured_size(p),
                true,
            );
        } else if *autofit == Autofit::Grow {
            super::text::fit_grow_text_frame(
                &mut frame,
                text,
                runs,
                style,
                paragraphs,
                super::text::parse_measured_size(p),
                true,
            );
        }
    }
    let name: Option<String> = opt(C, p, "name")?;
    let timing: Option<Timing> = opt(C, p, "timing")?;
    let id: Option<String> = opt(C, p, "id")?;
    let slide = slide_mut(s, &sid)?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if slide.local_ids().any(|existing| existing == custom) {
                return Err(EngineError::Invalid(format!(
                    "object id {custom:?} already exists on slide {sid:?}"
                )));
            }
            custom
        }
        None => {
            let prefix = match kind {
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
            };
            fresh_id(prefix, slide.local_ids())
        }
    };
    let o = Object {
        id: id.clone(),
        name: text(C, name.unwrap_or_default(), "name")?,
        kind,
        frame,
        timing: timing.unwrap_or_default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
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
    };
    match layer {
        Some(l) => layer_mut(slide, &l)?.objects.push(o),
        None => slide.objects.push(o),
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

/// Clamp shape adjustment values to the kind's limits before storing them.
fn clamp_shape_adjust(kind: &mut ObjectKind) {
    if let ObjectKind::Shape { shape, adjust, .. } = kind {
        *adjust = coursecraft_course::clamp_adjust(*shape, adjust);
    }
}

pub(super) fn find_object_mut<'a>(slide: &'a mut Slide, id: &str) -> Result<&'a mut Object> {
    let sid = slide.id.clone();
    slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
        .find(|o| o.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}")))
}

/// True when the `kind` given to `object.update` has no `text_style` key.
fn kind_omits_text_style(p: &Json) -> bool {
    p.get("kind")
        .and_then(Json::as_object)
        .is_some_and(|k| !k.contains_key("text_style"))
}

/// A hand edit that leaves out `text_style` keeps the link to the named style.
fn keep_text_style_link(old: &ObjectKind, new: &mut ObjectKind, omitted: bool) {
    if !omitted {
        return;
    }
    if let (
        ObjectKind::Text {
            text_style: Some(link),
            ..
        },
        ObjectKind::Text {
            text_style: slot @ None,
            ..
        },
    ) = (old, new)
    {
        *slot = Some(link.clone());
    }
}

/// Emphasis starts are absolute on the slide timeline, so moving an object or group moves them
/// by the same amount. Saturating; an emphasis never lands before the new start.
pub(super) fn shift_emphasis_slice(
    emphasis: &mut [coursecraft_course::Emphasis],
    old_start: u32,
    new_start: u32,
) {
    if new_start == old_start {
        return;
    }
    for e in emphasis {
        let moved = if new_start > old_start {
            e.start_ms.saturating_add(new_start - old_start)
        } else {
            e.start_ms.saturating_sub(old_start - new_start)
        };
        e.start_ms = moved.max(new_start);
    }
}

fn shift_emphasis_with_start(obj: &mut Object, new_start: u32) {
    shift_emphasis_slice(&mut obj.emphasis, obj.timing.start_ms, new_start);
}

pub(super) fn object_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: Option<String> = opt(C, p, "name")?;
    let kind: Option<ObjectKind> = opt(C, p, "kind")?.map(|mut kind| {
        clamp_shape_adjust(&mut kind);
        kind
    });
    let timing: Option<Timing> = opt(C, p, "timing")?;
    let initial_state: Option<String> = opt(C, p, "initial_state")?;
    let decorative: Option<bool> = opt(C, p, "decorative")?;
    let map = obj(C, p)?;

    let slide = slide_mut(s, &sid)?;
    let group_old_start = if timing.is_some() {
        slide
            .group_of_object(&oid)
            .filter(|(g, _)| !g.emphasis.is_empty())
            .and_then(|(g, owner)| {
                let owner_owned = owner.map(str::to_owned);
                let old_min = slide.group_timing(&g.id, owner)?.start_ms;
                Some((g.id.clone(), owner_owned, old_min))
            })
    } else {
        None
    };
    let obj = find_object_mut(slide, &oid)?;
    if let Some(n) = name {
        obj.name = text(C, n, "name")?;
    }
    if let Some(mut k) = kind {
        keep_text_style_link(&obj.kind, &mut k, kind_omits_text_style(p));
        let mut refit_point = false;
        let mut refit_grow = false;
        if let ObjectKind::Text {
            style,
            autofit,
            point_text,
            ..
        } = &mut k
        {
            if *point_text {
                if style.wrap || *autofit != Autofit::None {
                    *point_text = false;
                    if *autofit == Autofit::Grow {
                        refit_grow = true;
                    }
                } else {
                    refit_point = true;
                }
            } else if *autofit == Autofit::Grow {
                refit_grow = true;
            }
        }
        obj.kind = k;
        if (refit_point || refit_grow)
            && let ObjectKind::Text {
                text,
                style,
                runs,
                paragraphs,
                ..
            } = &obj.kind
        {
            if refit_point {
                super::text::fit_point_text_frame(
                    &mut obj.frame,
                    text,
                    runs,
                    style,
                    paragraphs,
                    None,
                    false,
                );
            } else {
                super::text::fit_grow_text_frame(
                    &mut obj.frame,
                    text,
                    runs,
                    style,
                    paragraphs,
                    None,
                    false,
                );
            }
        }
    }
    if let Some(t) = timing {
        shift_emphasis_with_start(obj, t.start_ms);
        obj.timing = t;
    }
    if let Some(is) = initial_state {
        let is = text(C, is, "initial_state")?;
        if is.trim().is_empty() {
            return Err(bad(C, "initial_state is empty"));
        }
        obj.initial_state = is;
    }
    if let Some(v) = map.get("focus_order") {
        obj.focus_order = match v {
            Json::Null => None,
            Json::Number(n) => {
                let Some(value) = n.as_u64().filter(|n| *n <= u64::from(u32::MAX)) else {
                    return Err(bad(
                        C,
                        "`focus_order` must be an unsigned 32-bit integer or null",
                    ));
                };
                #[allow(clippy::cast_possible_truncation)]
                Some(value as u32)
            }
            _ => {
                return Err(bad(
                    C,
                    "`focus_order`: expected an unsigned integer or null",
                ));
            }
        };
    }
    if let Some(v) = decorative {
        obj.decorative = v;
    }
    if let Some(v) = map.get("description") {
        obj.description = match v {
            Json::Null => None,
            Json::String(s) => {
                let s = text(C, s.clone(), "description")?;
                if s.trim().is_empty() { None } else { Some(s) }
            }
            _ => return Err(bad(C, "`description`: expected string or null")),
        };
    }
    if let Some((gid, owner, old_min)) = group_old_start
        && let Some(new_min) = slide
            .group_timing(&gid, owner.as_deref())
            .map(|t| t.start_ms)
        && new_min != old_min
    {
        let groups = groups_vec_mut(slide, owner.as_deref())?;
        if let Some(g) = groups.iter_mut().find(|g| g.id == gid) {
            shift_emphasis_slice(&mut g.emphasis, old_min, new_min);
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_reorder(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.reorder";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;

    let slide = slide_mut(s, &sid)?;
    let sid_str = slide.id.clone();
    if let Some(pos) = slide.objects.iter().position(|o| o.id == oid) {
        let obj = slide.objects.remove(pos);
        let at = index.min(slide.objects.len());
        slide.objects.insert(at, obj);
    } else {
        let mut found = false;
        for layer in &mut slide.layers {
            if let Some(pos) = layer.objects.iter().position(|o| o.id == oid) {
                let obj = layer.objects.remove(pos);
                let at = index.min(layer.objects.len());
                layer.objects.insert(at, obj);
                found = true;
                break;
            }
        }
        if !found {
            return Err(EngineError::NotFound(format!(
                "object {oid:?} is not on slide {sid_str:?}"
            )));
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Read and deduplicate an `objects` id list, rejecting an empty list.
pub(super) fn objects_param(c: &str, p: &Json) -> Result<Vec<String>> {
    let objects: Vec<String> = req(c, p, "objects")?;
    let mut unique: Vec<String> = Vec::new();
    for id in objects {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.is_empty() {
        return Err(bad(c, "`objects` is empty"));
    }
    Ok(unique)
}

fn finite_rect(rect: Rect) -> bool {
    [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .all(|value| value.is_finite())
}

fn frame_bbox(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    let mut iter = rects;
    let first = iter.next()?;
    let (mut left, mut top) = (first.x, first.y);
    let (mut right, mut bottom) = (first.x + first.w, first.y + first.h);
    for rect in iter {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x + rect.w);
        bottom = bottom.max(rect.y + rect.h);
    }
    Some(Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}

/// One rigid unit for align and distribute: an ungrouped object, or every
/// listed member of the same group. Its bounding box is what moves.
struct LayoutUnit {
    members: Vec<(String, Rect)>,
    bbox: Rect,
}

fn layout_units(entries: Vec<(String, Rect, Option<String>)>) -> Vec<LayoutUnit> {
    type GroupedUnit = (Option<String>, Vec<(String, Rect)>);
    let mut grouped: Vec<GroupedUnit> = Vec::new();
    for (id, frame, group) in entries {
        if let Some(ref group) = group
            && let Some(unit) = grouped
                .iter_mut()
                .find(|(key, _)| key.as_deref() == Some(group.as_str()))
        {
            unit.1.push((id, frame));
            continue;
        }
        grouped.push((group, vec![(id, frame)]));
    }
    grouped
        .into_iter()
        .filter_map(|(_, members)| {
            let bbox = frame_bbox(members.iter().map(|(_, rect)| *rect))?;
            Some(LayoutUnit { members, bbox })
        })
        .collect()
}

pub(super) fn object_align(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.align";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let edge: String = req(C, p, "edge")?;
    let to: Option<String> = opt(C, p, "to")?;
    let to = to.as_deref().unwrap_or("selection");
    if !matches!(
        edge.as_str(),
        "left" | "center" | "right" | "top" | "middle" | "bottom"
    ) {
        return Err(bad(
            C,
            "`edge` must be left, center, right, top, middle or bottom",
        ));
    }
    if !matches!(to, "selection" | "slide") {
        return Err(bad(C, "`to` must be selection or slide"));
    }

    let slide_size = s.course.slide_size;
    let (units, slide_rect) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries: Vec<(String, Rect, Option<String>)> = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame, object.group.clone()));
            }
        }
        let slide_rect = Rect {
            x: 0.0,
            y: 0.0,
            w: slide_size.w as f32,
            h: slide_size.h as f32,
        };
        (layout_units(entries), slide_rect)
    };
    if units.is_empty() {
        return Ok(json!({ "moved": 0 }));
    }
    let reference = if to == "slide" || units.len() == 1 {
        slide_rect
    } else {
        frame_bbox(units.iter().map(|unit| unit.bbox)).unwrap_or(slide_rect)
    };

    let mut updates: Vec<(String, Rect)> = Vec::new();
    for unit in &units {
        let bbox = unit.bbox;
        let (dx, dy) = match edge.as_str() {
            "left" => (reference.x - bbox.x, 0.0),
            "center" => (reference.x + reference.w * 0.5 - bbox.w * 0.5 - bbox.x, 0.0),
            "right" => (reference.x + reference.w - bbox.w - bbox.x, 0.0),
            "top" => (0.0, reference.y - bbox.y),
            "middle" => (0.0, reference.y + reference.h * 0.5 - bbox.h * 0.5 - bbox.y),
            "bottom" => (0.0, reference.y + reference.h - bbox.h - bbox.y),
            _ => (0.0, 0.0),
        };
        if !dx.is_finite() || !dy.is_finite() {
            continue;
        }
        for (id, frame) in &unit.members {
            let mut next = *frame;
            next.x += dx;
            next.y += dy;
            if finite_rect(next) && next != *frame {
                updates.push((id.clone(), next));
            }
        }
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

pub(super) fn object_distribute(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.distribute";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let axis: String = req(C, p, "axis")?;
    if !matches!(axis.as_str(), "horizontal" | "vertical") {
        return Err(bad(C, "`axis` must be horizontal or vertical"));
    }

    let units: Vec<(usize, LayoutUnit)> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame, object.group.clone()));
            }
        }
        layout_units(entries).into_iter().enumerate().collect()
    };
    if units.len() < 3 {
        return Err(bad(C, "distribute needs at least 3 objects"));
    }

    let horizontal = axis == "horizontal";
    let mut sorted = units;
    sorted.sort_by(|a, b| {
        let (a_pos, b_pos) = if horizontal {
            (a.1.bbox.x, b.1.bbox.x)
        } else {
            (a.1.bbox.y, b.1.bbox.y)
        };
        a_pos.total_cmp(&b_pos).then(a.0.cmp(&b.0))
    });

    let Some((_, first)) = sorted.first() else {
        return Ok(json!({ "moved": 0 }));
    };
    let Some((_, last)) = sorted.last() else {
        return Ok(json!({ "moved": 0 }));
    };
    let (first_pos, first_size) = if horizontal {
        (first.bbox.x, first.bbox.w)
    } else {
        (first.bbox.y, first.bbox.h)
    };
    let (last_pos, last_size) = if horizontal {
        (last.bbox.x, last.bbox.w)
    } else {
        (last.bbox.y, last.bbox.h)
    };
    let total_span = (last_pos + last_size) - first_pos;
    let sum_sizes: f32 = sorted
        .iter()
        .map(|(_, unit)| if horizontal { unit.bbox.w } else { unit.bbox.h })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let gap = (total_span - sum_sizes) / (sorted.len() as f32 - 1.0);
    if !gap.is_finite() {
        return Ok(json!({ "moved": 0 }));
    }

    let mut updates: Vec<(String, Rect)> = Vec::new();
    let mut cursor = first_pos + first_size;
    let middle_end = sorted.len().saturating_sub(1);
    for (_, unit) in sorted.iter().take(middle_end).skip(1) {
        let target = cursor + gap;
        let delta = if horizontal {
            target - unit.bbox.x
        } else {
            target - unit.bbox.y
        };
        if !delta.is_finite() {
            continue;
        }
        for (id, frame) in &unit.members {
            let mut next = *frame;
            if horizontal {
                next.x += delta;
            } else {
                next.y += delta;
            }
            if finite_rect(next) && next != *frame {
                updates.push((id.clone(), next));
            }
        }
        cursor = target + if horizontal { unit.bbox.w } else { unit.bbox.h };
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

pub(super) fn object_match_size(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.match_size";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let dimension: String = req(C, p, "dimension")?;
    if !matches!(dimension.as_str(), "width" | "height" | "both") {
        return Err(bad(C, "`dimension` must be width, height or both"));
    }

    let entries: Vec<(String, Rect)> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame));
            }
        }
        entries
    };
    let Some((_, primary)) = entries.first() else {
        return Ok(json!({ "moved": 0 }));
    };
    let (width, height) = (primary.w, primary.h);

    let mut updates: Vec<(String, Rect)> = Vec::new();
    for (id, frame) in entries.iter().skip(1) {
        let mut next = *frame;
        if matches!(dimension.as_str(), "width" | "both") {
            next.w = width;
        }
        if matches!(dimension.as_str(), "height" | "both") {
            next.h = height;
        }
        if finite_rect(next) && next != *frame {
            updates.push((id.clone(), next));
        }
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

fn layer_objects_ref<'a>(slide: &'a Slide, layer: Option<&str>) -> Result<&'a [Object]> {
    match layer {
        Some(lid) => slide
            .layer(lid)
            .map(|layer| layer.objects.as_slice())
            .ok_or_else(|| {
                EngineError::NotFound(format!("layer {lid:?} is not on slide {:?}", slide.id))
            }),
        None => Ok(slide.objects.as_slice()),
    }
}

fn objects_vec_mut<'a>(slide: &'a mut Slide, layer: Option<&str>) -> Result<&'a mut Vec<Object>> {
    match layer {
        Some(lid) => Ok(&mut layer_mut(slide, lid)?.objects),
        None => Ok(&mut slide.objects),
    }
}

fn groups_vec_mut<'a>(slide: &'a mut Slide, layer: Option<&str>) -> Result<&'a mut Vec<Group>> {
    match layer {
        Some(lid) => Ok(&mut layer_mut(slide, lid)?.groups),
        None => Ok(&mut slide.groups),
    }
}

pub(super) fn all_group_ids(slide: &Slide) -> Vec<String> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        .filter_map(|object| object.group.clone())
        .collect()
}

/// A plain group of one is no group: clear the id from a lone remaining member. A group with
/// a record (an interactive group, which triggers may target) keeps its member.
fn dissolve_single_member_groups(objects: &mut [Object], records: &[Group]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for g in objects.iter().filter_map(|o| o.group.as_ref()) {
        let n = counts.entry(g.clone()).or_insert(0);
        *n = n.saturating_add(1);
    }
    for o in objects.iter_mut() {
        let lone = |g: &String| counts.get(g) == Some(&1) && !records.iter().any(|r| &r.id == g);
        if o.group.as_ref().is_some_and(lone) {
            o.group = None;
        }
    }
}

/// Remove any [`Group`] records on `slide` (base or layers) that have zero
/// member objects remaining on their owning layer. Returns the removed group
/// IDs so callers can clean up dangling triggers in the same undo step.
pub(super) fn remove_empty_group_records(slide: &mut Slide) -> Vec<String> {
    dissolve_single_member_groups(&mut slide.objects, &slide.groups);
    for layer in &mut slide.layers {
        dissolve_single_member_groups(&mut layer.objects, &layer.groups);
    }
    let mut removed = Vec::new();
    let base_objects = &slide.objects;
    slide.groups.retain(|g| {
        let has_member = base_objects
            .iter()
            .any(|o| o.group.as_deref() == Some(g.id.as_str()));
        if !has_member {
            removed.push(g.id.clone());
        }
        has_member
    });
    for layer in &mut slide.layers {
        let layer_objects = &layer.objects;
        layer.groups.retain(|g| {
            let has_member = layer_objects
                .iter()
                .any(|o| o.group.as_deref() == Some(g.id.as_str()));
            if !has_member {
                removed.push(g.id.clone());
            }
            has_member
        });
    }
    removed
}

fn cleanup_empty_groups_and_triggers(s: &mut Session, sid: &str) -> Result<usize> {
    let removed_groups = {
        let slide = slide_mut(s, sid)?;
        remove_empty_group_records(slide)
    };
    if removed_groups.is_empty() {
        return Ok(0);
    }
    let mut needles = Vec::with_capacity(removed_groups.len().saturating_mul(3));
    for gid in &removed_groups {
        needles.push(format!("object {gid:?} is not on slide"));
        needles.push(format!("target {gid:?} is not on slide"));
        needles.push(format!("group {gid:?} on slide"));
    }
    Ok(remove_dangling_triggers(s, sid, &needles))
}

fn event_references_target(event: &Event, target_id: &str) -> bool {
    match event {
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::StateChanges { object, .. }
        | Event::MediaEnds { object }
        | Event::CueReached { object, .. }
        | Event::AnimationCompletes { object, .. }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object } => object == target_id,
        Event::KeyPressed { object, .. } => object.as_deref() == Some(target_id),
        Event::DroppedOn { object, target } => object == target_id || target == target_id,
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            a == target_id || b == target_id
        }
        Event::TimerEnds { timer } => timer == target_id,
        Event::TimelineStarts { .. }
        | Event::TimelineEnds { .. }
        | Event::TimelineReaches { .. }
        | Event::VariableChanges { .. }
        | Event::Submit => false,
    }
}

fn action_references_target(action: &Action, target_id: &str) -> bool {
    match action {
        Action::ChangeState { object, .. }
        | Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object }
        | Action::MoveOnPath { object, .. } => object == target_id,
        Action::StartTimer { timer }
        | Action::PauseTimer { timer }
        | Action::ResetTimer { timer } => timer == target_id,
        Action::JumpToSlide { .. }
        | Action::ShowLayer { .. }
        | Action::HideLayer { .. }
        | Action::SetVariable { .. }
        | Action::PauseTimeline
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
        | Action::LightboxSlide { .. }
        | Action::CloseLightbox => false,
    }
}

fn condition_references_target(condition: &Condition, target_id: &str) -> bool {
    match condition {
        Condition::All(list) | Condition::Any(list) => list
            .iter()
            .any(|inner| condition_references_target(inner, target_id)),
        Condition::Not(inner) => condition_references_target(inner, target_id),
        Condition::ObjectState { object, .. } => object == target_id,
        Condition::Compare { .. } => false,
    }
}

fn trigger_references_target(trigger: &Trigger, target_id: &str) -> bool {
    event_references_target(&trigger.when, target_id)
        || action_references_target(&trigger.action, target_id)
        || trigger
            .condition
            .as_ref()
            .is_some_and(|c| condition_references_target(c, target_id))
}

fn derive_group_name(members: &[&Object]) -> String {
    const MAX_NAME_CHARS: usize = 48;
    for member in members {
        let raw = match &member.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            ObjectKind::Button { label, .. } => Some(label.as_str()),
            _ => None,
        };
        if let Some(raw) = raw {
            let cleaned = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            if !cleaned.is_empty() {
                return cleaned.chars().take(MAX_NAME_CHARS).collect();
            }
        }
    }
    "Button group".to_owned()
}

fn members_have_state(members: &[&Object], state: &str) -> bool {
    matches!(state, "normal" | "hidden" | "disabled")
        || members
            .iter()
            .any(|m| m.states.iter().any(|s| s.name == state))
}

pub(super) fn object_group(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.group";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    if objects.len() < 2 {
        return Err(bad(C, "group needs at least 2 objects"));
    }

    let (layer, member_ids, insert_at, new_group) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut layer: Option<String> = None;
        let mut groups = BTreeSet::new();
        for (index, id) in objects.iter().enumerate() {
            let (object, owner) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            let owner = owner.map(str::to_owned);
            if index == 0 {
                layer = owner;
            } else if layer != owner {
                return Err(bad(C, "all objects must be on the same layer"));
            }
            if let Some(group) = &object.group {
                groups.insert(group.clone());
            }
        }
        let layer_objects = layer_objects_ref(slide, layer.as_deref())?;
        let selected: BTreeSet<&str> = objects.iter().map(String::as_str).collect();
        let member_ids: Vec<String> = layer_objects
            .iter()
            .filter(|object| {
                selected.contains(object.id.as_str())
                    || object
                        .group
                        .as_ref()
                        .is_some_and(|group| groups.contains(group))
            })
            .map(|object| object.id.clone())
            .collect();
        if member_ids.len() < 2 {
            return Err(bad(C, "group needs at least 2 objects"));
        }
        let topmost = member_ids
            .iter()
            .filter_map(|id| layer_objects.iter().position(|object| &object.id == id))
            .max()
            .ok_or_else(|| bad(C, "group members are not on this layer"))?;
        let insert_at = topmost.saturating_add(1).saturating_sub(member_ids.len());
        let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        taken.extend(all_group_ids(slide));
        let new_group = fresh_id_in("group", &taken);
        (layer, member_ids, insert_at, new_group)
    };

    let member_set: BTreeSet<&str> = member_ids.iter().map(String::as_str).collect();
    let count = {
        let slide = slide_mut(s, &sid)?;
        let layer_objects = objects_vec_mut(slide, layer.as_deref())?;
        let mut extracted: Vec<Object> = Vec::new();
        let mut index = 0usize;
        while index < layer_objects.len() {
            if member_set.contains(layer_objects[index].id.as_str()) {
                extracted.push(layer_objects.remove(index));
            } else {
                index = index.saturating_add(1);
            }
        }
        for object in &mut extracted {
            object.group = Some(new_group.clone());
        }
        let count = extracted.len();
        let at = insert_at.min(layer_objects.len());
        for (offset, object) in extracted.into_iter().enumerate() {
            layer_objects.insert(at.saturating_add(offset), object);
        }
        count
    };
    let _ = cleanup_empty_groups_and_triggers(s, &sid)?;

    keep_valid(s, was_valid)?;
    Ok(json!({ "group": new_group, "objects": count }))
}

pub(super) fn object_ungroup(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.ungroup";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;

    let groups: BTreeSet<String> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut groups = BTreeSet::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if let Some(group) = &object.group {
                groups.insert(group.clone());
            }
        }
        groups
    };
    if groups.is_empty() {
        return Ok(json!({ "objects": 0 }));
    }

    let mut count = 0usize;
    {
        let slide = slide_mut(s, &sid)?;
        for object in slide.objects.iter_mut().chain(
            slide
                .layers
                .iter_mut()
                .flat_map(|layer| layer.objects.iter_mut()),
        ) {
            if object
                .group
                .as_ref()
                .is_some_and(|group| groups.contains(group))
            {
                object.group = None;
                count = count.saturating_add(1);
            }
        }
    }
    let _ = cleanup_empty_groups_and_triggers(s, &sid)?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "objects": count }))
}

pub(super) fn group_set_interactive(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "group.set_interactive";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let group_id: String = req(C, p, "group")?;
    let interactive: bool = req(C, p, "interactive")?;
    let name_param: Option<String> = match opt::<String>(C, p, "name")? {
        Some(n) => Some(text(C, n, "name")?),
        None => None,
    };

    if interactive {
        if s.course.master(&sid).is_some() {
            return Err(bad(C, "master slides cannot have interactive groups"));
        }

        let (owner, effective_id, rewrite_members, derived_name) = {
            let slide = s
                .course
                .slide(&sid)
                .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;

            let mut owners: Vec<(Option<String>, Vec<&Object>)> = Vec::new();
            let base_members: Vec<&Object> = slide
                .objects
                .iter()
                .filter(|o| o.group.as_deref() == Some(group_id.as_str()))
                .collect();
            if !base_members.is_empty() {
                owners.push((None, base_members));
            }
            for layer in &slide.layers {
                let layer_members: Vec<&Object> = layer
                    .objects
                    .iter()
                    .filter(|o| o.group.as_deref() == Some(group_id.as_str()))
                    .collect();
                if !layer_members.is_empty() {
                    owners.push((Some(layer.id.clone()), layer_members));
                }
            }
            if owners.is_empty() {
                return Err(EngineError::NotFound(format!(
                    "group {group_id:?} is not on slide {sid:?}"
                )));
            }
            if owners.len() > 1 {
                return Err(bad(
                    C,
                    "all group members must be on the same layer to make an interactive group",
                ));
            }
            let Some((owner, members)) = owners.into_iter().next() else {
                return Err(EngineError::NotFound(format!(
                    "group {group_id:?} is not on slide {sid:?}"
                )));
            };
            if members.len() < 2 {
                return Err(bad(C, "interactive group needs at least 2 members"));
            }
            for member in &members {
                if coursecraft_course::is_disallowed_interactive_group_member(
                    member,
                    slide.question.as_ref(),
                ) {
                    return Err(bad(
                        C,
                        format!(
                            "interactive group cannot contain {:?} (inputs, scrolling panels, web, media, markers, tables, draggable objects, and question items are not allowed)",
                            member.id
                        ),
                    ));
                }
            }

            let collides_with_other_group = slide
                .group(&group_id)
                .is_some_and(|(_, existing_owner)| existing_owner != owner.as_deref());
            let collides = slide.object(&group_id).is_some()
                || slide.layer(&group_id).is_some()
                || collides_with_other_group
                || validate_id(C, &group_id).is_err();

            let (effective_id, rewrite_members) = if collides {
                let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
                taken.extend(all_group_ids(slide));
                (fresh_id_in("grp-", &taken), true)
            } else {
                (group_id.clone(), false)
            };

            let derived_name = derive_group_name(&members);
            (owner, effective_id, rewrite_members, derived_name)
        };

        // Never move, rewrite, or delete existing member triggers when opting in.
        let slide = slide_mut(s, &sid)?;
        if rewrite_members {
            let layer_objects = objects_vec_mut(slide, owner.as_deref())?;
            for obj in layer_objects.iter_mut() {
                if obj.group.as_deref() == Some(group_id.as_str()) {
                    obj.group = Some(effective_id.clone());
                }
            }
        }
        let groups = groups_vec_mut(slide, owner.as_deref())?;
        if let Some(existing) = groups.iter_mut().find(|g| g.id == effective_id) {
            existing.interactive = true;
            if let Some(n) = name_param {
                existing.name = n;
            } else if existing.name.trim().is_empty() {
                existing.name = derived_name;
            }
        } else {
            groups.push(Group {
                id: effective_id.clone(),
                name: name_param.unwrap_or(derived_name),
                interactive: true,
                initial_state: "normal".to_owned(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            });
        }

        keep_valid(s, was_valid)?;
        Ok(json!({ "group": effective_id }))
    } else {
        let has_record = {
            let slide = s
                .course
                .slide(&sid)
                .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
            let has_record = slide.group(&group_id).is_some();
            if !has_record && slide.group_members(&group_id).is_empty() {
                return Err(EngineError::NotFound(format!(
                    "group {group_id:?} is not on slide {sid:?}"
                )));
            }
            if has_record {
                let has_trigger = slide
                    .triggers
                    .iter()
                    .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
                    .any(|t| trigger_references_target(t, &group_id))
                    || slide
                        .question
                        .as_ref()
                        .and_then(|q| q.correct_when.as_ref())
                        .is_some_and(|c| condition_references_target(c, &group_id));
                if has_trigger {
                    return Err(EngineError::Invalid(
                        "Cannot remove button behaviour while triggers reference this group; delete or retarget those triggers first".to_owned(),
                    ));
                }
            }
            has_record
        };
        if !has_record {
            s.skip_undo = true;
            return Ok(json!({ "group": group_id }));
        }

        let slide = slide_mut(s, &sid)?;
        let mut changed = false;
        let update_list = |groups: &mut Vec<Group>, changed: &mut bool| {
            let before = groups.len();
            for g in groups.iter_mut().filter(|g| g.id == group_id) {
                if g.interactive
                    || g.initial_state != "normal"
                    || g.focus_order.is_some()
                    || g.description.is_some()
                {
                    *changed = true;
                }
                g.interactive = false;
                g.initial_state = "normal".to_owned();
                g.focus_order = None;
                g.description = None;
                if !g.has_animation() && !g.name.is_empty() {
                    g.name.clear();
                    *changed = true;
                }
            }
            groups.retain(|g| g.id != group_id || !g.is_plain_empty());
            if groups.len() != before {
                *changed = true;
            }
        };
        update_list(&mut slide.groups, &mut changed);
        for layer in &mut slide.layers {
            update_list(&mut layer.groups, &mut changed);
        }
        if !changed {
            s.skip_undo = true;
            return Ok(json!({ "group": group_id }));
        }

        keep_valid(s, was_valid)?;
        Ok(json!({ "group": group_id }))
    }
}

pub(super) fn group_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "group.update";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let gid: String = req(C, p, "group")?;
    let name: Option<String> = match opt::<String>(C, p, "name")? {
        Some(n) => Some(text(C, n, "name")?),
        None => None,
    };
    let initial_state: Option<String> = match opt::<String>(C, p, "initial_state")? {
        Some(is) => {
            let is = text(C, is, "initial_state")?;
            if is.trim().is_empty() {
                return Err(bad(C, "initial_state is empty"));
            }
            Some(is)
        }
        None => None,
    };
    let map = obj(C, p)?;
    let focus_order_patch: Option<Option<u32>> = if let Some(v) = map.get("focus_order") {
        let parsed = match v {
            Json::Null => None,
            Json::Number(n) => {
                let Some(value) = n.as_u64().filter(|n| *n <= u64::from(u32::MAX)) else {
                    return Err(bad(
                        C,
                        "`focus_order` must be an unsigned 32-bit integer or null",
                    ));
                };
                #[allow(clippy::cast_possible_truncation)]
                Some(value as u32)
            }
            _ => {
                return Err(bad(
                    C,
                    "`focus_order`: expected an unsigned integer or null",
                ));
            }
        };
        Some(parsed)
    } else {
        None
    };
    let description_patch: Option<Option<String>> = if let Some(v) = map.get("description") {
        let parsed = match v {
            Json::Null => None,
            Json::String(desc) => {
                let desc = text(C, desc.clone(), "description")?;
                if desc.trim().is_empty() {
                    None
                } else {
                    Some(desc)
                }
            }
            _ => return Err(bad(C, "`description`: expected string or null")),
        };
        Some(parsed)
    } else {
        None
    };

    let owner = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let (_, owner) = slide.group(&gid).ok_or_else(|| {
            EngineError::NotFound(format!("group {gid:?} is not on slide {sid:?}"))
        })?;
        if let Some(ref state) = initial_state
            && !slide.group_has_state(&gid, state)
        {
            return Err(bad(
                C,
                format!("state {state:?} is not defined on group {gid:?}"),
            ));
        }
        owner.map(str::to_owned)
    };

    let slide = slide_mut(s, &sid)?;
    let groups = groups_vec_mut(slide, owner.as_deref())?;
    let group = groups
        .iter_mut()
        .find(|g| g.id == gid)
        .ok_or_else(|| EngineError::NotFound(format!("group {gid:?} is not on slide {sid:?}")))?;
    if let Some(n) = name {
        group.name = n;
    }
    if let Some(is) = initial_state {
        group.initial_state = is;
    }
    if let Some(fo) = focus_order_patch {
        group.focus_order = fo;
    }
    if let Some(desc) = description_patch {
        group.description = desc;
    }

    keep_valid(s, was_valid)?;
    Ok(json!({ "group": gid }))
}

pub(super) fn object_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.duplicate";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects: Option<Vec<String>> = opt(C, p, "objects")?;
    let dx: f32 = opt(C, p, "dx")?.unwrap_or(20.0);
    let dy: f32 = opt(C, p, "dy")?.unwrap_or(20.0);
    if !dx.is_finite() || !dy.is_finite() {
        return Err(bad(C, "dx and dy must be finite"));
    }
    let mut ids: Vec<String> = Vec::new();
    if let Some(objects) = objects {
        for id in objects {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    } else {
        ids.push(req(C, p, "object")?);
    }
    if ids.is_empty() {
        return Err(bad(C, "`objects` is empty"));
    }

    let is_master = s.course.master(&sid).is_some();
    let (sources, mut taken, source_groups) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut sources = Vec::new();
        for id in &ids {
            let (object, layer) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            sources.push((object.clone(), layer.map(str::to_owned)));
        }
        let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        taken.extend(all_group_ids(slide));
        let mut source_groups: BTreeMap<(Option<String>, String), Group> = BTreeMap::new();
        for g in &slide.groups {
            source_groups.insert((None, g.id.clone()), g.clone());
        }
        for layer in &slide.layers {
            for g in &layer.groups {
                source_groups.insert((Some(layer.id.clone()), g.id.clone()), g.clone());
            }
        }
        (sources, taken, source_groups)
    };

    let mut group_map: BTreeMap<(Option<String>, String), String> = BTreeMap::new();
    let mut copy_map: BTreeMap<String, String> = BTreeMap::new();
    let mut cloned: Vec<(Object, Option<String>)> = Vec::new();
    let mut new_ids: Vec<String> = Vec::new();
    for (mut object, layer) in sources {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        copy_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id.clone();
        object.frame.x += dx;
        object.frame.y += dy;
        if let Some(old_group) = object.group.clone() {
            let new_group = group_map
                .entry((layer.clone(), old_group))
                .or_insert_with(|| {
                    let fresh = fresh_id_in("group", &taken);
                    taken.insert(fresh.clone());
                    fresh
                });
            object.group = Some(new_group.clone());
        }
        new_ids.push(new_id);
        cloned.push((object, layer));
    }
    for (object, _) in &mut cloned {
        remap_object_glue(object, &copy_map);
    }

    let mut cloned_groups: Vec<(Option<String>, Group)> = Vec::new();
    for ((owner, old_group), new_group) in &group_map {
        let Some(src_group) = source_groups.get(&(owner.clone(), old_group.clone())) else {
            continue;
        };
        if is_master && src_group.interactive {
            continue;
        }
        let mut g = src_group.clone();
        g.id = new_group.clone();
        if g.interactive {
            let members: Vec<&Object> = cloned
                .iter()
                .filter(|(o, l)| l == owner && o.group.as_deref() == Some(new_group.as_str()))
                .map(|(o, _)| o)
                .collect();
            if !members_have_state(&members, &g.initial_state) {
                g.initial_state = "normal".to_owned();
            }
        }
        cloned_groups.push((owner.clone(), g));
    }

    let slide = slide_mut(s, &sid)?;
    for (object, layer) in cloned {
        objects_vec_mut(slide, layer.as_deref())?.push(object);
    }
    for (owner, group) in cloned_groups {
        groups_vec_mut(slide, owner.as_deref())?.push(group);
    }

    keep_valid(s, was_valid)?;
    let first = new_ids.first().cloned().unwrap_or_default();
    Ok(json!({ "id": first, "ids": new_ids }))
}

fn collect_paste_source_groups(
    cmd: &str,
    src_slide: Option<&Slide>,
    groups_param: Vec<Group>,
) -> Result<BTreeMap<String, Group>> {
    let mut source_groups: BTreeMap<String, Group> = BTreeMap::new();
    if let Some(src_slide) = src_slide {
        for g in &src_slide.groups {
            source_groups.insert(g.id.clone(), g.clone());
        }
        for l in &src_slide.layers {
            for g in &l.groups {
                source_groups.insert(g.id.clone(), g.clone());
            }
        }
    }
    for mut g in groups_param {
        g.name = text(cmd, g.name, "name")?;
        if let Some(desc) = g.description {
            let desc = text(cmd, desc, "description")?;
            g.description = if desc.trim().is_empty() {
                None
            } else {
                Some(desc)
            };
        }
        source_groups.insert(g.id.clone(), g);
    }
    Ok(source_groups)
}

pub(super) fn object_paste(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.paste";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let objects: Vec<Object> = req(C, p, "objects")?;
    let triggers: Vec<Trigger> = opt(C, p, "triggers")?.unwrap_or_default();
    let groups_param: Vec<Group> = opt(C, p, "groups")?.unwrap_or_default();
    let offset: Option<f32> = opt(C, p, "offset")?;
    let source_slide: Option<String> = opt(C, p, "source_slide")?;

    if objects.is_empty() {
        return Err(bad(C, "`objects` is empty"));
    }
    if objects.len() > 500 || triggers.len() > 1000 || groups_param.len() > 500 {
        return Err(bad(C, "clipboard payload is too large"));
    }
    let offset = offset.unwrap_or(20.0);
    if !offset.is_finite() || offset.abs() > 100_000.0 {
        return Err(bad(C, "offset must be finite and within ±100000"));
    }
    for object in &objects {
        let frame = object.frame;
        if ![frame.x, frame.y, frame.w, frame.h]
            .iter()
            .all(|value| value.is_finite())
            || frame.w <= 0.0
            || frame.h <= 0.0
            || frame.x.abs() > 100_000.0
            || frame.y.abs() > 100_000.0
            || frame.w > 100_000.0
            || frame.h > 100_000.0
        {
            return Err(bad(
                C,
                "object frame values must be finite, positive and within ±100000",
            ));
        }
    }

    let slide_size = s.course.slide_size;
    let is_master = s.course.master(&sid).is_some();
    if is_master {
        if layer.is_some() {
            return Err(bad(C, "a master has no layers"));
        }
        if objects.iter().any(|object| {
            matches!(
                &object.kind,
                ObjectKind::Audio { .. } | ObjectKind::Video { .. }
            )
        }) {
            return Err(bad(C, "a master cannot hold audio or video"));
        }
    }
    let same_slide = source_slide.as_deref().is_none_or(|source| source == sid);
    let (mut taken, layers): (BTreeSet<String>, BTreeMap<String, String>) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        if let Some(ref lid) = layer
            && slide.layer(lid).is_none()
        {
            return Err(EngineError::NotFound(format!(
                "layer {lid:?} is not on slide {sid:?}"
            )));
        }
        let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        taken.extend(all_group_ids(slide));
        (
            taken,
            if same_slide {
                slide
                    .layers
                    .iter()
                    .map(|l| (l.id.clone(), l.id.clone()))
                    .collect()
            } else {
                BTreeMap::new()
            },
        )
    };

    let src_sid = source_slide.as_deref().unwrap_or(&sid);
    let source_groups = collect_paste_source_groups(C, s.course.slide(src_sid), groups_param)?;

    let mut object_map: BTreeMap<String, String> = BTreeMap::new();
    let mut pasted: Vec<Object> = Vec::with_capacity(objects.len());
    let mut ids: Vec<String> = Vec::with_capacity(objects.len());

    for mut object in objects {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        object_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id.clone();
        object.frame.x += offset;
        object.frame.y += offset;
        clamp_frame(&mut object.frame, slide_size);
        if is_master {
            object.states.clear();
            object.initial_state = "normal".to_owned();
        }
        ids.push(new_id);
        pasted.push(object);
    }

    remap_object_glues(&mut pasted, &object_map);

    let mut group_map: BTreeMap<String, String> = BTreeMap::new();
    for object in &mut pasted {
        if let Some(old_group) = object.group.clone() {
            let new_group = group_map.entry(old_group).or_insert_with(|| {
                let fresh = fresh_id_in("group", &taken);
                taken.insert(fresh.clone());
                fresh
            });
            object.group = Some(new_group.clone());
        }
    }

    let mut pasted_groups: Vec<Group> = Vec::new();
    let mut trigger_targets = object_map.clone();
    if !is_master {
        let target_question = s.course.slide(&sid).and_then(|sl| sl.question.as_ref());
        for (old_group, new_group) in &group_map {
            let Some(src_group) = source_groups.get(old_group) else {
                continue;
            };
            let members: Vec<&Object> = pasted
                .iter()
                .filter(|o| o.group.as_deref() == Some(new_group.as_str()))
                .collect();
            if members.is_empty() {
                continue;
            }
            let mut g = src_group.clone();
            g.id = new_group.clone();
            if g.interactive {
                if members.iter().any(|m| {
                    coursecraft_course::is_disallowed_interactive_group_member(m, target_question)
                }) {
                    continue;
                }
                if !members_have_state(&members, &g.initial_state) {
                    g.initial_state = "normal".to_owned();
                }
                trigger_targets.insert(old_group.clone(), new_group.clone());
            }
            pasted_groups.push(g);
        }
    }

    let mut kept: Vec<Trigger> = Vec::with_capacity(triggers.len());
    let mut skipped = 0usize;
    for mut trigger in triggers {
        if is_master {
            // A master has no play-time behaviour, so its triggers are dropped.
            skipped += 1;
            continue;
        }
        let owner = if same_slide { layer.as_deref() } else { None };
        if remap_trigger(
            &mut trigger,
            &trigger_targets,
            &layers,
            &s.course,
            None,
            owner,
            false,
        ) {
            kept.push(trigger);
        } else {
            skipped += 1;
        }
    }

    let slide = slide_mut(s, &sid)?;
    match &layer {
        Some(lid) => {
            let target = layer_mut(slide, lid)?;
            target.objects.extend(pasted);
            target.groups.extend(pasted_groups);
            target.triggers.extend(kept);
        }
        None => {
            slide.objects.extend(pasted);
            slide.groups.extend(pasted_groups);
            slide.triggers.extend(kept);
        }
    }

    keep_valid(s, was_valid)?;
    let first = ids.first().cloned().unwrap_or_default();
    Ok(json!({ "id": first, "ids": ids, "skipped": skipped }))
}

pub(super) fn object_set_frame(s: &mut Session, p: &Json) -> Result<Json> {
    let sid: String = req("object.set_frame", p, "slide")?;
    let oid: String = req("object.set_frame", p, "object")?;
    let frame: Rect = req("object.set_frame", p, "frame")?;
    {
        let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
        if obj.frame == frame {
            s.skip_undo = true;
            return Ok(Json::Null);
        }
    }
    let was_valid = s.is_playable();
    let obj = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    let width_changed =
        frame.w.is_finite() && obj.frame.w.is_finite() && (frame.w - obj.frame.w).abs() > 0.5;
    let height_changed =
        frame.h.is_finite() && obj.frame.h.is_finite() && (frame.h - obj.frame.h).abs() > 0.5;
    obj.frame = frame;
    if let ObjectKind::Text {
        text,
        runs,
        paragraphs,
        autofit,
        point_text,
        style,
        ..
    } = &mut obj.kind
    {
        if width_changed && *point_text {
            *point_text = false;
            style.wrap = true;
        } else if width_changed && !height_changed && !*point_text && *autofit == Autofit::Grow {
            super::text::fit_grow_text_frame(
                &mut obj.frame,
                text,
                runs,
                style,
                paragraphs,
                None,
                false,
            );
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Read `objects` when present, else the single `object` id. Deduplicated and
/// bounded, so one command can act on a multi-selection.
fn object_or_objects_param(c: &str, p: &Json) -> Result<Vec<String>> {
    if let Some(objects) = opt::<Vec<String>>(c, p, "objects")? {
        let mut unique: Vec<String> = Vec::new();
        for id in objects {
            if !unique.contains(&id) {
                unique.push(id);
            }
        }
        if unique.is_empty() {
            return Err(bad(c, "`objects` is empty"));
        }
        return Ok(unique);
    }
    Ok(vec![req(c, p, "object")?])
}

pub(super) fn object_set_rotation(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_rotation";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let degrees: f32 = req(C, p, "degrees")?;
    if !degrees.is_finite() {
        return Err(bad(C, "`degrees` must be finite"));
    }
    let rotation = coursecraft_course::normalize_rotation(degrees);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.rotation != rotation {
                object.rotation = rotation;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "rotation": rotation, "changed": changed }))
}

pub(super) fn object_flip(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.flip";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let axis: String = req(C, p, "axis")?;
    let horizontal = match axis.as_str() {
        "horizontal" => true,
        "vertical" => false,
        _ => return Err(bad(C, "`axis` must be horizontal or vertical")),
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if horizontal {
                object.flip_h = !object.flip_h;
                changed += 1;
            } else {
                object.flip_v = !object.flip_v;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_lock(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.lock";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let locked: bool = req(C, p, "locked")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.locked != locked {
                object.locked = locked;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_hide_in_editor(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.hide_in_editor";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let hidden: bool = req(C, p, "hidden")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.editor_hidden != hidden {
                object.editor_hidden = hidden;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

/// The appearance of one object, as copied by the format painter.
struct Look {
    opacity: f32,
    shadow: Option<Shadow>,
    glow: Option<Glow>,
    fill: Option<Color>,
    fill_style: Option<Option<FillStyle>>,
    outline: Option<Option<Outline>>,
    text_style: Option<TextStyle>,
    /// The highlight and shadow the source's text has all through.
    run_extras: RunExtras,
}

/// Run formatting the format painter carries with the look: a highlight and a shadow
/// that every run of the source text has.
#[derive(Default, Clone, PartialEq)]
struct RunExtras {
    highlight: Option<Color>,
    shadow: bool,
}

fn run_extras_of(runs: &[TextRun]) -> RunExtras {
    let Some(first) = runs.first() else {
        return RunExtras::default();
    };
    let extras = RunExtras {
        highlight: first.highlight.clone(),
        shadow: first.shadow.unwrap_or(false),
    };
    let uniform = runs
        .iter()
        .all(|r| r.highlight == extras.highlight && r.shadow.unwrap_or(false) == extras.shadow);
    if uniform {
        extras
    } else {
        RunExtras::default()
    }
}

/// Clear the run look, then give every character the source's highlight and shadow.
fn paint_runs(text: &str, runs: &mut Vec<TextRun>, extras: &RunExtras) {
    coursecraft_course::clear_run_look(runs);
    if extras.highlight.is_none() && !extras.shadow {
        return;
    }
    let patch = coursecraft_course::TextRunPatch {
        highlight: Some(extras.highlight.clone()),
        shadow: extras.shadow.then_some(true),
        ..coursecraft_course::TextRunPatch::default()
    };
    *runs = coursecraft_course::apply_style_to_range(runs, text, (0, 0), &patch);
}

fn look_of(object: &Object) -> Look {
    let mut look = Look {
        opacity: object.opacity,
        shadow: object.shadow.clone(),
        glow: object.glow.clone(),
        fill: None,
        fill_style: None,
        outline: None,
        text_style: None,
        run_extras: RunExtras::default(),
    };
    match &object.kind {
        ObjectKind::Text { style, runs, .. } => {
            look.text_style = Some(style.clone());
            look.run_extras = run_extras_of(runs);
        }
        ObjectKind::Shape {
            fill,
            fill_style,
            outline,
            text,
            ..
        } => {
            look.fill = Some(fill.clone());
            look.fill_style = Some(fill_style.clone());
            look.outline = Some(outline.clone());
            look.text_style = text.as_ref().map(|t| t.style.clone());
            if let Some(t) = text {
                look.run_extras = run_extras_of(&t.runs);
            }
        }
        ObjectKind::Button {
            fill, fill_style, ..
        } => {
            look.fill = Some(fill.clone());
            look.fill_style = Some(fill_style.clone());
        }
        ObjectKind::Path { path } => {
            if path.closed {
                look.fill = path.fill.clone();
                look.fill_style = Some(path.fill_style.clone());
            }
            look.outline = Some(Some(path.outline.clone()));
            look.text_style = path.text.as_ref().map(|t| t.style.clone());
            if let Some(t) = &path.text {
                look.run_extras = run_extras_of(&t.runs);
            }
        }
        _ => {}
    }
    look
}

/// Returns true when anything changed.
fn apply_look(object: &mut Object, look: &Look) -> bool {
    let before = object.clone();
    object.opacity = look.opacity;
    object.shadow = look.shadow.clone();
    object.glow = look.glow.clone();
    match &mut object.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            point_text,
            ..
        } => {
            if let Some(src) = &look.text_style {
                let was_point_text = *point_text;
                *style = src.clone();
                paint_runs(text, runs, &look.run_extras);
                if was_point_text {
                    style.wrap = false;
                }
            }
        }
        ObjectKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            text,
            ..
        } => {
            if let Some(src) = &look.fill {
                *fill = src.clone();
            }
            if let Some(src) = &look.fill_style {
                *fill_style = src.clone();
            }
            // Line shapes cannot carry an outline or text, so a look from
            // another shape leaves them out.
            if let Some(src) = &look.outline
                && shape_allows_text(*shape)
            {
                *outline = src.clone();
            }
            if let (Some(src), Some(t)) = (&look.text_style, text.as_mut()) {
                t.style = src.clone();
                paint_runs(&t.text, &mut t.runs, &look.run_extras);
            }
        }
        ObjectKind::Button {
            fill, fill_style, ..
        } => {
            if let Some(src) = &look.fill {
                *fill = src.clone();
            }
            if let Some(src) = &look.fill_style {
                *fill_style = src.clone();
            }
        }
        ObjectKind::Path { path } => {
            if path.closed {
                if let Some(src) = &look.fill {
                    path.fill = Some(src.clone());
                }
                if let Some(src) = &look.fill_style {
                    path.fill_style = src.clone();
                }
            }
            if let Some(Some(src)) = &look.outline {
                path.outline = src.clone();
            }
            if let (Some(src), Some(t)) = (&look.text_style, path.text.as_mut()) {
                t.style = src.clone();
                paint_runs(&t.text, &mut t.runs, &look.run_extras);
            }
        }
        _ => {}
    }
    if look.text_style.is_some()
        && let ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            point_text,
            ..
        } = &object.kind
    {
        if *point_text && !style.wrap {
            super::text::fit_point_text_frame(
                &mut object.frame,
                text,
                runs,
                style,
                paragraphs,
                None,
                false,
            );
        } else if !*point_text && *autofit == Autofit::Grow {
            super::text::fit_grow_text_frame(
                &mut object.frame,
                text,
                runs,
                style,
                paragraphs,
                None,
                false,
            );
        }
    }
    *object != before
}

pub(super) fn object_apply_style(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.apply_style";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let source_slide: String = req(C, p, "source_slide")?;
    let source: String = req(C, p, "source")?;
    let ids = object_or_objects_param(C, p)?;
    let look = {
        let slide = s.course.slide(&source_slide).ok_or_else(|| {
            EngineError::NotFound(format!("slide {source_slide:?} does not exist"))
        })?;
        let (object, _) = slide.object(&source).ok_or_else(|| {
            EngineError::NotFound(format!(
                "object {source:?} is not on slide {source_slide:?}"
            ))
        })?;
        look_of(object)
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if apply_look(object, &look) {
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_opacity(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_opacity";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let opacity: f32 = req(C, p, "opacity")?;
    if !opacity.is_finite() {
        return Err(bad(C, "`opacity` must be finite"));
    }
    let opacity = coursecraft_course::clamp_opacity(opacity);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if (object.opacity - opacity).abs() > f32::EPSILON {
                object.opacity = opacity;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "opacity": opacity, "changed": changed }))
}

pub(super) fn object_set_outline_join(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_outline_join";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let join: coursecraft_course::LineJoin = req(C, p, "join")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            match &mut object.kind {
                ObjectKind::Shape {
                    outline: Some(outline),
                    ..
                } if outline.join != join => {
                    outline.join = join;
                    changed += 1;
                }
                ObjectKind::Path { path } if path.outline.join != join => {
                    path.outline.join = join;
                    changed += 1;
                }
                _ => {}
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "join": join, "changed": changed }))
}

pub(super) fn object_set_fill_style(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_fill_style";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let fill_style: Option<FillStyle> = opt(C, p, "fill_style")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let slot = match &mut object.kind {
                ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
                    Some(fill_style)
                }
                _ => None,
            };
            let Some(slot) = slot else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a shape or button")));
                }
                continue;
            };
            if *slot != fill_style {
                *slot = fill_style.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_shadow(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_shadow";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let shadow: Option<Shadow> = opt(C, p, "shadow")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.shadow != shadow {
                object.shadow = shadow.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_glow(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_glow";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let glow: Option<Glow> = opt(C, p, "glow")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.glow != glow {
                object.glow = glow.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_drag(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_drag";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let drag: Option<coursecraft_course::DragSpec> = opt(C, p, "drag")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.drag != drag {
                object.drag = drag.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_lottie(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_lottie";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let loop_playback: Option<bool> = opt(C, p, "loop")?;
    let autoplay: Option<bool> = opt(C, p, "autoplay")?;
    let speed: Option<f32> = opt(C, p, "speed")?;
    if speed.is_some_and(|speed| !speed.is_finite()) {
        return Err(bad(C, "`speed` must be finite"));
    }
    let speed = speed.map(coursecraft_course::clamp_lottie_speed);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let ObjectKind::Lottie {
                loop_playback: current_loop,
                autoplay: current_autoplay,
                speed: current_speed,
                ..
            } = &mut object.kind
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a Lottie object")));
                }
                continue;
            };
            let mut object_changed = false;
            if let Some(value) = loop_playback
                && *current_loop != value
            {
                *current_loop = value;
                object_changed = true;
            }
            if let Some(value) = autoplay
                && *current_autoplay != value
            {
                *current_autoplay = value;
                object_changed = true;
            }
            if let Some(value) = speed
                && (*current_speed - value).abs() > f32::EPSILON
            {
                *current_speed = value;
                object_changed = true;
            }
            if object_changed {
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_web_source(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_web_source";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let source: WebSource = req(C, p, "source")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let ObjectKind::Web { source: current } = &mut object.kind else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a web object")));
                }
                continue;
            };
            if *current != source {
                *current = source.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

#[allow(clippy::cognitive_complexity)]
pub(super) fn object_set_input(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_input";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let parse = |key: &str| -> Result<Option<Json>> { Ok(patch.get(key).cloned()) };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            match &mut next {
                ObjectKind::TextEntry {
                    variable,
                    placeholder,
                    multiline,
                    max_chars,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("placeholder")? {
                        *placeholder = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.placeholder: {e}")))?;
                    }
                    if let Some(value) = parse("multiline")? {
                        *multiline = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.multiline: {e}")))?;
                    }
                    if let Some(value) = parse("max_chars")? {
                        *max_chars = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max_chars: {e}")))?;
                    }
                }
                ObjectKind::Slider {
                    variable,
                    min,
                    max,
                    step,
                    orientation,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("min")? {
                        *min = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.min: {e}")))?;
                    }
                    if let Some(value) = parse("max")? {
                        *max = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max: {e}")))?;
                    }
                    if let Some(value) = parse("step")? {
                        *step = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.step: {e}")))?;
                    }
                    if let Some(value) = parse("orientation")? {
                        *orientation = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.orientation: {e}")))?;
                    }
                }
                ObjectKind::Dial {
                    variable,
                    min,
                    max,
                    step,
                    start_angle,
                    end_angle,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("min")? {
                        *min = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.min: {e}")))?;
                    }
                    if let Some(value) = parse("max")? {
                        *max = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max: {e}")))?;
                    }
                    if let Some(value) = parse("step")? {
                        *step = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.step: {e}")))?;
                    }
                    if let Some(value) = parse("start_angle")? {
                        *start_angle = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.start_angle: {e}")))?;
                    }
                    if let Some(value) = parse("end_angle")? {
                        *end_angle = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.end_angle: {e}")))?;
                    }
                }
                ObjectKind::Checkbox { variable, label } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("label")? {
                        *label = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.label: {e}")))?;
                    }
                }
                ObjectKind::RadioGroup { variable, options } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("options")? {
                        *options = serde_json::from_value::<Vec<RadioOption>>(value)
                            .map_err(|e| bad(C, format!("patch.options: {e}")))?;
                    }
                }
                _ => {
                    if single {
                        return Err(bad(C, format!("object {id:?} is not an input")));
                    }
                    continue;
                }
            }
            // Validate through the model's own repair and range rules.
            if let ObjectKind::Slider { min, max, .. } | ObjectKind::Dial { min, max, .. } =
                &mut next
            {
                *min = if min.is_finite() { *min } else { 0.0 };
                *max = if max.is_finite() { *max } else { 100.0 };
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_marker(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_marker";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let parse = |key: &str| -> Result<Option<Json>> { Ok(patch.get(key).cloned()) };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::Marker {
                title,
                body,
                runs,
                paragraphs,
                icon,
                color,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a marker")));
                }
                continue;
            };
            if let Some(value) = parse("title")? {
                *title = text(
                    C,
                    serde_json::from_value(value)
                        .map_err(|e| bad(C, format!("patch.title: {e}")))?,
                    "patch.title",
                )?;
            }
            if let Some(value) = parse("body")? {
                *body = text(
                    C,
                    serde_json::from_value(value)
                        .map_err(|e| bad(C, format!("patch.body: {e}")))?,
                    "patch.body",
                )?;
                if patch.get("runs").is_none() {
                    runs.clear();
                }
            }
            if let Some(value) = parse("runs")? {
                *runs = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.runs: {e}")))?;
            }
            if let Some(value) = parse("paragraphs")? {
                *paragraphs = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.paragraphs: {e}")))?;
            }
            if let Some(value) = parse("icon")? {
                *icon = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.icon: {e}")))?;
            }
            if let Some(value) = parse("color")? {
                *color = match value {
                    Json::Null => None,
                    _ => Some(
                        serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.color: {e}")))?,
                    ),
                };
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_panel(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_panel";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let panel = match obj(C, p)?
        .get("panel")
        .ok_or_else(|| bad(C, "missing `panel`"))?
    {
        Json::Null => None,
        Json::String(id) => Some(id.clone()),
        _ => return Err(bad(C, "`panel` must be an id or null")),
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let layer = slide
                .object(id)
                .map(|(_, layer)| layer.map(str::to_owned))
                .ok_or_else(|| {
                    EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
                })?;
            if let Some(panel_id) = panel.as_deref() {
                if panel_id == id {
                    return Err(bad(C, "an object cannot be inside itself"));
                }
                let target = slide.object(panel_id).map(|(object, layer)| {
                    (
                        matches!(object.kind, ObjectKind::ScrollPanel { .. }),
                        layer.map(str::to_owned),
                    )
                });
                match target {
                    None => {
                        return Err(EngineError::NotFound(format!(
                            "panel {panel_id:?} is not on slide {sid:?}"
                        )));
                    }
                    Some((false, _)) => {
                        return Err(bad(
                            C,
                            format!("object {panel_id:?} is not a scrolling panel"),
                        ));
                    }
                    Some((true, target_layer)) if target_layer != layer => {
                        return Err(bad(
                            C,
                            format!("panel {panel_id:?} is on a different layer"),
                        ));
                    }
                    Some((true, _)) => {}
                }
            }
            let object = find_object_mut(slide, id)?;
            if panel.is_some() && matches!(object.kind, ObjectKind::ScrollPanel { .. }) {
                return Err(bad(
                    C,
                    format!("scrolling panel {id:?} cannot be inside another panel"),
                ));
            }
            if object.panel != panel {
                object.panel = panel.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_scroll_panel(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_scroll_panel";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::ScrollPanel {
                content_height,
                bar,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a scrolling panel")));
                }
                continue;
            };
            if let Some(value) = patch.get("content_height") {
                let value: f32 = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.content_height: {e}")))?;
                if !value.is_finite() {
                    return Err(bad(C, "patch.content_height must be finite"));
                }
                *content_height = coursecraft_course::safe_clamp(
                    value,
                    object.frame.h.max(0.0),
                    coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT,
                );
            }
            if let Some(value) = patch.get("bar") {
                *bar = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.bar: {e}")))?;
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_timer(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_timer";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::Timer {
                duration_ms,
                format,
                autostart,
                style,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a timer")));
                }
                continue;
            };
            if let Some(value) = patch.get("duration_ms") {
                let value: i64 = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.duration_ms: {e}")))?;
                *duration_ms =
                    value.clamp(1, i64::from(coursecraft_course::MAX_TIMER_DURATION_MS)) as u32;
            }
            if let Some(value) = patch.get("format") {
                *format = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.format: {e}")))?;
            }
            if let Some(value) = patch.get("autostart") {
                *autostart = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.autostart: {e}")))?;
            }
            if let Some(value) = patch.get("style") {
                *style = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.style: {e}")))?;
                if !style.size.is_finite() || style.size <= 0.0 {
                    style.size = coursecraft_course::TextStyle::default().size;
                }
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_meter(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_meter";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::ProgressMeter {
                variable,
                min,
                max,
                style,
                show_value,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a progress meter")));
                }
                continue;
            };
            if let Some(value) = patch.get("variable") {
                *variable = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
            }
            if let Some(value) = patch.get("min") {
                *min = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.min: {e}")))?;
            }
            if let Some(value) = patch.get("max") {
                *max = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.max: {e}")))?;
            }
            if let Some(value) = patch.get("style") {
                *style = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.style: {e}")))?;
            }
            if let Some(value) = patch.get("show_value") {
                *show_value = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.show_value: {e}")))?;
            }
            // Repair non-finite bounds the way loading does.
            *min = if min.is_finite() { *min } else { 0.0 };
            *max = if max.is_finite() { *max } else { 100.0 };
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

pub(super) fn object_set_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_path";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let points: Vec<coursecraft_course::Point> = req(C, p, "points")?;
    if points.len() < coursecraft_course::PATH_MIN_POINTS
        || points.len() > coursecraft_course::PATH_MAX_POINTS
    {
        return Err(bad(
            C,
            format!(
                "`points` must have {} to {} entries",
                coursecraft_course::PATH_MIN_POINTS,
                coursecraft_course::PATH_MAX_POINTS
            ),
        ));
    }
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(bad(C, "`points` must all be finite"));
    }
    let (frame, fractions) = coursecraft_course::path_frame_and_points(&points)
        .ok_or_else(|| bad(C, "`points` cannot form a path frame"))?;
    let params = obj(C, p)?;
    let start_present = params.contains_key("start_glue");
    let end_present = params.contains_key("end_glue");
    let start_glue: Option<Glue> = opt(C, p, "start_glue")?;
    let end_glue: Option<Glue> = opt(C, p, "end_glue")?;
    let object = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    match &mut object.kind {
        ObjectKind::Path { path } => {
            path.points = fractions;
            if start_present {
                path.start_glue = start_glue;
            }
            if end_present {
                path.end_glue = end_glue;
            }
            if path.connector.is_none() {
                path.start_glue = None;
                path.end_glue = None;
            }
        }
        _ => return Err(bad(C, format!("object {oid:?} is not a path"))),
    }
    object.frame = frame;
    keep_valid(s, was_valid)?;
    Ok(json!({ "frame": frame }))
}

pub(super) fn object_set_crop(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_crop";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let raw = obj(C, p)?
        .get("crop")
        .ok_or_else(|| bad(C, "missing `crop`"))?;
    let crop: Option<CropRect> = match raw {
        Json::Null => None,
        value => Some(
            serde_json::from_value(value.clone()).map_err(|e| bad(C, format!("`crop`: {e}")))?,
        ),
    };
    if let Some(rect) = crop {
        check_crop(rect)?;
    }
    // A crop that keeps the whole picture is stored as no crop, so it draws and
    // saves exactly like one.
    let crop = crop.filter(|rect| !rect.is_full());
    let frame: Option<Rect> = opt(C, p, "frame")?;
    if let Some(frame) = frame
        && (![frame.x, frame.y, frame.w, frame.h]
            .iter()
            .all(|v| v.is_finite())
            || frame.w < 0.0
            || frame.h < 0.0)
    {
        return Err(bad(C, "`frame` values must be finite and w/h non-negative"));
    }

    let object = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    match &mut object.kind {
        ObjectKind::Image {
            crop: current_crop, ..
        } => *current_crop = crop,
        _ => return Err(bad(C, format!("object {oid:?} is not an image"))),
    }
    if let Some(frame) = frame {
        object.frame = frame;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Reject a crop that is not a positive, finite window inside the source picture.
fn check_crop(rect: CropRect) -> Result<()> {
    const C: &str = "object.set_crop";
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return Err(bad(C, "`crop` values must be finite"));
    }
    if x < 0.0 || y < 0.0 || x > 1.0 || y > 1.0 || w > 1.0 || h > 1.0 {
        return Err(bad(C, "`crop` x, y, w and h must be fractions in 0..=1"));
    }
    if w <= 0.0 || h <= 0.0 {
        return Err(bad(C, "`crop` w and h must be greater than 0"));
    }
    if x + w > 1.000_001 || y + h > 1.000_001 {
        return Err(bad(
            C,
            "`crop` must lie inside the source (x + w, y + h <= 1)",
        ));
    }
    Ok(())
}

pub(super) fn object_set_captions(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_captions";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let value = obj(C, p)?
        .get("captions")
        .ok_or_else(|| bad(C, "missing `captions`"))?;
    let captions = match value {
        Json::Null => None,
        Json::String(path) => {
            let path = text(C, path.clone(), "captions")?;
            if path.trim().is_empty() {
                None
            } else {
                Some(path)
            }
        }
        _ => return Err(bad(C, "`captions`: expected string or null")),
    };

    let slide = slide_mut(s, &sid)?;
    let object = find_object_mut(slide, &oid)?;
    match &mut object.kind {
        ObjectKind::Audio {
            captions: current, ..
        }
        | ObjectKind::Video {
            captions: current, ..
        } => *current = captions,
        _ => {
            return Err(bad(C, format!("object {oid:?} is not audio or video")));
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_set_media(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_media";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let volume: Option<f32> = opt(C, p, "volume")?;
    if let Some(volume) = volume
        && (!volume.is_finite() || !(0.0..=1.0).contains(&volume))
    {
        return Err(bad(C, "`volume` must be finite and within 0..=1"));
    }
    let trim = match obj(C, p)?.get("trim") {
        None => None,
        Some(Json::Null) => Some(None),
        Some(value) => {
            let trim: Trim = serde_json::from_value(value.clone())
                .map_err(|e| bad(C, format!("`trim`: {e}")))?;
            if trim.start_ms > coursecraft_course::MAX_TRIM_MS {
                return Err(bad(
                    C,
                    format!(
                        "`trim.start_ms` must be at most {}",
                        coursecraft_course::MAX_TRIM_MS
                    ),
                ));
            }
            if let Some(end_ms) = trim.end_ms {
                if end_ms > coursecraft_course::MAX_TRIM_MS {
                    return Err(bad(
                        C,
                        format!(
                            "`trim.end_ms` must be at most {}",
                            coursecraft_course::MAX_TRIM_MS
                        ),
                    ));
                }
                if end_ms <= trim.start_ms {
                    return Err(bad(C, "`trim.end_ms` must be after `trim.start_ms`"));
                }
            }
            Some(Some(trim))
        }
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        let object = find_object_mut(slide, &oid)?;
        let (current_volume, current_trim) = match &mut object.kind {
            ObjectKind::Audio { volume, trim, .. } | ObjectKind::Video { volume, trim, .. } => {
                (volume, trim)
            }
            _ => {
                return Err(EngineError::Invalid(format!(
                    "object {oid:?} is not audio or video"
                )));
            }
        };
        if let Some(volume) = volume
            && (*current_volume - volume).abs() > f32::EPSILON
        {
            *current_volume = volume;
            changed += 1;
        }
        if let Some(trim) = trim
            && *current_trim != trim
        {
            *current_trim = trim;
            changed += 1;
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}
