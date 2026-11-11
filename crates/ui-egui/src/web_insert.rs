//! Insert ▸ Web object dialog and placement.
//!
//! A web object is either an `https://` address or a folder of files copied
//! into the project's media. Folder picking belongs to the host shell, so the
//! dialog asks for it through [`EditorRequest::PickWebFolder`].

use crate::dialog::Dialog;
use coursecraft_course::{ObjectKind, Rect, WebSource};
use egui::TextEdit;

use crate::{Editor, EditorRequest};

/// The frame a new web object gets: 60% of the slide, centred.
#[must_use]
pub fn web_frame(editor: &Editor) -> Rect {
    let size = editor.session.course.slide_size;
    let width = (size.w as f32 * 0.6).clamp(160.0, 960.0);
    let height = (size.h as f32 * 0.6).clamp(120.0, 720.0);
    Rect {
        x: ((size.w as f32 - width) * 0.5).max(0.0),
        y: ((size.h as f32 - height) * 0.5).max(0.0),
        w: width,
        h: height,
    }
}

/// Insert a web object through the normal object command and select it.
pub fn insert_web_object(editor: &mut Editor, source: WebSource) -> Option<String> {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return None;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status("Select a slide first");
        return None;
    }
    let kind = serde_json::to_value(ObjectKind::Web { source }).ok()?;
    let result = editor.run(
        "object.add",
        serde_json::json!({
            "slide": editor.selection.slide,
            "kind": kind,
            "frame": web_frame(editor),
        }),
    )?;
    let id = result.get("id").and_then(serde_json::Value::as_str)?;
    editor.selection.set_many([id.to_owned()]);
    Some(id.to_owned())
}

/// Draw the Insert ▸ Web object window.
pub fn show_web_insert_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    _requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.show_web_window {
        return;
    }
    let mut open = true;
    let mut close = false;
    Dialog::new("Insert Web Object")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.heading("Web address");
            ui.label("A page shown in the published course. It must use https://.");
            let width = ui.available_width().max(260.0);
            let response = ui.add_sized(
                [width, coursecraft_render::CONTROL_COMPACT],
                TextEdit::singleline(&mut editor.ui.web_url)
                    .id_salt("web_insert_url")
                    .hint_text("https://example.com"),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                close = try_insert_url(editor);
            }
            if let Some(error) = &editor.ui.web_error {
                ui.colored_label(coursecraft_render::palette(ui.ctx()).danger, error);
            }
            if ui.button("Insert address").clicked() {
                close = try_insert_url(editor);
            }

            ui.separator();
            ui.heading("Folder");
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.label("Copy HTML, CSS, JavaScript and media from a folder into this project.");
                if ui.button("Choose Folder…").clicked() {
                    _requests.push(EditorRequest::PickWebFolder);
                    close = true;
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                ui.label("Folders can be added in the desktop editor.");
            }
        });
    if close || !open {
        editor.ui.show_web_window = false;
        editor.ui.web_error = None;
    }
}

fn try_insert_url(editor: &mut Editor) -> bool {
    let url = editor.ui.web_url.trim().to_owned();
    if !coursecraft_course::is_valid_web_url(&url) {
        editor.ui.web_error = Some("Enter an https:// address with a host.".to_owned());
        return false;
    }
    if insert_web_object(editor, WebSource::Url { url }).is_some() {
        editor.ui.web_url.clear();
        editor.ui.web_error = None;
        true
    } else {
        editor.ui.web_error = Some("Couldn't insert the web object.".to_owned());
        false
    }
}
