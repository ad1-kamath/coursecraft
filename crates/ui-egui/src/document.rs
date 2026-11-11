//! Course from a document: paste text or choose a `.docx`, `.txt` or `.md`,
//! ask the configured endpoint for an outline, then review it in the same
//! window a pasted outline uses.
//!
//! Nothing here sends a request or touches the course. It builds the request
//! through `coursecraft_ai`, dispatches `EditorRequest::AiChat` with
//! `AiPurpose::Outline`, and puts the answer into `outline_text` for review.
//! Only `course.import_outline` changes the course, when the author presses
//! `Add slides` there.

use coursecraft_ai::{
    ChatReply, MAX_DOCUMENT_CHARS, OutlineOptions, endpoint_host, endpoint_is_local,
    outline_request, strip_code_fence,
};
use coursecraft_render::{SIZE_BODY, SIZE_SMALL, palette};
use egui::{Align2, Button, Color32, Id, Key, Modifiers, RichText, ScrollArea, TextEdit, Window};

use crate::{AiPurpose, Editor, EditorRequest};

/// Warning shown when the endpoint hit its answer limit.
pub const TRUNCATED_ANSWER: &str = "The answer was cut off, so the last slides may be missing.";

/// Warning shown when the document is longer than what is sent.
pub const OVER_LIMIT: &str = "Only the first 60,000 characters will be sent.";

/// What the Generate row is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DocumentRequestState {
    /// Nothing is running.
    #[default]
    Idle,
    /// A request is in flight.
    Running,
    /// The last request failed; the text is kept so the author can retry.
    Failed(String),
}

/// Transient state for the Course from Document window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDialogState {
    /// Whether the window is open.
    pub open: bool,
    /// The text that will be sent.
    pub text: String,
    /// Optional slide count, as typed (blank means no target).
    pub slides: String,
    /// Whether to ask for quiz questions.
    pub questions: bool,
    /// Request status.
    pub state: DocumentRequestState,
    /// Put the caret in the text field on the next frame.
    pub focus: bool,
    /// Whether this build can open a file picker.
    pub file_available: bool,
}

impl Default for DocumentDialogState {
    fn default() -> Self {
        Self {
            open: false,
            text: String::new(),
            slides: String::new(),
            questions: true,
            state: DocumentRequestState::Idle,
            focus: false,
            file_available: true,
        }
    }
}

impl DocumentDialogState {
    /// True while a request is in flight.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.state, DocumentRequestState::Running)
    }
}

impl Editor {
    /// Open the Course from Document window.
    pub fn open_document_dialog(&mut self) {
        if !self.ui.document.open {
            self.ui.document = DocumentDialogState {
                open: true,
                ..DocumentDialogState::default()
            };
        }
        self.ui.document.focus = true;
    }

    /// Replace the document text (a chosen file, or a test).
    pub fn set_document_text(&mut self, text: impl Into<String>) {
        self.ui.document.text = text.into();
        self.ui.document.state = DocumentRequestState::Idle;
    }

    /// Show a file-reading error in the window, keeping the text as it was.
    pub fn set_document_error(&mut self, message: impl Into<String>) {
        self.ui.document.state = DocumentRequestState::Failed(message.into());
    }
}

/// Close the window, cancelling a running request.
fn close(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if editor.ui.document.is_running() {
        requests.push(EditorRequest::CancelAi(AiPurpose::Outline));
    }
    editor.ui.document.state = DocumentRequestState::Idle;
    editor.ui.document.open = false;
    editor.ui.document.focus = false;
}

/// Close the window when Escape is pressed, before the canvas sees the key.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if editor.ui.document.open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        let mut requests = Vec::new();
        close(editor, &mut requests);
        editor.pending_requests.append(&mut requests);
    }
}

/// Hand a finished outline request back to this window.
///
/// A result only lands while this window is running the request, so a late or
/// misrouted answer changes nothing. A good answer opens `Slides from Outline`
/// with the stripped text; the course is untouched until the author adds it.
pub fn set_result(editor: &mut Editor, result: Result<ChatReply, String>) {
    if !editor.ui.document.is_running() {
        return;
    }
    match result {
        Ok(reply) => {
            let text = strip_code_fence(&reply.text).to_owned();
            editor.ui.document.state = DocumentRequestState::Idle;
            editor.ui.document.open = false;
            editor.ui.document.focus = false;
            editor.ui.outline_text = text;
            editor.ui.outline_warning = reply.truncated.then(|| TRUNCATED_ANSWER.to_string());
            editor.ui.outline_focus = true;
            editor.ui.outline_last_first_slide = None;
            editor.ui.show_outline_dialog = true;
        }
        Err(message) => {
            editor.ui.document.state = DocumentRequestState::Failed(message);
        }
    }
}

/// Forget a running request after the author cancelled it.
pub fn set_cancelled(editor: &mut Editor) {
    if editor.ui.document.is_running() {
        editor.ui.document.state = DocumentRequestState::Idle;
    }
}

/// The optional slide target, clamped to the window's 1–100 range.
#[must_use]
pub fn requested_slides(input: &str) -> Option<u32> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u32>().ok().map(|n| n.clamp(1, 100))
}

fn character_count(text: &str) -> usize {
    text.chars().count()
}

fn count_label(count: usize) -> String {
    if count == 1 {
        "1 character".to_string()
    } else {
        format!("{count} characters")
    }
}

fn consent_line(editor: &Editor) -> String {
    if let Some(destination) = crate::ai_settings::agent_destination(editor.ui.ai.source) {
        return format!("This sends the text above to {destination}.");
    }
    let host = endpoint_host(&editor.ui.ai.config);
    let local = endpoint_is_local(&editor.ui.ai.config);
    match host {
        Some(host) if local => {
            format!("This sends the text above to {host}, on this computer.")
        }
        Some(host) => format!("This sends the text above to {host}."),
        None => "This sends the text above to the AI endpoint.".to_string(),
    }
}

fn start_request(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if editor.ui.document.text.trim().is_empty() || editor.ui.document.is_running() {
        return;
    }
    let options = OutlineOptions {
        slides: requested_slides(&editor.ui.document.slides),
        questions: editor.ui.document.questions,
    };
    let request = outline_request(&editor.ui.document.text, &options);
    editor.ui.document.state = DocumentRequestState::Running;
    requests.push(EditorRequest::AiChat {
        purpose: AiPurpose::Outline,
        request,
    });
}

/// Draw the window when it is open.
pub fn show_document_dialog(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.document.open {
        return;
    }

    let screen = ctx.content_rect();
    let max_height = (screen.height() - 40.0).max(260.0);
    let ready = editor.ai_ready();
    let consent = consent_line(editor);
    let count = character_count(&editor.ui.document.text);

    let mut open = true;
    let mut close_clicked = false;
    let mut generate = false;
    let mut cancel = false;
    let mut choose_file = false;
    let mut open_settings = false;

    Window::new("Course from Document")
        .id(Id::unique(("course_from_document_dialog", 0_u64)))
        .collapsible(false)
        .resizable(true)
        .max_height(max_height)
        .default_width(640.0)
        .default_height((max_height - 20.0).max(240.0))
        .pivot(Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ui.ctx());
            ui.set_max_width(640.0);
            ScrollArea::vertical()
                .id_salt("document_dialog_scroll")
                .max_height((max_height - 60.0).max(200.0))
                .show(ui, |ui| {
                    if !ready {
                        if crate::ai_settings::show_ai_not_ready(
                            ui,
                            "turn a document into a course",
                        ) {
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
                            "Paste the document, or choose a .docx, .txt or .md file. You'll \
                             review the outline before anything is added to the course.",
                        )
                        .size(SIZE_SMALL)
                        .color(p.text_secondary),
                    );
                    ui.add_space(6.0);

                    let field_height = (max_height - 330.0).clamp(140.0, 380.0);
                    let response = ScrollArea::vertical()
                        .id_salt("document_dialog_source_scroll")
                        .max_height(field_height)
                        .min_scrolled_height(field_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::multiline(&mut editor.ui.document.text)
                                    .id_salt("document_dialog_source")
                                    .hint_text("Paste your document here")
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(14),
                            )
                        })
                        .inner;
                    if editor.ui.document.focus {
                        response.request_focus();
                        editor.ui.document.focus = false;
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(count_label(count))
                                .size(SIZE_SMALL)
                                .color(p.text_tertiary),
                        );
                        if editor.ui.document.file_available {
                            if ui.button("Choose file…").clicked() {
                                choose_file = true;
                            }
                            ui.label(
                                RichText::new(".docx, .txt or .md")
                                    .size(SIZE_SMALL)
                                    .color(p.text_tertiary),
                            );
                        } else {
                            ui.label(
                                RichText::new("Paste only in the browser.")
                                    .size(SIZE_SMALL)
                                    .color(p.text_tertiary),
                            );
                        }
                    });
                    if count > MAX_DOCUMENT_CHARS {
                        ui.label(RichText::new(OVER_LIMIT).size(SIZE_SMALL).color(p.warning));
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("About how many slides");
                        ui.add(
                            TextEdit::singleline(&mut editor.ui.document.slides)
                                .id_salt("document_dialog_slides")
                                .hint_text("Any")
                                .desired_width(64.0),
                        );
                        ui.label(
                            RichText::new("optional, 1 to 100")
                                .size(SIZE_SMALL)
                                .color(p.text_tertiary),
                        );
                    });
                    ui.checkbox(&mut editor.ui.document.questions, "Include quiz questions");

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.label(
                        RichText::new(consent)
                            .size(SIZE_BODY)
                            .color(p.text_secondary),
                    );
                    ui.add_space(4.0);

                    match &editor.ui.document.state {
                        DocumentRequestState::Running => {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new());
                                ui.label("Generating…");
                                if ui.button("Cancel").clicked() {
                                    cancel = true;
                                }
                            });
                        }
                        DocumentRequestState::Failed(message) => {
                            ui.label(RichText::new(message).color(p.danger));
                            ui.add_space(4.0);
                        }
                        DocumentRequestState::Idle => {}
                    }

                    if !editor.ui.document.is_running() {
                        let can_generate = !editor.ui.document.text.trim().is_empty();
                        let button =
                            Button::new(RichText::new("Generate outline").color(Color32::WHITE))
                                .fill(p.accent);
                        if ui
                            .add_enabled(can_generate, button)
                            .on_disabled_hover_text(if can_generate {
                                ""
                            } else {
                                "Paste a document first"
                            })
                            .clicked()
                        {
                            generate = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_clicked = true;
                        }
                    }
                });

            if ui
                .ctx()
                .input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
            {
                close_clicked = true;
            }
        });

    if !open {
        close_clicked = true;
    }

    if open_settings {
        editor.ui.document.open = false;
        editor.ui.document.focus = false;
        editor.open_ai_settings();
        return;
    }
    if choose_file {
        requests.push(EditorRequest::PickDocument);
    }
    if cancel {
        editor.ui.document.state = DocumentRequestState::Idle;
        requests.push(EditorRequest::CancelAi(AiPurpose::Outline));
        return;
    }
    if generate {
        start_request(editor, requests);
        return;
    }
    if close_clicked {
        close(editor, requests);
    }
}
