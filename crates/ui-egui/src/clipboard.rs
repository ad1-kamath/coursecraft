//! Editor clipboard: objects and slides as JSON text, plus plain-text paste.
//!
//! The OS clipboard is reached through egui's copy/cut/paste events so it works
//! in the browser and on the desktop. A private copy is kept in [`crate::UiState`]
//! for the menu, which cannot read the system clipboard on the web.

use std::collections::BTreeSet;

use crate::Editor;
use crate::tl;
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

/// The look of the course an object payload was copied from, so a paste into a
/// course with another theme can follow the destination theme.
///
/// Optional in the payload: payloads written by older builds have none and
/// paste as they are.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceTheme {
    /// The body font.
    pub font: String,
    /// The heading font, when the theme has a separate one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_font: Option<String>,
    pub accent: String,
    pub text: String,
    pub background: String,
}

impl SourceTheme {
    /// The theme of `course`. The heading font is read through the theme's JSON
    /// so this keeps working whether or not the model has one.
    #[must_use]
    pub fn of(course: &coursecraft_course::Course) -> Self {
        let heading_font = serde_json::to_value(&course.theme)
            .ok()
            .and_then(|value| {
                value
                    .get("heading_font")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|font| !font.is_empty());
        Self {
            font: course.theme.font.clone(),
            heading_font,
            accent: course.theme.accent.0.clone(),
            text: course.theme.text.0.clone(),
            background: course.theme.background.0.clone(),
        }
    }
}

/// How an object paste treats the formatting of what was copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasteMode {
    /// Fonts and colours of the source theme become those of the destination
    /// theme. The default for Ctrl+V and the main Paste button.
    #[default]
    DestinationTheme,
    /// Paste exactly as copied.
    KeepSource,
    /// Paste the text of the copied objects as one plain text box.
    TextOnly,
}

#[derive(Debug, Clone)]
pub struct Clipboard {
    pub kind: ClipboardKind,
    pub source_slide: Option<String>,
    pub source_layer: Option<String>,
    pub objects: Vec<Object>,
    pub triggers: Vec<Trigger>,
    /// The copied slides, in slide-list order (empty for an object payload).
    pub slides: Vec<Slide>,
    /// The theme of the course the objects came from. `None` for payloads written
    /// by older builds.
    pub theme: Option<SourceTheme>,
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
        if let Some(ref theme) = self.theme
            && let Ok(value) = serde_json::to_value(theme)
        {
            map.insert("theme".to_owned(), value);
        }
        // `slide` is the first slide, as older builds wrote it; `slides` holds all.
        if let Some(slide) = self.slides.first()
            && let Ok(value) = serde_json::to_value(slide)
        {
            map.insert("slide".to_owned(), value);
        }
        if self.slides.len() > 1
            && let Ok(value) = serde_json::to_value(&self.slides)
        {
            map.insert("slides".to_owned(), value);
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
        let slides: Vec<Slide> = match (map.get("slides"), map.get("slide")) {
            (Some(value), _) => serde_json::from_value(value.clone()).ok()?,
            (None, Some(value)) => vec![serde_json::from_value(value.clone()).ok()?],
            (None, None) => Vec::new(),
        };
        // An unreadable theme only means "no theme": the objects still paste.
        let theme: Option<SourceTheme> = map
            .get("theme")
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        if kind == ClipboardKind::Objects && objects.is_empty() {
            return None;
        }
        if kind == ClipboardKind::Slides && slides.is_empty() {
            return None;
        }
        Some(Self {
            kind,
            source_slide,
            source_layer,
            objects,
            triggers,
            slides,
            theme,
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

fn event_refs_only_copied(event: &Event, copied: &BTreeSet<String>) -> bool {
    match event {
        Event::DroppedOn { object, target } => copied.contains(object) && copied.contains(target),
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            copied.contains(a) && copied.contains(b)
        }
        other => event_object_ref(other).is_some_and(|id| copied.contains(id)),
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
    event_refs_only_copied(&trigger.when, copied)
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
            slides: Vec::new(),
            theme: Some(SourceTheme::of(&editor.session.course)),
        },
        skipped,
    ))
}

/// Copy every selected slide, in slide-list order.
#[must_use]
pub fn copy_slide(editor: &Editor) -> Option<Clipboard> {
    let slides: Vec<Slide> = editor
        .selected_slides_in_order()
        .iter()
        .filter_map(|id| editor.session.course.slide(id))
        .cloned()
        .collect();
    let first = slides.first()?;
    Some(Clipboard {
        kind: ClipboardKind::Slides,
        source_slide: Some(first.id.clone()),
        source_layer: None,
        objects: Vec::new(),
        triggers: Vec::new(),
        slides,
        theme: None,
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
        editor.set_status("Not available in slide masters");
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
    let message = if clipboard.slides.len() > 1 {
        format!("Copied {} slides", clipboard.slides.len())
    } else {
        "Copied slide".to_owned()
    };
    write(editor, clipboard, message);
}

/// Cut the current selection. Cut is copy then delete; the delete is one undo.
pub fn cut(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
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
    // A course keeps at least one slide: cutting all of them would leave none.
    if clipboard.slides.len() >= editor.session.course.slides().count() {
        editor.set_status("A course needs at least one slide");
        return;
    }
    let message = if clipboard.slides.len() > 1 {
        format!("Cut {} slides", clipboard.slides.len())
    } else {
        "Cut slide".to_owned()
    };
    write(editor, clipboard, message);
    // Deletes every selected slide as one undo step.
    editor.delete_selected_slides();
}

/// Paste from the private clipboard. Used by the Edit menu.
pub fn paste_private(editor: &mut Editor) {
    paste_private_with(editor, PasteMode::default());
}

/// Paste from the private clipboard with an explicit [`PasteMode`].
pub fn paste_private_with(editor: &mut Editor, mode: PasteMode) {
    if editor.ui.master_view {
        editor.set_status(tl!("Not available in slide masters"));
        return;
    }
    let Some(clipboard) = editor.ui.clipboard.clone() else {
        editor.set_status(tl!("Nothing to paste"));
        return;
    };
    paste_at(editor, clipboard, None, mode);
}

/// Paste with an explicit [`PasteMode`]: through the system clipboard on the
/// desktop (the mode waits for the paste event), else the editor's own.
pub fn paste_now_with(editor: &mut Editor, ctx: &egui::Context, mode: PasteMode) {
    if editor.file_ops_available {
        request_system_paste(editor, ctx, Some(mode));
    } else {
        paste_private_with(editor, mode);
    }
}

/// Ask the system clipboard for its text. `mode` applies to the answer of this
/// request only: `None` (the main Paste button) clears any older mode, and
/// [`expire_pending_paste_mode`] drops a mode that no paste event took.
pub fn request_system_paste(editor: &mut Editor, ctx: &egui::Context, mode: Option<PasteMode>) {
    editor.ui.pending_paste_mode = mode;
    editor.ui.pending_paste_frame = ctx.cumulative_frame_nr();
    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
}

/// Frames a Paste menu mode waits for the system clipboard to answer.
const PASTE_MODE_FRAMES: u64 = 2;

/// Drop a pending paste mode when the frame that should have handled its
/// request is over. Call after the frame's paste events were handled, so a
/// text field that took the event, or an empty clipboard, cannot leave the mode
/// for a later Ctrl+V.
pub fn expire_pending_paste_mode(editor: &mut Editor, frame: u64) {
    if editor.ui.pending_paste_mode.is_some()
        && frame
            >= editor
                .ui
                .pending_paste_frame
                .saturating_add(PASTE_MODE_FRAMES)
    {
        editor.ui.pending_paste_mode = None;
    }
}

/// Paste from the private clipboard, centring object copies on a slide point.
///
/// `centre` is in slide coordinates. Non-finite points and payloads that cannot
/// be placed fall back to the normal keyboard/Edit menu offset.
pub fn paste_private_at(editor: &mut Editor, centre: Option<egui::Pos2>) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let Some(clipboard) = editor.ui.clipboard.clone() else {
        editor.set_status("Nothing to paste");
        return;
    };
    paste_at(editor, clipboard, centre, PasteMode::default());
}

/// Paste clipboard text, falling back to a text box for plain text.
///
/// A mode chosen in the Paste menu waits in `pending_paste_mode` until the
/// system clipboard answers; Ctrl+V has none and uses the destination theme.
pub fn paste_text(editor: &mut Editor, text: &str) {
    let mode = editor.ui.pending_paste_mode.take().unwrap_or_default();
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    if text.len() > MAX_CLIPBOARD_BYTES {
        editor.set_status("Clipboard is too large to paste");
        return;
    }
    if let Some(clipboard) = Clipboard::from_text(text) {
        paste_with(editor, clipboard, mode);
    } else if let Some(clipboard) = editor.ui.clipboard.clone()
        && editor.ui.clipboard_text.as_deref() == Some(text)
    {
        paste_with(editor, clipboard, mode);
    } else if text.contains(CLIPBOARD_KEY) {
        editor.set_status("That clipboard payload is not valid CourseCraft data");
    } else {
        paste_plain_text(editor, text);
    }
}

/// Paste a parsed payload.
pub fn paste(editor: &mut Editor, clipboard: Clipboard) {
    paste_at(editor, clipboard, None, PasteMode::default());
}

/// Paste a parsed payload with an explicit [`PasteMode`].
pub fn paste_with(editor: &mut Editor, clipboard: Clipboard, mode: PasteMode) {
    paste_at(editor, clipboard, None, mode);
}

fn paste_at(
    editor: &mut Editor,
    clipboard: Clipboard,
    centre: Option<egui::Pos2>,
    mode: PasteMode,
) {
    match clipboard.kind {
        ClipboardKind::Slides if mode == PasteMode::TextOnly => {
            editor.set_status(tl!("Slides can't be pasted as text"));
        }
        ClipboardKind::Slides => paste_slide(editor, &clipboard),
        ClipboardKind::Objects => paste_objects(editor, &clipboard, centre, mode),
    }
}

/// JSON keys whose strings are author text, never a colour or a font name.
const TEXT_KEYS: &[&str] = &[
    "text",
    "name",
    "description",
    "title",
    "body",
    "label",
    "id",
    "alt",
    "value",
    "caption",
];

/// Swap a colour for the destination's when it is the source theme's accent,
/// text or background colour (case does not matter; an alpha suffix is kept).
fn map_color(value: &str, source: &SourceTheme, dest: &SourceTheme) -> Option<String> {
    if !value.starts_with('#') || !(value.len() == 7 || value.len() == 9) {
        return None;
    }
    let rgb = value.get(..7)?;
    let alpha = value.get(7..).unwrap_or_default();
    for (from, to) in [
        (&source.accent, &dest.accent),
        (&source.text, &dest.text),
        (&source.background, &dest.background),
    ] {
        if from.len() >= 7
            && to.len() >= 7
            && from.get(..7)?.eq_ignore_ascii_case(rgb)
            && !from.get(..7)?.eq_ignore_ascii_case(to.get(..7)?)
        {
            return Some(format!("{}{}", to.get(..7)?, alpha));
        }
    }
    None
}

fn adapt_value(value: &mut serde_json::Value, source: &SourceTheme, dest: &SourceTheme) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if let serde_json::Value::String(text) = child {
                    if key == "font" {
                        if *text == source.font && source.font != dest.font {
                            text.clone_from(&dest.font);
                        } else if source.heading_font.as_deref() == Some(text.as_str()) {
                            // A theme without a heading font uses its body font for headings.
                            let target = dest.heading_font.as_deref().unwrap_or(&dest.font);
                            if text != target {
                                *text = target.to_owned();
                            }
                        }
                    } else if !TEXT_KEYS.contains(&key.as_str())
                        && let Some(mapped) = map_color(text, source, dest)
                    {
                        *text = mapped;
                    }
                } else {
                    adapt_value(child, source, dest);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                adapt_value(item, source, dest);
            }
        }
        _ => {}
    }
}

/// Make copied objects follow the destination theme.
///
/// Text fonts equal to the source theme's font become the destination's font, and
/// colours equal to the source theme's accent, text or background colour become the
/// destination's. Nothing changes when the themes are equal, when the payload has
/// no source theme, or for anything that is not one of those exact values.
#[must_use]
pub fn adapt_to_theme(
    objects: &[Object],
    source: Option<&SourceTheme>,
    dest: &SourceTheme,
) -> Vec<Object> {
    let Some(source) = source else {
        return objects.to_vec();
    };
    if source == dest {
        return objects.to_vec();
    }
    objects
        .iter()
        .map(|object| {
            let Ok(mut value) = serde_json::to_value(object) else {
                return object.clone();
            };
            adapt_value(&mut value, source, dest);
            serde_json::from_value(value).unwrap_or_else(|_| object.clone())
        })
        .collect()
}

/// The text of the copied text objects, in order, one paragraph per object.
#[must_use]
pub fn text_of_objects(objects: &[Object]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for object in objects {
        if let coursecraft_course::ObjectKind::Text { text, runs, .. } = &object.kind {
            let plain: String = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|run| run.text.as_str()).collect()
            };
            if !plain.is_empty() {
                parts.push(plain);
            }
        }
    }
    parts.join("\n")
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
    let target_x = if half_w * 2.0 <= slide_w {
        coursecraft_course::safe_clamp(centre.x, half_w, slide_w - half_w)
    } else {
        slide_w * 0.5
    };
    let target_y = if half_h * 2.0 <= slide_h {
        coursecraft_course::safe_clamp(centre.y, half_h, slide_h - half_h)
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

fn paste_objects(
    editor: &mut Editor,
    clipboard: &Clipboard,
    centre: Option<egui::Pos2>,
    mode: PasteMode,
) {
    let target = editor.selection.slide.clone();
    if editor.session.course.slide(&target).is_none() {
        editor.set_status(tl!("Select a slide to paste into"));
        return;
    }
    if mode == PasteMode::TextOnly {
        let text = text_of_objects(&clipboard.objects);
        if text.is_empty() {
            editor.set_status(tl!("The copied objects have no text to paste"));
        } else {
            insert_text_box(editor, &text, true);
        }
        return;
    }
    let same_slide = clipboard.source_slide.as_deref() == Some(target.as_str());
    let next_paste = editor.ui.clipboard_paste_count.saturating_add(1);
    let mut offset = if same_slide {
        20.0 * next_paste as f32
    } else {
        0.0
    };
    let mut objects = if mode == PasteMode::DestinationTheme {
        adapt_to_theme(
            &clipboard.objects,
            clipboard.theme.as_ref(),
            &SourceTheme::of(&editor.session.course),
        )
    } else {
        clipboard.objects.clone()
    };
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
    if same_slide && centre.is_none() {
        editor.ui.clipboard_paste_count = next_paste;
    }
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
    if clipboard.slides.len() > 1 {
        paste_slides(editor, clipboard);
        return;
    }
    let Some(slide) = clipboard.slides.first().cloned() else {
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

/// Paste several slides after the current one, in their copied order, as one undo step.
///
/// Each is inserted right after the current slide, last first, so they end up in order.
fn paste_slides(editor: &mut Editor, clipboard: &Clipboard) {
    let after = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .map(|slide| slide.id.clone());
    let mut steps: Vec<(String, serde_json::Value)> = Vec::new();
    for slide in clipboard.slides.iter().rev() {
        let Ok(slide_value) = serde_json::to_value(slide) else {
            editor.set_status("That clipboard payload can't be pasted");
            return;
        };
        let mut params = serde_json::json!({ "slide": slide_value });
        if let Some(ref after) = after {
            params["after"] = serde_json::json!(after);
        }
        steps.push(("slide.paste".to_owned(), params));
    }
    let Some(results) = editor.run_batch(&steps) else {
        return;
    };
    // Results come back last slide first.
    let ids: Vec<String> = results
        .iter()
        .rev()
        .filter_map(|result| result.get("id").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect();
    let skipped: u64 = results
        .iter()
        .filter_map(|result| result.get("skipped").and_then(serde_json::Value::as_u64))
        .sum();
    if let Some((first, rest)) = ids.split_first() {
        editor.selection.slide = first.clone();
        editor.selection.also_slides = rest.to_vec();
        editor.selection.layer = None;
        editor.selection.clear_objects();
    }
    if skipped == 0 {
        editor.set_status(format!("Pasted {} slides", clipboard.slides.len()));
    } else {
        editor.set_status(format!(
            "Pasted {} slides; skipped {skipped} trigger(s)",
            clipboard.slides.len()
        ));
    }
}

fn paste_plain_text(editor: &mut Editor, text: &str) {
    insert_text_box(editor, text, false);
}

/// Put `text` in a new text box in the middle of the slide, selected. With
/// `themed` the box uses the theme body font and grows with the number of
/// paragraphs; plain OS text keeps the one fixed size.
fn insert_text_box(editor: &mut Editor, text: &str, themed: bool) {
    const MAX_TEXT_BYTES: usize = 4096;
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let text = text.get(..end).unwrap_or_default();
    let size = editor.session.course.slide_size;
    let w = 300.0f32.min(size.w as f32);
    let lines = text.lines().count().max(1) as f32;
    let h = if themed {
        (36.0 * lines).max(60.0).min(size.h as f32)
    } else {
        60.0f32.min(size.h as f32)
    };
    let x = ((size.w as f32 - w) * 0.5).max(0.0);
    let y = ((size.h as f32 - h) * 0.5).max(0.0);
    let mut style = serde_json::json!({ "color": editor.session.course.theme.text.0 });
    if themed {
        style["font"] = serde_json::json!(editor.session.course.theme.font);
    }
    let mut params = serde_json::json!({
        "slide": editor.selection.slide,
        "kind": {
            "type": "text",
            "text": text,
            "style": style,
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
        editor.set_status(tl!("Pasted as a text box"));
    }
}

/// A format the Paste Special window can paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasteFormat {
    /// CourseCraft objects, with the three [`PasteMode`] options. Without
    /// objects on the clipboard it pastes the system clipboard's text, which
    /// only [`PasteMode::TextOnly`] applies to.
    #[default]
    Objects,
    /// CourseCraft slides.
    Slides,
}

/// State of the Paste Special window.
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteSpecialState {
    pub open: bool,
    pub format: PasteFormat,
    pub mode: PasteMode,
}

/// Which formats can be pasted now. Each entry is `Err(reason)` when disabled.
pub fn paste_formats(editor: &Editor) -> [(PasteFormat, Result<(), &'static str>); 2] {
    const NOTHING: &str = "Nothing of this kind on the clipboard";
    let kind = editor.ui.clipboard.as_ref().map(|c| c.kind);
    // The system clipboard may hold text even when no objects were copied.
    let objects = if kind == Some(ClipboardKind::Objects) || editor.file_ops_available {
        Ok(())
    } else {
        Err(NOTHING)
    };
    let slides = if kind == Some(ClipboardKind::Slides) {
        Ok(())
    } else {
        Err(NOTHING)
    };
    [
        (PasteFormat::Objects, objects),
        (PasteFormat::Slides, slides),
    ]
}

impl Editor {
    /// Open the Paste Special window with the first format that can be pasted.
    pub fn open_paste_special(&mut self) {
        if self.ui.master_view {
            self.set_status(tl!("Not available in slide masters"));
            return;
        }
        let format = paste_formats(self)
            .into_iter()
            .find(|(_, available)| available.is_ok())
            .map_or(PasteFormat::Objects, |(format, _)| format);
        self.ui.paste_special = PasteSpecialState {
            open: true,
            format,
            mode: if self.ui.clipboard.as_ref().map(|c| c.kind) == Some(ClipboardKind::Objects) {
                PasteMode::DestinationTheme
            } else {
                PasteMode::TextOnly
            },
        };
    }
}

fn has_objects(editor: &Editor) -> bool {
    editor.ui.clipboard.as_ref().map(|c| c.kind) == Some(ClipboardKind::Objects)
}

/// Paste what the Paste Special window has chosen.
pub fn paste_special_ok(editor: &mut Editor, ctx: &egui::Context) {
    let state = editor.ui.paste_special;
    match state.format {
        PasteFormat::Objects => {
            let mode = if has_objects(editor) {
                state.mode
            } else {
                PasteMode::TextOnly
            };
            paste_now_with(editor, ctx, mode);
        }
        PasteFormat::Slides => paste_now_with(editor, ctx, PasteMode::default()),
    }
}

/// Draw the Paste Special window.
pub fn show_paste_special_window(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.paste_special.open {
        return;
    }
    let formats = paste_formats(editor);
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    crate::dialog::Dialog::new(tl!("Paste Special"))
        .open(&mut open)
        .width(340.0)
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                ui.label(tl!("As:"));
                let objects_label = tl!("CourseCraft objects");
                let slides_label = tl!("CourseCraft slides");
                let text_label = tl!("Clipboard text");
                let objects_on_clipboard = has_objects(editor);
                for (format, available) in formats {
                    let label = match format {
                        PasteFormat::Objects if objects_on_clipboard => objects_label,
                        PasteFormat::Objects => text_label,
                        PasteFormat::Slides => slides_label,
                    };
                    let response = ui.add_enabled(
                        available.is_ok(),
                        egui::RadioButton::new(editor.ui.paste_special.format == format, label),
                    );
                    if response.clicked() {
                        editor.ui.paste_special.format = format;
                    }
                    if let Err(reason) = available {
                        response.on_disabled_hover_text(tl!(reason));
                    }
                }
                if editor.ui.paste_special.format == PasteFormat::Objects {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(tl!("Formatting:"));
                    let mode = &mut editor.ui.paste_special.mode;
                    // Text has no source formatting or theme to follow.
                    let styled = objects_on_clipboard;
                    let reason = tl!("The clipboard holds text only, so it has no formatting");
                    for (value, label) in [
                        (PasteMode::DestinationTheme, tl!("Use Destination Theme")),
                        (PasteMode::KeepSource, tl!("Keep Source Formatting")),
                    ] {
                        let selected = styled && *mode == value;
                        let response =
                            ui.add_enabled(styled, egui::RadioButton::new(selected, label));
                        if response.clicked() {
                            *mode = value;
                        }
                        response.on_disabled_hover_text(reason);
                    }
                    ui.radio_value(mode, PasteMode::TextOnly, tl!("Keep Text Only"));
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    let can_paste = formats.iter().any(|(format, available)| {
                        *format == editor.ui.paste_special.format && available.is_ok()
                    });
                    if ui
                        .add_enabled(can_paste, egui::Button::new(tl!("OK")))
                        .on_disabled_hover_text(tl!("The clipboard has nothing of this kind"))
                        .clicked()
                    {
                        ok = true;
                    }
                    if ui.button(tl!("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if ok {
        paste_special_ok(editor, ctx);
        open = false;
    }
    if cancel {
        open = false;
    }
    if !open {
        editor.ui.paste_special.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_delta_centers_copy_wider_than_slide() {
        let obj: Object = serde_json::from_value(serde_json::json!({
            "id": "o1",
            "frame": { "x": 0.0, "y": 0.0, "w": 1400.0, "h": 200.0 },
            "kind": {
                "type": "shape",
                "shape": "rectangle",
                "fill": "#000000"
            }
        }))
        .expect("object");
        let delta = placement_delta(
            &[obj],
            egui::pos2(640.0, 360.0),
            coursecraft_course::Size { w: 1280, h: 720 },
        );
        assert_eq!(delta, Some((-60.0, 260.0)));
    }
}
