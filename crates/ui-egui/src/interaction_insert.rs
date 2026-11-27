//! Insert ▸ Interaction placement for markers and scrolling panels.

use coursecraft_course::{MarkerIcon, ObjectKind, Rect, ScrollBar};
use serde_json::{Value as Json, json};

use crate::Editor;

fn centred_frame(editor: &Editor, width: f32, height: f32) -> Rect {
    let size = editor.session.course.slide_size;
    let max_w = (size.w as f32).max(1.0);
    let max_h = (size.h as f32).max(1.0);
    let width = coursecraft_course::safe_clamp(width, 20.0_f32.min(max_w), max_w);
    let height = coursecraft_course::safe_clamp(height, 20.0_f32.min(max_h), max_h);
    Rect {
        x: ((size.w as f32 - width) * 0.5).max(0.0),
        y: ((size.h as f32 - height) * 0.5).max(0.0),
        w: width,
        h: height,
    }
}

fn insert(editor: &mut Editor, name: &str, frame: Rect, kind: ObjectKind) -> Option<String> {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return None;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status("Select a slide first");
        return None;
    }
    let kind_json = serde_json::to_value(kind).ok()?;
    let result = editor.run(
        "object.add",
        json!({
            "slide": editor.selection.slide,
            "layer": editor.selection.layer,
            "kind": kind_json,
            "frame": frame,
            "name": name,
        }),
    )?;
    let id = result.get("id").and_then(Json::as_str)?.to_owned();
    editor.selection.set_many([id.clone()]);
    Some(id)
}

/// Insert a marker dot. Selecting it on the canvas shows its label card.
pub fn insert_marker(editor: &mut Editor) -> Option<String> {
    insert(
        editor,
        "Marker",
        centred_frame(editor, 36.0, 36.0),
        ObjectKind::Marker {
            title: "Marker".to_owned(),
            body: String::new(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            icon: MarkerIcon::Info,
            color: None,
        },
    )
}

/// Insert a countdown timer showing its full duration on the canvas.
pub fn insert_timer(editor: &mut Editor) -> Option<String> {
    insert(
        editor,
        "Timer",
        centred_frame(editor, 200.0, 64.0),
        ObjectKind::Timer {
            duration_ms: coursecraft_course::DEFAULT_TIMER_DURATION_MS,
            format: coursecraft_course::TimerFormat::MmSs,
            autostart: true,
            style: coursecraft_course::TextStyle::default(),
        },
    )
}

/// Insert a progress meter with a new number variable, like the inputs.
pub fn insert_progress_meter(editor: &mut Editor) -> Option<String> {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return None;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status("Select a slide first");
        return None;
    }
    let variable = {
        let base = "progress";
        if editor.session.course.variable(base).is_none() {
            base.to_owned()
        } else {
            (2u64..)
                .map(|n| format!("{base}{n}"))
                .find(|name| editor.session.course.variable(name).is_none())
                .unwrap_or_else(|| format!("{base}2"))
        }
    };
    let kind_json = serde_json::to_value(ObjectKind::ProgressMeter {
        variable: variable.clone(),
        min: 0.0,
        max: 100.0,
        style: coursecraft_course::MeterStyle::Bar,
        show_value: false,
    })
    .ok()?;
    let steps = vec![
        (
            "variable.add".to_owned(),
            serde_json::json!({ "name": variable, "value": 0.0 }),
        ),
        (
            "object.add".to_owned(),
            serde_json::json!({
                "slide": editor.selection.slide,
                "layer": editor.selection.layer,
                "kind": kind_json,
                "frame": centred_frame(editor, 360.0, 48.0),
                "name": "Progress meter",
            }),
        ),
    ];
    let results = editor.run_batch(&steps)?;
    let id = results
        .get(1)
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)?
        .to_owned();
    editor.selection.set_many([id.clone()]);
    Some(id)
}

/// Insert a scrolling panel whose content starts at its frame height.
pub fn insert_scroll_panel(editor: &mut Editor) -> Option<String> {
    let frame = centred_frame(editor, 480.0, 300.0);
    insert(
        editor,
        "Scrolling panel",
        frame,
        ObjectKind::ScrollPanel {
            content_height: frame.h,
            bar: ScrollBar::Visible,
        },
    )
}

/// Insert a three-tab interaction built from ordinary buttons, layers and
/// triggers. The base buttons are selected afterwards.
pub fn insert_tabs(editor: &mut Editor) -> Option<Vec<String>> {
    insert_interaction(editor, "tabs")
}

/// Insert a three-section accordion built from buttons, layers and triggers.
pub fn insert_accordion(editor: &mut Editor) -> Option<Vec<String>> {
    insert_interaction(editor, "accordion")
}

pub(crate) fn insert_interaction(editor: &mut Editor, kind: &str) -> Option<Vec<String>> {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return None;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status("Select a slide first");
        return None;
    }
    let result = editor.run(
        "interaction.insert",
        json!({
            "slide": editor.selection.slide,
            "kind": kind,
        }),
    )?;
    let ids: Vec<String> = result
        .get("objects")
        .and_then(Json::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return None;
    }
    editor.selection.layer = None;
    editor.selection.set_many(ids.clone());
    Some(ids)
}
