//! File ▸ Export ▸ Video (MP4)…, PDF/Word for review, and the status bar's
//! export progress.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use coursecraft_render::{
    CONTROL_PRIMARY, RADIUS_MD, SIZE_BODY, SPACE_MD, SPACE_SM, palette, small,
};
use egui::{Color32, RichText};

use crate::{Editor, EditorRequest};

/// The review document format chosen in File ▸ Export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewFormat {
    /// PDF for review.
    #[default]
    Pdf,
    /// Word for review.
    Word,
}

impl ReviewFormat {
    /// File extension, without the dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Word => "docx",
        }
    }

    /// Name shown in the dialog title and the save filter.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Word => "Word",
        }
    }
}

/// The four checkboxes in the Export for Review dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewOptionsView {
    /// Add a page for every slide layer.
    pub include_layers: bool,
    /// Include the slide notes.
    pub include_notes: bool,
    /// Include the slide's text.
    pub include_text: bool,
    /// Append the question-bank slides.
    pub include_banks: bool,
}

impl Default for ReviewOptionsView {
    fn default() -> Self {
        Self {
            include_layers: false,
            include_notes: true,
            include_text: true,
            include_banks: true,
        }
    }
}

/// A running MP4 export shown in the status bar.
#[derive(Debug, Clone)]
pub struct ExportJobView {
    /// Zero-based index of the slide being drawn.
    pub slide_index: usize,
    /// Number of slides in the play order.
    pub slide_total: usize,
    /// Frames written so far.
    pub frames: u64,
    /// Set by the Cancel button; the export callback returns `!cancel`.
    pub cancel: Arc<AtomicBool>,
}

impl Editor {
    /// Open File ▸ Export ▸ Video (MP4)….
    pub fn open_export_dialog(&mut self) {
        self.ui.show_export_video = true;
    }

    /// Open File ▸ Export ▸ PDF/Word for review….
    pub fn open_export_review(&mut self, format: ReviewFormat) {
        self.ui.review_format = format;
        self.ui.show_export_review = true;
    }

    /// Show a finished export in the status bar.
    pub fn set_export_report(&mut self, message: impl Into<String>) {
        self.ui.export_job = None;
        self.ui.export_status = Some(message.into());
    }
}

/// Draw the Export Video dialog when it is open.
pub fn show_export_dialog(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.show_export_video {
        return;
    }

    let mut open = true;
    let mut close_after = false;
    let desktop = editor.file_ops_available && !cfg!(target_arch = "wasm32");

    egui::Window::new("Export Video")
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ctx);
            ui.set_max_width(380.0);

            ui.label(small("Size").size(SIZE_BODY).color(p.text_secondary));
            for (label, width) in [("720p", 1280u32), ("1080p", 1920u32)] {
                if ui
                    .add(egui::RadioButton::new(
                        editor.ui.export_width == width,
                        label,
                    ))
                    .clicked()
                {
                    editor.ui.export_width = width;
                }
            }
            ui.add_space(SPACE_SM);

            ui.horizontal(|ui| {
                ui.label(small("Frame rate").size(SIZE_BODY).color(p.text_secondary));
                egui::ComboBox::from_id_salt("export-video-fps")
                    .selected_text(format!("{} fps", editor.ui.export_fps))
                    .show_ui(ui, |ui| {
                        for fps in [24u32, 30u32] {
                            ui.selectable_value(
                                &mut editor.ui.export_fps,
                                fps,
                                format!("{fps} fps"),
                            );
                        }
                    });
            });
            ui.add_space(SPACE_MD);

            ui.label(
                small("The export follows the linear path. Questions are shown unanswered.")
                    .color(p.text_secondary),
            );
            ui.add_space(SPACE_MD);

            if !desktop {
                ui.label(small("Available in the desktop app").color(p.text_secondary));
                ui.add_space(SPACE_SM);
            }

            ui.horizontal(|ui| {
                if desktop {
                    let button = egui::Button::new(RichText::new("Export…").color(Color32::WHITE))
                        .fill(p.accent)
                        .corner_radius(RADIUS_MD as u8)
                        .min_size(egui::vec2(0.0, CONTROL_PRIMARY));
                    if ui.add(button).clicked() {
                        requests.push(EditorRequest::ExportMp4 {
                            width: editor.ui.export_width,
                            fps: editor.ui.export_fps,
                        });
                        close_after = true;
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Export…"));
                }
                if ui.button("Cancel").clicked() {
                    close_after = true;
                }
            });
        });

    if close_after || !open {
        editor.ui.show_export_video = false;
    }
}

/// Draw the Export for Review dialog when it is open.
pub fn show_export_review_dialog(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.show_export_review {
        return;
    }

    let mut open = true;
    let mut close_after = false;
    let desktop = editor.file_ops_available && !cfg!(target_arch = "wasm32");
    let label = editor.ui.review_format.label();

    egui::Window::new("Export for Review")
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ctx);
            ui.set_max_width(380.0);

            ui.label(
                small(format!("{label} document for reviewers"))
                    .size(SIZE_BODY)
                    .color(p.text_secondary),
            );
            ui.add_space(SPACE_SM);
            ui.checkbox(&mut editor.ui.review_options.include_layers, "Layers");
            ui.checkbox(&mut editor.ui.review_options.include_notes, "Notes");
            ui.checkbox(&mut editor.ui.review_options.include_text, "Slide text");
            ui.checkbox(&mut editor.ui.review_options.include_banks, "Bank slides");
            ui.add_space(SPACE_MD);

            ui.label(
                small("The document follows the linear path. Questions are shown unanswered.")
                    .color(p.text_secondary),
            );
            ui.add_space(SPACE_MD);

            if !desktop {
                ui.label(small("Available in the desktop app").color(p.text_secondary));
                ui.add_space(SPACE_SM);
            }

            ui.horizontal(|ui| {
                if desktop {
                    let button = egui::Button::new(RichText::new("Export…").color(Color32::WHITE))
                        .fill(p.accent)
                        .corner_radius(RADIUS_MD as u8)
                        .min_size(egui::vec2(0.0, CONTROL_PRIMARY));
                    if ui.add(button).clicked() {
                        requests.push(EditorRequest::ExportReview {
                            format: editor.ui.review_format,
                            options: editor.ui.review_options,
                        });
                        close_after = true;
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Export…"));
                }
                if ui.button("Cancel").clicked() {
                    close_after = true;
                }
            });
        });

    if close_after || !open {
        editor.ui.show_export_review = false;
    }
}

/// Show the running export and its Cancel button in the status bar.
pub fn show_export_status(editor: &mut Editor, ui: &mut egui::Ui) {
    let p = palette(ui.ctx());
    if let Some(job) = editor.ui.export_job.clone() {
        ui.separator();
        let total = job.slide_total.max(1);
        let current = job.slide_index.saturating_add(1).min(total);
        ui.label(
            small(format!("Exporting video: slide {current} of {total}")).color(p.text_secondary),
        );
        let fraction = current as f32 / total as f32;
        ui.add(egui::ProgressBar::new(fraction).desired_width(120.0));
        if ui.button("Cancel").clicked() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    } else if let Some(message) = editor.ui.export_status.clone() {
        ui.separator();
        ui.label(small(message).color(p.text_secondary));
        if ui.button("Show in folder").clicked() {
            editor.request(EditorRequest::ShowPublished);
        }
    }
}
