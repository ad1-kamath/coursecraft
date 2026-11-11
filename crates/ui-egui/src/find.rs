//! Find and Replace: a small non-modal window that searches every
//! author-visible string in the course.
//!
//! Search and replace go through the engine commands `course.find` and
//! `course.replace`, so one Replace is one undo step. The results are cached
//! on the query, the options and the course revision, so they refresh after
//! typing, undo, redo and every other course change.

use crate::dialog::Dialog;
use std::collections::BTreeSet;

use coursecraft_course::{Hit, MAX_HITS};
use coursecraft_render::{SIZE_BODY, SIZE_SMALL, palette};
use egui::text::{LayoutJob, TextFormat};
use egui::{Button, FontId, Id, Key, Modifiers, RichText, ScrollArea, TextEdit};
use serde_json::{Value as Json, json};

use crate::Editor;
use crate::properties::InspectorTab;
use crate::status::navigate_to_audit_finding;
use crate::timeline::BottomTab;

/// Longest search or replacement the engine accepts (the engine's text limit).
const MAX_TEXT_BYTES: usize = 4096;

/// Find and Replace window state, held in `UiState`.
#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub open: bool,
    /// Counts openings. Each opening gets a new window id, so the window
    /// centres again.
    openings: u64,
    window_id: Option<Id>,
    /// Put the caret in the Find field on the next frame.
    pub focus_query: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Index of the selected row in the result list.
    pub selected: usize,
    /// Result of the last Replace or Replace All.
    pub message: Option<String>,
    /// Course revision when the message was set. Cleared when the revision changes.
    pub message_revision: Option<u64>,
    results: Option<Results>,
}

/// The hits for one query, options and course revision.
#[derive(Debug, Clone)]
struct Results {
    key: ResultsKey,
    hits: Vec<Hit>,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResultsKey {
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    revision: u64,
}

/// What the window asks for this frame. Applied after the window is drawn,
/// so the window never edits the course while it is laid out.
enum Action {
    None,
    Select(usize),
    ReplaceOne(usize),
    ReplaceAll,
}

/// Open the window, or bring it forward and focus the Find field if it is open.
pub fn open(editor: &mut Editor) {
    let state = &mut editor.ui.find;
    if !state.open {
        state.open = true;
        state.openings = state.openings.wrapping_add(1);
        state.window_id = Some(Id::unique(("find_and_replace", state.openings)));
        state.selected = 0;
    }
    state.focus_query = true;
}

/// Close the window when Escape is pressed. Runs before the canvas sees the key,
/// so Escape closes this window and does not also clear the selection.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if editor.ui.find.open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        editor.ui.find.open = false;
    }
}

/// Draw the Find and Replace window when it is open.
pub fn show_find_window(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.find.open {
        return;
    }
    let screen = ctx.content_rect();
    let max_height = (screen.height() - 40.0).max(240.0);
    let window_id = editor
        .ui
        .find
        .window_id
        .unwrap_or_else(|| Id::unique(("find_and_replace", 0_u64)));
    let mut open = true;
    let mut action = Action::None;
    Dialog::new("Find and Replace")
        .id(window_id)
        // Pinned by its top edge, so the window doesn't jump as its height changes.
        .pinned_top()
        .width(440.0)
        .open(&mut open)
        .show(ctx, |ui| {
            action = window_body(editor, ui, max_height);
        });
    if !open {
        editor.ui.find.open = false;
    }
    match action {
        Action::None => {}
        Action::Select(index) => {
            editor.ui.find.selected = index;
            if let Some(hit) = hit_at(editor, index) {
                go_to(editor, &hit);
            }
        }
        Action::ReplaceOne(index) => replace_one(editor, index),
        Action::ReplaceAll => {
            let params = replace_params(editor, None);
            run_replace(editor, params);
        }
    }
}

fn window_body(editor: &mut Editor, ui: &mut egui::Ui, max_height: f32) -> Action {
    let p = palette(ui.ctx());
    let mut action = Action::None;

    ui.horizontal(|ui| {
        ui.label("Find");
        let response = ui.add(
            TextEdit::singleline(&mut editor.ui.find.query)
                .hint_text("Text to find")
                .desired_width(f32::INFINITY),
        );
        if editor.ui.find.focus_query {
            response.request_focus();
            editor.ui.find.focus_query = false;
        }
        if response.changed() {
            editor.ui.find.message = None;
            editor.ui.find.selected = 0;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Replace");
        ui.add(
            TextEdit::singleline(&mut editor.ui.find.replacement)
                .hint_text("Replacement text")
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut editor.ui.find.case_sensitive, "Match case")
            .changed()
        {
            editor.ui.find.message = None;
            editor.ui.find.selected = 0;
        }
        if ui
            .checkbox(&mut editor.ui.find.whole_word, "Whole word")
            .changed()
        {
            editor.ui.find.message = None;
            editor.ui.find.selected = 0;
        }
    });

    refresh(editor);

    let query_len = editor.ui.find.query.len();
    let searchable = !editor.ui.find.query.is_empty() && query_len <= MAX_TEXT_BYTES;
    let (hits, truncated) = match &editor.ui.find.results {
        Some(results) => (results.hits.as_slice(), results.truncated),
        None => (&[][..], false),
    };
    let selected = editor.ui.find.selected;

    ui.add_space(4.0);
    let summary = if editor.ui.find.query.is_empty() {
        "Type to search every string in the course.".to_owned()
    } else if query_len > MAX_TEXT_BYTES {
        "That search is too long.".to_owned()
    } else if hits.is_empty() {
        "No matches".to_owned()
    } else {
        let slides = hits
            .iter()
            .filter_map(|hit| hit.slide.as_deref())
            .collect::<BTreeSet<_>>()
            .len();
        format!(
            "{} on {}",
            plural(hits.len(), "match", "matches"),
            plural(slides, "slide", "slides")
        )
    };
    ui.label(RichText::new(summary).color(p.text_secondary));
    if truncated {
        ui.label(
            RichText::new(format!("Showing the first {MAX_HITS} matches."))
                .size(SIZE_SMALL)
                .color(p.text_tertiary),
        );
    }

    ui.add_space(4.0);
    // The list takes under half the window so the buttons always fit.
    let list_height = (max_height * 0.45).clamp(120.0, 420.0);
    ScrollArea::vertical()
        .id_salt("find_results")
        .auto_shrink([false, false])
        .max_height(list_height)
        .show(ui, |ui| {
            for (index, hit) in hits.iter().enumerate() {
                ui.label(
                    RichText::new(&hit.label)
                        .size(SIZE_SMALL)
                        .color(p.text_secondary),
                );
                if ui
                    .selectable_label(index == selected, snippet_job(ui, hit))
                    .clicked()
                {
                    action = Action::Select(index);
                }
                ui.add_space(2.0);
            }
        });

    ui.separator();
    let can_replace = searchable && !hits.is_empty();
    ui.horizontal(|ui| {
        let replace_one = ui
            .add_enabled(can_replace && selected < hits.len(), Button::new("Replace"))
            .on_hover_text("Replaces every match in this text");
        if replace_one.clicked() {
            action = Action::ReplaceOne(selected);
        }
        if ui
            .add_enabled(can_replace, Button::new("Replace All"))
            .clicked()
        {
            action = Action::ReplaceAll;
        }
    });
    if let Some(message) = &editor.ui.find.message {
        ui.label(RichText::new(message.as_str()).color(p.text_secondary));
    }
    action
}

/// Re-run the search when the query, the options or the course changed.
fn refresh(editor: &mut Editor) {
    let key = ResultsKey {
        query: editor.ui.find.query.clone(),
        case_sensitive: editor.ui.find.case_sensitive,
        whole_word: editor.ui.find.whole_word,
        revision: editor.session.revision,
    };
    if editor.ui.find.message_revision != Some(editor.session.revision) {
        editor.ui.find.message = None;
        editor.ui.find.message_revision = None;
    }
    if editor
        .ui
        .find
        .results
        .as_ref()
        .is_some_and(|results| results.key == key)
    {
        return;
    }
    let (hits, truncated) = if key.query.is_empty() || key.query.len() > MAX_TEXT_BYTES {
        (Vec::new(), false)
    } else {
        let params = json!({
            "query": key.query,
            "case": key.case_sensitive,
            "whole_word": key.whole_word,
        });
        editor
            .run("course.find", params)
            .map(|reply| read_hits(&reply))
            .unwrap_or_default()
    };
    let state = &mut editor.ui.find;
    state.selected = state.selected.min(hits.len().saturating_sub(1));
    state.results = Some(Results {
        key,
        hits,
        truncated,
    });
}

fn read_hits(reply: &Json) -> (Vec<Hit>, bool) {
    let hits = reply
        .get("hits")
        .and_then(|hits| serde_json::from_value::<Vec<Hit>>(hits.clone()).ok())
        .unwrap_or_default();
    let truncated = reply
        .get("truncated")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    (hits, truncated)
}

fn hit_at(editor: &Editor, index: usize) -> Option<Hit> {
    editor
        .ui
        .find
        .results
        .as_ref()
        .and_then(|results| results.hits.get(index))
        .cloned()
}

fn replace_one(editor: &mut Editor, index: usize) {
    let Some(hit) = hit_at(editor, index) else {
        return;
    };
    let only = json!([{
        "slide": hit.slide,
        "object": hit.object,
        "layer": hit.layer,
        "field": hit.field,
    }]);
    let params = replace_params(editor, Some(only));
    run_replace(editor, params);
}

fn replace_params(editor: &Editor, only: Option<Json>) -> Json {
    let state = &editor.ui.find;
    let mut params = json!({
        "query": state.query,
        "replacement": state.replacement,
        "case": state.case_sensitive,
        "whole_word": state.whole_word,
    });
    if let Some(only) = only {
        params["only"] = only;
    }
    params
}

fn run_replace(editor: &mut Editor, params: Json) {
    let Some(reply) = editor.run("course.replace", params) else {
        return;
    };
    let replaced = reply.get("replaced").and_then(Json::as_u64).unwrap_or(0);
    let skipped = reply
        .get("skipped_variable_refs")
        .and_then(Json::as_u64)
        .unwrap_or(0);
    editor.ui.find.message = Some(if skipped > 0 {
        format!("Replaced {replaced}. Skipped {skipped} inside variable references.")
    } else {
        format!("Replaced {replaced}.")
    });
    editor.ui.find.message_revision = Some(editor.session.revision);
}

/// Select the slide, layer, object or question tab a hit lives in.
fn go_to(editor: &mut Editor, hit: &Hit) {
    let on_master = hit
        .slide
        .as_deref()
        .is_some_and(|id| editor.session.course.master(id).is_some());
    if on_master {
        editor.enter_master_view();
    } else {
        editor.leave_master_view();
    }

    if let Some(slide) = hit.slide.as_deref() {
        navigate_to_audit_finding(
            &editor.session.course,
            &mut editor.selection,
            Some(slide),
            hit.object.as_deref(),
        );
    } else if let Some(scene_id) = scene_title_id(&hit.field) {
        let first = editor
            .session
            .course
            .scenes
            .iter()
            .find(|scene| scene.id == scene_id)
            .and_then(|scene| scene.slides.first())
            .map(|slide| slide.id.clone());
        if let Some(first) = first {
            navigate_to_audit_finding(
                &editor.session.course,
                &mut editor.selection,
                Some(&first),
                None,
            );
        }
    } else {
        editor.ui.inspector_tab = InspectorTab::Course;
    }

    if hit.field.starts_with("question/") {
        editor.ui.bottom_tab = BottomTab::Question;
    }
}

fn scene_title_id(field: &str) -> Option<&str> {
    field.strip_prefix("scene/")?.strip_suffix("/title")
}

/// The snippet with the match in the accent colour.
fn snippet_job(ui: &egui::Ui, hit: &Hit) -> LayoutJob {
    let p = palette(ui.ctx());
    let font = FontId::proportional(SIZE_BODY);
    let snippet = hit.snippet.as_str();
    let range = hit.snippet_match.clone();
    let before = snippet.get(..range.start).unwrap_or("");
    let matched = snippet.get(range.clone()).unwrap_or("");
    let after = snippet.get(range.end..).unwrap_or("");
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    job.append(before, 0.0, TextFormat::simple(font.clone(), p.text));
    job.append(matched, 0.0, TextFormat::simple(font.clone(), p.accent));
    job.append(after, 0.0, TextFormat::simple(font, p.text));
    job
}

fn plural(count: usize, one: &str, many: &str) -> String {
    let noun = if count == 1 { one } else { many };
    format!("{count} {noun}")
}
