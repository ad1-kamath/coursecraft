//! The Publish dialog: standard picker, pre-flight checks and publish results.

use crate::dialog::Dialog;
use coursecraft_render::{
    CONTROL_PRIMARY, RADIUS_MD, SIZE_BODY, SPACE_MD, SPACE_SM, palette, small,
};
use egui::{Color32, RichText};

use crate::{Editor, EditorRequest};

/// The five package standards an author can pick, without pulling the
/// publishing crate into the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublishStandard {
    /// SCORM 1.2, the safe default.
    #[default]
    Scorm12,
    /// SCORM 2004 4th edition.
    Scorm2004,
    /// cmi5, which uses xAPI.
    Cmi5,
    /// Plain xAPI (Tin Can).
    Xapi,
    /// LTI 1.3, for the CourseCraft LTI server.
    Lti,
}

impl PublishStandard {
    /// Every standard, in the order shown in the dialog.
    pub const ALL: [Self; 5] = [
        Self::Scorm12,
        Self::Scorm2004,
        Self::Cmi5,
        Self::Xapi,
        Self::Lti,
    ];

    /// Learner-facing name for the standard picker.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Scorm12 => "SCORM 1.2",
            Self::Scorm2004 => "SCORM 2004 (4th edition)",
            Self::Cmi5 => "cmi5",
            Self::Xapi => "xAPI (Tin Can)",
            Self::Lti => "LTI 1.3 (for the CourseCraft LTI server)",
        }
    }

    /// One line of plain help beneath the name.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Scorm12 => "Works in almost every LMS. Choose this if you're not sure.",
            Self::Scorm2004 => "For an LMS that asks for SCORM 2004.",
            Self::Cmi5 => "A modern standard using xAPI. Your LMS must support cmi5.",
            Self::Xapi => "Sends results to a Learning Record Store.",
            Self::Lti => "Launch from an LTI 1.3 platform through your own CourseCraft LTI server.",
        }
    }

    /// Short, filename-safe standard name used in the default zip name.
    #[must_use]
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Scorm12 => "scorm12",
            Self::Scorm2004 => "scorm2004",
            Self::Cmi5 => "cmi5",
            Self::Xapi => "xapi",
            Self::Lti => "lti",
        }
    }

    /// Help shown under a successful publish.
    #[must_use]
    pub fn upload_help(self) -> &'static str {
        match self {
            Self::Xapi => {
                "Upload this zip to your LMS, or host it and launch it with your LRS details."
            }
            Self::Lti => {
                "Publish this folder to the CourseCraft LTI server, then launch it from your platform."
            }
            Self::Scorm12 | Self::Scorm2004 | Self::Cmi5 => {
                "Upload this zip to your LMS as a new course."
            }
        }
    }
}

/// What the Publish dialog is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PublishPhase {
    /// Ready for the author to choose a standard and publish.
    #[default]
    Idle,
    /// A background publish is running; the dialog cannot be closed.
    Busy,
    /// The package was written.
    Success {
        /// File name shown to the author.
        file_name: String,
        /// Size in bytes.
        bytes: u64,
    },
    /// The publish failed and the author can try again.
    Failed {
        /// Plain sentence explaining the problem.
        message: String,
    },
}

/// Transient state for the Publish dialog, kept in `editor.ui` like other dialogs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PublishState {
    /// Whether the dialog is open.
    pub open: bool,
    /// Chosen standard; kept for the rest of the session.
    pub standard: PublishStandard,
    /// What the dialog is showing.
    pub phase: PublishPhase,
    /// A quiet status line, e.g. when "Show in folder" fails.
    pub status: Option<String>,
}

impl Editor {
    /// Open the Publish dialog and refresh its pre-flight checks.
    pub fn open_publish_dialog(&mut self) {
        if !self.file_ops_available {
            return;
        }
        self.ui.publish.open = true;
        self.ui.publish.status = None;
        if !matches!(self.ui.publish.phase, PublishPhase::Busy) {
            self.ui.publish.phase = PublishPhase::Idle;
        }
        self.refresh_validation();
    }

    /// Show that a publish is running.
    pub fn set_publish_busy(&mut self) {
        self.ui.publish.open = true;
        self.ui.publish.phase = PublishPhase::Busy;
        self.ui.publish.status = None;
    }

    /// Show a successful publish.
    pub fn set_publish_success(&mut self, file_name: impl Into<String>, bytes: u64) {
        self.ui.publish.open = true;
        self.ui.publish.phase = PublishPhase::Success {
            file_name: file_name.into(),
            bytes,
        };
        self.ui.publish.status = None;
    }

    /// Show a failed publish and keep the dialog open for a retry.
    pub fn set_publish_failure(&mut self, message: impl Into<String>) {
        self.ui.publish.open = true;
        self.ui.publish.phase = PublishPhase::Failed {
            message: message.into(),
        };
        self.ui.publish.status = None;
    }

    /// Set or clear the quiet status line under the result.
    pub fn set_publish_status(&mut self, message: Option<String>) {
        self.ui.publish.status = message;
    }

    /// Configure whether this build can publish (it has a bundled player).
    pub fn set_publish_available(&mut self, available: bool) {
        self.publish_available = available;
    }
}

/// Draw the Publish dialog when it is open.
pub fn show_publish_dialog(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.publish.open {
        return;
    }

    let busy = matches!(editor.ui.publish.phase, PublishPhase::Busy);
    let success = matches!(editor.ui.publish.phase, PublishPhase::Success { .. });
    let mut open = true;
    let mut close_after = false;

    let mut dialog = Dialog::new("Publish course").width(430.0);
    if !busy {
        dialog = dialog.open(&mut open);
    }
    dialog.show_with_footer(ctx, |dialog| {
        let p = palette(ctx);
        let validation_errors = editor.ui.validation_errors;
        let available = editor.publish_available;
        let missing_media = editor.missing_media();
        dialog.body(|ui| {
            ui.set_max_width(430.0);
            ui.label(
                small(editor.session.course.title.clone())
                    .size(SIZE_BODY)
                    .color(p.text_secondary),
            );
            ui.add_space(SPACE_MD);

            if success {
                show_success_details(editor, ui);
                return;
            }

            show_standards(editor, ui, busy);

            let warnings = editor.ui.validation_warnings;
            let audit_errors = editor
                .ui
                .audit_findings
                .iter()
                .filter(|f| f.severity == "error")
                .count();

            if validation_errors > 0 {
                ui.add_space(SPACE_SM);
                ui.label(
                    RichText::new(format!(
                        "This course has {validation_errors} problem(s) to fix before publishing."
                    ))
                    .color(p.danger),
                );
                if ui.button("Show problems").clicked() {
                    editor.ui.show_validation_window = true;
                }
            } else if warnings > 0 {
                ui.add_space(SPACE_SM);
                ui.label(
                    small(format!("{warnings} warning(s). You can still publish."))
                        .color(p.text_secondary),
                );
            }

            if !missing_media.is_empty() {
                ui.add_space(SPACE_SM);
                for src in &missing_media {
                    ui.label(
                        RichText::new(format!("Missing media: {src}"))
                            .color(p.danger)
                            .size(SIZE_BODY),
                    );
                }
            }

            if audit_errors > 0 {
                ui.add_space(SPACE_SM);
                ui.horizontal(|ui| {
                    ui.label(
                        small(format!(
                            "{audit_errors} accessibility findings. You can still publish."
                        ))
                        .color(p.text_secondary),
                    );
                    if ui.button("Show findings").clicked() {
                        editor.ui.show_validation_window = true;
                    }
                });
            }

            if !available {
                ui.add_space(SPACE_SM);
                ui.label(
                    small(
                        "This build doesn't include the player. Run cargo xtask build-player and rebuild.",
                    )
                    .color(p.danger),
                );
            }

            if let PublishPhase::Failed { ref message } = editor.ui.publish.phase {
                ui.add_space(SPACE_SM);
                ui.label(RichText::new(message.clone()).color(p.danger));
            }

            if let Some(ref status) = editor.ui.publish.status {
                ui.add_space(SPACE_SM);
                ui.label(small(status.clone()).color(p.danger));
            }
        });

        // The action buttons stay put below the scrolling body.
        dialog.footer(|ui| {
            ui.add_space(SPACE_SM);
            if success {
                show_success_buttons(editor, ui, requests, &mut close_after);
            } else if busy {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label("Publishing…");
                });
            } else {
                ui.horizontal(|ui| {
                    let blocked = validation_errors > 0 || !available || !missing_media.is_empty();
                    if blocked {
                        ui.add_enabled(false, egui::Button::new("Publish"));
                    } else if publish_button(ui, "Publish").clicked() {
                        editor.ui.publish.phase = PublishPhase::Idle;
                        editor.ui.publish.status = None;
                        requests.push(EditorRequest::Publish {
                            standard: editor.ui.publish.standard,
                        });
                    }
                    if ui.button("Cancel").clicked() {
                        close_after = true;
                    }
                });
            }
        });
    });

    if close_after || (!busy && !open) {
        editor.ui.publish.open = false;
    }
}

fn show_standards(editor: &mut Editor, ui: &mut egui::Ui, busy: bool) {
    let p = palette(ui.ctx());
    for standard in PublishStandard::ALL {
        let selected = editor.ui.publish.standard == standard;
        if ui
            .add_enabled(!busy, egui::RadioButton::new(selected, standard.label()))
            .clicked()
        {
            editor.ui.publish.standard = standard;
        }
        ui.add_space(2.0);
        ui.label(small(standard.help()).color(p.text_secondary));
        ui.add_space(SPACE_SM);
    }
    ui.add_space(SPACE_SM);
}

fn show_success_details(editor: &mut Editor, ui: &mut egui::Ui) {
    let p = palette(ui.ctx());
    let PublishPhase::Success {
        ref file_name,
        bytes,
    } = editor.ui.publish.phase
    else {
        return;
    };

    ui.label(
        RichText::new(format!("Published to {file_name}"))
            .size(SIZE_BODY)
            .strong()
            .color(p.text),
    );
    ui.add_space(SPACE_SM);
    let megabytes = bytes as f64 / (1024.0 * 1024.0);
    ui.label(small(format!("{megabytes:.1} MB")).color(p.text_secondary));

    if let Some(ref status) = editor.ui.publish.status {
        ui.add_space(SPACE_SM);
        ui.label(small(status.clone()).color(p.danger));
    }

    ui.add_space(SPACE_SM);
    ui.label(small(editor.ui.publish.standard.upload_help()).color(p.text_secondary));
}

fn show_success_buttons(
    _editor: &mut Editor,
    ui: &mut egui::Ui,
    requests: &mut Vec<EditorRequest>,
    close_after: &mut bool,
) {
    ui.horizontal(|ui| {
        if ui.button("Show in folder").clicked() {
            requests.push(EditorRequest::ShowPublished);
        }
        if publish_button(ui, "Done").clicked() {
            *close_after = true;
        }
    });
}

fn publish_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = palette(ui.ctx());
    let button = egui::Button::new(RichText::new(label).color(Color32::WHITE))
        .fill(p.accent)
        .corner_radius(RADIUS_MD as u8)
        .min_size(egui::vec2(0.0, CONTROL_PRIMARY));
    ui.add(button)
}
