//! Reversible structural deltas for undo/redo in [`crate::Session`].
//!
//! Every [`SlideDelta::apply`] and [`CourseHeaderPatch::apply`] swaps old and
//! new values in place so each delta becomes its own exact inverse with zero
//! allocations and byte-identical `course.json` restoration.
//!
//! `Course`, `Slide`, and `Layer` are destructured exhaustively (no `..`) so
//! adding any future model field fails to compile until delta support handles
//! it.

use std::collections::BTreeMap;
use std::mem::size_of;

use coursecraft_course::{
    BackgroundAudio, BankDraw, Color, Course, EditorSettings, FontFace, GlossaryEntry, Group,
    Layer, MotionPath, NamedTextStyle, Object, ObjectKind, PlayerSettings, Question, QuestionBank,
    QuizSettings, Resource, RevisitMode, Scene, Size, Slide, SlideCue, SlidePlayerOverrides,
    StateDef, Theme, TrackingSettings, Transition, Trigger, VariableDef,
};

use crate::{EngineError, Result};

/// Pre-edit snapshot scope declared for a command.
///
/// Unknown or unannotated commands default to [`UndoScope::FullCourse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoScope {
    FullCourse,
    SingleSlide,
    CourseHeader,
}

/// Identifies one slide anywhere inside a [`Course`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlideLoc {
    SceneSlide { scene: usize, slide: usize },
    Master(usize),
    Template(usize),
    BankSlide { bank: usize, slide: usize },
}

impl SlideLoc {
    /// Locate a slide by id across scenes, masters, templates, and banks.
    #[must_use]
    pub(crate) fn find(course: &Course, slide_id: &str) -> Option<Self> {
        for (scene_idx, scene) in course.scenes.iter().enumerate() {
            if let Some(slide_idx) = scene.slides.iter().position(|s| s.id == slide_id) {
                return Some(Self::SceneSlide {
                    scene: scene_idx,
                    slide: slide_idx,
                });
            }
        }
        if let Some(idx) = course.masters.iter().position(|s| s.id == slide_id) {
            return Some(Self::Master(idx));
        }
        if let Some(idx) = course.templates.iter().position(|s| s.id == slide_id) {
            return Some(Self::Template(idx));
        }
        for (bank_idx, bank) in course.banks.iter().enumerate() {
            if let Some(slide_idx) = bank.slides.iter().position(|s| s.id == slide_id) {
                return Some(Self::BankSlide {
                    bank: bank_idx,
                    slide: slide_idx,
                });
            }
        }
        None
    }

    /// Locate an editable slide by id in the exact precedence order used by
    /// [`Course::slide_mut`]: scenes, then banks, then masters (never templates).
    #[must_use]
    pub(crate) fn find_editable(course: &Course, slide_id: &str) -> Option<Self> {
        for (scene_idx, scene) in course.scenes.iter().enumerate() {
            if let Some(slide_idx) = scene.slides.iter().position(|s| s.id == slide_id) {
                return Some(Self::SceneSlide {
                    scene: scene_idx,
                    slide: slide_idx,
                });
            }
        }
        for (bank_idx, bank) in course.banks.iter().enumerate() {
            if let Some(slide_idx) = bank.slides.iter().position(|s| s.id == slide_id) {
                return Some(Self::BankSlide {
                    bank: bank_idx,
                    slide: slide_idx,
                });
            }
        }
        if let Some(idx) = course.masters.iter().position(|s| s.id == slide_id) {
            return Some(Self::Master(idx));
        }
        None
    }
}

/// Borrow the slide at `loc`, returning `None` if any index is out of range.
#[must_use]
pub(crate) fn slide_at(course: &Course, loc: SlideLoc) -> Option<&Slide> {
    match loc {
        SlideLoc::SceneSlide { scene, slide } => {
            course.scenes.get(scene).and_then(|s| s.slides.get(slide))
        }
        SlideLoc::Master(idx) => course.masters.get(idx),
        SlideLoc::Template(idx) => course.templates.get(idx),
        SlideLoc::BankSlide { bank, slide } => {
            course.banks.get(bank).and_then(|b| b.slides.get(slide))
        }
    }
}

/// Mutably borrow the slide at `loc`, returning `None` if any index is out of range.
#[must_use]
pub(crate) fn slide_at_mut(course: &mut Course, loc: SlideLoc) -> Option<&mut Slide> {
    match loc {
        SlideLoc::SceneSlide { scene, slide } => course
            .scenes
            .get_mut(scene)
            .and_then(|s| s.slides.get_mut(slide)),
        SlideLoc::Master(idx) => course.masters.get_mut(idx),
        SlideLoc::Template(idx) => course.templates.get_mut(idx),
        SlideLoc::BankSlide { bank, slide } => course
            .banks
            .get_mut(bank)
            .and_then(|b| b.slides.get_mut(slide)),
    }
}

fn swap_if_some<T>(patch: &mut Option<T>, target: &mut T) {
    if let Some(prev) = patch.as_mut() {
        std::mem::swap(prev, target);
    }
}

fn diff_field<T: PartialEq>(before: T, after: &T, changed: &mut bool) -> Option<T> {
    if before != *after {
        *changed = true;
        Some(before)
    } else {
        None
    }
}

/// Sparse reversible patch over the 14 non-`objects`, non-`layers` fields of [`Slide`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SlideFieldPatch {
    pub(crate) id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) duration_ms: Option<u32>,
    pub(crate) background: Option<Option<Color>>,
    pub(crate) groups: Option<Vec<Group>>,
    pub(crate) triggers: Option<Vec<Trigger>>,
    pub(crate) cue_points: Option<Vec<SlideCue>>,
    pub(crate) question: Option<Option<Question>>,
    pub(crate) draw: Option<Option<BankDraw>>,
    pub(crate) transition: Option<Option<Transition>>,
    pub(crate) notes: Option<String>,
    pub(crate) master: Option<Option<String>>,
    pub(crate) revisit: Option<RevisitMode>,
    pub(crate) player: Option<Option<SlidePlayerOverrides>>,
}

impl SlideFieldPatch {
    /// Swap every present field in place with `slide`, turning `self` into its own inverse.
    pub(crate) fn apply(&mut self, slide: &mut Slide) {
        let Self {
            id: p_id,
            title: p_title,
            duration_ms: p_duration_ms,
            background: p_background,
            groups: p_groups,
            triggers: p_triggers,
            cue_points: p_cue_points,
            question: p_question,
            draw: p_draw,
            transition: p_transition,
            notes: p_notes,
            master: p_master,
            revisit: p_revisit,
            player: p_player,
        } = self;
        let Slide {
            id,
            title,
            duration_ms,
            background,
            objects: _,
            layers: _,
            groups,
            triggers,
            cue_points,
            question,
            draw,
            transition,
            notes,
            master,
            revisit,
            player,
        } = slide;
        swap_if_some(p_id, id);
        swap_if_some(p_title, title);
        swap_if_some(p_duration_ms, duration_ms);
        swap_if_some(p_background, background);
        swap_if_some(p_groups, groups);
        swap_if_some(p_triggers, triggers);
        swap_if_some(p_cue_points, cue_points);
        swap_if_some(p_question, question);
        swap_if_some(p_draw, draw);
        swap_if_some(p_transition, transition);
        swap_if_some(p_notes, notes);
        swap_if_some(p_master, master);
        swap_if_some(p_revisit, revisit);
        swap_if_some(p_player, player);
    }

    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let mut total = size_of::<Self>();
        if let Some(id) = &self.id {
            total = total.saturating_add(id.capacity());
        }
        if let Some(title) = &self.title {
            total = total.saturating_add(title.capacity());
        }
        if let Some(Some(bg)) = &self.background {
            total = total.saturating_add(bg.0.capacity());
        }
        if let Some(groups) = &self.groups {
            total = total.saturating_add(estimate_groups(groups));
        }
        if let Some(triggers) = &self.triggers {
            total = total.saturating_add(estimate_vec_plain(triggers));
        }
        if let Some(cues) = &self.cue_points {
            total = total.saturating_add(estimate_vec_plain(cues));
        }
        if let Some(Some(q)) = &self.question {
            total = total.saturating_add(estimate_question_heap(q));
        }
        if let Some(Some(draw)) = &self.draw {
            total = total.saturating_add(draw.bank.capacity());
        }
        if let Some(notes) = &self.notes {
            total = total.saturating_add(notes.capacity());
        }
        if let Some(Some(master)) = &self.master {
            total = total.saturating_add(master.capacity());
        }
        total
    }
}

/// Reversible change inside a single [`Slide`].
///
/// Calling [`SlideDelta::apply`] swaps old and new state in place so the delta
/// becomes its own exact inverse.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SlideDelta {
    /// One object modified in place (`layer: None` = base layer, `Some(li)` = `slide.layers[li]`).
    ObjectModified {
        layer: Option<usize>,
        index: usize,
        object: Box<Object>,
    },
    /// One object inserted at `index` (applying removes it -> becomes `ObjectRemoved`).
    ObjectInserted { layer: Option<usize>, index: usize },
    /// One object removed from `index` (applying inserts it -> becomes `ObjectInserted`).
    ObjectRemoved {
        layer: Option<usize>,
        index: usize,
        object: Box<Object>,
    },
    /// One object moved from `from` to `to` (applying moves `to -> from` and swaps `from`/`to`).
    ObjectReordered {
        layer: Option<usize>,
        from: usize,
        to: usize,
    },
    /// One or more slide metadata fields changed without replacing objects or layers.
    Fields(Box<SlideFieldPatch>),
    /// Fallback when multiple objects or layers changed in one step.
    FullSlide(Box<Slide>),
}

impl SlideDelta {
    /// Compute the smallest reversible delta that transforms `after` back into `before`.
    /// Returns `None` when `before == *after`.
    #[must_use]
    pub(crate) fn diff(before: Slide, after: &Slide) -> Option<Self> {
        let Slide {
            id: b_id,
            title: b_title,
            duration_ms: b_duration_ms,
            background: b_background,
            objects: b_objects,
            layers: mut b_layers,
            groups: b_groups,
            triggers: b_triggers,
            cue_points: b_cue_points,
            question: b_question,
            draw: b_draw,
            transition: b_transition,
            notes: b_notes,
            master: b_master,
            revisit: b_revisit,
            player: b_player,
        } = before;
        let Slide {
            id: a_id,
            title: a_title,
            duration_ms: a_duration_ms,
            background: a_background,
            objects: a_objects,
            layers: a_layers,
            groups: a_groups,
            triggers: a_triggers,
            cue_points: a_cue_points,
            question: a_question,
            draw: a_draw,
            transition: a_transition,
            notes: a_notes,
            master: a_master,
            revisit: a_revisit,
            player: a_player,
        } = after;

        let meta_changed = b_id != *a_id
            || b_title != *a_title
            || b_duration_ms != *a_duration_ms
            || b_background != *a_background
            || b_groups != *a_groups
            || b_triggers != *a_triggers
            || b_cue_points != *a_cue_points
            || b_question != *a_question
            || b_draw != *a_draw
            || b_transition != *a_transition
            || b_notes != *a_notes
            || b_master != *a_master
            || b_revisit != *a_revisit
            || b_player != *a_player;
        let objects_eq = b_objects == *a_objects;
        let layers_eq = b_layers == *a_layers;

        if !meta_changed && objects_eq && layers_eq {
            return None;
        }

        if meta_changed && objects_eq && layers_eq {
            let mut any = false;
            let patch = SlideFieldPatch {
                id: diff_field(b_id, a_id, &mut any),
                title: diff_field(b_title, a_title, &mut any),
                duration_ms: diff_field(b_duration_ms, a_duration_ms, &mut any),
                background: diff_field(b_background, a_background, &mut any),
                groups: diff_field(b_groups, a_groups, &mut any),
                triggers: diff_field(b_triggers, a_triggers, &mut any),
                cue_points: diff_field(b_cue_points, a_cue_points, &mut any),
                question: diff_field(b_question, a_question, &mut any),
                draw: diff_field(b_draw, a_draw, &mut any),
                transition: diff_field(b_transition, a_transition, &mut any),
                notes: diff_field(b_notes, a_notes, &mut any),
                master: diff_field(b_master, a_master, &mut any),
                revisit: diff_field(b_revisit, a_revisit, &mut any),
                player: diff_field(b_player, a_player, &mut any),
            };
            return Some(Self::Fields(Box::new(patch)));
        }

        let b_objects = if !meta_changed && !objects_eq && layers_eq {
            match diff_object_vec(None, b_objects, a_objects) {
                Ok(delta) => return Some(delta),
                Err(objs) => objs,
            }
        } else {
            b_objects
        };

        if !meta_changed
            && objects_eq
            && !layers_eq
            && let Some(delta) = diff_single_layer_objects(&mut b_layers, a_layers)
        {
            return Some(delta);
        }

        Some(Self::FullSlide(Box::new(Slide {
            id: b_id,
            title: b_title,
            duration_ms: b_duration_ms,
            background: b_background,
            objects: b_objects,
            layers: b_layers,
            groups: b_groups,
            triggers: b_triggers,
            cue_points: b_cue_points,
            question: b_question,
            draw: b_draw,
            transition: b_transition,
            notes: b_notes,
            master: b_master,
            revisit: b_revisit,
            player: b_player,
        })))
    }

    /// Apply this delta in place, mutating `self` into its own exact inverse.
    ///
    /// On `Err`, both `slide` and `self` are left untouched.
    pub(crate) fn apply(&mut self, slide: &mut Slide) -> Result<()> {
        match self {
            Self::ObjectModified {
                layer,
                index,
                object,
            } => {
                let objects = slide_objects_mut(slide, *layer)?;
                let len = objects.len();
                let slot = objects.get_mut(*index).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta object index {index} out of bounds for len {len}"
                    ))
                })?;
                std::mem::swap(slot, object.as_mut());
                Ok(())
            }
            Self::ObjectInserted { layer, index } => {
                let layer = *layer;
                let index = *index;
                let objects = slide_objects_mut(slide, layer)?;
                if index >= objects.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta object index {index} out of bounds for len {}",
                        objects.len()
                    )));
                }
                let removed = objects.remove(index);
                *self = Self::ObjectRemoved {
                    layer,
                    index,
                    object: Box::new(removed),
                };
                Ok(())
            }
            Self::ObjectRemoved { layer, index, .. } => {
                let layer = *layer;
                let index = *index;
                let objects = slide_objects_mut(slide, layer)?;
                if index > objects.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta object index {index} out of bounds for len {}",
                        objects.len()
                    )));
                }
                let prev = std::mem::replace(self, Self::ObjectInserted { layer, index });
                if let Self::ObjectRemoved { object, .. } = prev {
                    objects.insert(index, *object);
                }
                Ok(())
            }
            Self::ObjectReordered { layer, from, to } => {
                let objects = slide_objects_mut(slide, *layer)?;
                if *from >= objects.len() || *to >= objects.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta reorder ({from} -> {to}) out of bounds for len {}",
                        objects.len()
                    )));
                }
                if *from != *to {
                    let item = objects.remove(*to);
                    objects.insert(*from, item);
                    std::mem::swap(from, to);
                }
                Ok(())
            }
            Self::Fields(patch) => {
                patch.apply(slide);
                Ok(())
            }
            Self::FullSlide(prev) => {
                std::mem::swap(slide, prev.as_mut());
                Ok(())
            }
        }
    }

    /// Rough heap + stack footprint in bytes for memory-budget diagnostics.
    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let base = size_of::<Self>();
        match self {
            Self::ObjectModified { object, .. } | Self::ObjectRemoved { object, .. } => {
                base.saturating_add(estimate_object(object))
            }
            Self::ObjectInserted { .. } | Self::ObjectReordered { .. } => base,
            Self::Fields(patch) => base.saturating_add(patch.estimated_bytes()),
            Self::FullSlide(slide) => base.saturating_add(estimate_slide(slide)),
        }
    }
}

fn slide_objects_mut(slide: &mut Slide, layer: Option<usize>) -> Result<&mut Vec<Object>> {
    match layer {
        None => Ok(&mut slide.objects),
        Some(li) => {
            let layer_len = slide.layers.len();
            slide
                .layers
                .get_mut(li)
                .map(|l| &mut l.objects)
                .ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta layer index {li} out of bounds for len {layer_len}"
                    ))
                })
        }
    }
}

/// Check whether two [`Layer`] values are identical in all 13 non-`objects` fields.
/// Destructures [`Layer`] exhaustively without `..`.
fn layer_metadata_eq(before: &Layer, after: &Layer) -> bool {
    let Layer {
        id: b_id,
        name: b_name,
        duration_ms: b_duration_ms,
        objects: _,
        triggers: b_triggers,
        hide_other_layers: b_hide_other_layers,
        pause_base_timeline: b_pause_base_timeline,
        prevent_base_clicks: b_prevent_base_clicks,
        hide_base_objects: b_hide_base_objects,
        hide_when_timeline_ends: b_hide_when_timeline_ends,
        allow_seek: b_allow_seek,
        revisit: b_revisit,
        cue_points: b_cue_points,
        groups: b_groups,
    } = before;
    let Layer {
        id: a_id,
        name: a_name,
        duration_ms: a_duration_ms,
        objects: _,
        triggers: a_triggers,
        hide_other_layers: a_hide_other_layers,
        pause_base_timeline: a_pause_base_timeline,
        prevent_base_clicks: a_prevent_base_clicks,
        hide_base_objects: a_hide_base_objects,
        hide_when_timeline_ends: a_hide_when_timeline_ends,
        allow_seek: a_allow_seek,
        revisit: a_revisit,
        cue_points: a_cue_points,
        groups: a_groups,
    } = after;
    b_id == a_id
        && b_name == a_name
        && b_duration_ms == a_duration_ms
        && b_triggers == a_triggers
        && b_hide_other_layers == a_hide_other_layers
        && b_pause_base_timeline == a_pause_base_timeline
        && b_prevent_base_clicks == a_prevent_base_clicks
        && b_hide_base_objects == a_hide_base_objects
        && b_hide_when_timeline_ends == a_hide_when_timeline_ends
        && b_allow_seek == a_allow_seek
        && b_revisit == a_revisit
        && b_cue_points == a_cue_points
        && b_groups == a_groups
}

/// If exactly one layer differs and only in its `objects` vec, try to produce a
/// compact object-level [`SlideDelta`]. Restores `b_layers` intact on fallback.
fn diff_single_layer_objects(b_layers: &mut [Layer], a_layers: &[Layer]) -> Option<SlideDelta> {
    if b_layers.len() != a_layers.len() {
        return None;
    }
    let mut changed_idx = None;
    for (i, (b, a)) in b_layers.iter().zip(a_layers.iter()).enumerate() {
        if b != a {
            if changed_idx.is_some() {
                return None;
            }
            changed_idx = Some(i);
        }
    }
    let li = changed_idx?;
    let b_layer = b_layers.get_mut(li)?;
    let a_layer = a_layers.get(li)?;
    if !layer_metadata_eq(b_layer, a_layer) {
        return None;
    }
    let before_objs = std::mem::take(&mut b_layer.objects);
    match diff_object_vec(Some(li), before_objs, &a_layer.objects) {
        Ok(delta) => Some(delta),
        Err(objs) => {
            b_layer.objects = objs;
            None
        }
    }
}

/// Diff two object lists for single modify, insert, remove, or reorder.
/// On fallback, returns `Err(before)` so the caller retains ownership with zero clones.
fn diff_object_vec(
    layer: Option<usize>,
    mut before: Vec<Object>,
    after: &[Object],
) -> std::result::Result<SlideDelta, Vec<Object>> {
    if before.len() == after.len() {
        return diff_object_vec_same_len(layer, before, after);
    }
    if after.len() == before.len().saturating_add(1) && after.len() > before.len() {
        let idx = before
            .iter()
            .zip(after.iter())
            .position(|(b, a)| b != a)
            .unwrap_or(before.len());
        if before.get(idx..) == after.get(idx.saturating_add(1)..) {
            return Ok(SlideDelta::ObjectInserted { layer, index: idx });
        }
        return Err(before);
    }
    if before.len() == after.len().saturating_add(1) && before.len() > after.len() {
        let idx = before
            .iter()
            .zip(after.iter())
            .position(|(b, a)| b != a)
            .unwrap_or(after.len());
        if before.get(idx.saturating_add(1)..) == after.get(idx..) && idx < before.len() {
            let removed = before.swap_remove(idx);
            return Ok(SlideDelta::ObjectRemoved {
                layer,
                index: idx,
                object: Box::new(removed),
            });
        }
        return Err(before);
    }
    Err(before)
}

fn diff_object_vec_same_len(
    layer: Option<usize>,
    mut before: Vec<Object>,
    after: &[Object],
) -> std::result::Result<SlideDelta, Vec<Object>> {
    let mut first = None;
    let mut last = 0;
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        if b != a {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
        }
    }
    let Some(a) = first else {
        return Err(before);
    };
    if a == last && a < before.len() {
        let prev_obj = before.swap_remove(a);
        return Ok(SlideDelta::ObjectModified {
            layer,
            index: a,
            object: Box::new(prev_obj),
        });
    }
    let b = last;
    if before.get(a) == after.get(b) && before.get(a.saturating_add(1)..=b) == after.get(a..b) {
        return Ok(SlideDelta::ObjectReordered {
            layer,
            from: a,
            to: b,
        });
    }
    if before.get(b) == after.get(a) && before.get(a..b) == after.get(a.saturating_add(1)..=b) {
        return Ok(SlideDelta::ObjectReordered {
            layer,
            from: b,
            to: a,
        });
    }
    Err(before)
}

/// Snapshot of the 17 non-slide header fields of [`Course`].
///
/// Capturing this clones zero scenes, masters, templates, or question banks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CourseHeaderSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) slide_size: Size,
    pub(crate) variables: Vec<VariableDef>,
    pub(crate) start_slide: Option<String>,
    pub(crate) quiz: Option<QuizSettings>,
    pub(crate) tracking: TrackingSettings,
    pub(crate) player: PlayerSettings,
    pub(crate) theme: Theme,
    pub(crate) language: Option<String>,
    pub(crate) strings: BTreeMap<String, String>,
    pub(crate) fonts: Vec<FontFace>,
    pub(crate) background_audio: Option<BackgroundAudio>,
    pub(crate) text_styles: Vec<NamedTextStyle>,
    pub(crate) resources: Vec<Resource>,
    pub(crate) glossary: Vec<GlossaryEntry>,
    pub(crate) editor: EditorSettings,
}

impl CourseHeaderSnapshot {
    /// Clone all 17 header fields from `course` without touching slide collections.
    #[must_use]
    pub(crate) fn capture(course: &Course) -> Self {
        let Course {
            schema_version,
            id,
            title,
            slide_size,
            variables,
            scenes: _,
            masters: _,
            templates: _,
            banks: _,
            start_slide,
            quiz,
            tracking,
            player,
            theme,
            language,
            strings,
            fonts,
            background_audio,
            text_styles,
            resources,
            glossary,
            editor,
        } = course;
        Self {
            schema_version: *schema_version,
            id: id.clone(),
            title: title.clone(),
            slide_size: *slide_size,
            variables: variables.clone(),
            start_slide: start_slide.clone(),
            quiz: *quiz,
            tracking: tracking.clone(),
            player: player.clone(),
            theme: theme.clone(),
            language: language.clone(),
            strings: strings.clone(),
            fonts: fonts.clone(),
            background_audio: background_audio.clone(),
            text_styles: text_styles.clone(),
            resources: resources.clone(),
            glossary: glossary.clone(),
            editor: editor.clone(),
        }
    }

    /// Move all 17 header fields back into `course` (used for O(1) error rollback).
    pub(crate) fn restore(self, course: &mut Course) {
        let Self {
            schema_version: s_schema_version,
            id: s_id,
            title: s_title,
            slide_size: s_slide_size,
            variables: s_variables,
            start_slide: s_start_slide,
            quiz: s_quiz,
            tracking: s_tracking,
            player: s_player,
            theme: s_theme,
            language: s_language,
            strings: s_strings,
            fonts: s_fonts,
            background_audio: s_background_audio,
            text_styles: s_text_styles,
            resources: s_resources,
            glossary: s_glossary,
            editor: s_editor,
        } = self;
        let Course {
            schema_version,
            id,
            title,
            slide_size,
            variables,
            scenes: _,
            masters: _,
            templates: _,
            banks: _,
            start_slide,
            quiz,
            tracking,
            player,
            theme,
            language,
            strings,
            fonts,
            background_audio,
            text_styles,
            resources,
            glossary,
            editor,
        } = course;
        *schema_version = s_schema_version;
        *id = s_id;
        *title = s_title;
        *slide_size = s_slide_size;
        *variables = s_variables;
        *start_slide = s_start_slide;
        *quiz = s_quiz;
        *tracking = s_tracking;
        *player = s_player;
        *theme = s_theme;
        *language = s_language;
        *strings = s_strings;
        *fonts = s_fonts;
        *background_audio = s_background_audio;
        *text_styles = s_text_styles;
        *resources = s_resources;
        *glossary = s_glossary;
        *editor = s_editor;
    }
}

/// Sparse reversible patch over the 17 header fields of [`Course`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CourseHeaderPatch {
    pub(crate) schema_version: Option<u32>,
    pub(crate) id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) slide_size: Option<Size>,
    pub(crate) variables: Option<Vec<VariableDef>>,
    pub(crate) start_slide: Option<Option<String>>,
    pub(crate) quiz: Option<Option<QuizSettings>>,
    pub(crate) tracking: Option<TrackingSettings>,
    pub(crate) player: Option<PlayerSettings>,
    pub(crate) theme: Option<Theme>,
    pub(crate) language: Option<Option<String>>,
    pub(crate) strings: Option<BTreeMap<String, String>>,
    pub(crate) fonts: Option<Vec<FontFace>>,
    pub(crate) background_audio: Option<Option<BackgroundAudio>>,
    pub(crate) text_styles: Option<Vec<NamedTextStyle>>,
    pub(crate) resources: Option<Vec<Resource>>,
    pub(crate) glossary: Option<Vec<GlossaryEntry>>,
    pub(crate) editor: Option<EditorSettings>,
}

impl CourseHeaderPatch {
    /// Diff a pre-edit header snapshot against `after`, moving only changed fields.
    /// Returns `None` if all 17 header fields are equal.
    #[must_use]
    pub(crate) fn diff(before: CourseHeaderSnapshot, after: &Course) -> Option<Self> {
        let CourseHeaderSnapshot {
            schema_version: b_schema_version,
            id: b_id,
            title: b_title,
            slide_size: b_slide_size,
            variables: b_variables,
            start_slide: b_start_slide,
            quiz: b_quiz,
            tracking: b_tracking,
            player: b_player,
            theme: b_theme,
            language: b_language,
            strings: b_strings,
            fonts: b_fonts,
            background_audio: b_background_audio,
            text_styles: b_text_styles,
            resources: b_resources,
            glossary: b_glossary,
            editor: b_editor,
        } = before;
        let Course {
            schema_version: a_schema_version,
            id: a_id,
            title: a_title,
            slide_size: a_slide_size,
            variables: a_variables,
            scenes: _,
            masters: _,
            templates: _,
            banks: _,
            start_slide: a_start_slide,
            quiz: a_quiz,
            tracking: a_tracking,
            player: a_player,
            theme: a_theme,
            language: a_language,
            strings: a_strings,
            fonts: a_fonts,
            background_audio: a_background_audio,
            text_styles: a_text_styles,
            resources: a_resources,
            glossary: a_glossary,
            editor: a_editor,
        } = after;

        let mut changed = false;
        let patch = Self {
            schema_version: diff_field(b_schema_version, a_schema_version, &mut changed),
            id: diff_field(b_id, a_id, &mut changed),
            title: diff_field(b_title, a_title, &mut changed),
            slide_size: diff_field(b_slide_size, a_slide_size, &mut changed),
            variables: diff_field(b_variables, a_variables, &mut changed),
            start_slide: diff_field(b_start_slide, a_start_slide, &mut changed),
            quiz: diff_field(b_quiz, a_quiz, &mut changed),
            tracking: diff_field(b_tracking, a_tracking, &mut changed),
            player: diff_field(b_player, a_player, &mut changed),
            theme: diff_field(b_theme, a_theme, &mut changed),
            language: diff_field(b_language, a_language, &mut changed),
            strings: diff_field(b_strings, a_strings, &mut changed),
            fonts: diff_field(b_fonts, a_fonts, &mut changed),
            background_audio: diff_field(b_background_audio, a_background_audio, &mut changed),
            text_styles: diff_field(b_text_styles, a_text_styles, &mut changed),
            resources: diff_field(b_resources, a_resources, &mut changed),
            glossary: diff_field(b_glossary, a_glossary, &mut changed),
            editor: diff_field(b_editor, a_editor, &mut changed),
        };
        changed.then_some(patch)
    }

    /// Swap every present field in place with `course`, turning `self` into its own inverse.
    pub(crate) fn apply(&mut self, course: &mut Course) {
        let Self {
            schema_version: p_schema_version,
            id: p_id,
            title: p_title,
            slide_size: p_slide_size,
            variables: p_variables,
            start_slide: p_start_slide,
            quiz: p_quiz,
            tracking: p_tracking,
            player: p_player,
            theme: p_theme,
            language: p_language,
            strings: p_strings,
            fonts: p_fonts,
            background_audio: p_background_audio,
            text_styles: p_text_styles,
            resources: p_resources,
            glossary: p_glossary,
            editor: p_editor,
        } = self;
        let Course {
            schema_version,
            id,
            title,
            slide_size,
            variables,
            scenes: _,
            masters: _,
            templates: _,
            banks: _,
            start_slide,
            quiz,
            tracking,
            player,
            theme,
            language,
            strings,
            fonts,
            background_audio,
            text_styles,
            resources,
            glossary,
            editor,
        } = course;
        swap_if_some(p_schema_version, schema_version);
        swap_if_some(p_id, id);
        swap_if_some(p_title, title);
        swap_if_some(p_slide_size, slide_size);
        swap_if_some(p_variables, variables);
        swap_if_some(p_start_slide, start_slide);
        swap_if_some(p_quiz, quiz);
        swap_if_some(p_tracking, tracking);
        swap_if_some(p_player, player);
        swap_if_some(p_theme, theme);
        swap_if_some(p_language, language);
        swap_if_some(p_strings, strings);
        swap_if_some(p_fonts, fonts);
        swap_if_some(p_background_audio, background_audio);
        swap_if_some(p_text_styles, text_styles);
        swap_if_some(p_resources, resources);
        swap_if_some(p_glossary, glossary);
        swap_if_some(p_editor, editor);
    }

    /// Rough heap + stack footprint in bytes for memory-budget diagnostics.
    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let mut total = size_of::<Self>();
        if let Some(id) = &self.id {
            total = total.saturating_add(id.capacity());
        }
        if let Some(title) = &self.title {
            total = total.saturating_add(title.capacity());
        }
        if let Some(vars) = &self.variables {
            total = total.saturating_add(estimate_vec_plain(vars));
        }
        if let Some(Some(start)) = &self.start_slide {
            total = total.saturating_add(start.capacity());
        }
        if let Some(tracking) = &self.tracking
            && let Some(quiz) = &tracking.quiz
            && let Some(slide) = &quiz.slide
        {
            total = total.saturating_add(slide.capacity());
        }
        if let Some(player) = &self.player
            && let Some(logo) = &player.logo
        {
            total = total.saturating_add(logo.capacity());
        }
        if let Some(theme) = &self.theme {
            total = total.saturating_add(theme.font.capacity());
            total = total.saturating_add(estimate_opt_string(&theme.heading_font));
            total = total.saturating_add(estimate_vec_plain(&theme.palette));
        }
        if let Some(Some(lang)) = &self.language {
            total = total.saturating_add(lang.capacity());
        }
        if let Some(strings) = &self.strings {
            for (k, v) in strings {
                total = total
                    .saturating_add(k.capacity())
                    .saturating_add(v.capacity())
                    .saturating_add(size_of::<(String, String)>());
            }
        }
        if let Some(fonts) = &self.fonts {
            total = total.saturating_add(estimate_vec_plain(fonts));
        }
        if let Some(Some(audio)) = &self.background_audio {
            total = total.saturating_add(audio.src.capacity());
        }
        if let Some(styles) = &self.text_styles {
            total = total.saturating_add(estimate_vec_plain(styles));
        }
        if let Some(resources) = &self.resources {
            total = total.saturating_add(estimate_vec_plain(resources));
        }
        if let Some(glossary) = &self.glossary {
            total = total.saturating_add(estimate_vec_plain(glossary));
        }
        if let Some(editor) = &self.editor {
            total = total.saturating_add(estimate_vec_plain(&editor.guides));
        }
        total
    }
}

/// Reversible single-item structural edit on a `Vec<T>`.
///
/// Calling [`ListOp::apply`] transforms `after -> before` and turns `self`
/// into its own exact inverse in place with zero clones and zero panics.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ListOp<T> {
    /// One item inserted at `index` (applying removes it -> becomes `Remove`).
    Insert { index: usize },
    /// One item removed from `index` (applying inserts it -> becomes `Insert`).
    Remove { index: usize, item: Box<T> },
    /// One item moved from `from` to `to` (applying moves `to -> from` and swaps `from`/`to`).
    Move { from: usize, to: usize },
}

impl<T> ListOp<T> {
    /// Apply this list op in place, turning `self` into its own exact inverse.
    ///
    /// On `Err`, both `vec` and `self` are left untouched.
    pub(crate) fn apply(&mut self, vec: &mut Vec<T>) -> Result<()> {
        match self {
            Self::Insert { index } => {
                let index = *index;
                if index >= vec.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta list insert index {index} out of bounds for len {}",
                        vec.len()
                    )));
                }
                let removed = vec.remove(index);
                *self = Self::Remove {
                    index,
                    item: Box::new(removed),
                };
                Ok(())
            }
            Self::Remove { index, .. } => {
                let index = *index;
                if index > vec.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta list remove index {index} out of bounds for len {}",
                        vec.len()
                    )));
                }
                let prev = std::mem::replace(self, Self::Insert { index });
                if let Self::Remove { item, .. } = prev {
                    vec.insert(index, *item);
                }
                Ok(())
            }
            Self::Move { from, to } => {
                if *from >= vec.len() || *to >= vec.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta list move ({from} -> {to}) out of bounds for len {}",
                        vec.len()
                    )));
                }
                if *from != *to {
                    let item = vec.remove(*to);
                    vec.insert(*from, item);
                    std::mem::swap(from, to);
                }
                Ok(())
            }
        }
    }

    #[must_use]
    pub(crate) fn estimated_bytes(&self, item_bytes: impl FnOnce(&T) -> usize) -> usize {
        let base = size_of::<Self>();
        match self {
            Self::Insert { .. } | Self::Move { .. } => base,
            Self::Remove { item, .. } => base.saturating_add(item_bytes(item)),
        }
    }
}

/// Reversible structural change to a [`Course`]'s slide collections.
///
/// Every variant validates all bounds before mutating anything and transforms
/// itself into its own exact inverse in place (zero clones, zero panics).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StructuralOp {
    SceneSlideInsert {
        scene: usize,
        index: usize,
    },
    SceneSlideRemove {
        scene: usize,
        index: usize,
        slide: Box<Slide>,
    },
    SceneSlideMove {
        from_scene: usize,
        from_index: usize,
        to_scene: usize,
        to_index: usize,
    },
    SceneOp(ListOp<Scene>),
    SceneMeta {
        scene: usize,
        id: Option<String>,
        title: Option<String>,
    },
    MasterOp(ListOp<Slide>),
    TemplateOp(ListOp<Slide>),
    BankOp(ListOp<QuestionBank>),
    BankMeta {
        bank: usize,
        id: Option<String>,
        title: Option<String>,
    },
    BankSlideInsert {
        bank: usize,
        index: usize,
    },
    BankSlideRemove {
        bank: usize,
        index: usize,
        slide: Box<Slide>,
    },
    BankSlideMove {
        from_bank: usize,
        from_index: usize,
        to_bank: usize,
        to_index: usize,
    },
}

trait SlideGroup: PartialEq + Sized {
    const KIND: &'static str;
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn slides(&self) -> &[Slide];
    fn slides_mut(&mut self) -> &mut Vec<Slide>;
    fn into_parts(self) -> (String, String, Vec<Slide>);
    fn slide_loc(group: usize, slide: usize) -> SlideLoc;
    fn meta_op(group: usize, id: Option<String>, title: Option<String>) -> StructuralOp;
    fn list_op(op: ListOp<Self>) -> StructuralOp;
    fn slide_insert_op(group: usize, index: usize) -> StructuralOp;
    fn slide_remove_op(group: usize, index: usize, slide: Box<Slide>) -> StructuralOp;
    fn slide_move_op(
        from_group: usize,
        from_index: usize,
        to_group: usize,
        to_index: usize,
    ) -> StructuralOp;
}

impl SlideGroup for Scene {
    const KIND: &'static str = "scene";
    fn id(&self) -> &str {
        &self.id
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn slides(&self) -> &[Slide] {
        &self.slides
    }
    fn slides_mut(&mut self) -> &mut Vec<Slide> {
        &mut self.slides
    }
    fn into_parts(self) -> (String, String, Vec<Slide>) {
        let Self { id, title, slides } = self;
        (id, title, slides)
    }
    fn slide_loc(group: usize, slide: usize) -> SlideLoc {
        SlideLoc::SceneSlide {
            scene: group,
            slide,
        }
    }
    fn meta_op(group: usize, id: Option<String>, title: Option<String>) -> StructuralOp {
        StructuralOp::SceneMeta {
            scene: group,
            id,
            title,
        }
    }
    fn list_op(op: ListOp<Self>) -> StructuralOp {
        StructuralOp::SceneOp(op)
    }
    fn slide_insert_op(group: usize, index: usize) -> StructuralOp {
        StructuralOp::SceneSlideInsert {
            scene: group,
            index,
        }
    }
    fn slide_remove_op(group: usize, index: usize, slide: Box<Slide>) -> StructuralOp {
        StructuralOp::SceneSlideRemove {
            scene: group,
            index,
            slide,
        }
    }
    fn slide_move_op(
        from_group: usize,
        from_index: usize,
        to_group: usize,
        to_index: usize,
    ) -> StructuralOp {
        StructuralOp::SceneSlideMove {
            from_scene: from_group,
            from_index,
            to_scene: to_group,
            to_index,
        }
    }
}

impl SlideGroup for QuestionBank {
    const KIND: &'static str = "bank";
    fn id(&self) -> &str {
        &self.id
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn slides(&self) -> &[Slide] {
        &self.slides
    }
    fn slides_mut(&mut self) -> &mut Vec<Slide> {
        &mut self.slides
    }
    fn into_parts(self) -> (String, String, Vec<Slide>) {
        let Self { id, title, slides } = self;
        (id, title, slides)
    }
    fn slide_loc(group: usize, slide: usize) -> SlideLoc {
        SlideLoc::BankSlide { bank: group, slide }
    }
    fn meta_op(group: usize, id: Option<String>, title: Option<String>) -> StructuralOp {
        StructuralOp::BankMeta {
            bank: group,
            id,
            title,
        }
    }
    fn list_op(op: ListOp<Self>) -> StructuralOp {
        StructuralOp::BankOp(op)
    }
    fn slide_insert_op(group: usize, index: usize) -> StructuralOp {
        StructuralOp::BankSlideInsert { bank: group, index }
    }
    fn slide_remove_op(group: usize, index: usize, slide: Box<Slide>) -> StructuralOp {
        StructuralOp::BankSlideRemove {
            bank: group,
            index,
            slide,
        }
    }
    fn slide_move_op(
        from_group: usize,
        from_index: usize,
        to_group: usize,
        to_index: usize,
    ) -> StructuralOp {
        StructuralOp::BankSlideMove {
            from_bank: from_group,
            from_index,
            to_bank: to_group,
            to_index,
        }
    }
}

fn apply_grouped_slide_move<G: SlideGroup>(
    groups: &mut [G],
    from_group: &mut usize,
    from_index: &mut usize,
    to_group: &mut usize,
    to_index: &mut usize,
) -> Result<()> {
    let kind = G::KIND;
    let groups_len = groups.len();
    if *from_group == *to_group {
        let group = groups.get_mut(*to_group).ok_or_else(|| {
            EngineError::Invalid(format!(
                "undo delta {kind} index {to_group} out of bounds for len {groups_len}"
            ))
        })?;
        let slides = group.slides_mut();
        if *from_index >= slides.len() || *to_index >= slides.len() {
            return Err(EngineError::Invalid(format!(
                "undo delta {kind} slide move ({from_index} -> {to_index}) out of bounds for len {}",
                slides.len()
            )));
        }
        if *from_index != *to_index {
            let slide = slides.remove(*to_index);
            slides.insert(*from_index, slide);
            std::mem::swap(from_index, to_index);
        }
        return Ok(());
    }

    let to_len = groups
        .get(*to_group)
        .map(|g| g.slides().len())
        .ok_or_else(|| {
            EngineError::Invalid(format!(
                "undo delta target {kind} index {to_group} out of bounds for len {groups_len}"
            ))
        })?;
    let from_len = groups
        .get(*from_group)
        .map(|g| g.slides().len())
        .ok_or_else(|| {
            EngineError::Invalid(format!(
                "undo delta source {kind} index {from_group} out of bounds for len {groups_len}"
            ))
        })?;
    if *to_index >= to_len {
        return Err(EngineError::Invalid(format!(
            "undo delta target {kind} slide index {to_index} out of bounds for len {to_len}"
        )));
    }
    if *from_index > from_len {
        return Err(EngineError::Invalid(format!(
            "undo delta source {kind} slide index {from_index} out of bounds for len {from_len}"
        )));
    }

    let Some(to_g) = groups.get_mut(*to_group) else {
        return Err(EngineError::Invalid(format!(
            "undo delta target {kind} index {to_group} missing"
        )));
    };
    let slide = to_g.slides_mut().remove(*to_index);
    let Some(from_g) = groups.get_mut(*from_group) else {
        return Err(EngineError::Invalid(format!(
            "undo delta source {kind} index {from_group} missing"
        )));
    };
    from_g.slides_mut().insert(*from_index, slide);
    std::mem::swap(from_group, to_group);
    std::mem::swap(from_index, to_index);
    Ok(())
}

impl StructuralOp {
    /// Apply this structural op in place, turning `self` into its own exact inverse.
    ///
    /// On `Err`, both `course` and `self` are left untouched.
    pub(crate) fn apply(&mut self, course: &mut Course) -> Result<()> {
        match self {
            Self::SceneSlideInsert { scene, index } => {
                let scene = *scene;
                let index = *index;
                let scenes_len = course.scenes.len();
                let s = course.scenes.get_mut(scene).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta scene index {scene} out of bounds for len {scenes_len}"
                    ))
                })?;
                if index >= s.slides.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta scene slide insert index {index} out of bounds for len {}",
                        s.slides.len()
                    )));
                }
                let removed = s.slides.remove(index);
                *self = Self::SceneSlideRemove {
                    scene,
                    index,
                    slide: Box::new(removed),
                };
                Ok(())
            }
            Self::SceneSlideRemove { scene, index, .. } => {
                let scene = *scene;
                let index = *index;
                let scenes_len = course.scenes.len();
                let s = course.scenes.get_mut(scene).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta scene index {scene} out of bounds for len {scenes_len}"
                    ))
                })?;
                if index > s.slides.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta scene slide remove index {index} out of bounds for len {}",
                        s.slides.len()
                    )));
                }
                let prev = std::mem::replace(self, Self::SceneSlideInsert { scene, index });
                if let Self::SceneSlideRemove { slide, .. } = prev {
                    s.slides.insert(index, *slide);
                }
                Ok(())
            }
            Self::SceneSlideMove {
                from_scene,
                from_index,
                to_scene,
                to_index,
            } => apply_grouped_slide_move(
                &mut course.scenes,
                from_scene,
                from_index,
                to_scene,
                to_index,
            ),
            Self::SceneOp(op) => op.apply(&mut course.scenes),
            Self::SceneMeta {
                scene,
                id: p_id,
                title: p_title,
            } => {
                let scenes_len = course.scenes.len();
                let s = course.scenes.get_mut(*scene).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta scene index {scene} out of bounds for len {scenes_len}"
                    ))
                })?;
                let Scene {
                    id,
                    title,
                    slides: _,
                } = s;
                swap_if_some(p_id, id);
                swap_if_some(p_title, title);
                Ok(())
            }
            Self::MasterOp(op) => op.apply(&mut course.masters),
            Self::TemplateOp(op) => op.apply(&mut course.templates),
            Self::BankOp(op) => op.apply(&mut course.banks),
            Self::BankMeta {
                bank,
                id: p_id,
                title: p_title,
            } => {
                let banks_len = course.banks.len();
                let b = course.banks.get_mut(*bank).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta bank index {bank} out of bounds for len {banks_len}"
                    ))
                })?;
                let QuestionBank {
                    id,
                    title,
                    slides: _,
                } = b;
                swap_if_some(p_id, id);
                swap_if_some(p_title, title);
                Ok(())
            }
            Self::BankSlideInsert { bank, index } => {
                let bank = *bank;
                let index = *index;
                let banks_len = course.banks.len();
                let b = course.banks.get_mut(bank).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta bank index {bank} out of bounds for len {banks_len}"
                    ))
                })?;
                if index >= b.slides.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta bank slide insert index {index} out of bounds for len {}",
                        b.slides.len()
                    )));
                }
                let removed = b.slides.remove(index);
                *self = Self::BankSlideRemove {
                    bank,
                    index,
                    slide: Box::new(removed),
                };
                Ok(())
            }
            Self::BankSlideRemove { bank, index, .. } => {
                let bank = *bank;
                let index = *index;
                let banks_len = course.banks.len();
                let b = course.banks.get_mut(bank).ok_or_else(|| {
                    EngineError::Invalid(format!(
                        "undo delta bank index {bank} out of bounds for len {banks_len}"
                    ))
                })?;
                if index > b.slides.len() {
                    return Err(EngineError::Invalid(format!(
                        "undo delta bank slide remove index {index} out of bounds for len {}",
                        b.slides.len()
                    )));
                }
                let prev = std::mem::replace(self, Self::BankSlideInsert { bank, index });
                if let Self::BankSlideRemove { slide, .. } = prev {
                    b.slides.insert(index, *slide);
                }
                Ok(())
            }
            Self::BankSlideMove {
                from_bank,
                from_index,
                to_bank,
                to_index,
            } => apply_grouped_slide_move(
                &mut course.banks,
                from_bank,
                from_index,
                to_bank,
                to_index,
            ),
        }
    }

    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let base = size_of::<Self>();
        match self {
            Self::SceneSlideInsert { .. }
            | Self::SceneSlideMove { .. }
            | Self::BankSlideInsert { .. }
            | Self::BankSlideMove { .. } => base,
            Self::SceneSlideRemove { slide, .. } | Self::BankSlideRemove { slide, .. } => {
                base.saturating_add(estimate_slide(slide))
            }
            Self::SceneOp(op) => base.saturating_add(op.estimated_bytes(estimate_scene)),
            Self::SceneMeta { id, title, .. } | Self::BankMeta { id, title, .. } => base
                .saturating_add(estimate_opt_string(id))
                .saturating_add(estimate_opt_string(title)),
            Self::MasterOp(op) | Self::TemplateOp(op) => {
                base.saturating_add(op.estimated_bytes(estimate_slide))
            }
            Self::BankOp(op) => base.saturating_add(op.estimated_bytes(estimate_bank)),
        }
    }
}

/// Multi-slide and/or header patch when collection structure matches.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CoursePatch {
    pub(crate) header: Option<CourseHeaderPatch>,
    pub(crate) slides: Vec<(SlideLoc, SlideDelta)>,
}

impl CoursePatch {
    /// Apply all header and slide patches atomically, turning `self` into its inverse.
    ///
    /// On `Err`, both `course` and `self` are left untouched.
    pub(crate) fn apply(&mut self, course: &mut Course) -> Result<()> {
        for (loc, _) in &self.slides {
            if slide_at(course, *loc).is_none() {
                return Err(EngineError::Invalid(format!(
                    "undo delta patch slide location {loc:?} out of bounds"
                )));
            }
        }
        if let Some(header) = self.header.as_mut() {
            header.apply(course);
        }
        for i in 0..self.slides.len() {
            let res = if let Some((loc, change)) = self.slides.get_mut(i) {
                match slide_at_mut(course, *loc) {
                    Some(slide) => change.apply(slide),
                    None => Err(EngineError::Invalid(format!(
                        "undo delta patch slide location {loc:?} out of bounds"
                    ))),
                }
            } else {
                Ok(())
            };
            if let Err(err) = res {
                for j in (0..i).rev() {
                    if let Some((loc, change)) = self.slides.get_mut(j)
                        && let Some(slide) = slide_at_mut(course, *loc)
                    {
                        let _ = change.apply(slide);
                    }
                }
                if let Some(header) = self.header.as_mut() {
                    header.apply(course);
                }
                return Err(err);
            }
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let mut total = size_of::<Self>();
        if let Some(header) = &self.header {
            total = total.saturating_add(header.estimated_bytes());
        }
        for (_, delta) in &self.slides {
            total = total
                .saturating_add(size_of::<SlideLoc>())
                .saturating_add(delta.estimated_bytes());
        }
        total
    }
}

/// Reversible top-level change to a [`Course`].
///
/// Calling [`CourseDelta::apply`] transforms `after -> before` and turns `self`
/// into its own exact inverse in place with zero clones and zero panics.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CourseDelta {
    /// Single slide modified in place.
    SlideChange { loc: SlideLoc, change: SlideDelta },
    /// Course-header fields only (zero slides touched).
    Header(Box<CourseHeaderPatch>),
    /// Single structural edit on scenes, masters, templates, or banks.
    Structural(Box<StructuralOp>),
    /// Matching collection structure with multiple changed slides and/or header.
    Patch(Box<CoursePatch>),
    /// Ordered forward sub-transitions (applied in reverse on undo, reversed in place for redo).
    Sequence(Vec<CourseDelta>),
    /// Ultimate exact fallback for wholesale course rewrites.
    FullCourse(Box<Course>),
}

impl CourseDelta {
    /// Compute the smallest reversible delta that transforms `after` back into `before`.
    /// Returns `None` when `before == *after`.
    #[must_use]
    pub(crate) fn diff(before: Course, after: &Course) -> Option<Self> {
        if before == *after {
            return None;
        }

        let scenes_plan = plan_group_collection(&before.scenes, &after.scenes);
        let masters_plan = plan_flat_list(&before.masters, &after.masters, |s| s.id.as_str());
        let templates_plan = plan_flat_list(&before.templates, &after.templates, |s| s.id.as_str());
        let banks_plan = plan_group_collection(&before.banks, &after.banks);

        if matches!(scenes_plan, GroupCollectionPlan::Wholesale)
            || matches!(masters_plan, FlatListPlan::Wholesale)
            || matches!(templates_plan, FlatListPlan::Wholesale)
            || matches!(banks_plan, GroupCollectionPlan::Wholesale)
        {
            return Some(Self::FullCourse(Box::new(before)));
        }

        let all_identical = matches!(scenes_plan, GroupCollectionPlan::Identical)
            && matches!(masters_plan, FlatListPlan::Identical)
            && matches!(templates_plan, FlatListPlan::Identical)
            && matches!(banks_plan, GroupCollectionPlan::Identical);

        let all_in_place = is_all_in_place(scenes_plan, masters_plan, templates_plan, banks_plan);

        // Case A wholesale threshold: check counts BEFORE consuming `before` (zero clones!).
        if all_in_place && !all_identical {
            let (total_slides, changed_slides) = count_matching_slides(&before, after);
            if changed_slides > 1 && changed_slides.saturating_mul(2) > total_slides {
                return Some(Self::FullCourse(Box::new(before)));
            }
        }

        // Destructure `before` exhaustively into all 21 fields without `..` (M33-11).
        let Course {
            schema_version: b_schema_version,
            id: b_id,
            title: b_title,
            slide_size: b_slide_size,
            variables: b_variables,
            scenes: b_scenes,
            masters: b_masters,
            templates: b_templates,
            banks: b_banks,
            start_slide: b_start_slide,
            quiz: b_quiz,
            tracking: b_tracking,
            player: b_player,
            theme: b_theme,
            language: b_language,
            strings: b_strings,
            fonts: b_fonts,
            background_audio: b_background_audio,
            text_styles: b_text_styles,
            resources: b_resources,
            glossary: b_glossary,
            editor: b_editor,
        } = before;

        let header_snap = CourseHeaderSnapshot {
            schema_version: b_schema_version,
            id: b_id,
            title: b_title,
            slide_size: b_slide_size,
            variables: b_variables,
            start_slide: b_start_slide,
            quiz: b_quiz,
            tracking: b_tracking,
            player: b_player,
            theme: b_theme,
            language: b_language,
            strings: b_strings,
            fonts: b_fonts,
            background_audio: b_background_audio,
            text_styles: b_text_styles,
            resources: b_resources,
            glossary: b_glossary,
            editor: b_editor,
        };
        let header_patch = CourseHeaderPatch::diff(header_snap, after);

        if all_identical {
            return header_patch.map(|p| Self::Header(Box::new(p)));
        }

        if all_in_place {
            return build_in_place_delta(
                header_patch,
                b_scenes,
                b_masters,
                b_templates,
                b_banks,
                after,
            );
        }

        // Case B: compact structural/metadata changes in forward sub-transition order.
        let mut steps = Vec::new();
        emit_group_steps(scenes_plan, b_scenes, &after.scenes, &mut steps);
        emit_flat_slide_steps(
            masters_plan,
            b_masters,
            &after.masters,
            SlideLoc::Master,
            StructuralOp::MasterOp,
            &mut steps,
        );
        emit_flat_slide_steps(
            templates_plan,
            b_templates,
            &after.templates,
            SlideLoc::Template,
            StructuralOp::TemplateOp,
            &mut steps,
        );
        emit_group_steps(banks_plan, b_banks, &after.banks, &mut steps);
        if let Some(patch) = header_patch {
            steps.push(Self::Header(Box::new(patch)));
        }

        if steps.len() == 1 {
            steps.pop()
        } else if steps.is_empty() {
            None
        } else {
            Some(Self::Sequence(steps))
        }
    }

    /// Apply this delta in place, mutating `self` into its own exact inverse.
    ///
    /// On `Err`, both `course` and `self` are left untouched.
    pub(crate) fn apply(&mut self, course: &mut Course) -> Result<()> {
        match self {
            Self::SlideChange { loc, change } => {
                let slide = slide_at_mut(course, *loc).ok_or_else(|| {
                    EngineError::Invalid(format!("undo delta slide location {loc:?} out of bounds"))
                })?;
                change.apply(slide)
            }
            Self::Header(patch) => {
                patch.apply(course);
                Ok(())
            }
            Self::Structural(op) => op.apply(course),
            Self::Patch(patch) => patch.apply(course),
            Self::Sequence(steps) => {
                for i in (0..steps.len()).rev() {
                    let res = match steps.get_mut(i) {
                        Some(step) => step.apply(course),
                        None => Ok(()),
                    };
                    if let Err(err) = res {
                        for j in i.saturating_add(1)..steps.len() {
                            if let Some(step) = steps.get_mut(j) {
                                let _ = step.apply(course);
                            }
                        }
                        return Err(err);
                    }
                }
                steps.reverse();
                Ok(())
            }
            Self::FullCourse(prev) => {
                std::mem::swap(course, prev.as_mut());
                Ok(())
            }
        }
    }

    /// Rough heap + stack footprint in bytes for memory-budget diagnostics.
    #[must_use]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let base = size_of::<Self>();
        match self {
            Self::SlideChange { change, .. } => base.saturating_add(change.estimated_bytes()),
            Self::Header(patch) => base.saturating_add(patch.estimated_bytes()),
            Self::Structural(op) => base.saturating_add(op.estimated_bytes()),
            Self::Patch(patch) => base.saturating_add(patch.estimated_bytes()),
            Self::Sequence(steps) => steps
                .iter()
                .fold(base, |acc, d| acc.saturating_add(d.estimated_bytes())),
            Self::FullCourse(course) => base.saturating_add(estimate_course(course)),
        }
    }

    /// Merge two consecutive undo deltas (`older` for `S0 -> S1`, `newer` for `S1 -> S2`)
    /// into one reversible delta for `S0 -> S2`.
    ///
    /// Collapses repeated in-place edits on the same object, slide, or full course in O(1)
    /// space by keeping `older`'s `S0` state and dropping the intermediate `S1` state.
    /// Mixed edits compose into a flat [`CourseDelta::Sequence`] in forward order.
    #[must_use]
    pub(crate) fn merge(older: Self, newer: Self) -> Self {
        if can_collapse_drop_newer(&older, &newer) {
            return older;
        }
        let mut steps = Vec::new();
        push_merged_step(&mut steps, older);
        push_merged_step(&mut steps, newer);
        if steps.len() == 1
            && let Some(single) = steps.pop()
        {
            return single;
        }
        Self::Sequence(steps)
    }
}

fn can_collapse_drop_newer(older: &CourseDelta, newer: &CourseDelta) -> bool {
    match (older, newer) {
        (
            CourseDelta::SlideChange {
                loc: l1,
                change:
                    SlideDelta::ObjectModified {
                        layer: ly1,
                        index: i1,
                        ..
                    },
            },
            CourseDelta::SlideChange {
                loc: l2,
                change:
                    SlideDelta::ObjectModified {
                        layer: ly2,
                        index: i2,
                        ..
                    },
            },
        ) => (l1, ly1, i1) == (l2, ly2, i2),
        (
            CourseDelta::SlideChange {
                loc: l1,
                change: SlideDelta::FullSlide(_),
            },
            CourseDelta::SlideChange {
                loc: l2,
                change: SlideDelta::FullSlide(_),
            },
        ) => l1 == l2,
        (CourseDelta::FullCourse(_), CourseDelta::FullCourse(_)) => true,
        _ => false,
    }
}

fn push_merged_step(steps: &mut Vec<CourseDelta>, delta: CourseDelta) {
    match delta {
        CourseDelta::Sequence(nested) => {
            for item in nested {
                push_merged_step(steps, item);
            }
        }
        other => {
            if steps
                .last()
                .is_some_and(|prev| can_collapse_drop_newer(prev, &other))
            {
                return;
            }
            steps.push(other);
        }
    }
}

// ------------------------------------------------------------------ structural diff planners

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlatListPlan {
    Identical,
    InPlace,
    Insert { index: usize },
    Remove { index: usize },
    Move { from: usize, to: usize },
    Wholesale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupCollectionPlan {
    Identical,
    InPlaceNoMeta,
    InPlaceWithMeta,
    GroupInsert {
        index: usize,
    },
    GroupRemove {
        index: usize,
    },
    GroupMove {
        from: usize,
        to: usize,
    },
    SlideInsert {
        group: usize,
        index: usize,
    },
    SlideRemove {
        group: usize,
        index: usize,
    },
    SlideMove {
        from_group: usize,
        from_index: usize,
        to_group: usize,
        to_index: usize,
    },
    Wholesale,
}

fn ids_match_slice<T>(a: Option<&[T]>, b: Option<&[T]>, id_of: fn(&T) -> &str) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| id_of(x) == id_of(y))
}

fn mapped_index_after_move(idx: usize, from: usize, to: usize) -> usize {
    if idx == from {
        to
    } else if from < to && idx > from && idx <= to {
        idx.saturating_sub(1)
    } else if from > to && idx >= to && idx < from {
        idx.saturating_add(1)
    } else {
        idx
    }
}

fn plan_flat_list<T: PartialEq>(before: &[T], after: &[T], id_of: fn(&T) -> &str) -> FlatListPlan {
    if before == after {
        return FlatListPlan::Identical;
    }
    if before.len() == after.len() {
        let mut first = None;
        let mut last = 0;
        for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
            if id_of(b) != id_of(a) {
                if first.is_none() {
                    first = Some(i);
                }
                last = i;
            }
        }
        let Some(a) = first else {
            return FlatListPlan::InPlace;
        };
        if a == last {
            return FlatListPlan::InPlace;
        }
        let b = last;
        if before.get(a).map(id_of) == after.get(b).map(id_of)
            && ids_match_slice(before.get(a.saturating_add(1)..=b), after.get(a..b), id_of)
        {
            return FlatListPlan::Move { from: a, to: b };
        }
        if before.get(b).map(id_of) == after.get(a).map(id_of)
            && ids_match_slice(before.get(a..b), after.get(a.saturating_add(1)..=b), id_of)
        {
            return FlatListPlan::Move { from: b, to: a };
        }
        return FlatListPlan::Wholesale;
    }
    if after.len() == before.len().saturating_add(1) && after.len() > before.len() {
        let ins = before
            .iter()
            .zip(after.iter())
            .position(|(b, a)| id_of(b) != id_of(a))
            .unwrap_or(before.len());
        if ids_match_slice(before.get(ins..), after.get(ins.saturating_add(1)..), id_of) {
            return FlatListPlan::Insert { index: ins };
        }
        return FlatListPlan::Wholesale;
    }
    if before.len() == after.len().saturating_add(1) && before.len() > after.len() {
        let rem = before
            .iter()
            .zip(after.iter())
            .position(|(b, a)| id_of(b) != id_of(a))
            .unwrap_or(after.len());
        if rem < before.len()
            && ids_match_slice(before.get(rem.saturating_add(1)..), after.get(rem..), id_of)
        {
            return FlatListPlan::Remove { index: rem };
        }
        return FlatListPlan::Wholesale;
    }
    FlatListPlan::Wholesale
}

fn slides_in_place_compatible(before: &[Slide], after: &[Slide]) -> bool {
    matches!(
        plan_flat_list(before, after, |s| s.id.as_str()),
        FlatListPlan::Identical | FlatListPlan::InPlace
    )
}

fn is_all_in_place(
    scenes: GroupCollectionPlan,
    masters: FlatListPlan,
    templates: FlatListPlan,
    banks: GroupCollectionPlan,
) -> bool {
    matches!(
        scenes,
        GroupCollectionPlan::Identical | GroupCollectionPlan::InPlaceNoMeta
    ) && matches!(masters, FlatListPlan::Identical | FlatListPlan::InPlace)
        && matches!(templates, FlatListPlan::Identical | FlatListPlan::InPlace)
        && matches!(
            banks,
            GroupCollectionPlan::Identical | GroupCollectionPlan::InPlaceNoMeta
        )
}

fn push_group_slide_diffs<G: SlideGroup>(
    before: Vec<G>,
    after: &[G],
    out: &mut Vec<(SlideLoc, SlideDelta)>,
) {
    for (gi, (b_group, a_group)) in before.into_iter().zip(after.iter()).enumerate() {
        let (_, _, b_slides) = b_group.into_parts();
        for (si, (b_slide, a_slide)) in b_slides
            .into_iter()
            .zip(a_group.slides().iter())
            .enumerate()
        {
            if let Some(delta) = SlideDelta::diff(b_slide, a_slide) {
                out.push((G::slide_loc(gi, si), delta));
            }
        }
    }
}

fn push_flat_slide_diffs(
    before: Vec<Slide>,
    after: &[Slide],
    make_loc: fn(usize) -> SlideLoc,
    out: &mut Vec<(SlideLoc, SlideDelta)>,
) {
    for (i, (b_slide, a_slide)) in before.into_iter().zip(after.iter()).enumerate() {
        if let Some(delta) = SlideDelta::diff(b_slide, a_slide) {
            out.push((make_loc(i), delta));
        }
    }
}

fn build_in_place_delta(
    header_patch: Option<CourseHeaderPatch>,
    b_scenes: Vec<Scene>,
    b_masters: Vec<Slide>,
    b_templates: Vec<Slide>,
    b_banks: Vec<QuestionBank>,
    after: &Course,
) -> Option<CourseDelta> {
    let mut changed = Vec::new();
    push_group_slide_diffs(b_scenes, &after.scenes, &mut changed);
    push_flat_slide_diffs(b_masters, &after.masters, SlideLoc::Master, &mut changed);
    push_flat_slide_diffs(
        b_templates,
        &after.templates,
        SlideLoc::Template,
        &mut changed,
    );
    push_group_slide_diffs(b_banks, &after.banks, &mut changed);

    if header_patch.is_none() && changed.len() == 1 {
        return changed
            .pop()
            .map(|(loc, change)| CourseDelta::SlideChange { loc, change });
    }
    if header_patch.is_none() && changed.is_empty() {
        return None;
    }
    Some(CourseDelta::Patch(Box::new(CoursePatch {
        header: header_patch,
        slides: changed,
    })))
}

fn plan_group_collection<G: SlideGroup>(before: &[G], after: &[G]) -> GroupCollectionPlan {
    if before == after {
        return GroupCollectionPlan::Identical;
    }
    match plan_flat_list(before, after, G::id) {
        FlatListPlan::Insert { index } => {
            for (bi, b_g) in before.iter().enumerate() {
                let ai = if bi < index { bi } else { bi.saturating_add(1) };
                let Some(a_g) = after.get(ai) else {
                    return GroupCollectionPlan::Wholesale;
                };
                if !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
                    return GroupCollectionPlan::Wholesale;
                }
            }
            GroupCollectionPlan::GroupInsert { index }
        }
        FlatListPlan::Remove { index } => {
            for (bi, b_g) in before.iter().enumerate() {
                if bi == index {
                    continue;
                }
                let ai = if bi < index { bi } else { bi.saturating_sub(1) };
                let Some(a_g) = after.get(ai) else {
                    return GroupCollectionPlan::Wholesale;
                };
                if !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
                    return GroupCollectionPlan::Wholesale;
                }
            }
            GroupCollectionPlan::GroupRemove { index }
        }
        FlatListPlan::Move { from, to } => {
            for (bi, b_g) in before.iter().enumerate() {
                let ai = mapped_index_after_move(bi, from, to);
                let Some(a_g) = after.get(ai) else {
                    return GroupCollectionPlan::Wholesale;
                };
                if !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
                    return GroupCollectionPlan::Wholesale;
                }
            }
            GroupCollectionPlan::GroupMove { from, to }
        }
        FlatListPlan::Wholesale => GroupCollectionPlan::Wholesale,
        FlatListPlan::Identical | FlatListPlan::InPlace => plan_same_len_groups(before, after),
    }
}

fn plan_same_len_groups<G: SlideGroup>(before: &[G], after: &[G]) -> GroupCollectionPlan {
    let meta_changed = before
        .iter()
        .zip(after.iter())
        .any(|(b, a)| b.id() != a.id() || b.title() != a.title());

    let mut grew = Vec::new();
    let mut shrank = Vec::new();
    for (gi, (b_g, a_g)) in before.iter().zip(after.iter()).enumerate() {
        let b_len = b_g.slides().len();
        let a_len = a_g.slides().len();
        if a_len == b_len {
            continue;
        }
        if a_len == b_len.saturating_add(1) && a_len > b_len {
            grew.push(gi);
        } else if b_len == a_len.saturating_add(1) && b_len > a_len {
            shrank.push(gi);
        } else {
            return GroupCollectionPlan::Wholesale;
        }
    }

    match (grew.as_slice(), shrank.as_slice()) {
        ([], []) => plan_groups_equal_slide_lens(before, after, meta_changed),
        ([g], []) => plan_group_single_slide_insert(before, after, *g),
        ([], [g]) => plan_group_single_slide_remove(before, after, *g),
        ([to_g], [from_g]) => plan_group_cross_slide_move(before, after, *from_g, *to_g),
        _ => GroupCollectionPlan::Wholesale,
    }
}

fn plan_groups_equal_slide_lens<G: SlideGroup>(
    before: &[G],
    after: &[G],
    meta_changed: bool,
) -> GroupCollectionPlan {
    let mut single_move = None;
    for (gi, (b_g, a_g)) in before.iter().zip(after.iter()).enumerate() {
        match plan_flat_list(b_g.slides(), a_g.slides(), |s| s.id.as_str()) {
            FlatListPlan::Identical | FlatListPlan::InPlace => {}
            FlatListPlan::Move { from, to } => {
                if single_move.is_some() {
                    return GroupCollectionPlan::Wholesale;
                }
                single_move = Some((gi, from, to));
            }
            FlatListPlan::Insert { .. } | FlatListPlan::Remove { .. } | FlatListPlan::Wholesale => {
                return GroupCollectionPlan::Wholesale;
            }
        }
    }
    if let Some((g, from, to)) = single_move {
        GroupCollectionPlan::SlideMove {
            from_group: g,
            from_index: from,
            to_group: g,
            to_index: to,
        }
    } else if meta_changed {
        GroupCollectionPlan::InPlaceWithMeta
    } else {
        GroupCollectionPlan::InPlaceNoMeta
    }
}

fn plan_group_single_slide_insert<G: SlideGroup>(
    before: &[G],
    after: &[G],
    g: usize,
) -> GroupCollectionPlan {
    for (gi, (b_g, a_g)) in before.iter().zip(after.iter()).enumerate() {
        if gi != g && !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
            return GroupCollectionPlan::Wholesale;
        }
    }
    let (Some(b_g), Some(a_g)) = (before.get(g), after.get(g)) else {
        return GroupCollectionPlan::Wholesale;
    };
    match plan_flat_list(b_g.slides(), a_g.slides(), |s| s.id.as_str()) {
        FlatListPlan::Insert { index } => GroupCollectionPlan::SlideInsert { group: g, index },
        _ => GroupCollectionPlan::Wholesale,
    }
}

fn plan_group_single_slide_remove<G: SlideGroup>(
    before: &[G],
    after: &[G],
    g: usize,
) -> GroupCollectionPlan {
    for (gi, (b_g, a_g)) in before.iter().zip(after.iter()).enumerate() {
        if gi != g && !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
            return GroupCollectionPlan::Wholesale;
        }
    }
    let (Some(b_g), Some(a_g)) = (before.get(g), after.get(g)) else {
        return GroupCollectionPlan::Wholesale;
    };
    match plan_flat_list(b_g.slides(), a_g.slides(), |s| s.id.as_str()) {
        FlatListPlan::Remove { index } => GroupCollectionPlan::SlideRemove { group: g, index },
        _ => GroupCollectionPlan::Wholesale,
    }
}

fn plan_group_cross_slide_move<G: SlideGroup>(
    before: &[G],
    after: &[G],
    from_g: usize,
    to_g: usize,
) -> GroupCollectionPlan {
    for (gi, (b_g, a_g)) in before.iter().zip(after.iter()).enumerate() {
        if gi != to_g && gi != from_g && !slides_in_place_compatible(b_g.slides(), a_g.slides()) {
            return GroupCollectionPlan::Wholesale;
        }
    }
    let (Some(b_from), Some(a_from), Some(b_to), Some(a_to)) = (
        before.get(from_g),
        after.get(from_g),
        before.get(to_g),
        after.get(to_g),
    ) else {
        return GroupCollectionPlan::Wholesale;
    };
    let FlatListPlan::Remove { index: from_index } =
        plan_flat_list(b_from.slides(), a_from.slides(), |s| s.id.as_str())
    else {
        return GroupCollectionPlan::Wholesale;
    };
    let FlatListPlan::Insert { index: to_index } =
        plan_flat_list(b_to.slides(), a_to.slides(), |s| s.id.as_str())
    else {
        return GroupCollectionPlan::Wholesale;
    };
    let moved_id_matches = b_from.slides().get(from_index).map(|s| s.id.as_str())
        == a_to.slides().get(to_index).map(|s| s.id.as_str());
    if moved_id_matches {
        GroupCollectionPlan::SlideMove {
            from_group: from_g,
            from_index,
            to_group: to_g,
            to_index,
        }
    } else {
        GroupCollectionPlan::Wholesale
    }
}

fn count_matching_slides(before: &Course, after: &Course) -> (usize, usize) {
    let mut total = 0usize;
    let mut changed = 0usize;
    for (bs, as_) in before.scenes.iter().zip(after.scenes.iter()) {
        for (b, a) in bs.slides.iter().zip(as_.slides.iter()) {
            total = total.saturating_add(1);
            if b != a {
                changed = changed.saturating_add(1);
            }
        }
    }
    for (b, a) in before.masters.iter().zip(after.masters.iter()) {
        total = total.saturating_add(1);
        if b != a {
            changed = changed.saturating_add(1);
        }
    }
    for (b, a) in before.templates.iter().zip(after.templates.iter()) {
        total = total.saturating_add(1);
        if b != a {
            changed = changed.saturating_add(1);
        }
    }
    for (bb, ab) in before.banks.iter().zip(after.banks.iter()) {
        for (b, a) in bb.slides.iter().zip(ab.slides.iter()) {
            total = total.saturating_add(1);
            if b != a {
                changed = changed.saturating_add(1);
            }
        }
    }
    (total, changed)
}

fn emit_flat_slide_steps(
    plan: FlatListPlan,
    before: Vec<Slide>,
    after: &[Slide],
    make_loc: fn(usize) -> SlideLoc,
    make_op: fn(ListOp<Slide>) -> StructuralOp,
    steps: &mut Vec<CourseDelta>,
) {
    match plan {
        FlatListPlan::Identical | FlatListPlan::Wholesale => {}
        FlatListPlan::InPlace => {
            for (i, (b, a)) in before.into_iter().zip(after.iter()).enumerate() {
                if let Some(change) = SlideDelta::diff(b, a) {
                    steps.push(CourseDelta::SlideChange {
                        loc: make_loc(i),
                        change,
                    });
                }
            }
        }
        FlatListPlan::Insert { index } => {
            for (i, b) in before.into_iter().enumerate() {
                let ai = if i < index { i } else { i.saturating_add(1) };
                if let Some(a) = after.get(ai)
                    && let Some(change) = SlideDelta::diff(b, a)
                {
                    steps.push(CourseDelta::SlideChange {
                        loc: make_loc(i),
                        change,
                    });
                }
            }
            steps.push(CourseDelta::Structural(Box::new(make_op(ListOp::Insert {
                index,
            }))));
        }
        FlatListPlan::Remove { index } => {
            let mut removed = None;
            for (i, b) in before.into_iter().enumerate() {
                if i == index {
                    removed = Some(b);
                } else {
                    let ai = if i < index { i } else { i.saturating_sub(1) };
                    if let Some(a) = after.get(ai)
                        && let Some(change) = SlideDelta::diff(b, a)
                    {
                        steps.push(CourseDelta::SlideChange {
                            loc: make_loc(i),
                            change,
                        });
                    }
                }
            }
            if let Some(item) = removed {
                steps.push(CourseDelta::Structural(Box::new(make_op(ListOp::Remove {
                    index,
                    item: Box::new(item),
                }))));
            }
        }
        FlatListPlan::Move { from, to } => {
            for (i, b) in before.into_iter().enumerate() {
                let ai = mapped_index_after_move(i, from, to);
                if let Some(a) = after.get(ai)
                    && let Some(change) = SlideDelta::diff(b, a)
                {
                    steps.push(CourseDelta::SlideChange {
                        loc: make_loc(i),
                        change,
                    });
                }
            }
            steps.push(CourseDelta::Structural(Box::new(make_op(ListOp::Move {
                from,
                to,
            }))));
        }
    }
}

fn emit_group_meta<G: SlideGroup>(
    group_idx: usize,
    b_id: String,
    b_title: String,
    a_group: &G,
    steps: &mut Vec<CourseDelta>,
) {
    let mut meta_changed = false;
    let id_patch = if b_id != a_group.id() {
        meta_changed = true;
        Some(b_id)
    } else {
        None
    };
    let title_patch = if b_title != a_group.title() {
        meta_changed = true;
        Some(b_title)
    } else {
        None
    };
    if meta_changed {
        steps.push(CourseDelta::Structural(Box::new(G::meta_op(
            group_idx,
            id_patch,
            title_patch,
        ))));
    }
}

fn emit_group_meta_and_slides<G: SlideGroup>(
    group_idx: usize,
    b_group: G,
    a_group: &G,
    steps: &mut Vec<CourseDelta>,
) {
    let (b_id, b_title, b_slides) = b_group.into_parts();
    emit_group_meta(group_idx, b_id, b_title, a_group, steps);
    for (si, (b, a)) in b_slides
        .into_iter()
        .zip(a_group.slides().iter())
        .enumerate()
    {
        if let Some(change) = SlideDelta::diff(b, a) {
            steps.push(CourseDelta::SlideChange {
                loc: G::slide_loc(group_idx, si),
                change,
            });
        }
    }
}

fn emit_group_slide_insert<G: SlideGroup>(
    before: Vec<G>,
    after: &[G],
    group: usize,
    index: usize,
    steps: &mut Vec<CourseDelta>,
) {
    for (gi, (b_g, a_g)) in before.into_iter().zip(after.iter()).enumerate() {
        if gi != group {
            emit_group_meta_and_slides(gi, b_g, a_g, steps);
            continue;
        }
        let (b_id, b_title, b_slides) = b_g.into_parts();
        emit_group_meta(gi, b_id, b_title, a_g, steps);
        for (i, b) in b_slides.into_iter().enumerate() {
            let ai = if i < index { i } else { i.saturating_add(1) };
            if let Some(a) = a_g.slides().get(ai)
                && let Some(change) = SlideDelta::diff(b, a)
            {
                steps.push(CourseDelta::SlideChange {
                    loc: G::slide_loc(gi, i),
                    change,
                });
            }
        }
    }
    steps.push(CourseDelta::Structural(Box::new(G::slide_insert_op(
        group, index,
    ))));
}

fn emit_group_slide_remove<G: SlideGroup>(
    before: Vec<G>,
    after: &[G],
    group: usize,
    index: usize,
    steps: &mut Vec<CourseDelta>,
) {
    let mut removed = None;
    for (gi, (b_g, a_g)) in before.into_iter().zip(after.iter()).enumerate() {
        if gi != group {
            emit_group_meta_and_slides(gi, b_g, a_g, steps);
            continue;
        }
        let (b_id, b_title, b_slides) = b_g.into_parts();
        emit_group_meta(gi, b_id, b_title, a_g, steps);
        for (i, b) in b_slides.into_iter().enumerate() {
            if i == index {
                removed = Some(b);
            } else {
                let ai = if i < index { i } else { i.saturating_sub(1) };
                if let Some(a) = a_g.slides().get(ai)
                    && let Some(change) = SlideDelta::diff(b, a)
                {
                    steps.push(CourseDelta::SlideChange {
                        loc: G::slide_loc(gi, i),
                        change,
                    });
                }
            }
        }
    }
    if let Some(slide) = removed {
        steps.push(CourseDelta::Structural(Box::new(G::slide_remove_op(
            group,
            index,
            Box::new(slide),
        ))));
    }
}

fn emit_group_slide_move<G: SlideGroup>(
    before: Vec<G>,
    after: &[G],
    from_group: usize,
    from_index: usize,
    to_group: usize,
    to_index: usize,
    steps: &mut Vec<CourseDelta>,
) {
    for (gi, (b_g, a_g)) in before.into_iter().zip(after.iter()).enumerate() {
        if gi != from_group && gi != to_group {
            emit_group_meta_and_slides(gi, b_g, a_g, steps);
            continue;
        }
        let (b_id, b_title, b_slides) = b_g.into_parts();
        emit_group_meta(gi, b_id, b_title, a_g, steps);
        for (i, b) in b_slides.into_iter().enumerate() {
            let target = if from_group == to_group {
                a_g.slides()
                    .get(mapped_index_after_move(i, from_index, to_index))
            } else if gi == from_group {
                if i == from_index {
                    after.get(to_group).and_then(|g| g.slides().get(to_index))
                } else {
                    let ai = if i < from_index {
                        i
                    } else {
                        i.saturating_sub(1)
                    };
                    a_g.slides().get(ai)
                }
            } else {
                let ai = if i < to_index { i } else { i.saturating_add(1) };
                a_g.slides().get(ai)
            };
            if let Some(a) = target
                && let Some(change) = SlideDelta::diff(b, a)
            {
                steps.push(CourseDelta::SlideChange {
                    loc: G::slide_loc(gi, i),
                    change,
                });
            }
        }
    }
    steps.push(CourseDelta::Structural(Box::new(G::slide_move_op(
        from_group, from_index, to_group, to_index,
    ))));
}

fn emit_group_steps<G: SlideGroup>(
    plan: GroupCollectionPlan,
    before: Vec<G>,
    after: &[G],
    steps: &mut Vec<CourseDelta>,
) {
    match plan {
        GroupCollectionPlan::Identical | GroupCollectionPlan::Wholesale => {}
        GroupCollectionPlan::InPlaceNoMeta | GroupCollectionPlan::InPlaceWithMeta => {
            for (gi, (b_g, a_g)) in before.into_iter().zip(after.iter()).enumerate() {
                emit_group_meta_and_slides(gi, b_g, a_g, steps);
            }
        }
        GroupCollectionPlan::GroupInsert { index } => {
            for (bi, b_g) in before.into_iter().enumerate() {
                let ai = if bi < index { bi } else { bi.saturating_add(1) };
                if let Some(a_g) = after.get(ai) {
                    emit_group_meta_and_slides(bi, b_g, a_g, steps);
                }
            }
            steps.push(CourseDelta::Structural(Box::new(G::list_op(
                ListOp::Insert { index },
            ))));
        }
        GroupCollectionPlan::GroupRemove { index } => {
            let mut removed = None;
            for (bi, b_g) in before.into_iter().enumerate() {
                if bi == index {
                    removed = Some(b_g);
                } else {
                    let ai = if bi < index { bi } else { bi.saturating_sub(1) };
                    if let Some(a_g) = after.get(ai) {
                        emit_group_meta_and_slides(bi, b_g, a_g, steps);
                    }
                }
            }
            if let Some(item) = removed {
                steps.push(CourseDelta::Structural(Box::new(G::list_op(
                    ListOp::Remove {
                        index,
                        item: Box::new(item),
                    },
                ))));
            }
        }
        GroupCollectionPlan::GroupMove { from, to } => {
            for (bi, b_g) in before.into_iter().enumerate() {
                let ai = mapped_index_after_move(bi, from, to);
                if let Some(a_g) = after.get(ai) {
                    emit_group_meta_and_slides(bi, b_g, a_g, steps);
                }
            }
            steps.push(CourseDelta::Structural(Box::new(G::list_op(
                ListOp::Move { from, to },
            ))));
        }
        GroupCollectionPlan::SlideInsert { group, index } => {
            emit_group_slide_insert(before, after, group, index, steps);
        }
        GroupCollectionPlan::SlideRemove { group, index } => {
            emit_group_slide_remove(before, after, group, index, steps);
        }
        GroupCollectionPlan::SlideMove {
            from_group,
            from_index,
            to_group,
            to_index,
        } => {
            emit_group_slide_move(
                before, after, from_group, from_index, to_group, to_index, steps,
            );
        }
    }
}

// ------------------------------------------------------------------ size estimation

fn estimate_vec_plain<T>(vec: &[T]) -> usize {
    vec.len().saturating_mul(size_of::<T>())
}

fn estimate_opt_string(opt: &Option<String>) -> usize {
    opt.as_ref().map_or(0, String::capacity)
}

fn estimate_question_heap(q: &Question) -> usize {
    q.prompt
        .capacity()
        .saturating_add(estimate_vec_plain(&q.choices))
        .saturating_add(estimate_vec_plain(&q.accepted))
        .saturating_add(estimate_vec_plain(&q.pairs))
        .saturating_add(estimate_vec_plain(&q.regions))
        .saturating_add(estimate_vec_plain(&q.drag_items))
        .saturating_add(estimate_vec_plain(&q.drop_targets))
}

fn estimate_object_kind_heap(kind: &ObjectKind) -> usize {
    match kind {
        ObjectKind::Text {
            text,
            runs,
            paragraphs,
            text_style,
            ..
        } => text
            .capacity()
            .saturating_add(estimate_vec_plain(runs))
            .saturating_add(estimate_vec_plain(paragraphs))
            .saturating_add(estimate_opt_string(text_style)),
        ObjectKind::Shape { text, adjust, .. } => {
            let text_bytes = text.as_ref().map_or(0, |t| {
                t.text
                    .capacity()
                    .saturating_add(estimate_vec_plain(&t.runs))
                    .saturating_add(estimate_vec_plain(&t.paragraphs))
            });
            text_bytes.saturating_add(estimate_vec_plain(adjust))
        }
        ObjectKind::Image { src, alt, .. } => src.capacity().saturating_add(alt.capacity()),
        ObjectKind::Audio { src, captions, .. } => {
            src.capacity().saturating_add(estimate_opt_string(captions))
        }
        ObjectKind::Video {
            src,
            captions,
            cues,
            poster,
            ..
        } => src
            .capacity()
            .saturating_add(estimate_opt_string(captions))
            .saturating_add(estimate_vec_plain(cues))
            .saturating_add(estimate_opt_string(poster)),
        ObjectKind::Table { columns, rows, .. } => {
            let row_bytes = rows.iter().fold(estimate_vec_plain(rows), |acc, r| {
                acc.saturating_add(estimate_vec_plain(&r.cells))
            });
            estimate_vec_plain(columns).saturating_add(row_bytes)
        }
        ObjectKind::Path { path } => {
            estimate_vec_plain(&path.points).saturating_add(estimate_vec_plain(&path.subpaths))
        }
        ObjectKind::Marker {
            title,
            body,
            runs,
            paragraphs,
            ..
        } => title
            .capacity()
            .saturating_add(body.capacity())
            .saturating_add(estimate_vec_plain(runs))
            .saturating_add(estimate_vec_plain(paragraphs)),
        ObjectKind::Button { label, .. } => label.capacity(),
        ObjectKind::RadioGroup { variable, options } => variable
            .capacity()
            .saturating_add(estimate_vec_plain(options)),
        ObjectKind::Lottie { src, .. } => src.capacity(),
        ObjectKind::TextEntry {
            variable,
            placeholder,
            ..
        } => variable.capacity().saturating_add(placeholder.capacity()),
        ObjectKind::Checkbox { variable, label } => {
            variable.capacity().saturating_add(label.capacity())
        }
        ObjectKind::Slider { variable, .. }
        | ObjectKind::Dial { variable, .. }
        | ObjectKind::ProgressMeter { variable, .. } => variable.capacity(),
        ObjectKind::Web { .. } | ObjectKind::ScrollPanel { .. } | ObjectKind::Timer { .. } => 0,
    }
}

pub(crate) fn estimate_object(obj: &Object) -> usize {
    let state_bytes = obj.states.iter().fold(
        obj.states.capacity().saturating_mul(size_of::<StateDef>()),
        |acc, s| {
            acc.saturating_add(s.name.capacity())
                .saturating_add(estimate_opt_string(&s.text))
                .saturating_add(estimate_opt_string(&s.src))
                .saturating_add(estimate_vec_plain(&s.extras))
        },
    );
    let path_bytes = obj.motion_paths.iter().fold(
        obj.motion_paths
            .capacity()
            .saturating_mul(size_of::<MotionPath>()),
        |acc, mp| acc.saturating_add(estimate_vec_plain(&mp.points)),
    );
    size_of::<Object>()
        .saturating_add(obj.id.capacity())
        .saturating_add(obj.name.capacity())
        .saturating_add(obj.initial_state.capacity())
        .saturating_add(estimate_opt_string(&obj.description))
        .saturating_add(estimate_opt_string(&obj.group))
        .saturating_add(estimate_opt_string(&obj.panel))
        .saturating_add(state_bytes)
        .saturating_add(path_bytes)
        .saturating_add(estimate_object_kind_heap(&obj.kind))
}

fn estimate_groups(groups: &[Group]) -> usize {
    groups.iter().fold(estimate_vec_plain(groups), |acc, g| {
        acc.saturating_add(g.id.capacity())
            .saturating_add(g.name.capacity())
            .saturating_add(g.initial_state.capacity())
            .saturating_add(estimate_opt_string(&g.description))
    })
}

fn estimate_layer(layer: &Layer) -> usize {
    let obj_bytes = layer
        .objects
        .iter()
        .fold(0usize, |acc, o| acc.saturating_add(estimate_object(o)));
    size_of::<Layer>()
        .saturating_add(layer.id.capacity())
        .saturating_add(layer.name.capacity())
        .saturating_add(obj_bytes)
        .saturating_add(estimate_vec_plain(&layer.triggers))
        .saturating_add(estimate_vec_plain(&layer.cue_points))
        .saturating_add(estimate_groups(&layer.groups))
}

pub(crate) fn estimate_slide(slide: &Slide) -> usize {
    let obj_bytes = slide
        .objects
        .iter()
        .fold(0usize, |acc, o| acc.saturating_add(estimate_object(o)));
    let layer_bytes = slide
        .layers
        .iter()
        .fold(0usize, |acc, l| acc.saturating_add(estimate_layer(l)));
    size_of::<Slide>()
        .saturating_add(slide.id.capacity())
        .saturating_add(slide.title.capacity())
        .saturating_add(slide.notes.capacity())
        .saturating_add(estimate_opt_string(&slide.master))
        .saturating_add(obj_bytes)
        .saturating_add(layer_bytes)
        .saturating_add(estimate_groups(&slide.groups))
        .saturating_add(estimate_vec_plain(&slide.triggers))
        .saturating_add(estimate_vec_plain(&slide.cue_points))
        .saturating_add(slide.question.as_ref().map_or(0, estimate_question_heap))
}

fn estimate_scene(scene: &Scene) -> usize {
    let slide_bytes = scene
        .slides
        .iter()
        .fold(0usize, |acc, s| acc.saturating_add(estimate_slide(s)));
    size_of::<Scene>()
        .saturating_add(scene.id.capacity())
        .saturating_add(scene.title.capacity())
        .saturating_add(slide_bytes)
}

fn estimate_bank(bank: &QuestionBank) -> usize {
    let slide_bytes = bank
        .slides
        .iter()
        .fold(0usize, |acc, s| acc.saturating_add(estimate_slide(s)));
    size_of::<QuestionBank>()
        .saturating_add(bank.id.capacity())
        .saturating_add(bank.title.capacity())
        .saturating_add(slide_bytes)
}

fn estimate_course(course: &Course) -> usize {
    let mut total = size_of::<Course>()
        .saturating_add(course.id.capacity())
        .saturating_add(course.title.capacity())
        .saturating_add(estimate_vec_plain(&course.variables))
        .saturating_add(estimate_opt_string(&course.start_slide))
        .saturating_add(estimate_opt_string(&course.player.logo))
        .saturating_add(course.theme.font.capacity())
        .saturating_add(estimate_opt_string(&course.theme.heading_font))
        .saturating_add(estimate_vec_plain(&course.theme.palette))
        .saturating_add(estimate_opt_string(&course.language))
        .saturating_add(estimate_vec_plain(&course.fonts))
        .saturating_add(
            course
                .background_audio
                .as_ref()
                .map_or(0, |a| a.src.capacity()),
        )
        .saturating_add(estimate_vec_plain(&course.text_styles))
        .saturating_add(estimate_vec_plain(&course.resources))
        .saturating_add(estimate_vec_plain(&course.glossary))
        .saturating_add(estimate_vec_plain(&course.editor.guides));
    for (k, v) in &course.strings {
        total = total
            .saturating_add(k.capacity())
            .saturating_add(v.capacity())
            .saturating_add(size_of::<(String, String)>());
    }
    for s in &course.scenes {
        total = total.saturating_add(estimate_scene(s));
    }
    for m in &course.masters {
        total = total.saturating_add(estimate_slide(m));
    }
    for t in &course.templates {
        total = total.saturating_add(estimate_slide(t));
    }
    for b in &course.banks {
        total = total.saturating_add(estimate_bank(b));
    }
    total
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::Session;
    use coursecraft_course::{
        Action, Autofit, Choice, Event, FontStyle, Guide, QuestionKind, Rect, ResourceTarget,
        Scoring, SeekMode, ShapeKind, SlideTarget, TextStyle, Timing, TransitionEffect, Value,
    };

    fn sample_object(id: &str, x: f32) -> Object {
        Object {
            id: id.to_owned(),
            name: format!("Obj {id}"),
            kind: ObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                fill: Color("#2F6FEB".to_owned()),
                fill_style: None,
                outline: None,
                text: None,
                adjust: Vec::new(),
            },
            frame: Rect {
                x,
                y: 20.0,
                w: 100.0,
                h: 50.0,
            },
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".to_owned(),
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

    fn sample_layer(id: &str, objects: Vec<Object>) -> Layer {
        Layer {
            id: id.to_owned(),
            name: format!("Layer {id}"),
            duration_ms: 5000,
            objects,
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
        }
    }

    fn sample_slide() -> Slide {
        Slide {
            id: "slide1".to_owned(),
            title: "Slide 1".to_owned(),
            duration_ms: 5000,
            background: None,
            objects: vec![
                sample_object("o1", 10.0),
                sample_object("o2", 120.0),
                sample_object("o3", 230.0),
                sample_object("o4", 340.0),
            ],
            layers: vec![sample_layer(
                "layer1",
                vec![
                    sample_object("lo1", 10.0),
                    sample_object("lo2", 120.0),
                    sample_object("lo3", 230.0),
                    sample_object("lo4", 340.0),
                ],
            )],
            groups: Vec::new(),
            triggers: Vec::new(),
            cue_points: Vec::new(),
            question: None,
            draw: None,
            transition: None,
            notes: String::new(),
            master: None,
            revisit: RevisitMode::Auto,
            player: None,
        }
    }

    fn slide_json(slide: &Slide) -> String {
        serde_json::to_string_pretty(slide).unwrap()
    }

    fn assert_slide_roundtrip(before: Slide, mut after: Slide) -> SlideDelta {
        let before_json = slide_json(&before);
        let after_json = slide_json(&after);
        assert_ne!(before_json, after_json);

        let mut delta = SlideDelta::diff(before, &after).expect("expected Some(delta)");
        let returned_variant = delta.clone();

        delta.apply(&mut after).expect("undo apply must succeed");
        assert_eq!(
            slide_json(&after),
            before_json,
            "undo must restore exact JSON"
        );

        delta.apply(&mut after).expect("redo apply must succeed");
        assert_eq!(
            slide_json(&after),
            after_json,
            "redo must restore exact JSON"
        );

        returned_variant
    }

    #[test]
    fn slide_loc_find_and_lookup_all_collections() {
        let mut course = Session::blank("Loc Test").course;
        let mut master = sample_slide();
        master.id = "master1".to_owned();
        course.masters.push(master);

        let mut tmpl = sample_slide();
        tmpl.id = "tmpl1".to_owned();
        course.templates.push(tmpl);

        let mut bank_slide = sample_slide();
        bank_slide.id = "bank_q1".to_owned();
        course.banks.push(QuestionBank {
            id: "bank1".to_owned(),
            title: "Bank 1".to_owned(),
            slides: vec![bank_slide],
        });

        let scene_loc = SlideLoc::find(&course, "slide1").unwrap();
        assert_eq!(scene_loc, SlideLoc::SceneSlide { scene: 0, slide: 0 });
        assert_eq!(slide_at(&course, scene_loc).unwrap().id, "slide1");

        let master_loc = SlideLoc::find(&course, "master1").unwrap();
        assert_eq!(master_loc, SlideLoc::Master(0));
        assert_eq!(slide_at(&course, master_loc).unwrap().id, "master1");

        let tmpl_loc = SlideLoc::find(&course, "tmpl1").unwrap();
        assert_eq!(tmpl_loc, SlideLoc::Template(0));
        assert_eq!(slide_at(&course, tmpl_loc).unwrap().id, "tmpl1");

        let bank_loc = SlideLoc::find(&course, "bank_q1").unwrap();
        assert_eq!(bank_loc, SlideLoc::BankSlide { bank: 0, slide: 0 });
        assert_eq!(slide_at(&course, bank_loc).unwrap().id, "bank_q1");

        slide_at_mut(&mut course, bank_loc).unwrap().title = "Edited Bank".to_owned();
        assert_eq!(slide_at(&course, bank_loc).unwrap().title, "Edited Bank");

        assert_eq!(SlideLoc::find(&course, "missing"), None);
        assert!(
            slide_at(
                &course,
                SlideLoc::SceneSlide {
                    scene: 99,
                    slide: 0
                }
            )
            .is_none()
        );
        assert!(slide_at(&course, SlideLoc::Master(99)).is_none());
        assert!(slide_at(&course, SlideLoc::Template(99)).is_none());
        assert!(slide_at(&course, SlideLoc::BankSlide { bank: 99, slide: 0 }).is_none());
        assert!(slide_at_mut(&mut course, SlideLoc::Master(99)).is_none());
    }

    /// U-01: Single-object modify / insert / remove / reorder on base layer & slide layers.
    #[test]
    fn u_01_slide_delta_single_object_ops_base_and_layer() {
        let base = sample_slide();
        assert_eq!(SlideDelta::diff(base.clone(), &base), None);

        fn objects_mut(s: &mut Slide, layer: Option<usize>) -> &mut Vec<Object> {
            match layer {
                None => &mut s.objects,
                Some(li) => &mut s.layers[li].objects,
            }
        }

        for layer in [None, Some(0usize)] {
            // 1. Modify
            let mut after_mod = base.clone();
            objects_mut(&mut after_mod, layer)[1].frame.x = 777.0;
            let delta = assert_slide_roundtrip(base.clone(), after_mod);
            assert!(matches!(
                delta,
                SlideDelta::ObjectModified {
                    layer: l,
                    index: 1,
                    ..
                } if l == layer
            ));
            assert!(delta.estimated_bytes() < 4096);

            // 2. Insert at start, middle, end
            for insert_idx in [0usize, 2, 4] {
                let mut after_ins = base.clone();
                objects_mut(&mut after_ins, layer)
                    .insert(insert_idx, sample_object("new_obj", 555.0));
                let delta = assert_slide_roundtrip(base.clone(), after_ins);
                assert_eq!(
                    delta,
                    SlideDelta::ObjectInserted {
                        layer,
                        index: insert_idx
                    }
                );
                assert!(delta.estimated_bytes() < 128);
            }

            // 3. Remove from start, middle, end
            for remove_idx in [0usize, 2, 3] {
                let mut after_rem = base.clone();
                let removed = objects_mut(&mut after_rem, layer).remove(remove_idx);
                let delta = assert_slide_roundtrip(base.clone(), after_rem);
                assert!(matches!(
                    delta,
                    SlideDelta::ObjectRemoved {
                        layer: l,
                        index,
                        ref object
                    } if l == layer && index == remove_idx && **object == removed
                ));
            }

            // 4. Reorder forward (from < to): move index 0 -> index 3
            let mut after_fwd = base.clone();
            let item = objects_mut(&mut after_fwd, layer).remove(0);
            objects_mut(&mut after_fwd, layer).insert(3, item);
            let delta = assert_slide_roundtrip(base.clone(), after_fwd);
            assert_eq!(
                delta,
                SlideDelta::ObjectReordered {
                    layer,
                    from: 0,
                    to: 3
                }
            );
            assert!(delta.estimated_bytes() < 128);

            // 5. Reorder backward (from > to): move index 3 -> index 1
            let mut after_bwd = base.clone();
            let item = objects_mut(&mut after_bwd, layer).remove(3);
            objects_mut(&mut after_bwd, layer).insert(1, item);
            let delta = assert_slide_roundtrip(base.clone(), after_bwd);
            assert_eq!(
                delta,
                SlideDelta::ObjectReordered {
                    layer,
                    from: 3,
                    to: 1
                }
            );
            assert!(delta.estimated_bytes() < 128);

            // 6. Adjacent swap: index 1 <-> 2
            let mut after_swap = base.clone();
            objects_mut(&mut after_swap, layer).swap(1, 2);
            let delta = assert_slide_roundtrip(base.clone(), after_swap);
            assert!(matches!(
                delta,
                SlideDelta::ObjectReordered { layer: l, .. } if l == layer
            ));

            // 7. Multi-object change falls back to FullSlide
            let mut after_multi = base.clone();
            objects_mut(&mut after_multi, layer)[0].frame.x = 1.0;
            objects_mut(&mut after_multi, layer)[2].frame.x = 2.0;
            let delta = assert_slide_roundtrip(base.clone(), after_multi);
            assert!(matches!(delta, SlideDelta::FullSlide(_)));
        }

        // Layer metadata change falls back to FullSlide and roundtrips cleanly.
        let mut after_layer_meta = base.clone();
        after_layer_meta.layers[0].name = "Renamed Layer".to_owned();
        let delta = assert_slide_roundtrip(base, after_layer_meta);
        assert!(matches!(delta, SlideDelta::FullSlide(_)));
    }

    fn assert_slide_fields_roundtrip(base: &Slide, mutate: impl FnOnce(&mut Slide)) {
        let mut s = base.clone();
        mutate(&mut s);
        let delta = assert_slide_roundtrip(base.clone(), s);
        assert!(
            matches!(delta, SlideDelta::Fields(_)),
            "expected SlideDelta::Fields, got {delta:?}"
        );
        assert!(delta.estimated_bytes() > 0);
    }

    /// U-02: Mutate each of the 15 `Slide` fields individually and verify delta + exact roundtrip.
    #[test]
    fn u_02_slide_delta_all_15_fields() {
        let base = sample_slide();

        // 1. id
        assert_slide_fields_roundtrip(&base, |s| s.id = "slide_renamed".to_owned());
        // 2. title
        assert_slide_fields_roundtrip(&base, |s| s.title = "Updated Title".to_owned());
        // 3. duration_ms
        assert_slide_fields_roundtrip(&base, |s| s.duration_ms = 12_500);
        // 4. background
        assert_slide_fields_roundtrip(&base, |s| s.background = Some(Color("#112233".to_owned())));

        // 5. objects
        let mut s = base.clone();
        s.objects.push(sample_object("o_extra", 500.0));
        assert!(matches!(
            assert_slide_roundtrip(base.clone(), s),
            SlideDelta::ObjectInserted {
                layer: None,
                index: 4
            }
        ));

        // 6. layers (adding a new layer -> FullSlide; editing object in layer -> ObjectModified)
        let mut s = base.clone();
        s.layers.push(sample_layer("layer2", Vec::new()));
        assert!(matches!(
            assert_slide_roundtrip(base.clone(), s),
            SlideDelta::FullSlide(_)
        ));

        let mut s = base.clone();
        s.layers[0].objects[0].name = "Renamed in Layer".to_owned();
        assert!(matches!(
            assert_slide_roundtrip(base.clone(), s),
            SlideDelta::ObjectModified {
                layer: Some(0),
                index: 0,
                ..
            }
        ));

        // 7. triggers
        assert_slide_fields_roundtrip(&base, |s| {
            s.triggers.push(Trigger {
                action: Action::JumpToSlide {
                    target: SlideTarget::Next,
                },
                when: Event::TimelineEnds { layer: None },
                condition: None,
                disabled: false,
            });
        });
        // 8. cue_points
        assert_slide_fields_roundtrip(&base, |s| {
            s.cue_points.push(SlideCue {
                id: "cue1".to_owned(),
                name: "Intro".to_owned(),
                at_ms: 1500,
            });
        });
        // 9. question
        assert_slide_fields_roundtrip(&base, |s| {
            s.question = Some(Question {
                kind: QuestionKind::MultipleChoice,
                prompt: "2 + 2?".to_owned(),
                choices: vec![Choice {
                    id: "a".to_owned(),
                    text: "4".to_owned(),
                    correct: true,
                    points: 10,
                }],
                points: 10,
                scoring: Scoring::AllOrNothing,
                attempts: 1,
                shuffle: false,
                correct_layer: None,
                incorrect_layer: None,
                try_again_layer: None,
                accepted: Vec::new(),
                case_sensitive: false,
                pairs: Vec::new(),
                hotspot_image: None,
                regions: Vec::new(),
                markers: 1,
                drag_items: Vec::new(),
                drop_targets: Vec::new(),
                snap_to_center: true,
                return_if_wrong: false,
                one_per_target: true,
                correct_when: None,
                frame: None,
            });
        });
        // 10. draw
        assert_slide_fields_roundtrip(&base, |s| {
            s.draw = Some(BankDraw {
                bank: "bank1".to_owned(),
                count: 3,
            });
        });
        // 11. transition
        assert_slide_fields_roundtrip(&base, |s| {
            s.transition = Some(Transition {
                effect: TransitionEffect::Fade,
                duration_ms: 400,
            });
        });
        // 12. notes
        assert_slide_fields_roundtrip(&base, |s| s.notes = "Speaker notes here.".to_owned());
        // 13. master
        assert_slide_fields_roundtrip(&base, |s| s.master = Some("master1".to_owned()));
        // 14. revisit
        assert_slide_fields_roundtrip(&base, |s| s.revisit = RevisitMode::Reset);
        // 15. player
        assert_slide_fields_roundtrip(&base, |s| {
            s.player = Some(SlidePlayerOverrides {
                show_bar: Some(false),
                next_requires: None,
                show_nav: Some(false),
                show_seekbar: None,
                show_menu: None,
            });
        });
    }

    fn assert_header_field_roundtrip(base: &Course, mutate: impl FnOnce(&mut Course)) {
        let before_json = base.to_json().unwrap();
        let mut course = base.clone();
        let snap = CourseHeaderSnapshot::capture(&course);
        mutate(&mut course);
        let after_json = course.to_json().unwrap();
        assert_ne!(before_json, after_json);

        // Verify restore()
        let mut for_restore = course.clone();
        snap.clone().restore(&mut for_restore);
        assert_eq!(for_restore.to_json().unwrap(), before_json);

        // Verify diff() + apply() undo -> redo
        let mut patch = CourseHeaderPatch::diff(snap, &course).expect("expected header patch");
        assert!(patch.estimated_bytes() > 0);

        patch.apply(&mut course);
        assert_eq!(
            course.to_json().unwrap(),
            before_json,
            "header undo mismatch"
        );

        patch.apply(&mut course);
        assert_eq!(
            course.to_json().unwrap(),
            after_json,
            "header redo mismatch"
        );
    }

    /// U-03: Mutate each of the 17 `Course` header fields individually and verify exact roundtrip.
    #[test]
    fn u_03_course_header_patch_all_17_fields() {
        let base = Session::blank("Header Test").course;
        let unchanged_snap = CourseHeaderSnapshot::capture(&base);
        assert_eq!(CourseHeaderPatch::diff(unchanged_snap, &base), None);

        // 1. schema_version
        assert_header_field_roundtrip(&base, |c| c.schema_version = 2);
        // 2. id
        assert_header_field_roundtrip(&base, |c| c.id = "course_v2".to_owned());
        // 3. title
        assert_header_field_roundtrip(&base, |c| c.title = "Renamed Course".to_owned());
        // 4. slide_size
        assert_header_field_roundtrip(&base, |c| c.slide_size = Size { w: 1920, h: 1080 });
        // 5. variables
        assert_header_field_roundtrip(&base, |c| {
            c.variables.push(VariableDef {
                name: "score".to_owned(),
                value: Value::Number(42.0),
            });
        });
        // 6. start_slide
        assert_header_field_roundtrip(&base, |c| c.start_slide = Some("slide1".to_owned()));
        // 7. quiz
        assert_header_field_roundtrip(&base, |c| {
            c.quiz = Some(QuizSettings {
                passing_percent: 80,
            });
        });
        // 8. player
        assert_header_field_roundtrip(&base, |c| {
            c.player.show_bar = false;
            c.player.logo = Some("logo.png".to_owned());
        });
        // 9. theme
        assert_header_field_roundtrip(&base, |c| {
            c.theme.accent = Color("#FF0000".to_owned());
            c.theme.font = "Lato".to_owned();
        });
        // 10. language
        assert_header_field_roundtrip(&base, |c| c.language = Some("fr-CA".to_owned()));
        // 11. strings
        assert_header_field_roundtrip(&base, |c| {
            c.strings.insert("play".to_owned(), "Lire".to_owned());
        });
        // 12. fonts
        assert_header_field_roundtrip(&base, |c| {
            c.fonts.push(FontFace {
                family: "Lato".to_owned(),
                style: FontStyle::Regular,
                src: "fonts/lato-regular.ttf".to_owned(),
            });
        });
        // 13. background_audio
        assert_header_field_roundtrip(&base, |c| {
            c.background_audio = Some(BackgroundAudio {
                src: "audio/bg.mp3".to_owned(),
                volume: 0.4,
                looping: true,
            });
        });
        // 14. text_styles
        assert_header_field_roundtrip(&base, |c| {
            c.text_styles.push(NamedTextStyle {
                id: "heading".to_owned(),
                name: "Heading".to_owned(),
                style: TextStyle::default(),
                paragraphs: None,
            });
        });
        // 15. resources
        assert_header_field_roundtrip(&base, |c| {
            c.resources.push(Resource {
                id: "res1".to_owned(),
                title: "Handbook".to_owned(),
                target: ResourceTarget::Url {
                    url: "https://example.com".to_owned(),
                },
            });
        });
        // 16. glossary
        assert_header_field_roundtrip(&base, |c| {
            c.glossary.push(GlossaryEntry {
                id: "g1".to_owned(),
                term: "LMS".to_owned(),
                definition: "Learning Management System".to_owned(),
            });
        });
        // 17. editor
        assert_header_field_roundtrip(&base, |c| {
            c.editor.show_rulers = true;
            c.editor.guides.push(Guide {
                vertical: true,
                at: 640.0,
            });
        });

        // Sanity check that `Autofit` import isn't needed elsewhere; verify a Text object estimate too.
        let mut text_obj = sample_object("t1", 0.0);
        text_obj.kind = ObjectKind::Text {
            text: "Hello CourseCraft".to_owned(),
            style: TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
            outline: None,
            point_text: false,
        };
        assert!(estimate_object(&text_obj) > size_of::<Object>());
    }

    /// U-06: Hostile out-of-bounds `SlideDelta::apply` returns `Err(Invalid)` with zero panics
    /// and leaves both the slide and the delta untouched.
    #[test]
    fn u_06_slide_delta_hostile_out_of_bounds_no_panic() {
        let orig = sample_slide();
        let orig_json = slide_json(&orig);
        let obj = Box::new(sample_object("hostile", 0.0));

        let bad_deltas = vec![
            // Out-of-bounds base object indices
            SlideDelta::ObjectModified {
                layer: None,
                index: 4,
                object: obj.clone(),
            },
            SlideDelta::ObjectModified {
                layer: None,
                index: usize::MAX,
                object: obj.clone(),
            },
            SlideDelta::ObjectInserted {
                layer: None,
                index: 4,
            },
            SlideDelta::ObjectInserted {
                layer: None,
                index: usize::MAX,
            },
            SlideDelta::ObjectRemoved {
                layer: None,
                index: 5, // > len (4)
                object: obj.clone(),
            },
            SlideDelta::ObjectRemoved {
                layer: None,
                index: usize::MAX,
                object: obj.clone(),
            },
            SlideDelta::ObjectReordered {
                layer: None,
                from: 4,
                to: 0,
            },
            SlideDelta::ObjectReordered {
                layer: None,
                from: 0,
                to: 4,
            },
            SlideDelta::ObjectReordered {
                layer: None,
                from: usize::MAX,
                to: usize::MAX,
            },
            // Out-of-bounds layer index (layer 99 does not exist)
            SlideDelta::ObjectModified {
                layer: Some(99),
                index: 0,
                object: obj.clone(),
            },
            SlideDelta::ObjectInserted {
                layer: Some(99),
                index: 0,
            },
            SlideDelta::ObjectRemoved {
                layer: Some(99),
                index: 0,
                object: obj.clone(),
            },
            SlideDelta::ObjectReordered {
                layer: Some(99),
                from: 0,
                to: 1,
            },
            // Valid layer 0, out-of-bounds object index
            SlideDelta::ObjectModified {
                layer: Some(0),
                index: 4,
                object: obj.clone(),
            },
            SlideDelta::ObjectInserted {
                layer: Some(0),
                index: 4,
            },
            SlideDelta::ObjectRemoved {
                layer: Some(0),
                index: 5,
                object: obj,
            },
            SlideDelta::ObjectReordered {
                layer: Some(0),
                from: 1,
                to: 99,
            },
        ];

        for mut delta in bad_deltas {
            let delta_before = delta.clone();
            let mut slide = orig.clone();
            let res = delta.apply(&mut slide);
            assert!(
                matches!(res, Err(EngineError::Invalid(_))),
                "expected Err(Invalid), got {res:?} for {delta_before:?}"
            );
            assert_eq!(slide_json(&slide), orig_json, "slide mutated on error");
            assert_eq!(delta, delta_before, "delta mutated on error");
        }
    }

    fn named_slide(id: &str, title: &str) -> Slide {
        let mut s = sample_slide();
        s.id = id.to_owned();
        s.title = title.to_owned();
        s
    }

    fn multi_scene_course() -> Course {
        let mut course = Session::blank("Structural Test").course;
        course.scenes = vec![
            Scene {
                id: "sc1".to_owned(),
                title: "Scene 1".to_owned(),
                slides: vec![
                    named_slide("s1", "Slide 1"),
                    named_slide("s2", "Slide 2"),
                    named_slide("s3", "Slide 3"),
                ],
            },
            Scene {
                id: "sc2".to_owned(),
                title: "Scene 2".to_owned(),
                slides: vec![named_slide("s4", "Slide 4"), named_slide("s5", "Slide 5")],
            },
        ];
        course.masters = vec![named_slide("m1", "Master 1"), named_slide("m2", "Master 2")];
        course.templates = vec![
            named_slide("t1", "Template 1"),
            named_slide("t2", "Template 2"),
        ];
        course.banks = vec![
            QuestionBank {
                id: "b1".to_owned(),
                title: "Bank 1".to_owned(),
                slides: vec![named_slide("bq1", "Bank Q1"), named_slide("bq2", "Bank Q2")],
            },
            QuestionBank {
                id: "b2".to_owned(),
                title: "Bank 2".to_owned(),
                slides: vec![named_slide("bq3", "Bank Q3")],
            },
        ];
        // Attach a master to s1 and jump triggers to s2 from s3 (same scene) and s4 (other scene).
        course.scenes[0].slides[0].master = Some("m1".to_owned());
        let jump_to_s2 = Trigger {
            action: Action::JumpToSlide {
                target: SlideTarget::Id("s2".to_owned()),
            },
            when: Event::TimelineEnds { layer: None },
            condition: None,
            disabled: false,
        };
        course.scenes[0].slides[2].triggers.push(jump_to_s2.clone());
        course.scenes[1].slides[0].triggers.push(jump_to_s2);
        course
    }

    fn assert_course_roundtrip(before: Course, mut after: Course) -> CourseDelta {
        let before_json = before.to_json().unwrap();
        let after_json = after.to_json().unwrap();
        assert_ne!(before_json, after_json);

        let mut delta = CourseDelta::diff(before, &after).expect("expected Some(CourseDelta)");
        assert!(delta.estimated_bytes() > 0);
        let returned = delta.clone();

        delta.apply(&mut after).expect("course undo must succeed");
        assert_eq!(
            after.to_json().unwrap(),
            before_json,
            "course undo must restore exact JSON"
        );

        delta.apply(&mut after).expect("course redo must succeed");
        assert_eq!(
            after.to_json().unwrap(),
            after_json,
            "course redo must restore exact JSON"
        );

        returned
    }

    fn assert_compact_roundtrip(
        base: &Course,
        label: &str,
        mutate: impl FnOnce(&mut Course),
    ) -> CourseDelta {
        let mut after = base.clone();
        mutate(&mut after);
        let delta = assert_course_roundtrip(base.clone(), after);
        assert!(
            !matches!(delta, CourseDelta::FullCourse(_)),
            "{label} must be compact, got {delta:?}"
        );
        delta
    }

    /// U-04: Structural scene/slide/master/template/bank ops are compact (never `FullCourse`)
    /// and roundtrip `undo -> redo` byte-identically.
    #[test]
    fn u_04_course_delta_structural_ops_are_compact_and_roundtrip() {
        let base = multi_scene_course();
        assert_eq!(CourseDelta::diff(base.clone(), &base), None);

        // 1. Slide insert (start, middle, end)
        for idx in [0usize, 1, 3] {
            assert_compact_roundtrip(&base, "slide insert", |c| {
                c.scenes[0]
                    .slides
                    .insert(idx, named_slide("s_new", "Inserted"));
            });
        }

        // 2. Slide delete with cross-slide trigger cleanup in SAME scene (s3) AND OTHER scene (s4)
        assert_compact_roundtrip(&base, "slide delete + trigger cleanup", |c| {
            c.scenes[0].slides.remove(1); // remove s2
            c.scenes[0].slides[1].triggers.clear(); // s3 (shifted to index 1)
            c.scenes[1].slides[0].triggers.clear(); // s4 in scene 1
            c.start_slide = Some("s1".to_owned());
        });

        // 3. Slide move within same scene (forward and backward)
        assert_compact_roundtrip(&base, "same-scene slide move fwd", |c| {
            let s = c.scenes[0].slides.remove(0);
            c.scenes[0].slides.insert(2, s);
        });
        assert_compact_roundtrip(&base, "same-scene slide move bwd", |c| {
            let s = c.scenes[0].slides.remove(2);
            c.scenes[0].slides.insert(0, s);
        });

        // 4. Slide move across scenes (also modifying the moved slide and another slide)
        assert_compact_roundtrip(&base, "cross-scene slide move", |c| {
            let mut moved = c.scenes[0].slides.remove(1); // s2
            moved.title = "Moved S2".to_owned();
            c.scenes[1].slides.insert(1, moved);
            c.scenes[0].slides[1].notes = "Sibling updated".to_owned();
        });

        // 5. Scene add, delete (with cross-scene trigger cleanup), move, and rename
        assert_compact_roundtrip(&base, "scene add", |c| {
            c.scenes.push(Scene {
                id: "sc3".to_owned(),
                title: "Scene 3".to_owned(),
                slides: vec![named_slide("s6", "Slide 6")],
            });
        });
        assert_compact_roundtrip(&base, "scene delete", |c| {
            c.scenes.remove(0);
            c.scenes[0].triggers_clear_for_test();
        });
        assert_compact_roundtrip(&base, "scene move", |c| {
            let sc = c.scenes.remove(1);
            c.scenes.insert(0, sc);
        });
        let rename_scene = assert_compact_roundtrip(&base, "scene rename", |c| {
            c.scenes[0].title = "Renamed Scene 1".to_owned();
        });
        assert!(matches!(
            rename_scene,
            CourseDelta::Structural(ref op) if matches!(op.as_ref(), StructuralOp::SceneMeta { scene: 0, .. })
        ));

        // 6. Master add, delete (clearing slide.master on s1), and move
        assert_compact_roundtrip(&base, "master add", |c| {
            c.masters.push(named_slide("m3", "Master 3"));
        });
        assert_compact_roundtrip(&base, "master delete + slide.master clear", |c| {
            c.masters.remove(0); // remove m1
            c.scenes[0].slides[0].master = None;
        });
        assert_compact_roundtrip(&base, "master move", |c| {
            c.masters.swap(0, 1);
        });

        // 7. Template add, delete, and move
        assert_compact_roundtrip(&base, "template add", |c| {
            c.templates.insert(0, named_slide("t0", "Template 0"));
        });
        assert_compact_roundtrip(&base, "template delete", |c| {
            c.templates.remove(1);
        });
        assert_compact_roundtrip(&base, "template move", |c| {
            c.templates.swap(0, 1);
        });

        // 8. Bank add, delete, rename, and bank slide add/remove/move (same & cross bank)
        assert_compact_roundtrip(&base, "bank add", |c| {
            c.banks.push(QuestionBank {
                id: "b3".to_owned(),
                title: "Bank 3".to_owned(),
                slides: Vec::new(),
            });
        });
        assert_compact_roundtrip(&base, "bank delete", |c| {
            c.banks.remove(0);
        });
        let rename_bank = assert_compact_roundtrip(&base, "bank rename", |c| {
            c.banks[0].title = "Renamed Bank".to_owned();
        });
        assert!(matches!(
            rename_bank,
            CourseDelta::Structural(ref op) if matches!(op.as_ref(), StructuralOp::BankMeta { bank: 0, .. })
        ));
        assert_compact_roundtrip(&base, "bank slide add", |c| {
            c.banks[0].slides.insert(1, named_slide("bq_new", "New Q"));
        });
        assert_compact_roundtrip(&base, "bank slide remove", |c| {
            c.banks[0].slides.remove(0);
        });
        assert_compact_roundtrip(&base, "bank slide move within bank", |c| {
            c.banks[0].slides.swap(0, 1);
        });
        assert_compact_roundtrip(&base, "bank slide move across banks", |c| {
            let moved = c.banks[0].slides.remove(1);
            c.banks[1].slides.insert(0, moved);
        });

        // 9. Cross-collection slide move (scene -> bank) represented as Sequence of two StructuralOps
        assert_compact_roundtrip(&base, "scene -> bank move", |c| {
            let slide = c.scenes[1].slides.remove(1);
            c.banks[1].slides.push(slide);
        });
    }

    trait SceneTriggerTestExt {
        fn triggers_clear_for_test(&mut self);
    }

    impl SceneTriggerTestExt for Scene {
        fn triggers_clear_for_test(&mut self) {
            for s in &mut self.slides {
                s.triggers.clear();
            }
        }
    }

    /// U-05: Wholesale fallback when strictly more than 50% of slides change (6 of 10 slides).
    #[test]
    fn u_05_course_delta_wholesale_fallback_over_half_slides() {
        let mut base = Session::blank("Wholesale Test").course;
        base.scenes[0].slides = (0..10)
            .map(|i| named_slide(&format!("s{i}"), &format!("Slide {i}")))
            .collect();

        // 1 slide changed -> SlideChange
        let mut one = base.clone();
        one.scenes[0].slides[3].objects[0].frame.x = 333.0;
        let d_one = assert_course_roundtrip(base.clone(), one);
        assert!(matches!(d_one, CourseDelta::SlideChange { .. }));

        // 5 of 10 slides changed (exactly 50%, not strictly > 50%) -> Patch
        let mut five = base.clone();
        for (i, slide) in five.scenes[0].slides.iter_mut().take(5).enumerate() {
            slide.title = format!("Edited {i}");
        }
        let d_five = assert_course_roundtrip(base.clone(), five);
        assert!(matches!(d_five, CourseDelta::Patch(_)));

        // 6 of 10 slides changed (> 50%) -> FullCourse
        let mut six = base.clone();
        for (i, slide) in six.scenes[0].slides.iter_mut().take(6).enumerate() {
            slide.title = format!("Scaled {i}");
        }
        let d_six = assert_course_roundtrip(base.clone(), six);
        assert!(
            matches!(d_six, CourseDelta::FullCourse(_)),
            "expected FullCourse when 6/10 slides changed, got {d_six:?}"
        );

        // Single-slide course with 1 slide edited must NOT fall back to FullCourse
        let single_base = Session::blank("One Slide").course;
        let mut single_after = single_base.clone();
        single_after.scenes[0].slides[0].title = "Edited Single".to_owned();
        let d_single = assert_course_roundtrip(single_base, single_after);
        assert!(
            matches!(d_single, CourseDelta::SlideChange { .. }),
            "1-slide edit on 1-slide course should be SlideChange, got {d_single:?}"
        );
    }

    /// U-06: Hostile out-of-bounds `CourseDelta::apply` returns `Err(Invalid)` with zero panics
    /// and leaves both the course and the delta completely untouched (including partial Sequence/Patch unwind).
    #[test]
    fn u_06_course_delta_hostile_out_of_bounds_no_panic() {
        let orig = multi_scene_course();
        let orig_json = orig.to_json().unwrap();
        let dummy_slide = Box::new(named_slide("hostile_s", "Hostile"));
        let dummy_scene = Box::new(Scene {
            id: "hostile_sc".to_owned(),
            title: "Hostile Scene".to_owned(),
            slides: Vec::new(),
        });
        let dummy_bank = Box::new(QuestionBank {
            id: "hostile_b".to_owned(),
            title: "Hostile Bank".to_owned(),
            slides: Vec::new(),
        });

        let valid_header = CourseDelta::Header(Box::new(CourseHeaderPatch {
            schema_version: None,
            id: None,
            title: Some("Mutated Before Failure".to_owned()),
            slide_size: None,
            variables: None,
            start_slide: None,
            quiz: None,
            tracking: None,
            player: None,
            theme: None,
            language: None,
            strings: None,
            fonts: None,
            background_audio: None,
            text_styles: None,
            resources: None,
            glossary: None,
            editor: None,
        }));
        let valid_slide_delta = SlideDelta::Fields(Box::new(SlideFieldPatch {
            id: None,
            title: Some("Mutated Slide".to_owned()),
            duration_ms: None,
            background: None,
            groups: None,
            triggers: None,
            cue_points: None,
            question: None,
            draw: None,
            transition: None,
            notes: None,
            master: None,
            revisit: None,
            player: None,
        }));
        let failing_slide_delta = SlideDelta::ObjectInserted {
            layer: None,
            index: 99,
        };

        let bad_deltas = vec![
            // Out-of-bounds SlideChange locations
            CourseDelta::SlideChange {
                loc: SlideLoc::SceneSlide {
                    scene: 99,
                    slide: 0,
                },
                change: valid_slide_delta.clone(),
            },
            CourseDelta::SlideChange {
                loc: SlideLoc::SceneSlide {
                    scene: 0,
                    slide: 99,
                },
                change: valid_slide_delta.clone(),
            },
            CourseDelta::SlideChange {
                loc: SlideLoc::Master(99),
                change: valid_slide_delta.clone(),
            },
            CourseDelta::SlideChange {
                loc: SlideLoc::Template(99),
                change: valid_slide_delta.clone(),
            },
            CourseDelta::SlideChange {
                loc: SlideLoc::BankSlide { bank: 99, slide: 0 },
                change: valid_slide_delta.clone(),
            },
            CourseDelta::SlideChange {
                loc: SlideLoc::BankSlide { bank: 0, slide: 99 },
                change: valid_slide_delta.clone(),
            },
            // Out-of-bounds StructuralOps
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideInsert {
                scene: 99,
                index: 0,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideInsert {
                scene: 0,
                index: 3, // == len
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideRemove {
                scene: 0,
                index: 4, // > len
                slide: dummy_slide.clone(),
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideMove {
                from_scene: 0,
                from_index: 0,
                to_scene: 0,
                to_index: 99,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideMove {
                from_scene: 0,
                from_index: 99, // > from_len
                to_scene: 1,
                to_index: 0,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideMove {
                from_scene: 0,
                from_index: 0,
                to_scene: 1,
                to_index: 2, // == to_len
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneSlideMove {
                from_scene: 99,
                from_index: 0,
                to_scene: 1,
                to_index: 0,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::SceneOp(ListOp::Insert { index: 2 }))),
            CourseDelta::Structural(Box::new(StructuralOp::SceneOp(ListOp::Remove {
                index: 3,
                item: dummy_scene,
            }))),
            CourseDelta::Structural(Box::new(StructuralOp::SceneOp(ListOp::Move {
                from: 0,
                to: 99,
            }))),
            CourseDelta::Structural(Box::new(StructuralOp::SceneMeta {
                scene: 99,
                id: None,
                title: Some("X".to_owned()),
            })),
            CourseDelta::Structural(Box::new(StructuralOp::MasterOp(ListOp::Insert {
                index: 99,
            }))),
            CourseDelta::Structural(Box::new(StructuralOp::TemplateOp(ListOp::Remove {
                index: 99,
                item: dummy_slide.clone(),
            }))),
            CourseDelta::Structural(Box::new(StructuralOp::BankOp(ListOp::Remove {
                index: 99,
                item: dummy_bank,
            }))),
            CourseDelta::Structural(Box::new(StructuralOp::BankMeta {
                bank: 99,
                id: None,
                title: Some("X".to_owned()),
            })),
            CourseDelta::Structural(Box::new(StructuralOp::BankSlideInsert {
                bank: 0,
                index: 2,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::BankSlideRemove {
                bank: 0,
                index: 3,
                slide: dummy_slide,
            })),
            CourseDelta::Structural(Box::new(StructuralOp::BankSlideMove {
                from_bank: 0,
                from_index: 99,
                to_bank: 1,
                to_index: 0,
            })),
            // Patch with valid header + valid first slide + failing second slide -> must unwind all!
            CourseDelta::Patch(Box::new(CoursePatch {
                header: Some(CourseHeaderPatch {
                    schema_version: None,
                    id: None,
                    title: Some("Patched Title".to_owned()),
                    slide_size: None,
                    variables: None,
                    start_slide: None,
                    quiz: None,
                    tracking: None,
                    player: None,
                    theme: None,
                    language: None,
                    strings: None,
                    fonts: None,
                    background_audio: None,
                    text_styles: None,
                    resources: None,
                    glossary: None,
                    editor: None,
                }),
                slides: vec![
                    (
                        SlideLoc::SceneSlide { scene: 0, slide: 0 },
                        valid_slide_delta.clone(),
                    ),
                    (
                        SlideLoc::SceneSlide { scene: 0, slide: 1 },
                        failing_slide_delta.clone(),
                    ),
                ],
            })),
            // Patch with out-of-bounds SlideLoc -> must reject before touching header
            CourseDelta::Patch(Box::new(CoursePatch {
                header: None,
                slides: vec![(SlideLoc::Master(99), valid_slide_delta.clone())],
            })),
            // Sequence where step 2 (last) succeeds, step 1 (second-to-last) fails -> must unwind step 2!
            CourseDelta::Sequence(vec![
                CourseDelta::SlideChange {
                    loc: SlideLoc::SceneSlide { scene: 0, slide: 0 },
                    change: valid_slide_delta,
                },
                CourseDelta::SlideChange {
                    loc: SlideLoc::SceneSlide { scene: 0, slide: 1 },
                    change: failing_slide_delta,
                },
                valid_header,
            ]),
            // Sequence where structural last step succeeds (removes s3), second-to-last fails -> must unwind structural op!
            CourseDelta::Sequence(vec![
                CourseDelta::Structural(Box::new(StructuralOp::SceneSlideInsert {
                    scene: 99,
                    index: 0,
                })),
                CourseDelta::Structural(Box::new(StructuralOp::SceneSlideInsert {
                    scene: 0,
                    index: 2,
                })),
            ]),
        ];

        for mut delta in bad_deltas {
            let delta_before = delta.clone();
            let mut course = orig.clone();
            let res = delta.apply(&mut course);
            assert!(
                matches!(res, Err(EngineError::Invalid(_))),
                "expected Err(Invalid), got {res:?} for {delta_before:?}"
            );
            assert_eq!(
                course.to_json().unwrap(),
                orig_json,
                "course mutated on error for {delta_before:?}"
            );
            assert_eq!(
                delta, delta_before,
                "delta mutated on error for {delta_before:?}"
            );
        }
    }

    fn assert_u07_object_modified_collapse(s0: &Course, s0_json: &str) {
        let mut cur = s0.clone();
        let mut deltas = Vec::new();
        for step in 1..=5 {
            let prev = cur.clone();
            cur.scenes[0].slides[0].objects[0].frame.x = 100.0 + (step as f32) * 15.0;
            cur.scenes[0].slides[0].objects[0].frame.y = 50.0 + (step as f32) * 7.5;
            let d = CourseDelta::diff(prev, &cur).expect("object move must produce delta");
            assert!(
                matches!(
                    d,
                    CourseDelta::SlideChange {
                        change: SlideDelta::ObjectModified { .. },
                        ..
                    }
                ),
                "expected ObjectModified, got {d:?}"
            );
            deltas.push(d);
        }
        let s5_json = cur.to_json().unwrap();
        assert_ne!(s0_json, s5_json);

        let first_bytes = deltas[0].estimated_bytes();
        let mut iter = deltas.into_iter();
        let first = iter.next().unwrap();
        let mut merged = iter.fold(first, CourseDelta::merge);
        assert!(
            matches!(
                merged,
                CourseDelta::SlideChange {
                    change: SlideDelta::ObjectModified { .. },
                    ..
                }
            ),
            "5 merged ObjectModified deltas must collapse to 1 ObjectModified, got {merged:?}"
        );
        assert_eq!(
            merged.estimated_bytes(),
            first_bytes,
            "merged ObjectModified must stay constant size (1 delta, not 5x)"
        );

        merged.apply(&mut cur).expect("undo merged moves");
        assert_eq!(cur.to_json().unwrap(), s0_json);
        merged.apply(&mut cur).expect("redo merged moves");
        assert_eq!(cur.to_json().unwrap(), s5_json);
    }

    fn assert_u07_full_slide_collapse(s0: &Course, s0_json: &str) {
        let mut fs1 = s0.clone();
        fs1.scenes[0].slides[0].objects[0].frame.x = 200.0;
        fs1.scenes[0].slides[0].objects[1].frame.x = 300.0;
        let d_fs1 = CourseDelta::diff(s0.clone(), &fs1).unwrap();
        assert!(matches!(
            d_fs1,
            CourseDelta::SlideChange {
                change: SlideDelta::FullSlide(_),
                ..
            }
        ));
        let fs1_bytes = d_fs1.estimated_bytes();

        let mut fs2 = fs1.clone();
        fs2.scenes[0].slides[0].objects[0].frame.x = 400.0;
        fs2.scenes[0].slides[0].objects[1].frame.x = 500.0;
        let d_fs2 = CourseDelta::diff(fs1, &fs2).unwrap();
        assert!(matches!(
            d_fs2,
            CourseDelta::SlideChange {
                change: SlideDelta::FullSlide(_),
                ..
            }
        ));

        let mut merged = CourseDelta::merge(d_fs1, d_fs2);
        assert!(matches!(
            merged,
            CourseDelta::SlideChange {
                change: SlideDelta::FullSlide(_),
                ..
            }
        ));
        assert_eq!(merged.estimated_bytes(), fs1_bytes);
        let fs2_json = fs2.to_json().unwrap();
        merged.apply(&mut fs2).expect("undo merged FullSlide");
        assert_eq!(fs2.to_json().unwrap(), s0_json);
        merged.apply(&mut fs2).expect("redo merged FullSlide");
        assert_eq!(fs2.to_json().unwrap(), fs2_json);
    }

    fn bump_all_slide_titles(course: &mut Course, suffix: &str) {
        for scene in &mut course.scenes {
            for slide in &mut scene.slides {
                slide.title.push_str(suffix);
            }
        }
        for slide in &mut course.masters {
            slide.title.push_str(suffix);
        }
        for bank in &mut course.banks {
            for slide in &mut bank.slides {
                slide.title.push_str(suffix);
            }
        }
    }

    fn assert_u07_full_course_collapse(s0: &Course, s0_json: &str) {
        let mut fc1 = s0.clone();
        bump_all_slide_titles(&mut fc1, " v1");
        let d_fc1 = CourseDelta::diff(s0.clone(), &fc1).unwrap();
        assert!(matches!(d_fc1, CourseDelta::FullCourse(_)));
        let fc1_bytes = d_fc1.estimated_bytes();

        let mut fc2 = fc1.clone();
        bump_all_slide_titles(&mut fc2, " v2");
        let d_fc2 = CourseDelta::diff(fc1, &fc2).unwrap();
        assert!(matches!(d_fc2, CourseDelta::FullCourse(_)));

        let mut merged = CourseDelta::merge(d_fc1, d_fc2);
        assert!(matches!(merged, CourseDelta::FullCourse(_)));
        assert_eq!(merged.estimated_bytes(), fc1_bytes);
        let fc2_json = fc2.to_json().unwrap();
        merged.apply(&mut fc2).expect("undo merged FullCourse");
        assert_eq!(fc2.to_json().unwrap(), s0_json);
        merged.apply(&mut fc2).expect("redo merged FullCourse");
        assert_eq!(fc2.to_json().unwrap(), fc2_json);
    }

    fn assert_u07_mixed_sequence_merge(s0: &Course, s0_json: &str) {
        let mut m1 = s0.clone();
        m1.scenes[0].slides[0].objects[0].frame.x = 777.0;
        let dm1 = CourseDelta::diff(s0.clone(), &m1).unwrap();
        assert!(matches!(
            dm1,
            CourseDelta::SlideChange {
                change: SlideDelta::ObjectModified { .. },
                ..
            }
        ));

        let mut m2 = m1.clone();
        m2.title = "Merged Header Title".to_owned();
        let dm2 = CourseDelta::diff(m1, &m2).unwrap();
        assert!(matches!(dm2, CourseDelta::Header(_)));

        let mut m3 = m2.clone();
        m3.scenes[0].slides[0]
            .objects
            .push(sample_object("merged_new_obj", 420.0));
        let dm3 = CourseDelta::diff(m2, &m3).unwrap();
        assert!(matches!(
            dm3,
            CourseDelta::SlideChange {
                change: SlideDelta::ObjectInserted { .. },
                ..
            }
        ));
        let m3_json = m3.to_json().unwrap();

        let mut merged = CourseDelta::merge(CourseDelta::merge(dm1, dm2), dm3);
        match &merged {
            CourseDelta::Sequence(steps) => {
                assert_eq!(steps.len(), 3);
                assert!(
                    steps.iter().all(|s| !matches!(s, CourseDelta::Sequence(_))),
                    "merged Sequence must stay flat"
                );
            }
            other => panic!("expected flat Sequence for mixed merge, got {other:?}"),
        }

        merged.apply(&mut m3).expect("undo merged mixed");
        assert_eq!(m3.to_json().unwrap(), s0_json);
        merged.apply(&mut m3).expect("redo merged mixed");
        assert_eq!(m3.to_json().unwrap(), m3_json);
    }

    /// U-07: `CourseDelta::merge` collapses repeated same-object/same-slide/full-course edits
    /// in constant space and composes mixed edits into a flat reversible `Sequence`.
    #[test]
    fn u_07_delta_merge_collapses_same_object_and_sequences_mixed() {
        let s0 = multi_scene_course();
        let s0_json = s0.to_json().unwrap();
        assert_u07_object_modified_collapse(&s0, &s0_json);
        assert_u07_full_slide_collapse(&s0, &s0_json);
        assert_u07_full_course_collapse(&s0, &s0_json);
        assert_u07_mixed_sequence_merge(&s0, &s0_json);
    }

    #[test]
    fn gb_01_slide_delta_groups_roundtrip() {
        let base = sample_slide();
        let sample_group = Group {
            id: "g-btn".to_owned(),
            name: "Continue button".to_owned(),
            interactive: true,
            initial_state: "normal".to_owned(),
            focus_order: Some(2),
            description: Some("Advances to the next slide".to_owned()),
        };

        // 1. Mutating `slide.groups` produces a compact `SlideDelta::Fields` patch and round-trips.
        assert_slide_fields_roundtrip(&base, |s| {
            s.groups.push(sample_group.clone());
        });

        // 2. Updating an existing `slide.groups` entry also produces `SlideDelta::Fields`.
        let mut with_group = base.clone();
        with_group.groups.push(sample_group.clone());
        assert_slide_fields_roundtrip(&with_group, |s| {
            s.groups[0].name = "Next button".to_owned();
            s.groups[0].initial_state = "disabled".to_owned();
        });

        // 3. Mutating `layer.groups` falls back to `SlideDelta::FullSlide` and round-trips cleanly.
        let mut after_layer_group = base.clone();
        after_layer_group.layers[0].groups.push(Group {
            id: "lg-btn".to_owned(),
            name: "Close layer".to_owned(),
            interactive: true,
            initial_state: "normal".to_owned(),
            focus_order: None,
            description: None,
        });
        let layer_delta = assert_slide_roundtrip(base, after_layer_group);
        assert!(matches!(layer_delta, SlideDelta::FullSlide(_)));
        assert!(layer_delta.estimated_bytes() > 0);
    }

    #[test]
    fn tr_header_delta_tracking_roundtrip() {
        let before = multi_scene_course();
        let before_json = before.to_json().unwrap();
        let mut after = before.clone();
        after.tracking = coursecraft_course::TrackingSettings {
            slides_viewed: Some(coursecraft_course::SlidesViewedTracking::AtLeast { count: 2 }),
            quiz: Some(coursecraft_course::QuizTracking {
                slide: Some("s1".into()),
            }),
        };
        let after_json = after.to_json().unwrap();

        let snap = CourseHeaderSnapshot::capture(&before);
        let mut patch = CourseHeaderPatch::diff(snap, &after).expect("tracking diff");
        assert!(patch.tracking.is_some());

        let mut course = after;
        patch.apply(&mut course);
        assert_eq!(course.to_json().unwrap(), before_json);
        patch.apply(&mut course);
        assert_eq!(course.to_json().unwrap(), after_json);
    }
}
