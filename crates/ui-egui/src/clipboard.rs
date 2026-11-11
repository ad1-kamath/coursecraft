//! Editor clipboard: objects and slides as JSON text, plus plain-text paste.
//!
//! The OS clipboard is reached through egui's copy/cut/paste events so it works
//! in the browser and on the desktop. A private copy is kept in [`crate::UiState`]
//! for the menu, which cannot read the system clipboard on the web.

use std::collections::BTreeSet;

use crate::Editor;
use coursecraft_course::{Action, Condition, Event, Object, Slide, Trigger};

/// Marker key and version for our clipboard JSON.
pub const CLIPBOARD_KEY: &str = "coursecraft_clipboard";
pub const CLIPBOARD_VERSION: u64 = 1;
/// Hostile clipboard text is capped at 16 MiB.
pub const MAX_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardKind {
    Objects,
    Slides,
}

#[derive(Debug, Clone)]
pub struct Clipboard {
    pub kind: ClipboardKind,
    pub source_slide: Option<String>,
    pub source_layer: Option<String>,
    pub objects: Vec<Object>,
    pub triggers: Vec<Trigger>,
    pub slide: Option<Slide>,
}

impl Clipboard {
    #[must_use]
    pub fn to_text(&self) -> String {
        let kind = match self.kind {
            ClipboardKind::Objects => "objects",
            ClipboardKind::Slides => "slides",
        };
        let mut map = serde_json::Map::new();
        map.insert(
            CLIPBOARD_KEY.to_owned(),
            serde_json::json!(CLIPBOARD_VERSION),
        );
        map.insert("kind".to_owned(), serde_json::json!(kind));
        if let Some(ref slide) = self.source_slide {
            map.insert("source_slide".to_owned(), serde_json::json!(slide));
        }
        if let Some(ref layer) = self.source_layer {
            map.insert("source_layer".to_owned(), serde_json::json!(layer));
        }
        if !self.objects.is_empty()
            && let Ok(value) = serde_json::to_value(&self.objects)
        {
            map.insert("objects".to_owned(), value);
        }
        if !self.triggers.is_empty()
            && let Ok(value) = serde_json::to_value(&self.triggers)
        {
            map.insert("triggers".to_owned(), value);
        }
        if let Some(ref slide) = self.slide
            && let Ok(value) = serde_json::to_value(slide)
        {
            map.insert("slide".to_owned(), value);
        }
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
    }

    /// Parse clipboard text. Returns `None` for plain text or a bad payload.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        if text.len() > MAX_CLIPBOARD_BYTES {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let map = value.as_object()?;
        if map.get(CLIPBOARD_KEY)?.as_u64()? != CLIPBOARD_VERSION {
            return None;
        }
        let kind = match map.get("kind")?.as_str()? {
            "objects" => ClipboardKind::Objects,
            "slides" => ClipboardKind::Slides,
            _ => return None,
        };
        let source_slide = map
            .get("source_slide")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let source_layer = map
            .get("source_layer")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let objects: Vec<Object> = match map.get("objects") {
            Some(value) => serde_json::from_value(value.clone()).ok()?,
            None => Vec::new(),
        };
        let triggers: Vec<Trigger> = match map.get("triggers") {
            Some(value) => serde_json::from_value(value.clone()).ok()?,
            None => Vec::new(),
        };
        let slide: Option<Slide> = match map.get("slide") {
            Some(value) => Some(serde_json::from_value(value.clone()).ok()?),
            None => None,
        };
        if kind == ClipboardKind::Objects && objects.is_empty() {
            return None;
        }
        if kind == ClipboardKind::Slides && slide.is_none() {
            return None;
        }
        Some(Self {
            kind,
            source_slide,
            source_layer,
            objects,
            triggers,
            slide,
        })
    }
}

fn event_object_ref(event: &Event) -> Option<&str> {
    match event {
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MediaEnds { object }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object }
        | Event::TimerEnds { timer: object } => Some(object),
        Event::StateChanges { object, .. } | Event::AnimationCompletes { object, .. } => {
            Some(object)
        }
        Event::CueReached { object, .. } => Some(object),
        Event::DroppedOn { object, .. } => Some(object),
        Event::ObjectsIntersect { a, .. } | Event::ObjectsStopIntersecting { a, .. } => Some(a),
        Event::KeyPressed { object, .. } => object.as_deref(),
        Event::TimelineStarts { .. }
        | Event::TimelineEnds { .. }
        | Event::TimelineReaches { .. }
        | Event::VariableChanges { .. }
        | Event::Submit => None,
    }
}

fn action_refs_only_copied(action: &Action, copied: &BTreeSet<String>) -> bool {
    match action {
        Action::ChangeState { object, .. }
        | Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object }
        | Action::StartTimer { timer: object }
        | Action::PauseTimer { timer: object }
        | Action::ResetTimer { timer: object }
        | Action::MoveOnPath { object, .. } => copied.contains(object),
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
        | Action::CloseLightbox => true,
    }
}

fn condition_refs_only_copied(condition: &Condition, copied: &BTreeSet<String>) -> bool {
    match condition {
        Condition::All(list) | Condition::Any(list) => list
            .iter()
            .all(|inner| condition_refs_only_copied(inner, copied)),
        Condition::Not(inner) => condition_refs_only_copied(inner, copied),
        Condition::Compare { .. } => true,
        Condition::ObjectState { object, .. } => copied.contains(object),
    }
}

fn trigger_refs_only_copied(trigger: &Trigger, copied: &BTreeSet<String>) -> bool {
    event_object_ref(&trigger.when).is_some_and(|id| copied.contains(id))
        && action_refs_only_copied(&trigger.action, copied)
        && trigger
            .condition
            .as_ref()
            .is_none_or(|condition| condition_refs_only_copied(condition, copied))
}

fn all_triggers(slide: &Slide) -> Vec<&Trigger> {
    slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
        .collect()
}

/// Copy the selected objects on the focused layer, plus the triggers that only
/// refer to them. Returns the payload and the number of triggers left behind.
#[must_use]
pub fn copy_objects(editor: &Editor) -> Option<(Clipboard, usize)> {
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    let ids: Vec<String> = editor
        .selection
        .ids()
        .into_iter()
        .filter(|id| match editor.selection.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .is_some_and(|layer| layer.objects.iter().any(|object| &object.id == id)),
            None => slide.objects.iter().any(|object| &object.id == id),
        })
        .collect();
    if ids.is_empty() {
        return None;
    }
    let copied: BTreeSet<String> = ids.iter().cloned().collect();
    let mut objects = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some((object, _)) = slide.object(id) {
            objects.push(object.clone());
        }
    }
    if objects.is_empty() {
        return None;
    }
    let mut triggers = Vec::new();
    let mut skipped = 0usize;
    for trigger in all_triggers(slide) {
        if event_object_ref(&trigger.when).is_some_and(|id| copied.contains(id)) {
            if trigger_refs_only_copied(trigger, &copied) {
                triggers.push(trigger.clone());
            } else {
                skipped += 1;
            }
        }
    }
    Some((
        Clipboard {
            kind: ClipboardKind::Objects,
            source_slide: Some(slide.id.clone()),
            source_layer: editor.selection.layer.clone(),
            objects,
            triggers,
            slide: None,
        },
        skipped,
    ))
}

/// Copy the selected slide.
#[must_use]
pub fn copy_slide(editor: &Editor) -> Option<Clipboard> {
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    Some(Clipboard {
        kind: ClipboardKind::Slides,
        source_slide: Some(slide.id.clone()),
        source_layer: None,
        objects: Vec::new(),
        triggers: Vec::new(),
        slide: Some(slide.clone()),
    })
}

/// Put a payload in the OS clipboard and keep a private copy for the menu.
pub fn write(editor: &mut Editor, clipboard: Clipboard, message: String) {
    let text = clipboard.to_text();
    editor.ui.pending_copy_text = Some(text.clone());
    editor.ui.clipboard = Some(clipboard);
    editor.ui.clipboard_text = Some(text);
    editor.ui.clipboard_paste_count = 0;
    editor.set_status(message);
}

/// Copy the current object selection (or the slide when the navigator is active).
pub fn copy(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return;
    }
    if editor.ui.last_active_area == crate::ActiveArea::Navigator {
        copy_slide_action(editor);
    } else {
        copy_objects_action(editor);
    }
}

fn copy_objects_action(editor: &mut Editor) {
    let Some((clipboard, skipped)) = copy_objects(editor) else {
        editor.set_status("Select an object to copy");
        return;
    };
    let count = clipboard.objects.len();
    let message = if skipped == 0 {
        format!("Copied {count} object(s)")
    } else {
        format!("Copied {count} object(s); skipped {skipped} trigger(s)")
    };
    write(editor, clipboard, message);
}

fn copy_slide_action(editor: &mut Editor) {
    let Some(clipboard) = copy_slide(editor) else {
        editor.set_status("Select a slide to copy");
        return;
    };
    write(editor, clipboard, "Copied slide".to_owned());
}

/// Cut the current selection. Cut is copy then delete; the delete is one undo.
pub fn cut(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return;
    }
    if editor.ui.last_active_area == crate::ActiveArea::Navigator {
        cut_slide_action(editor);
    } else {
        cut_objects_action(editor);
    }
}

fn cut_objects_action(editor: &mut Editor) {
    let Some((clipboard, skipped)) = copy_objects(editor) else {
        editor.set_status("Select an object to cut");
        return;
    };
    let count = clipboard.objects.len();
    let message = if skipped == 0 {
        format!("Cut {count} object(s)")
    } else {
        format!("Cut {count} object(s); skipped {skipped} trigger(s)")
    };
    write(editor, clipboard, message);
    editor.delete_selection();
}

fn cut_slide_action(editor: &mut Editor) {
    if editor.session.course.slides().count() <= 1 {
        editor.set_status("Can't cut the only slide");
        return;
    }
    let Some(clipboard) = copy_slide(editor) else {
        editor.set_status("Select a slide to cut");
        return;
    };
    write(editor, clipboard, "Cut slide".to_owned());
    let slide_id = editor.selection.slide.clone();
    editor.run("slide.delete", serde_json::json!({ "slide": slide_id }));
}

/// Paste from the private clipboard. Used by the Edit menu.
pub fn paste_private(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return;
    }
    let Some(clipboard) = editor.ui.clipboard.clone() else {
        editor.set_status("Nothing to paste");
        return;
    };
    paste_at(editor, clipboard, None);
}

/// Paste from the private clipboard, centring object copies on a slide point.
///
/// `centre` is in slide coordinates. Non-finite points and payloads that cannot
/// be placed fall back to the normal keyboard/Edit menu offset.
pub fn paste_private_at(editor: &mut Editor, centre: Option<egui::Pos2>) {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return;
    }
    let Some(clipboard) = editor.ui.clipboard.clone() else {
        editor.set_status("Nothing to paste");
        return;
    };
    paste_at(editor, clipboard, centre);
}

/// Paste clipboard text, falling back to a text box for plain text.
pub fn paste_text(editor: &mut Editor, text: &str) {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return;
    }
    if text.len() > MAX_CLIPBOARD_BYTES {
        editor.set_status("Clipboard is too large to paste");
        return;
    }
    if let Some(clipboard) = Clipboard::from_text(text) {
        paste(editor, clipboard);
    } else if let Some(clipboard) = editor.ui.clipboard.clone()
        && editor.ui.clipboard_text.as_deref() == Some(text)
    {
        paste(editor, clipboard);
    } else if text.contains(CLIPBOARD_KEY) {
        editor.set_status("That clipboard payload is not valid CourseCraft data");
    } else {
        paste_plain_text(editor, text);
    }
}

/// Paste a parsed payload.
pub fn paste(editor: &mut Editor, clipboard: Clipboard) {
    paste_at(editor, clipboard, None);
}

fn paste_at(editor: &mut Editor, clipboard: Clipboard, centre: Option<egui::Pos2>) {
    match clipboard.kind {
        ClipboardKind::Slides => paste_slide(editor, &clipboard),
        ClipboardKind::Objects => paste_objects(editor, &clipboard, centre),
    }
}

/// Translation that moves the copies' joint centre to `centre`, clamped so the
/// whole joint box stays on the slide. `None` when the payload has no finite box.
fn placement_delta(
    objects: &[Object],
    centre: egui::Pos2,
    slide_size: coursecraft_course::Size,
) -> Option<(f32, f32)> {
    if !centre.x.is_finite() || !centre.y.is_finite() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for object in objects {
        let frame = object.frame;
        if ![frame.x, frame.y, frame.w, frame.h]
            .iter()
            .all(|value| value.is_finite())
            || frame.w <= 0.0
            || frame.h <= 0.0
        {
            return None;
        }
        min_x = min_x.min(frame.x);
        min_y = min_y.min(frame.y);
        max_x = max_x.max(frame.x + frame.w);
        max_y = max_y.max(frame.y + frame.h);
    }
    if ![min_x, min_y, max_x, max_y]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    let slide_w = slide_size.w as f32;
    let slide_h = slide_size.h as f32;
    if !slide_w.is_finite() || !slide_h.is_finite() || slide_w <= 0.0 || slide_h <= 0.0 {
        return None;
    }
    let half_w = (max_x - min_x) * 0.5;
    let half_h = (max_y - min_y) * 0.5;
    if !half_w.is_finite() || !half_h.is_finite() || half_w < 0.0 || half_h < 0.0 {
        return None;
    }
    let target_x = if half_w <= slide_w {
        centre.x.clamp(half_w, slide_w - half_w)
    } else {
        slide_w * 0.5
    };
    let target_y = if half_h <= slide_h {
        centre.y.clamp(half_h, slide_h - half_h)
    } else {
        slide_h * 0.5
    };
    let dx = target_x - (min_x + max_x) * 0.5;
    let dy = target_y - (min_y + max_y) * 0.5;
    if dx.is_finite() && dy.is_finite() {
        Some((dx, dy))
    } else {
        None
    }
}

fn paste_objects(editor: &mut Editor, clipboard: &Clipboard, centre: Option<egui::Pos2>) {
    let target = editor.selection.slide.clone();
    if editor.session.course.slide(&target).is_none() {
        editor.set_status("Select a slide to paste into");
        return;
    }
    let next_paste = editor.ui.clipboard_paste_count.saturating_add(1);
    let mut offset = if clipboard.source_slide.as_deref() == Some(target.as_str()) {
        20.0 * next_paste as f32
    } else {
        0.0
    };
    let mut objects = clipboard.objects.clone();
    if let Some(point) = centre
        && let Some((dx, dy)) = placement_delta(&objects, point, editor.session.course.slide_size)
    {
        for object in &mut objects {
            object.frame.x += dx;
            object.frame.y += dy;
        }
        offset = 0.0;
    }
    let Ok(objects) = serde_json::to_value(&objects) else {
        editor.set_status("That clipboard payload can't be pasted");
        return;
    };
    let mut params = serde_json::json!({
        "slide": target,
        "objects": objects,
        "offset": offset,
    });
    if let Some(ref source) = clipboard.source_slide {
        params["source_slide"] = serde_json::json!(source);
    }
    if let Some(ref layer) = editor.selection.layer {
        params["layer"] = serde_json::json!(layer);
    }
    if !clipboard.triggers.is_empty()
        && let Ok(triggers) = serde_json::to_value(&clipboard.triggers)
    {
        params["triggers"] = triggers;
    }
    let Some(result) = editor.run("object.paste", params) else {
        return;
    };
    editor.ui.clipboard_paste_count = next_paste;
    let ids: Vec<String> = result
        .get("ids")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if !ids.is_empty() {
        editor.selection.set_many(ids);
    }
    let skipped = result
        .get("skipped")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if skipped == 0 {
        editor.set_status("Pasted");
    } else {
        editor.set_status(format!("Pasted; skipped {skipped} trigger(s)"));
    }
}

fn paste_slide(editor: &mut Editor, clipboard: &Clipboard) {
    let Some(slide) = clipboard.slide.clone() else {
        editor.set_status("That clipboard payload has no slide");
        return;
    };
    let after = if editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some()
    {
        Some(editor.selection.slide.clone())
    } else {
        None
    };
    let Ok(slide_value) = serde_json::to_value(&slide) else {
        editor.set_status("That clipboard payload can't be pasted");
        return;
    };
    let mut params = serde_json::json!({ "slide": slide_value });
    if let Some(after) = after {
        params["after"] = serde_json::json!(after);
    }
    let Some(result) = editor.run("slide.paste", params) else {
        return;
    };
    if let Some(id) = result.get("id").and_then(serde_json::Value::as_str) {
        editor.selection.slide = id.to_owned();
        editor.selection.layer = None;
        editor.selection.clear_objects();
    }
    let skipped = result
        .get("skipped")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if skipped == 0 {
        editor.set_status("Pasted slide");
    } else {
        editor.set_status(format!("Pasted slide; skipped {skipped} trigger(s)"));
    }
}

fn paste_plain_text(editor: &mut Editor, text: &str) {
    const MAX_TEXT_BYTES: usize = 4096;
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let text = text.get(..end).unwrap_or_default();
    let size = editor.session.course.slide_size;
    let w = 300.0f32.min(size.w as f32);
    let h = 60.0f32.min(size.h as f32);
    let x = ((size.w as f32 - w) * 0.5).max(0.0);
    let y = ((size.h as f32 - h) * 0.5).max(0.0);
    let mut params = serde_json::json!({
        "slide": editor.selection.slide,
        "kind": {
            "type": "text",
            "text": text,
            "style": { "color": editor.session.course.theme.text.0 },
        },
        "frame": { "x": x, "y": y, "w": w, "h": h },
    });
    if let Some(ref layer) = editor.selection.layer {
        params["layer"] = serde_json::json!(layer);
    }
    if let Some(result) = editor.run("object.add", params)
        && let Some(id) = result.get("id").and_then(serde_json::Value::as_str)
    {
        editor.selection.object = Some(id.to_owned());
        editor.selection.also.clear();
        editor.set_status("Pasted as a text box");
    }
}
