//! Translate a course with AI, filling the XLIFF the importer already owns.
//!
//! The course is never changed from here. A run asks the endpoint for batches
//! of strings, the author reviews and edits them, and `Apply` builds an XLIFF
//! that goes through `course.import_xliff`, so the checks, the report, the
//! language change and the single undo step are the importer's.

use crate::dialog::Dialog;
use std::collections::BTreeMap;

use coursecraft_ai::{
    AiError, ChatReply, TranslationOutcome, UnitIn, batch_units, parse_translations,
    translate_request, translation_problem,
};
use coursecraft_course::{Course, is_language_tag_valid};
use coursecraft_format::xliff::{fill_xliff, translation_units};
use coursecraft_render::{CONTROL_COMPACT, SIZE_BODY, SIZE_SMALL, palette};
use egui::{Button, ComboBox, Id, Key, Label, Modifiers, RichText, ScrollArea, TextEdit, vec2};

use crate::{AiPurpose, Editor, EditorRequest};

/// Common target languages offered by the dropdown.
pub const COMMON_LANGUAGES: &[(&str, &str)] = &[
    ("French", "fr"),
    ("Spanish", "es"),
    ("German", "de"),
    ("Portuguese (Brazil)", "pt-BR"),
    ("Italian", "it"),
    ("Dutch", "nl"),
];

/// Fixed height of one review row, so a few thousand rows stay cheap.
pub const ROW_HEIGHT: f32 = 86.0;

/// One review row: a source string, its editable target and any problem.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranslateRow {
    /// XLIFF unit id.
    pub id: String,
    /// Human-readable place in the course.
    pub location: String,
    /// Source text with formatting markers.
    pub source: String,
    /// The author's or model's target text.
    pub target: String,
    /// Why the model did not supply a usable target, if it did not.
    pub failure: Option<String>,
    /// The current problem with `target`, recomputed as it is edited.
    pub problem: Option<String>,
}

/// One batch in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTranslate {
    /// Zero-based batch position in the run.
    pub batch: usize,
    /// Units sent, in order.
    pub units: Vec<UnitIn>,
    /// Whether this batch has already used its one retry.
    pub retried: bool,
}

/// Transient state for Translate with AI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslateState {
    /// Whether the window is open.
    pub open: bool,
    /// Target BCP 47 tag.
    pub target_language: String,
    /// Units known when the window opened, for the consent count.
    pub units: Vec<UnitIn>,
    /// Review rows, filled when a run starts.
    pub rows: Vec<TranslateRow>,
    /// Batches not started yet.
    pub queue: Vec<Vec<UnitIn>>,
    /// The batch in flight.
    pub pending: Option<PendingTranslate>,
    /// Number of batches in the run.
    pub total_batches: usize,
    /// One-based number the next batch will use.
    pub next_batch: usize,
    /// Whether a run is active.
    pub running: bool,
    /// Course revision when the run started.
    pub revision: u64,
    /// True when the course changed after the run started.
    pub changed: bool,
    /// Show only rows that need attention.
    pub only_attention: bool,
    /// A line shown above the table (apply or save problems).
    pub notice: Option<String>,
}

impl Default for TranslateState {
    fn default() -> Self {
        Self {
            open: false,
            target_language: "fr".to_string(),
            units: Vec::new(),
            rows: Vec::new(),
            queue: Vec::new(),
            pending: None,
            total_batches: 0,
            next_batch: 0,
            running: false,
            revision: 0,
            changed: false,
            only_attention: false,
            notice: None,
        }
    }
}

impl TranslateState {
    /// True while a run is active.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Editor {
    /// Open Translate with AI and collect how many strings the course has.
    pub fn open_translate_dialog(&mut self) {
        if self.ui.translate.running {
            self.ui.translate.open = true;
            return;
        }
        let target = if is_language_tag_valid(self.ui.translate.target_language.trim()) {
            self.ui.translate.target_language.trim().to_string()
        } else {
            "fr".to_string()
        };
        let units = collect_units(&self.session.course, &target);
        self.ui.translate = TranslateState {
            open: true,
            target_language: target,
            units,
            revision: self.session.revision,
            ..TranslateState::default()
        };
    }

    /// Start a translation run for every string in the course.
    pub fn start_translation(&mut self) -> bool {
        if self.ui.translate.running {
            return false;
        }
        let target = self.ui.translate.target_language.trim().to_string();
        if !is_language_tag_valid(&target) {
            self.ui.translate.notice = Some("Use a tag such as fr, en-GB or pt-BR.".to_string());
            return false;
        }
        let units = collect_units(&self.session.course, &target);
        if units.is_empty() {
            self.ui.translate.rows.clear();
            self.ui.translate.units = units;
            self.ui.translate.notice = Some("This course has no text to translate.".to_string());
            return false;
        }
        let rows: Vec<TranslateRow> = units
            .iter()
            .map(|unit| TranslateRow {
                id: unit.id.clone(),
                location: unit_location(&self.session.course, &unit.id),
                source: unit.text.clone(),
                target: String::new(),
                failure: None,
                problem: Some("No translation yet.".to_string()),
            })
            .collect();
        let queue = batch_units(&units);
        let total = queue.len();
        let state = &mut self.ui.translate;
        state.target_language = target;
        state.units = units;
        state.rows = rows;
        state.queue = queue;
        state.pending = None;
        state.total_batches = total;
        state.next_batch = 0;
        state.running = true;
        state.revision = self.session.revision;
        state.changed = false;
        state.notice = None;
        start_next(self);
        true
    }
}

/// Hand a finished translation request to the window.
pub fn set_result(editor: &mut Editor, batch: usize, result: Result<ChatReply, String>) {
    let Some(pending) = editor.ui.translate.pending.clone() else {
        return;
    };
    if !editor.ui.translate.running || pending.batch != batch {
        return;
    }
    editor.ui.translate.pending = None;
    match result {
        Ok(reply) => {
            let parsed = parse_translations(&reply.text, &pending.units);
            let usable = parsed.translated() > 0;
            if !usable && !pending.retried && !reply.truncated {
                retry_batch(editor, pending);
                return;
            }
            apply_batch(editor, parsed, reply.truncated);
            start_next(editor);
        }
        Err(message) => {
            if message == AiError::RejectedKey.to_string() {
                mark_batch_failed(editor, &pending, message);
                editor.ui.translate.queue.clear();
                editor.ui.translate.running = false;
                editor.ui.translate.pending = None;
                return;
            }
            if !pending.retried {
                retry_batch(editor, pending);
                return;
            }
            mark_batch_failed(editor, &pending, message);
            start_next(editor);
        }
    }
}

/// Forget a late answer after Cancel.
pub fn set_cancelled(editor: &mut Editor, batch: usize) {
    if editor
        .ui
        .translate
        .pending
        .as_ref()
        .is_some_and(|pending| pending.batch == batch)
    {
        editor.ui.translate.pending = None;
        editor.ui.translate.running = false;
    }
}

/// Escape closes the window, cancelling any run first.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.translate.open {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        if editor.ui.translate.running {
            cancel_run(editor);
        }
        close(editor);
    }
}

fn collect_units(course: &Course, target_language: &str) -> Vec<UnitIn> {
    translation_units(course, target_language)
        .into_iter()
        .map(|unit| UnitIn {
            id: unit.id,
            text: unit.source,
        })
        .collect()
}

fn start_next(editor: &mut Editor) {
    let Some(batch) = editor.ui.translate.queue.first().cloned() else {
        editor.ui.translate.running = false;
        editor.ui.translate.pending = None;
        return;
    };
    editor.ui.translate.queue.remove(0);
    let number = editor.ui.translate.next_batch;
    editor.ui.translate.next_batch = number.saturating_add(1);
    let request = translate_request(
        &batch,
        editor.session.course.language.as_deref(),
        &editor.ui.translate.target_language,
    );
    editor.ui.translate.running = true;
    editor.ui.translate.pending = Some(PendingTranslate {
        batch: number,
        units: batch,
        retried: false,
    });
    editor.request(EditorRequest::AiChat {
        purpose: AiPurpose::Translate { batch: number },
        request,
    });
}

fn retry_batch(editor: &mut Editor, mut pending: PendingTranslate) {
    pending.retried = true;
    let request = translate_request(
        &pending.units,
        editor.session.course.language.as_deref(),
        &editor.ui.translate.target_language,
    );
    editor.ui.translate.running = true;
    editor.ui.translate.pending = Some(pending.clone());
    editor.request(EditorRequest::AiChat {
        purpose: AiPurpose::Translate {
            batch: pending.batch,
        },
        request,
    });
}

fn apply_batch(editor: &mut Editor, parsed: coursecraft_ai::TranslationBatch, truncated: bool) {
    for result in parsed.units {
        let Some(row) = editor
            .ui
            .translate
            .rows
            .iter_mut()
            .find(|row| row.id == result.id)
        else {
            continue;
        };
        match result.outcome {
            TranslationOutcome::Translated(text) => {
                row.target = text;
                row.failure = None;
            }
            TranslationOutcome::Missing => {
                row.target.clear();
                row.failure = Some(if truncated {
                    "The answer was cut off before it finished.".to_string()
                } else {
                    "The model didn't translate this one.".to_string()
                });
            }
            TranslationOutcome::Rejected(reason) => {
                row.target.clear();
                row.failure = Some(reason);
            }
        }
        row.problem = row_problem(row);
    }
    refresh_problems(editor);
}

fn mark_batch_failed(editor: &mut Editor, pending: &PendingTranslate, message: String) {
    for unit in &pending.units {
        if let Some(row) = editor
            .ui
            .translate
            .rows
            .iter_mut()
            .find(|row| row.id == unit.id)
        {
            row.target.clear();
            row.failure = Some(message.clone());
            row.problem = row_problem(row);
        }
    }
}

fn cancel_run(editor: &mut Editor) {
    if let Some(pending) = editor.ui.translate.pending.take() {
        editor.request(EditorRequest::CancelAi(AiPurpose::Translate {
            batch: pending.batch,
        }));
    }
    editor.ui.translate.queue.clear();
    editor.ui.translate.running = false;
}

fn close(editor: &mut Editor) {
    let target = editor.ui.translate.target_language.clone();
    editor.ui.translate = TranslateState {
        target_language: target,
        ..TranslateState::default()
    };
}

fn refresh_problems(editor: &mut Editor) {
    for row in &mut editor.ui.translate.rows {
        row.problem = row_problem(row);
    }
}

fn row_problem(row: &TranslateRow) -> Option<String> {
    if row.target.trim().is_empty() {
        return Some(
            row.failure
                .clone()
                .unwrap_or_else(|| "No translation yet.".to_string()),
        );
    }
    translation_problem(&row.source, &row.target, row.id.starts_with("chrome/"))
}

fn valid_targets(editor: &Editor) -> BTreeMap<String, String> {
    editor
        .ui
        .translate
        .rows
        .iter()
        .filter(|row| row.problem.is_none() && !row.target.trim().is_empty())
        .map(|row| (row.id.clone(), row.target.clone()))
        .collect()
}

fn attention_count(editor: &Editor) -> usize {
    editor
        .ui
        .translate
        .rows
        .iter()
        .filter(|row| row.problem.is_some())
        .count()
}

fn translated_count(editor: &Editor) -> usize {
    editor
        .ui
        .translate
        .rows
        .len()
        .saturating_sub(attention_count(editor))
}

/// Draw the Translate with AI window when it is open.
pub fn show_translate_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.translate.open {
        return;
    }

    // Any edit, undo or course replace makes the collected sources stale.
    if !editor.ui.translate.rows.is_empty()
        && editor.ui.translate.revision != editor.session.revision
        && !editor.ui.translate.changed
    {
        editor.ui.translate.changed = true;
        if editor.ui.translate.running {
            cancel_run(editor);
        }
    }
    refresh_problems(editor);

    let _screen = ctx.content_rect();
    let max_height = crate::dialog::dialog_area(ctx).height().max(300.0);
    let ready = editor.ai_ready();
    let units_count = if editor.ui.translate.rows.is_empty() {
        editor.ui.translate.units.len()
    } else {
        editor.ui.translate.rows.len()
    };
    let translated = translated_count(editor);
    let attention = attention_count(editor);
    let table_height = (max_height - 380.0).max(160.0);
    let only_attention = editor.ui.translate.only_attention;
    let visible: Vec<usize> = editor
        .ui
        .translate
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !only_attention || row.problem.is_some())
        .map(|(index, _)| index)
        .collect();

    let mut open = true;
    let mut close_clicked = false;
    let mut translate = false;
    let mut cancel = false;
    let mut apply = false;
    let mut save = false;
    let mut open_settings = false;

    Dialog::new("Translate with AI")
        .id(Id::unique(("translate_with_ai_dialog", 0_u64)))
        .width(760.0)
        .open(&mut open)
        .resizable(true)
        .show_with_footer(ctx, |dialog| {
            let p = palette(ctx);
            dialog.body(|ui| {
                ui.set_max_width(760.0);
                if !ready {
                    if crate::ai_settings::show_ai_not_ready(ui, "translate this course") {
                        open_settings = true;
                    }
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        close_clicked = true;
                    }
                    return;
                }

                ui.label(
                    RichText::new(
                        "CourseCraft fills the exported XLIFF for you. Nothing changes until \
                             Apply, and Apply is the normal translation import.",
                    )
                    .size(SIZE_SMALL)
                    .color(p.text_secondary),
                );
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Translate to:");
                    ui.add(
                        TextEdit::singleline(&mut editor.ui.translate.target_language)
                            .desired_width(80.0)
                            .hint_text("fr"),
                    );
                    ComboBox::from_id_salt("translate_common_language")
                        .selected_text("Common languages")
                        .show_ui(ui, |ui| {
                            for (name, tag) in COMMON_LANGUAGES {
                                if ui
                                    .selectable_label(
                                        editor.ui.translate.target_language == *tag,
                                        format!("{name} ({tag})"),
                                    )
                                    .clicked()
                                {
                                    editor.ui.translate.target_language = (*tag).to_string();
                                }
                            }
                        });
                });
                let target = editor.ui.translate.target_language.trim().to_string();
                let valid = is_language_tag_valid(&target);
                if !valid && !target.is_empty() {
                    ui.label(
                        RichText::new("Use a tag such as fr, en-GB or pt-BR.").color(p.danger),
                    );
                }
                ui.label(
                    RichText::new(
                        "The bundled font is Latin only; non-Latin scripts aren't drawn yet.",
                    )
                    .size(SIZE_SMALL)
                    .color(p.text_tertiary),
                );
                if language_may_lack_glyphs(&target) {
                    ui.label(
                        RichText::new(
                            "This language uses a script the bundled font may not draw. \
                                 The translation will still be saved.",
                        )
                        .size(SIZE_SMALL)
                        .color(p.warning),
                    );
                }
                ui.add_space(4.0);

                if units_count == 0 {
                    ui.label("This course has no text to translate.");
                } else {
                    ui.label(
                        RichText::new(consent_line(editor, units_count))
                            .size(SIZE_BODY)
                            .color(p.text_secondary),
                    );
                }

                if let Some(notice) = &editor.ui.translate.notice {
                    ui.add_space(4.0);
                    ui.label(RichText::new(notice).color(p.danger));
                }
                if editor.ui.translate.changed {
                    ui.add_space(4.0);
                    ui.label(RichText::new("The course changed. Translate again.").color(p.danger));
                }

                if editor.ui.translate.running {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        let number = editor
                            .ui
                            .translate
                            .pending
                            .as_ref()
                            .map_or(1, |pending| pending.batch.saturating_add(1));
                        ui.label(format!(
                            "Batch {number} of {}",
                            editor.ui.translate.total_batches
                        ));
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                } else {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let can_translate = valid && units_count > 0;
                        if ui
                            .add_enabled(can_translate, Button::new("Translate"))
                            .clicked()
                        {
                            translate = true;
                        }
                    });
                }

                if !editor.ui.translate.rows.is_empty() {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{translated} translated, {attention} need attention."
                            ))
                            .strong(),
                        );
                        ui.checkbox(
                            &mut editor.ui.translate.only_attention,
                            "Show only those needing attention",
                        );
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [190.0, CONTROL_COMPACT],
                            Label::new(RichText::new("Location").strong()).truncate(),
                        );
                        ui.add_sized(
                            [220.0, CONTROL_COMPACT],
                            Label::new(RichText::new("Source").strong()).truncate(),
                        );
                        ui.label(RichText::new("Target").strong());
                    });

                    ScrollArea::vertical()
                        .id_salt("translate_review_rows")
                        .max_height(table_height)
                        .show_rows(ui, ROW_HEIGHT, visible.len(), |ui, range| {
                            for visible_index in range {
                                let Some(&row_index) = visible.get(visible_index) else {
                                    continue;
                                };
                                let Some(row) = editor.ui.translate.rows.get_mut(row_index) else {
                                    continue;
                                };
                                ui.allocate_ui_with_layout(
                                    vec2(ui.available_width(), ROW_HEIGHT),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            let colour = if row.problem.is_some() {
                                                p.danger
                                            } else {
                                                p.text
                                            };
                                            ui.add_sized(
                                                [190.0, CONTROL_COMPACT],
                                                Label::new(
                                                    RichText::new(&row.location)
                                                        .strong()
                                                        .color(colour),
                                                )
                                                .truncate(),
                                            );
                                            ui.add_sized(
                                                [220.0, CONTROL_COMPACT],
                                                Label::new(
                                                    RichText::new(&row.source)
                                                        .size(SIZE_SMALL)
                                                        .color(p.text_secondary),
                                                )
                                                .truncate(),
                                            );
                                            let width = ui.available_width().max(120.0);
                                            ui.add_sized(
                                                [width, CONTROL_COMPACT],
                                                TextEdit::singleline(&mut row.target)
                                                    .id_salt(("translate_target", row_index))
                                                    .hint_text("Translation"),
                                            );
                                        });
                                        if let Some(problem) = &row.problem {
                                            ui.label(
                                                RichText::new(problem)
                                                    .size(SIZE_SMALL)
                                                    .color(p.danger),
                                            );
                                        }
                                    },
                                );
                                row.problem = row_problem(row);
                            }
                        });
                }
            });
            if ready {
                dialog.footer(|ui| {
                    ui.horizontal(|ui| {
                        if !editor.ui.translate.rows.is_empty() {
                            let valid_count = valid_targets(editor).len();
                            let valid =
                                is_language_tag_valid(editor.ui.translate.target_language.trim());
                            let can_apply = valid
                                && !editor.ui.translate.running
                                && !editor.ui.translate.changed
                                && valid_count > 0;
                            if ui
                                .add_enabled(can_apply, Button::new(format!("Apply {valid_count}")))
                                .clicked()
                            {
                                apply = true;
                            }
                            if editor.file_ops_available {
                                if ui.button("Save as XLIFF…").clicked() {
                                    save = true;
                                }
                            } else {
                                ui.add_enabled(false, Button::new("Save as XLIFF…"))
                                    .on_disabled_hover_text(
                                        "Saving files is desktop-only in this version",
                                    );
                            }
                        }
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                    });
                });
            }
        });

    if !open {
        close_clicked = true;
    }

    if open_settings {
        close(editor);
        editor.open_ai_settings();
        return;
    }
    if cancel {
        cancel_run(editor);
        return;
    }
    if translate {
        editor.start_translation();
        return;
    }
    if apply {
        apply_translation(editor);
        return;
    }
    if save {
        save_translation(editor, requests);
        return;
    }
    if close_clicked {
        if editor.ui.translate.running {
            cancel_run(editor);
        }
        close(editor);
    }
}

fn apply_translation(editor: &mut Editor) {
    let targets = valid_targets(editor);
    if targets.is_empty() {
        editor.ui.translate.notice = Some("There is nothing to apply yet.".to_string());
        return;
    }
    let target_language = editor.ui.translate.target_language.trim().to_string();
    let xliff = match fill_xliff(&editor.session.course, &target_language, &targets) {
        Ok(text) => text,
        Err(error) => {
            editor.ui.translate.notice = Some(format!("Couldn't build the XLIFF: {error}"));
            return;
        }
    };
    match editor.run("course.import_xliff", serde_json::json!({ "xliff": xliff })) {
        Some(report) => {
            let text = crate::translation::import_report_text(&report);
            close(editor);
            editor.set_translation_report(text);
        }
        None => {
            let message = editor
                .ui
                .last_error
                .as_ref()
                .map(|(message, _)| message.clone())
                .unwrap_or_else(|| "Couldn't apply the translation.".to_string());
            editor.ui.translate.notice = Some(message);
        }
    }
}

fn save_translation(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let targets = valid_targets(editor);
    if targets.is_empty() {
        editor.ui.translate.notice = Some("There is nothing to save yet.".to_string());
        return;
    }
    let target_language = editor.ui.translate.target_language.trim().to_string();
    match fill_xliff(&editor.session.course, &target_language, &targets) {
        Ok(xliff) => requests.push(EditorRequest::SaveTranslation { xliff }),
        Err(error) => {
            editor.ui.translate.notice = Some(format!("Couldn't build the XLIFF: {error}"));
        }
    }
}

fn consent_line(editor: &Editor, count: usize) -> String {
    let pieces = if count == 1 {
        "1 piece of text".to_string()
    } else {
        format!("{count} pieces of text")
    };
    if let Some(destination) = crate::ai_settings::agent_destination(editor.ui.ai.source) {
        return format!("This sends {pieces} from this course to {destination}.");
    }
    let host = coursecraft_ai::endpoint_host(&editor.ui.ai.config);
    let local = coursecraft_ai::endpoint_is_local(&editor.ui.ai.config);
    match host {
        Some(host) if local => {
            format!("This sends {pieces} from this course to {host}, on this computer.")
        }
        Some(host) => format!("This sends {pieces} from this course to {host}."),
        None => format!("This sends {pieces} from this course to the AI endpoint."),
    }
}

fn language_may_lack_glyphs(tag: &str) -> bool {
    let primary = tag
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        primary.as_str(),
        "hi" | "ar"
            | "zh"
            | "ja"
            | "ko"
            | "ru"
            | "he"
            | "fa"
            | "ur"
            | "th"
            | "bn"
            | "ta"
            | "te"
            | "ml"
            | "kn"
            | "gu"
            | "pa"
            | "mr"
            | "ne"
            | "si"
            | "my"
            | "am"
            | "km"
            | "lo"
            | "bo"
            | "uk"
            | "bg"
            | "sr"
            | "mk"
            | "be"
            | "kk"
    )
}

fn unit_location(course: &Course, id: &str) -> String {
    if id == "course/title" {
        return "Course title".to_string();
    }
    if let Some(key) = id.strip_prefix("chrome/") {
        return format!("Course text ▸ {key}");
    }
    if let Some(rest) = id.strip_prefix("scene/")
        && let Some(scene_id) = rest.strip_suffix("/title")
    {
        let title = course
            .scenes
            .iter()
            .find(|scene| scene.id == scene_id)
            .map(|scene| scene.title.trim().to_string())
            .unwrap_or_default();
        return if title.is_empty() {
            "Scene ▸ Title".to_string()
        } else {
            format!("Scene ▸ {title} ▸ Title")
        };
    }
    if let Some(rest) = id.strip_prefix("master/")
        && let Some((master_id, field)) = rest.split_once('/')
    {
        let title = course
            .masters
            .iter()
            .find(|master| master.id == master_id)
            .map(|master| master.title.trim().to_string())
            .unwrap_or_default();
        let place = if title.is_empty() {
            "Master".to_string()
        } else {
            format!("Master ▸ {title}")
        };
        return format!("{place} ▸ {}", field_label(field));
    }
    if let Some(rest) = id.strip_prefix("slide/")
        && let Some((slide_id, field)) = rest.split_once('/')
    {
        let index = course
            .slides()
            .position(|slide| slide.id == slide_id)
            .map(|index| index.saturating_add(1));
        let title = course
            .slide(slide_id)
            .map(|slide| slide.title.trim().to_string())
            .unwrap_or_default();
        let bank_title = course
            .banks
            .iter()
            .find(|bank| bank.slides.iter().any(|slide| slide.id == slide_id))
            .map(|bank| bank.title.trim().to_string())
            .unwrap_or_default();
        let place = match (index, title.is_empty(), bank_title.is_empty()) {
            (Some(number), false, _) => format!("Slide {number} ▸ {title}"),
            (Some(number), true, _) => format!("Slide {number}"),
            (None, false, false) => format!("Bank ▸ {bank_title} ▸ {title}"),
            (None, true, false) => format!("Bank ▸ {bank_title}"),
            (None, false, true) => title,
            (None, true, true) => "Slide".to_string(),
        };
        return format!("{place} ▸ {}", field_label(field));
    }
    id.to_string()
}

fn field_label(field: &str) -> String {
    if field == "title" {
        return "Title".to_string();
    }
    if field == "notes" {
        return "Notes".to_string();
    }
    if field == "question/prompt" {
        return "Question prompt".to_string();
    }
    if field.starts_with("question/choice/") {
        return "Choice".to_string();
    }
    if field.starts_with("question/accepted/") {
        return "Accepted answer".to_string();
    }
    if field.ends_with("/left") {
        return "Pair left".to_string();
    }
    if field.ends_with("/right") {
        return "Pair right".to_string();
    }
    let last = field.rsplit('/').next().unwrap_or(field);
    match last {
        "text" => "Text".to_string(),
        "label" => "Button label".to_string(),
        "alt" => "Alt text".to_string(),
        "description" => "Description".to_string(),
        other => other.to_string(),
    }
}
