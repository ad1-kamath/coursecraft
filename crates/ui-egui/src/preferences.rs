//! Preferences window: autosave interval, appearance and the default slide size.
//!
//! The window holds a view of the saved settings. Every change applies at once
//! and goes to the app as an [`EditorRequest`], which saves it.

use coursecraft_render::ThemePref;
use egui::{DragValue, Window};

use crate::slide_size_dialog::PRESETS;
use crate::{Editor, EditorRequest};

/// Autosave interval bounds in minutes. 0 turns autosave off.
pub const MAX_AUTOSAVE_MINUTES: u32 = 60;

/// Most file versions a project can keep. 0 turns versions off.
pub const MAX_VERSIONS_TO_KEEP: u32 = 100;

/// What the window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsState {
    pub open: bool,
    pub autosave_minutes: u32,
    /// How many earlier saves of a project to keep. 0 keeps none.
    pub versions_to_keep: u32,
    pub default_slide: (u32, u32),
}

impl Default for PrefsState {
    fn default() -> Self {
        Self {
            open: false,
            autosave_minutes: 1,
            versions_to_keep: 10,
            default_slide: (PRESETS[0].1, PRESETS[0].2),
        }
    }
}

impl Editor {
    /// Show the saved settings in the window.
    pub fn set_preferences(&mut self, autosave_minutes: u32, default_slide: (u32, u32)) {
        self.ui.prefs.autosave_minutes = autosave_minutes.min(MAX_AUTOSAVE_MINUTES);
        self.ui.prefs.default_slide = if PRESETS.iter().any(|p| (p.1, p.2) == default_slide) {
            default_slide
        } else {
            (PRESETS[0].1, PRESETS[0].2)
        };
    }

    /// Show the saved number of versions to keep in the window.
    pub fn set_versions_to_keep(&mut self, versions: u32) {
        self.ui.prefs.versions_to_keep = versions.min(MAX_VERSIONS_TO_KEEP);
    }

    /// Open the Preferences window.
    pub fn open_preferences(&mut self) {
        self.ui.prefs.open = true;
    }
}

/// Show the window when open.
pub fn show_preferences_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.prefs.open {
        return;
    }
    let mut open = true;
    let before = editor.ui.prefs.clone();
    let mut theme = editor.theme_pref;
    Window::new("Preferences")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let prefs = &mut editor.ui.prefs;
            ui.horizontal(|ui| {
                ui.label("Autosave every");
                ui.add(
                    DragValue::new(&mut prefs.autosave_minutes)
                        .range(0..=MAX_AUTOSAVE_MINUTES)
                        .suffix(" min"),
                );
            });
            ui.small("0 turns autosave off.");
            ui.horizontal(|ui| {
                ui.label("Versions to keep");
                ui.add(
                    DragValue::new(&mut prefs.versions_to_keep)
                        .range(0..=MAX_VERSIONS_TO_KEEP)
                        .suffix(" per file"),
                );
            });
            ui.small("Each save keeps the file it replaced. 0 turns versions off.");
            ui.separator();
            ui.label("Appearance");
            for (pref, label) in [
                (ThemePref::System, "System"),
                (ThemePref::Light, "Light"),
                (ThemePref::Dark, "Dark"),
            ] {
                if ui.radio(theme == pref, label).clicked() {
                    theme = pref;
                }
            }
            ui.separator();
            ui.label("Default slide size for new courses");
            for (label, w, h) in PRESETS {
                if ui.radio(prefs.default_slide == (w, h), label).clicked() {
                    prefs.default_slide = (w, h);
                }
            }
        });
    if theme != editor.theme_pref {
        editor.theme_pref = theme;
        requests.push(EditorRequest::SetTheme(theme));
    }
    if editor.ui.prefs != before {
        requests.push(EditorRequest::SetPreferences {
            autosave_minutes: editor.ui.prefs.autosave_minutes,
            versions_to_keep: editor.ui.prefs.versions_to_keep,
            default_slide: editor.ui.prefs.default_slide,
        });
    }
    if !open {
        editor.ui.prefs.open = false;
    }
}
