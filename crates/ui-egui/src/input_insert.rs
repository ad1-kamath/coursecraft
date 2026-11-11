//! Insert ▸ Input placement.
//!
//! Every input is created with a new variable of the right kind, in one
//! `variable.add` + `object.add` batch, so it works straight away and one undo
//! removes both.

use coursecraft_course::{ObjectKind, Orientation, RadioOption, Rect, Value, ValueKind};
use serde_json::{Value as Json, json};

use crate::Editor;

fn unique_variable(editor: &Editor, base: &str) -> String {
    if editor.session.course.variable(base).is_none() {
        return base.to_owned();
    }
    (2u64..)
        .map(|n| format!("{base}{n}"))
        .find(|name| editor.session.course.variable(name).is_none())
        .unwrap_or_else(|| format!("{base}2"))
}

fn centred_frame(editor: &Editor, width: f32, height: f32) -> Rect {
    let size = editor.session.course.slide_size;
    let width = width.clamp(20.0, size.w as f32);
    let height = height.clamp(20.0, size.h as f32);
    Rect {
        x: ((size.w as f32 - width) * 0.5).max(0.0),
        y: ((size.h as f32 - height) * 0.5).max(0.0),
        w: width,
        h: height,
    }
}

fn insert(
    editor: &mut Editor,
    base_variable: &str,
    value: Value,
    name: &str,
    frame: Rect,
    kind: ObjectKind,
) -> Option<String> {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return None;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status("Select a slide first");
        return None;
    }
    let variable = unique_variable(editor, base_variable);
    let kind_json = serde_json::to_value(kind).ok()?;
    let steps = vec![
        (
            "variable.add".to_owned(),
            json!({ "name": variable, "value": value }),
        ),
        (
            "object.add".to_owned(),
            json!({
                "slide": editor.selection.slide,
                "kind": kind_json,
                "frame": frame,
                "name": name,
            }),
        ),
    ];
    let results = editor.run_batch(&steps)?;
    let id = results
        .get(1)
        .and_then(|value| value.get("id"))
        .and_then(Json::as_str)?
        .to_owned();
    editor.selection.set_many([id.clone()]);
    Some(id)
}

/// Insert a text-entry field with a new text variable.
pub fn insert_text_entry(editor: &mut Editor) -> Option<String> {
    let variable = unique_variable(editor, "text_entry");
    insert(
        editor,
        "text_entry",
        Value::Text(String::new()),
        "Text entry",
        centred_frame(editor, 360.0, 56.0),
        ObjectKind::TextEntry {
            variable,
            placeholder: "Type here".to_owned(),
            multiline: false,
            max_chars: 0,
        },
    )
}

/// Insert a horizontal slider with a new number variable.
pub fn insert_slider(editor: &mut Editor) -> Option<String> {
    let variable = unique_variable(editor, "slider");
    insert(
        editor,
        "slider",
        Value::Number(0.0),
        "Slider",
        centred_frame(editor, 360.0, 48.0),
        ObjectKind::Slider {
            variable,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            orientation: Orientation::Horizontal,
        },
    )
}

/// Insert a dial with a new number variable.
pub fn insert_dial(editor: &mut Editor) -> Option<String> {
    let variable = unique_variable(editor, "dial");
    insert(
        editor,
        "dial",
        Value::Number(0.0),
        "Dial",
        centred_frame(editor, 200.0, 200.0),
        ObjectKind::Dial {
            variable,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            start_angle: -135.0,
            end_angle: 135.0,
        },
    )
}

/// Insert a checkbox with a new true/false variable.
pub fn insert_checkbox(editor: &mut Editor) -> Option<String> {
    let variable = unique_variable(editor, "checkbox");
    insert(
        editor,
        "checkbox",
        Value::Bool(false),
        "Checkbox",
        centred_frame(editor, 300.0, 40.0),
        ObjectKind::Checkbox {
            variable,
            label: "Checkbox".to_owned(),
        },
    )
}

/// Insert a two-option radio group with a new text variable.
pub fn insert_radio_group(editor: &mut Editor) -> Option<String> {
    let variable = unique_variable(editor, "radio");
    insert(
        editor,
        "radio",
        Value::Text("option1".to_owned()),
        "Radio buttons",
        centred_frame(editor, 320.0, 80.0),
        ObjectKind::RadioGroup {
            variable,
            options: vec![
                RadioOption {
                    label: "Option 1".to_owned(),
                    value: "option1".to_owned(),
                },
                RadioOption {
                    label: "Option 2".to_owned(),
                    value: "option2".to_owned(),
                },
            ],
        },
    )
}

/// Kind label used by the inspector variable filter when it needs one.
#[must_use]
pub fn input_value_kind(kind: &ObjectKind) -> Option<ValueKind> {
    kind.input_variable().map(|(_, value_kind)| value_kind)
}
