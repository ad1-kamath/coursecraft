//! Routes text commands of the Format and Text inspectors into a state while Edit-State Mode
//! is on (`N84`).
//!
//! The inspector controls build commands for the base object (`text.set`, `text.style_range`,
//! `text.set_paragraph`, `text.set_paragraphs`, `text.set_link`, `object.set_autofit`). The
//! engine runs those on the base object only. This module turns each one into an
//! `object.patch_state` patch on the edited state, computed from what the state shows now.

use coursecraft_course::{
    Autofit, Link, Paragraph, TextRun, TextRunPatch, TextStyle, apply_link_to_range,
    apply_style_to_range_with_base, text::paragraph_count,
};
use serde_json::{Value as Json, json};

/// What the edited state shows now: its own text values, or the base object's.
pub(crate) struct StateTextView<'a> {
    pub style: &'a TextStyle,
    pub runs: &'a [TextRun],
    pub paragraphs: &'a [Paragraph],
    pub text: &'a str,
}

impl StateTextView<'_> {
    /// The runs that fit the text. Base runs do not fit a state that has its own text.
    fn fitting_runs(&self) -> &[TextRun] {
        let joined: String = self.runs.iter().map(|r| r.text.as_str()).collect();
        if self.runs.is_empty() || joined == self.text {
            self.runs
        } else {
            &[]
        }
    }
}

/// A `patch` object for `object.patch_state` for `action`, or `None` when `action` is not a text
/// command that this module knows.
pub(crate) fn patch_for(action: &(&'static str, Json), view: &StateTextView<'_>) -> Option<Json> {
    let params = &action.1;
    match action.0 {
        "text.style_range" => Some(style_range_patch(params, view)),
        "text.set_paragraph" => Some(paragraph_patch(params, view)),
        "text.set_paragraphs" => Some(paragraphs_patch(params, view)),
        "text.set_link" => Some(link_patch(params, view)),
        "object.set_autofit" => autofit_patch(params),
        _ => None,
    }
}

fn get_usize(params: &Json, key: &str) -> Option<usize> {
    params
        .get(key)
        .and_then(Json::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

fn get_bool(params: &Json, key: &str) -> Option<bool> {
    params.get(key).and_then(Json::as_bool)
}

fn get_f32(params: &Json, key: &str) -> Option<f32> {
    params
        .get(key)
        .and_then(Json::as_f64)
        .filter(|v| v.is_finite())
        .map(|v| v as f32)
}

fn get_string(params: &Json, key: &str) -> Option<String> {
    params.get(key).and_then(Json::as_str).map(str::to_owned)
}

fn get_color(value: &Json) -> Option<coursecraft_course::Color> {
    value
        .as_str()
        .map(|s| coursecraft_course::Color(s.to_owned()))
}

fn run_patch(params: &Json) -> TextRunPatch {
    let highlight = match params.get("highlight") {
        None => None,
        Some(Json::Null) => Some(None),
        Some(value) => get_color(value).map(Some),
    };
    TextRunPatch {
        bold: get_bool(params, "bold"),
        italic: get_bool(params, "italic"),
        underline: get_bool(params, "underline"),
        strikethrough: get_bool(params, "strikethrough"),
        baseline: params
            .get("baseline")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        letter_spacing: get_f32(params, "letter_spacing"),
        size: get_f32(params, "size"),
        color: params.get("color").and_then(get_color),
        font: get_string(params, "font"),
        link: None,
        highlight,
        shadow: get_bool(params, "shadow"),
    }
}

/// The engine's range rule: `0..0` is the whole text, otherwise clamp and order the ends.
fn clamped_range(start: usize, end: usize, total: usize) -> (usize, usize) {
    if start == 0 && end == 0 {
        (0, 0)
    } else {
        let (s, e) = (start.min(total), end.min(total));
        (s.min(e), s.max(e))
    }
}

fn style_range_patch(params: &Json, view: &StateTextView<'_>) -> Json {
    let total = view.text.chars().count();
    let range = clamped_range(
        get_usize(params, "start").unwrap_or(0),
        get_usize(params, "end").unwrap_or(0),
        total,
    );
    let runs = apply_style_to_range_with_base(
        view.fitting_runs(),
        view.text,
        range,
        &run_patch(params),
        Some(view.style),
    );
    json!({ "runs": runs })
}

fn merged_style(view: &StateTextView<'_>, fields: &[(&str, Json)]) -> Json {
    let mut merged = serde_json::to_value(view.style).unwrap_or(Json::Null);
    if let Some(dst) = merged.as_object_mut() {
        for (key, value) in fields {
            dst.insert((*key).to_owned(), value.clone());
        }
    }
    merged
}

fn paragraph_patch(params: &Json, view: &StateTextView<'_>) -> Json {
    let fields: Vec<(&str, Json)> = ["align", "valign", "wrap", "direction", "insets"]
        .into_iter()
        .filter_map(|key| params.get(key).map(|value| (key, value.clone())))
        .collect();
    json!({ "text_style": merged_style(view, &fields) })
}

fn paragraphs_patch(params: &Json, view: &StateTextView<'_>) -> Json {
    let count = paragraph_count(view.text);
    let first = get_usize(params, "start").unwrap_or(0).min(count);
    let last = get_usize(params, "end")
        .unwrap_or(count)
        .clamp(first, count);
    let mut paragraphs = coursecraft_course::text::paragraphs_for(view.text, view.paragraphs);
    let edit: Json = params.clone();
    for paragraph in paragraphs.iter_mut().skip(first).take(last - first) {
        apply_paragraph_fields(paragraph, &edit);
    }
    while paragraphs.last() == Some(&Paragraph::default()) {
        paragraphs.pop();
    }
    json!({ "paragraphs": paragraphs })
}

/// Applies the fields that `text.set_paragraphs` carries to one paragraph.
fn apply_paragraph_fields(paragraph: &mut Paragraph, params: &Json) {
    // Round-trip through JSON so the field names and the enums stay the model's own.
    let Ok(Json::Object(mut current)) = serde_json::to_value(&*paragraph) else {
        return;
    };
    let renames = [
        ("list", "list"),
        ("level", "level"),
        ("line_spacing", "line_spacing"),
        ("space_before", "space_before"),
        ("space_after", "space_after"),
        ("number_scheme", "number_scheme"),
        ("start_at", "start_at"),
    ];
    for (param, field) in renames {
        match params.get(param) {
            None => {}
            Some(Json::Null) => {
                current.remove(field);
            }
            Some(value) => {
                current.insert(field.to_owned(), value.clone());
            }
        }
    }
    if let Ok(updated) = serde_json::from_value::<Paragraph>(Json::Object(current)) {
        *paragraph = updated;
    }
}

fn link_patch(params: &Json, view: &StateTextView<'_>) -> Json {
    let link: Option<Link> = params
        .get("link")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let total = view.text.chars().count();
    let start = get_usize(params, "start").unwrap_or(0).min(total);
    let end = get_usize(params, "end").unwrap_or(total).min(total);
    let range = if start == 0 && end == total {
        (0, 0)
    } else {
        (start.min(end), start.max(end))
    };
    let runs = apply_link_to_range(view.fitting_runs(), view.text, range, link);
    json!({ "runs": runs })
}

fn autofit_patch(params: &Json) -> Option<Json> {
    let autofit: Autofit = serde_json::from_value(params.get("autofit")?.clone()).ok()?;
    Some(json!({ "autofit": autofit }))
}
