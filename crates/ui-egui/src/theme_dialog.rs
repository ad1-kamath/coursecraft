//! Theme window dialog: accent, background, text color, player bar colors, font name.

use crate::EditorRequest;
use crate::dialog::Dialog;
use crate::font_picker::{FontPickerAction, show_font_picker};
use crate::properties::{is_committed, show_color_field};
use coursecraft_course::{Course, Theme};

/// Local buffers for editing the Course Theme.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThemeDialogState {
    pub last_synced_revision: crate::SyncRevision,
    pub accent: String,
    pub background: String,
    pub text: String,
    pub font: String,
    pub bar: String,
    pub bar_text: String,
    /// The Create New Theme Fonts dialog is open.
    pub fonts_open: bool,
    /// The Theme window opened that dialog, so closing the Theme window closes it too.
    pub fonts_from_theme_window: bool,
    /// Heading and body font chosen in that dialog, not yet saved.
    pub fonts_heading: String,
    pub fonts_body: String,
}

/// The most course fonts the Design › Fonts ▾ built-in pairs draw on.
const MAX_PAIR_FONTS: usize = 4;

/// The built-in `(heading, body)` pairs Design › Fonts ▾ lists: Inter for both,
/// then each course font as heading over Inter and as body under Inter.
#[must_use]
pub fn builtin_font_pairs(course_fonts: &[&str]) -> Vec<(String, String)> {
    let mut pairs = vec![("Inter".to_owned(), "Inter".to_owned())];
    for font in course_fonts
        .iter()
        .filter(|f| !f.eq_ignore_ascii_case("Inter") && !f.eq_ignore_ascii_case("Open Sans"))
        .take(MAX_PAIR_FONTS)
    {
        for pair in [
            ((*font).to_owned(), "Inter".to_owned()),
            ("Inter".to_owned(), (*font).to_owned()),
        ] {
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
    }
    pairs
}

/// The `course.set_theme` parameters that set one heading and body pair in a
/// single command. A heading equal to the body clears `heading_font`.
#[must_use]
pub fn font_pair_params(heading: &str, body: &str) -> serde_json::Value {
    let heading = heading.trim();
    let body = body.trim();
    if heading.is_empty() || heading.eq_ignore_ascii_case(body) {
        serde_json::json!({ "font": body, "heading_font": serde_json::Value::Null })
    } else {
        serde_json::json!({ "font": body, "heading_font": heading })
    }
}

/// Open the Create New Theme Fonts dialog with the theme's current fonts.
pub fn open_theme_fonts_dialog(editor: &mut crate::Editor) {
    let theme = &editor.session.course.theme;
    editor.ui.theme_dialog.fonts_heading = theme.heading_font_or_body().to_owned();
    editor.ui.theme_dialog.fonts_body = theme.font.clone();
    editor.ui.theme_dialog.fonts_open = true;
    editor.ui.theme_dialog.fonts_from_theme_window = false;
}

/// Design › Fonts ▾: the built-in pairs and Create New Theme Fonts….
pub fn theme_fonts_menu(editor: &mut crate::Editor, ui: &mut egui::Ui) {
    let families: Vec<String> = editor
        .session
        .course
        .font_families()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let refs: Vec<&str> = families.iter().map(String::as_str).collect();
    let theme = &editor.session.course.theme;
    let (cur_h, cur_b) = (theme.heading_font_or_body().to_owned(), theme.font.clone());
    let mut chosen: Option<(String, String)> = None;
    for (h, b) in builtin_font_pairs(&refs) {
        let on = h.eq_ignore_ascii_case(&cur_h) && b.eq_ignore_ascii_case(&cur_b);
        let label = format!("{h} / {b}");
        if ui.add(egui::Button::selectable(on, label)).clicked() {
            chosen = Some((h, b));
        }
    }
    ui.separator();
    if ui.button(tl!("Create New Theme Fonts…")).clicked() {
        open_theme_fonts_dialog(editor);
        ui.close();
    }
    if let Some((h, b)) = chosen {
        editor.run("course.set_theme", font_pair_params(&h, &b));
        ui.close();
    }
}

/// The Create New Theme Fonts dialog: heading and body pickers, a live sample,
/// and Save, which sets both fonts in one `course.set_theme`.
pub fn show_theme_fonts_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.theme_dialog.fonts_open {
        return;
    }
    editor.ui.properties_lists.ensure(
        editor.session.revision,
        &editor.session.course,
        &editor.media,
    );
    let mut open = true;
    let mut close = false;
    let mut save = false;
    Dialog::new(tl!("Create New Theme Fonts"))
        .id(egui::Id::unique("theme_fonts_dialog"))
        .open(&mut open)
        .width(420.0)
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                for heading in [true, false] {
                    let label = if heading {
                        tl!("Heading font")
                    } else {
                        tl!("Body font")
                    };
                    let current = if heading {
                        editor.ui.theme_dialog.fonts_heading.clone()
                    } else {
                        editor.ui.theme_dialog.fonts_body.clone()
                    };
                    let state = &mut editor.ui;
                    let picked = crate::properties::inspector_row(ui, label, |ui| {
                        show_font_picker(
                            ui,
                            if heading {
                                "theme_fonts_heading"
                            } else {
                                "theme_fonts_body"
                            },
                            &current,
                            &state.properties_lists.font_families,
                            &mut state.properties.recent_fonts,
                            state.system_fonts.as_deref(),
                            &mut state.properties.system_fonts_list_requested,
                            &mut state.properties.system_font_search,
                            &mut editor.pending_requests,
                        )
                    });
                    let name = match picked {
                        Some(FontPickerAction::PickCourseOrRecent(name)) => Some(name),
                        Some(FontPickerAction::PickSystem(name)) => {
                            editor.pending_requests.push(EditorRequest::AddSystemFont {
                                family: name.clone(),
                                apply: None,
                            });
                            Some(name)
                        }
                        Some(FontPickerAction::AddFont) => {
                            editor.pending_requests.push(EditorRequest::PickFonts);
                            None
                        }
                        None => None,
                    };
                    if let Some(name) = name {
                        if heading {
                            editor.ui.theme_dialog.fonts_heading = name;
                        } else {
                            editor.ui.theme_dialog.fonts_body = name;
                        }
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                let p = coursecraft_render::palette(ui.ctx());
                let heading_font = coursecraft_render::slide_font_named(
                    ui.ctx(),
                    Some(&editor.ui.theme_dialog.fonts_heading),
                    true,
                    false,
                    24.0,
                );
                let body_font = coursecraft_render::slide_font_named(
                    ui.ctx(),
                    Some(&editor.ui.theme_dialog.fonts_body),
                    false,
                    false,
                    15.0,
                );
                ui.label(
                    egui::RichText::new(tl!("Heading"))
                        .font(heading_font)
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(tl!("Body text sample"))
                        .font(body_font)
                        .color(p.text),
                );
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    if crate::primary_button(ui, tl!("Save")).clicked() {
                        save = true;
                    }
                    if ui.button(tl!("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    if save {
        let params = font_pair_params(
            &editor.ui.theme_dialog.fonts_heading,
            &editor.ui.theme_dialog.fonts_body,
        );
        editor.run("course.set_theme", params);
        close = true;
    }
    if close || !open {
        editor.ui.theme_dialog.fonts_open = false;
    }
}

impl ThemeDialogState {
    /// Resync state buffers from course data on revision change.
    pub fn sync(&mut self, course: &Course, revision: u64) {
        if self.last_synced_revision.is(revision) {
            return;
        }
        self.last_synced_revision.set(revision);
        self.accent = course.theme.accent.0.clone();
        self.background = course.theme.background.0.clone();
        self.text = course.theme.text.0.clone();
        self.font = course.theme.font.clone();
        self.bar = course.theme.player.bar.0.clone();
        self.bar_text = course.theme.player.bar_text.0.clone();
    }
}

/// Render the Theme modal window dialog.
pub fn show_theme_dialog_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if editor.ui.theme_dialog.fonts_from_theme_window && !editor.ui.show_theme_window {
        // A child window never outlives the Theme window that opened it.
        editor.ui.theme_dialog.fonts_open = false;
        editor.ui.theme_dialog.fonts_from_theme_window = false;
    }
    show_theme_fonts_window(editor, ctx);
    if !editor.ui.show_theme_window {
        return;
    }

    editor
        .ui
        .theme_dialog
        .sync(&editor.session.course, editor.session.revision);
    editor.ui.properties_lists.ensure(
        editor.session.revision,
        &editor.session.course,
        &editor.media,
    );

    let mut open = true;
    let mut close = false;
    let mut update_theme: Option<serde_json::Value> = None;
    let mut palette_actions = Vec::new();
    let mut open_fonts = false;

    // The id stays English, so a changed language keeps the window where it was.
    Dialog::new(tl!("Theme"))
        .id(egui::Id::unique("theme_dialog"))
        .open(&mut open)
        .width(420.0)
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                ui.heading(tl!("Colors"));

                let (cr_acc, valid_acc) = show_color_field(
                    ui,
                    tl!("Accent"),
                    &mut editor.ui.theme_dialog.accent,
                    "theme_accent",
                );
                if is_committed(&cr_acc, ui)
                    && valid_acc
                    && editor.ui.theme_dialog.accent.trim() != editor.session.course.theme.accent.0
                {
                    update_theme = Some(serde_json::json!({
                        "accent": editor.ui.theme_dialog.accent.trim(),
                    }));
                }

                let (cr_bg, valid_bg) = show_color_field(
                    ui,
                    tl!("Slide background"),
                    &mut editor.ui.theme_dialog.background,
                    "theme_bg",
                );
                if is_committed(&cr_bg, ui)
                    && valid_bg
                    && editor.ui.theme_dialog.background.trim()
                        != editor.session.course.theme.background.0
                {
                    update_theme = Some(serde_json::json!({
                        "background": editor.ui.theme_dialog.background.trim(),
                    }));
                }

                let (cr_txt, valid_txt) = show_color_field(
                    ui,
                    tl!("Text color"),
                    &mut editor.ui.theme_dialog.text,
                    "theme_text",
                );
                if is_committed(&cr_txt, ui)
                    && valid_txt
                    && editor.ui.theme_dialog.text.trim() != editor.session.course.theme.text.0
                {
                    update_theme = Some(serde_json::json!({
                        "text": editor.ui.theme_dialog.text.trim(),
                    }));
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading(tl!("Player chrome"));

                let (cr_bar, valid_bar) = show_color_field(
                    ui,
                    tl!("Bar fill"),
                    &mut editor.ui.theme_dialog.bar,
                    "theme_bar",
                );
                if is_committed(&cr_bar, ui)
                    && valid_bar
                    && editor.ui.theme_dialog.bar.trim() != editor.session.course.theme.player.bar.0
                {
                    update_theme = Some(serde_json::json!({
                        "bar": editor.ui.theme_dialog.bar.trim(),
                    }));
                }

                let (cr_bar_txt, valid_bar_txt) = show_color_field(
                    ui,
                    tl!("Bar text"),
                    &mut editor.ui.theme_dialog.bar_text,
                    "theme_bar_text",
                );
                if is_committed(&cr_bar_txt, ui)
                    && valid_bar_txt
                    && editor.ui.theme_dialog.bar_text.trim()
                        != editor.session.course.theme.player.bar_text.0
                {
                    update_theme = Some(serde_json::json!({
                        "bar_text": editor.ui.theme_dialog.bar_text.trim(),
                    }));
                }

                ui.add_space(8.0);
                ui.separator();
                palette_actions =
                    crate::text_styles_ui::show_palette_section(ui, &editor.session.course);

                ui.add_space(8.0);
                ui.separator();
                ui.heading(tl!("Typography"));

                let state = &mut editor.ui;
                let picked = crate::properties::inspector_row(ui, tl!("Font name"), |ui| {
                    show_font_picker(
                        ui,
                        "theme_font",
                        &state.theme_dialog.font,
                        &state.properties_lists.font_families,
                        &mut state.properties.recent_fonts,
                        state.system_fonts.as_deref(),
                        &mut state.properties.system_fonts_list_requested,
                        &mut state.properties.system_font_search,
                        &mut editor.pending_requests,
                    )
                });
                if ui.button(tl!("Theme fonts…")).clicked() {
                    open_fonts = true;
                }
                match picked {
                    Some(FontPickerAction::PickCourseOrRecent(name))
                        if state.theme_dialog.font != name =>
                    {
                        state.theme_dialog.font = name;
                        update_theme = Some(serde_json::json!({ "font": state.theme_dialog.font }));
                    }
                    Some(FontPickerAction::PickSystem(name)) => {
                        // The host applies the command only after embedding succeeds.
                        // The dialog then picks up the new font on revision sync.
                        editor.pending_requests.push(EditorRequest::AddSystemFont {
                            family: name.clone(),
                            apply: Some((
                                "course.set_theme".to_owned(),
                                serde_json::json!({ "font": name }),
                            )),
                        });
                    }
                    Some(FontPickerAction::AddFont) => {
                        editor.pending_requests.push(EditorRequest::PickFonts);
                    }
                    Some(FontPickerAction::PickCourseOrRecent(_)) | None => {}
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    if ui.button(tl!("Reset to defaults")).clicked() {
                        let def = Theme::default();
                        editor.ui.theme_dialog.accent = def.accent.0.clone();
                        editor.ui.theme_dialog.background = def.background.0.clone();
                        editor.ui.theme_dialog.text = def.text.0.clone();
                        editor.ui.theme_dialog.font = def.font.clone();
                        editor.ui.theme_dialog.bar = def.player.bar.0.clone();
                        editor.ui.theme_dialog.bar_text = def.player.bar_text.0.clone();
                        update_theme = Some(serde_json::json!({
                            "accent": def.accent.0,
                            "background": def.background.0,
                            "text": def.text.0,
                            "font": def.font,
                            "heading_font": serde_json::Value::Null,
                            "bar": def.player.bar.0,
                            "bar_text": def.player.bar_text.0,
                        }));
                    }

                    if crate::primary_button(ui, tl!("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if open_fonts {
        open_theme_fonts_dialog(editor);
        editor.ui.theme_dialog.fonts_from_theme_window = true;
    }
    if let Some(params) = update_theme {
        editor.run("course.set_theme", params);
    }

    for (id, params) in palette_actions {
        editor.run(id, params);
    }

    if close || !open {
        editor.ui.show_theme_window = false;
        // Draft edits that were never applied must not come back on the next open (`N103`).
        editor.ui.theme_dialog.last_synced_revision = crate::SyncRevision::default();
    }
}
