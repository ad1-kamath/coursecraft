//! File > Version History: a list of earlier saves of the open project, each
//! opened as a copy or deleted. The app owns the files; this window only shows
//! the rows it is given and asks for an action through an [`EditorRequest`].

use crate::dialog::Dialog;
use egui::{Button, Grid, ScrollArea};

use crate::{Editor, EditorRequest};
use coursecraft_render::design;

/// One earlier save of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    /// Names the version to the app. Not shown.
    pub id: String,
    /// Date and time, such as `2026-11-11 15:30:45 UTC`.
    pub when: String,
    /// Size, such as `1.2 MB`.
    pub size: String,
    /// Slide count, or `None` when the version cannot be read.
    pub slides: Option<usize>,
}

/// What the window shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionHistoryState {
    pub open: bool,
    /// Whether the project has a file on disk. Without one there is no history.
    pub has_file: bool,
    /// The project is a folder kept in git, which is its own history. No
    /// versions are kept for it, and the window says so.
    pub folder_project: bool,
    /// Newest first.
    pub rows: Vec<VersionRow>,
}

impl Editor {
    /// Tell the editor whether the open project has a file on disk, which
    /// enables File > Version History.
    pub fn set_project_has_file(&mut self, has_file: bool) {
        self.ui.versions.has_file = has_file;
    }

    /// Tell the editor the open project is a folder for git, so the Version
    /// History window explains that git is the history.
    pub fn set_project_is_folder(&mut self, folder: bool) {
        self.ui.versions.folder_project = folder;
    }

    /// Show the window with these rows, newest first.
    pub fn show_version_history(&mut self, rows: Vec<VersionRow>) {
        self.ui.versions.rows = rows;
        self.ui.versions.open = true;
    }

    /// Replace the rows of an open window, after a delete.
    pub fn set_version_rows(&mut self, rows: Vec<VersionRow>) {
        self.ui.versions.rows = rows;
    }
}

/// Show the window when open.
pub fn show_version_history_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.versions.open {
        return;
    }
    let mut open = true;
    Dialog::new("Version History")
        .open(&mut open)
        .resizable(true)
        .width(460.0)
        .show(ctx, |ui| {
            if editor.ui.versions.folder_project {
                ui.label(
                    "This project is a folder, so git is its history. CourseCraft keeps no \
                     versions of it. Use git to see or restore earlier states.",
                );
                return;
            }
            let rows = editor.ui.versions.rows.clone();
            if rows.is_empty() {
                ui.label("No earlier versions yet. A version is kept each time you save over an existing file.");
                return;
            }
            ui.label(design::caption(
                "Each save keeps the file it replaced. Open as Copy never changes your project.",
            ));
            ui.add_space(design::SPACE_MD);
            ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                Grid::new("version_history_rows")
                    .num_columns(4)
                    .spacing([design::SPACE_XL, design::SPACE_MD])
                    .show(ui, |ui| {
                        for row in &rows {
                            ui.label(&row.when);
                            ui.label(&row.size);
                            match row.slides {
                                Some(1) => ui.label("1 slide"),
                                Some(n) => ui.label(format!("{n} slides")),
                                None => ui.colored_label(ui.visuals().error_fg_color, "Damaged"),
                            };
                            ui.horizontal(|ui| {
                                if row.slides.is_some()
                                    && ui
                                        .add(Button::new("Open as Copy"))
                                        .on_hover_text(format!(
                                            "Open the version from {} as a new copy",
                                            row.when
                                        ))
                                        .clicked()
                                {
                                    requests
                                        .push(EditorRequest::OpenVersionCopy { id: row.id.clone() });
                                }
                                if ui
                                    .add(Button::new("Delete"))
                                    .on_hover_text(format!("Delete the version from {}", row.when))
                                    .clicked()
                                {
                                    requests
                                        .push(EditorRequest::DeleteVersion { id: row.id.clone() });
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        });
    if !open {
        editor.ui.versions.open = false;
    }
}
