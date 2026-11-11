//! XLIFF translation dialogs: the target-language prompt and the import report.

use crate::{Editor, EditorRequest};

/// Show the export language prompt and the import report, when open.
pub fn show_translation_windows(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    show_export_prompt(editor, ctx, requests);
    show_report(editor, ctx);
}

fn show_export_prompt(editor: &mut Editor, ctx: &egui::Context, requests: &mut Vec<EditorRequest>) {
    if !editor.ui.show_export_translation {
        return;
    }
    let mut open = true;
    egui::Window::new("Export Translation")
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Target language (BCP 47, for example fr or pt-BR):");
            ui.text_edit_singleline(&mut editor.ui.translation_target_language);
            let target = editor.ui.translation_target_language.trim().to_owned();
            let valid = coursecraft_course::is_language_tag_valid(&target);
            if !valid && !target.is_empty() {
                ui.label(
                    egui::RichText::new("Use a tag such as fr, en-GB or pt-BR.")
                        .color(coursecraft_render::palette(ui.ctx()).danger),
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(valid, egui::Button::new("Export…"))
                    .clicked()
                {
                    requests.push(EditorRequest::ExportTranslation {
                        target_language: target.clone(),
                    });
                    editor.ui.show_export_translation = false;
                }
                if ui.button("Translate with AI…").clicked() {
                    editor.open_translate_dialog();
                    editor.ui.translate.target_language = target.clone();
                    editor.ui.show_export_translation = false;
                }
                if ui.button("Cancel").clicked() {
                    editor.ui.show_export_translation = false;
                }
            });
        });
    if !open {
        editor.ui.show_export_translation = false;
    }
}

/// Format the engine's XLIFF import report for the report window.
#[must_use]
pub fn import_report_text(report: &serde_json::Value) -> String {
    let applied = report
        .get("applied")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let mut text = format!("Applied {applied} unit(s).");
    for (label, key) in [
        ("Missing target", "missing_target"),
        ("Source changed", "source_changed"),
        ("Unknown id", "unknown_ids"),
        ("Placeholder mismatch", "placeholder_mismatch"),
        ("Unsupported script", "unsupported_script"),
    ] {
        let Some(list) = report.get(key).and_then(serde_json::Value::as_array) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        let ids: Vec<&str> = list.iter().filter_map(serde_json::Value::as_str).collect();
        text.push_str(&format!("\n{label}: {}", ids.join(", ")));
    }
    text
}

fn show_report(editor: &mut Editor, ctx: &egui::Context) {
    let Some(report) = editor.ui.translation_report.clone() else {
        return;
    };
    let mut open = true;
    let max_height = (ctx.content_rect().height() - 40.0).max(240.0);
    egui::Window::new("Translation Import Report")
        .collapsible(false)
        .resizable(true)
        .max_height(max_height)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height((max_height - 80.0).max(160.0))
                .show(ui, |ui| {
                    ui.label(report);
                });
            if ui.button("OK").clicked() {
                editor.ui.translation_report = None;
            }
        });
    if !open {
        editor.ui.translation_report = None;
    }
}
