//! Slides from an outline: a pasted-Markdown window that calls
//! `course.import_outline`.
//!
//! The parser and its limits live in `coursecraft_course::outline`; this module
//! only draws the window, shows a live summary, and dispatches the one engine
//! command. It needs no file dialog or network, so it behaves the same on the
//! web and on the desktop.

use crate::dialog::Dialog;
use coursecraft_course::{CourseOutline, OutlineProblem, outline_from_markdown};
use coursecraft_render::{SIZE_BODY, SIZE_SMALL, palette};
use egui::{Button, Color32, Id, Key, Modifiers, RichText, ScrollArea, TextEdit};
use serde_json::{Value as Json, json};

use crate::Editor;

/// A small example shown under the help line.
const EXAMPLE: &str = "# Safety\n\
## Welcome\n\
- Keep exits clear\n\
- Know the alarm sound\n\
> Point at the nearest exit.\n\
## Quiz\n\
? What do you do when the alarm sounds?\n\
[x] Leave by the nearest exit\n\
[ ] Wait for an email";

/// Open the window, or bring it forward and focus the field if it is open.
pub fn open(editor: &mut Editor) {
    if !editor.ui.show_outline_dialog {
        editor.ui.show_outline_dialog = true;
        editor.ui.outline_text.clear();
        editor.ui.outline_last_first_slide = None;
        editor.ui.outline_warning = None;
    }
    editor.ui.outline_focus = true;
}

/// Close the window when Escape is pressed. Runs before the canvas sees the
/// key, so Escape closes this window instead of moving or clearing anything.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if editor.ui.show_outline_dialog
        && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
    {
        close(editor);
    }
}

fn close(editor: &mut Editor) {
    editor.ui.show_outline_dialog = false;
    editor.ui.outline_focus = false;
}

/// Parse the field with the same rules the command will apply.
pub fn parse(editor: &Editor) -> Result<CourseOutline, Vec<OutlineProblem>> {
    outline_from_markdown(&editor.ui.outline_text)
}

/// True when the field parses and has at least one slide to add.
#[must_use]
pub fn can_add(editor: &Editor) -> bool {
    match parse(editor) {
        Ok(outline) => outline.slides().next().is_some(),
        Err(_) => false,
    }
}

/// Run the import for the current field. Returns whether it succeeded.
pub fn add(editor: &mut Editor) -> bool {
    let Ok(outline) = parse(editor) else {
        return false;
    };
    if outline.slides().next().is_none() {
        return false;
    }
    let after = if editor
        .session
        .course
        .play_slide(&editor.selection.slide)
        .is_some()
    {
        Some(editor.selection.slide.clone())
    } else {
        None
    };
    let params = json!({ "outline": outline, "after": after });
    let Some(result) = editor.run("course.import_outline", params) else {
        return false;
    };
    let first = result
        .get("first_slide")
        .and_then(Json::as_str)
        .map(str::to_owned);
    let slides = result
        .get("slides")
        .and_then(Json::as_u64)
        .unwrap_or_default();
    let scenes = result
        .get("scenes")
        .and_then(Json::as_u64)
        .unwrap_or_default();
    if let Some(first) = first {
        editor.selection.slide = first.clone();
        editor.selection.layer = None;
        editor.selection.clear_objects();
        editor.ui.outline_last_first_slide = Some(first);
    }
    close(editor);
    editor.set_status(format!(
        "Added {} in {}",
        plural(slides, "slide", "slides"),
        plural(scenes, "scene", "scenes")
    ));
    true
}

/// Draw the window when it is open.
pub fn show_outline_dialog(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.show_outline_dialog {
        return;
    }
    let max_height = crate::dialog::dialog_area(ctx).height().max(260.0);
    let mut open = true;
    let mut add_clicked = false;
    let mut cancel_clicked = false;

    Dialog::new("Slides from Outline")
        .id(Id::unique(("slides_from_outline_dialog", 0_u64)))
        .width(580.0)
        .open(&mut open)
        .resizable(true)
        .show_with_footer(ctx, |dialog| {
            let p = palette(ctx);
            let mut can = false;
            dialog.body(|ui| {
                ui.label(
                    RichText::new(
                        "Paste an outline: `#` starts a scene, `##` a slide, `-` a body line, \
                     `>` speaker notes, `?` a question and `[x]` / `[ ]` its choices.",
                    )
                    .size(SIZE_SMALL)
                    .color(p.text_secondary),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(EXAMPLE)
                        .monospace()
                        .size(SIZE_SMALL)
                        .color(p.text_tertiary),
                );
                if let Some(warning) = editor.ui.outline_warning.clone() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(warning).size(SIZE_SMALL).color(p.warning));
                }
                ui.add_space(6.0);

                let field_height = (max_height - 280.0).clamp(120.0, 420.0);
                let response = ScrollArea::vertical()
                    .id_salt("outline_source_scroll")
                    .max_height(field_height)
                    .min_scrolled_height(field_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut editor.ui.outline_text)
                                .id_salt("outline_source")
                                .hint_text("Paste your outline here")
                                .desired_width(f32::INFINITY)
                                .desired_rows(14),
                        )
                    })
                    .inner;
                if editor.ui.outline_focus {
                    response.request_focus();
                    editor.ui.outline_focus = false;
                }

                ui.add_space(6.0);
                let parsed = parse(editor);
                can = matches!(&parsed, Ok(outline) if outline.slides().next().is_some());
                match &parsed {
                    Ok(outline) => {
                        ui.label(
                            RichText::new(summary(outline))
                                .size(SIZE_BODY)
                                .color(p.text_secondary),
                        );
                    }
                    Err(problems) => {
                        for problem in problems {
                            ui.label(
                                RichText::new(problem.to_string())
                                    .size(SIZE_SMALL)
                                    .color(p.danger),
                            );
                        }
                    }
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    let add = Button::new(RichText::new("Add slides").color(Color32::WHITE))
                        .fill(p.accent);
                    if ui
                        .add_enabled(can, add)
                        .on_disabled_hover_text(if can {
                            ""
                        } else {
                            "Fix the problems above first"
                        })
                        .clicked()
                    {
                        add_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                });
            });
        });

    if add_clicked {
        let _ = add(editor);
    } else if cancel_clicked || !open {
        close(editor);
    }
}

fn summary(outline: &CourseOutline) -> String {
    let scenes = outline
        .scenes
        .iter()
        .filter(|scene| !scene.slides.is_empty())
        .count() as u64;
    let slides = outline.slides().count() as u64;
    let questions = outline.question_count() as u64;
    if questions == 0 {
        format!(
            "{}, {}",
            plural(scenes, "scene", "scenes"),
            plural(slides, "slide", "slides")
        )
    } else {
        format!(
            "{}, {}, {}",
            plural(scenes, "scene", "scenes"),
            plural(slides, "slide", "slides"),
            plural(questions, "question", "questions")
        )
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
