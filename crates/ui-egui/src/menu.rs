//! Editor menu bar, command adapters, and keyboard shortcut handling.

use crate::{Editor, EditorRequest};
use coursecraft_engine::registry;
use egui::{Button, Key, KeyboardShortcut, MenuBar, Modifiers, Ui};
use serde_json::Value as Json;

/// Check if the proportional font has the given glyph.
#[must_use]
pub fn font_has_glyph(ctx: &egui::Context, c: char) -> bool {
    let font_id = egui::FontId::proportional(12.0);
    ctx.fonts_mut(|f| f.has_glyph(&font_id, c))
}

/// Platform-aware menu hint for a registry shortcut string (`⌘M` on macOS, `Ctrl+M` elsewhere).
/// Falls back to the registry string if it cannot be parsed.
#[must_use]
pub fn shortcut_label(ctx: &egui::Context, shortcut: &str) -> String {
    let is_mac = ctx.os().is_mac();
    format_shortcut(shortcut, is_mac, |c| font_has_glyph(ctx, c))
}

/// Format a shortcut string like `"Ctrl+Z"`, `"Ctrl+="`, `"Ctrl+-"`, `"Ctrl+Shift+Z"`, `"Delete"`
/// for the specified platform (`is_mac`) and glyph availability.
///
/// On macOS: ⌘Z, ⌘=, ⌘−, ⇧⌘Z, ⌫ for Delete, Esc.
/// Elsewhere: Ctrl+Z, Ctrl+=, Ctrl+−, Delete.
///
/// If glyphs are missing according to `has_glyph`, falls back to text ("Cmd", "Shift", "Delete", "-").
#[must_use]
pub fn format_shortcut(shortcut: &str, is_mac: bool, has_glyph: impl Fn(char) -> bool) -> String {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed == "?" {
        return "?".to_owned();
    }
    if trimmed.eq_ignore_ascii_case("escape") || trimmed.eq_ignore_ascii_case("esc") {
        return "Esc".to_owned();
    }
    if trimmed.eq_ignore_ascii_case("delete") || trimmed.eq_ignore_ascii_case("del") {
        if is_mac {
            return if has_glyph('⌫') {
                "⌫".to_owned()
            } else {
                "Delete".to_owned()
            };
        }
        return "Delete".to_owned();
    }

    let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
    if parts.is_empty() {
        return trimmed.to_owned();
    }

    if parts.len() == 1 {
        return trimmed.to_owned();
    }

    let (&key_part, mod_parts) = match parts.split_last() {
        Some(pair) => pair,
        None => return trimmed.to_owned(),
    };

    let mut has_cmd = false;
    let mut has_shift = false;
    let mut has_alt = false;
    for &m in mod_parts {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => has_cmd = true,
            "shift" => has_shift = true,
            "alt" | "opt" | "option" => has_alt = true,
            _ => return trimmed.to_owned(),
        }
    }

    let key_str = match key_part.to_ascii_lowercase().as_str() {
        "equals" | "=" => "=".to_owned(),
        "minus" | "-" | "−" => {
            if has_glyph('−') {
                "−".to_owned()
            } else {
                "-".to_owned()
            }
        }
        "delete" | "del" => {
            if is_mac && has_glyph('⌫') {
                "⌫".to_owned()
            } else {
                "Delete".to_owned()
            }
        }
        "escape" | "esc" => "Esc".to_owned(),
        _ => {
            if key_part.len() == 1 {
                key_part.to_ascii_uppercase()
            } else {
                return trimmed.to_owned();
            }
        }
    };

    if is_mac {
        let can_use_cmd = has_glyph('⌘');
        let can_use_shift = has_glyph('⇧');
        let can_use_alt = has_glyph('⌥');

        if can_use_cmd {
            let mut res = String::new();
            if has_shift {
                if can_use_shift {
                    res.push('⇧');
                } else {
                    res.push_str("Shift+");
                }
            }
            if has_alt {
                if can_use_alt {
                    res.push('⌥');
                } else {
                    res.push_str("Opt+");
                }
            }
            if has_cmd {
                res.push('⌘');
            }
            res.push_str(&key_str);
            res
        } else {
            let mut mods = Vec::new();
            if has_shift {
                mods.push("Shift");
            }
            if has_alt {
                mods.push("Opt");
            }
            if has_cmd {
                mods.push("Cmd");
            }
            mods.push(&key_str);
            mods.join("+")
        }
    } else {
        let mut mods = Vec::new();
        if has_cmd {
            mods.push("Ctrl");
        }
        if has_alt {
            mods.push("Alt");
        }
        if has_shift {
            mods.push("Shift");
        }
        mods.push(&key_str);
        mods.join("+")
    }
}

/// Parse a shortcut string such as `"Ctrl+Z"`, `"Ctrl+Y"`, `"Ctrl+M"`, `"Delete"`.
///
/// On macOS, `"Ctrl"` maps to `egui::Modifiers::COMMAND`.
#[must_use]
pub fn parse_shortcut(s: &str) -> Option<KeyboardShortcut> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    if parts.is_empty() {
        return None;
    }
    let (&key_part, mod_parts) = parts.split_last()?;
    let mut modifiers = Modifiers::NONE;
    for &m in mod_parts {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => modifiers |= Modifiers::COMMAND,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "opt" | "option" => modifiers |= Modifiers::ALT,
            _ => return None,
        }
    }

    let key = match key_part.to_ascii_lowercase().as_str() {
        "delete" | "del" => Key::Delete,
        "backspace" => Key::Backspace,
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        _ => {
            if key_part.len() == 1 {
                let ch = key_part.chars().next()?;
                if ch.is_ascii_alphanumeric() {
                    let upper = key_part.to_ascii_uppercase();
                    Key::from_name(&upper)?
                } else {
                    Key::from_name(key_part)?
                }
            } else {
                Key::from_name(key_part)?
            }
        }
    };

    Some(KeyboardShortcut::new(modifiers, key))
}

/// Calculate reorder parameters for the currently selected object.
///
/// `delta > 0` brings forward (index + 1); `delta < 0` sends backward (index - 1).
#[must_use]
pub fn reorder_params(editor: &Editor, delta: isize) -> Option<Json> {
    let slide_id = &editor.selection.slide;
    if slide_id.is_empty() {
        return None;
    }
    let obj_id = editor.selection.object.as_ref()?;
    let slide = editor.session.course.slide(slide_id)?;

    let (pos, len) = match editor.selection.layer.as_deref() {
        Some(lid) => {
            let layer = slide.layer(lid)?;
            let p = layer.objects.iter().position(|o| &o.id == obj_id)?;
            (p, layer.objects.len())
        }
        None => {
            let p = slide.objects.iter().position(|o| &o.id == obj_id)?;
            (p, slide.objects.len())
        }
    };

    if delta > 0 {
        if pos + 1 < len {
            Some(serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
                "index": pos + 1,
            }))
        } else {
            None
        }
    } else if pos > 0 {
        Some(serde_json::json!({
            "slide": slide_id,
            "object": obj_id,
            "index": pos.saturating_sub(1),
        }))
    } else {
        None
    }
}

/// How a registry command that appears in a menu is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuRoute {
    /// Build params from the current selection and run it.
    Action,
    /// Open Course Settings.
    OpenCourseSettings,
    /// Switch the bottom panel to Variables.
    OpenVariables,
    /// Open the New Trigger dialog.
    OpenTriggerDialog,
    /// Ask before deleting the current slide's scene.
    OpenSceneDelete,
    /// Bring/send the selected object.
    Reorder,
    /// Open the Find and Replace window.
    OpenFind,
    /// Open the Slides from Outline window.
    OpenOutline,
}

/// The route for every registry command that has a menu entry.
///
/// A `None` here for a command with a non-empty menu would mean a permanently
/// greyed-out menu item, which `menu_routes_cover_every_menu_command` rejects.
#[must_use]
pub fn menu_route(id: &str) -> Option<MenuRoute> {
    match id {
        "course.update" | "course.set_player" | "course.set_quiz" => {
            Some(MenuRoute::OpenCourseSettings)
        }
        "variable.add" => Some(MenuRoute::OpenVariables),
        "trigger.add" => Some(MenuRoute::OpenTriggerDialog),
        "scene.delete" => Some(MenuRoute::OpenSceneDelete),
        "object.reorder" => Some(MenuRoute::Reorder),
        "course.find" => Some(MenuRoute::OpenFind),
        "course.import_outline" => Some(MenuRoute::OpenOutline),
        "course.validate" | "edit.undo" | "edit.redo" | "scene.add" | "slide.add"
        | "slide.delete" | "layer.add" | "layer.delete" | "object.duplicate" | "object.delete"
        | "object.group" | "object.ungroup" | "question.insert" | "question.delete"
        | "results.insert" | "certificate.insert" => Some(MenuRoute::Action),
        _ => None,
    }
}

/// Whether the current selection includes an object that belongs to a group.
///
/// The canvas context menu, the Arrange menu and its disabled reason all use
/// this one rule, so Ungroup is never offered for a lone, ungrouped object.
#[must_use]
pub(crate) fn can_ungroup(editor: &Editor) -> bool {
    if editor.selection.slide.is_empty() {
        return false;
    }
    let ids = editor.selection.ids();
    if ids.is_empty() {
        return false;
    }
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return false;
    };
    ids.iter().any(|id| {
        slide
            .object(id)
            .is_some_and(|(object, _)| object.group.is_some())
    })
}

/// Build parameters for the given command id from the current editor selection.
///
/// Returns `None` if the command has no adapter, or if the current selection is invalid
/// for this command.
#[must_use]
pub fn adapter(editor: &Editor, id: &str) -> Option<Json> {
    if editor.ui.master_view
        && matches!(
            id,
            "slide.add"
                | "slide.delete"
                | "layer.add"
                | "layer.delete"
                | "question.insert"
                | "question.delete"
        )
    {
        return None;
    }
    match id {
        "edit.undo" => {
            if editor.session.can_undo() {
                Some(serde_json::json!({}))
            } else {
                None
            }
        }
        "edit.redo" => {
            if editor.session.can_redo() {
                Some(serde_json::json!({}))
            } else {
                None
            }
        }
        "course.validate" | "scene.add" => Some(serde_json::json!({})),
        "slide.add" => {
            let slide_id = &editor.selection.slide;
            if slide_id.is_empty() || editor.session.course.slide(slide_id).is_none() {
                return None;
            }
            let scene_id = editor.session.course.scenes.iter().find_map(|sc| {
                if sc.slides.iter().any(|s| &s.id == slide_id) {
                    Some(sc.id.clone())
                } else {
                    None
                }
            })?;
            Some(serde_json::json!({
                "scene": scene_id,
                "after": slide_id,
                "title": crate::toolbar::next_slide_title(&editor.session.course),
            }))
        }
        "slide.delete" => {
            let slide_id = &editor.selection.slide;
            if slide_id.is_empty() || editor.session.course.slide(slide_id).is_none() {
                return None;
            }
            Some(serde_json::json!({ "slide": slide_id }))
        }
        "layer.add" => {
            let slide_id = &editor.selection.slide;
            if slide_id.is_empty() || editor.session.course.slide(slide_id).is_none() {
                return None;
            }
            Some(serde_json::json!({ "slide": slide_id }))
        }
        "layer.delete" => {
            let slide_id = &editor.selection.slide;
            let layer_id = editor.selection.layer.as_ref()?;
            let slide = editor.session.course.slide(slide_id)?;
            slide.layer(layer_id)?;
            Some(serde_json::json!({
                "slide": slide_id,
                "layer": layer_id,
            }))
        }
        "question.insert" => {
            let slide_id = &editor.selection.slide;
            let slide = editor.session.course.slide(slide_id)?;
            if slide.question.is_some() {
                return None;
            }
            Some(serde_json::json!({
                "slide": slide_id,
                "kind": "multiple_choice",
            }))
        }
        "question.delete" => {
            let slide_id = &editor.selection.slide;
            let slide = editor.session.course.slide(slide_id)?;
            slide.question.as_ref()?;
            Some(serde_json::json!({ "slide": slide_id }))
        }
        "object.delete" => {
            let slide_id = &editor.selection.slide;
            let obj_id = editor.selection.object.as_ref()?;
            if slide_id.is_empty() || editor.session.course.slide(slide_id).is_none() {
                return None;
            }
            Some(serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
            }))
        }
        "object.duplicate" => {
            let slide_id = &editor.selection.slide;
            let obj_id = editor.selection.object.as_ref()?;
            if slide_id.is_empty() || editor.session.course.slide(slide_id).is_none() {
                return None;
            }
            Some(serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
            }))
        }
        "object.reorder" | "object.bring_forward" => reorder_params(editor, 1),
        "object.reorder_backward" | "object.send_backward" => reorder_params(editor, -1),
        "object.group" => {
            let slide = &editor.selection.slide;
            let ids = editor.selection.ids();
            if slide.is_empty() || ids.len() < 2 {
                return None;
            }
            editor.session.course.slide(slide)?;
            Some(serde_json::json!({
                "slide": slide,
                "objects": ids,
            }))
        }
        "object.ungroup" => {
            let slide_id = &editor.selection.slide;
            if !can_ungroup(editor) {
                return None;
            }
            Some(serde_json::json!({
                "slide": slide_id,
                "objects": editor.selection.ids(),
            }))
        }
        _ => None,
    }
}

/// Returns a human-readable explanation of why a command is disabled.
#[must_use]
pub fn adapter_reason(editor: &Editor, id: &str) -> &'static str {
    if editor.ui.master_view
        && matches!(
            id,
            "slide.add"
                | "slide.delete"
                | "layer.add"
                | "layer.delete"
                | "question.insert"
                | "question.delete"
        )
    {
        return "Not available in Master View";
    }
    match id {
        "edit.undo" => "Nothing to undo",
        "edit.redo" => "Nothing to redo",
        "slide.add" | "slide.delete" | "layer.add" => {
            if editor.selection.slide.is_empty() {
                "Select a slide first"
            } else {
                "Slide not found"
            }
        }
        "layer.delete" => {
            if editor.selection.layer.is_none() {
                "Select a layer first"
            } else {
                "Layer not found"
            }
        }
        "question.insert" => "This slide already has a question",
        "question.delete" => "This slide has no question",
        "scene.delete" => {
            if editor.selection.slide.is_empty() {
                "Select a slide first"
            } else if editor.session.course.scenes.len() <= 1 {
                "A course needs at least one scene"
            } else {
                "Scene not found"
            }
        }
        "object.delete" | "object.duplicate" => {
            if editor.selection.object.is_none() {
                "Select an object first"
            } else {
                "Object not found"
            }
        }
        "object.reorder" | "object.bring_forward" => {
            if editor.selection.object.is_none() {
                "Select an object first"
            } else if !can_reorder(editor, 1) {
                "Object is already at the front"
            } else {
                ""
            }
        }
        "object.reorder_backward" | "object.send_backward" => {
            if editor.selection.object.is_none() {
                "Select an object first"
            } else if !can_reorder(editor, -1) {
                "Object is already at the back"
            } else {
                ""
            }
        }
        "object.group" => {
            if editor.selection.object.is_none() {
                "Select an object first"
            } else {
                "Select at least two objects"
            }
        }
        "object.ungroup" => {
            if editor.selection.object.is_none() {
                "Select an object first"
            } else if !can_ungroup(editor) {
                "This object isn't in a group"
            } else {
                ""
            }
        }
        _ => "Command not yet implemented",
    }
}

/// Render the menu bar.
pub fn show_menu_bar(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let mut action_to_run: Option<(&'static str, Json)> = None;

    // Collect top-level categories from the command registry in appearance order.
    let mut categories: Vec<&'static str> = Vec::new();
    for cmd in registry() {
        if let Some((first, _)) = cmd.menu.split_once('/')
            && !categories.contains(&first)
        {
            categories.push(first);
        }
    }

    MenuBar::new().ui(ui, |ui| {
        // 1. File menu (always first, combining shell requests and any File/* commands)
        ui.menu_button("File", |ui| {
            if ui.button("New").clicked() {
                requests.push(EditorRequest::New);
                ui.close();
            }

            const FILE_OPS_DISABLED_MSG: &str =
                "Opening and saving files is desktop-only in this version";

            if editor.file_ops_available {
                if ui.button("Open…").clicked() {
                    requests.push(EditorRequest::Open);
                    ui.close();
                }
            } else {
                ui.add_enabled(false, Button::new("Open…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            file_op_item(
                ui,
                editor.file_ops_available,
                "Open Folder…",
                "Open a project folder, such as one kept in git",
                EditorRequest::OpenFolder,
                requests,
            );

            if editor.file_ops_available {
                ui.menu_button("Open Recent", |ui| {
                    if editor.recent_files.is_empty() {
                        ui.add_enabled(false, Button::new("(No recent files)"));
                    } else {
                        for (idx, full_path) in editor.recent_files.iter().enumerate() {
                            let name = std::path::Path::new(full_path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(full_path);
                            if ui.button(name).on_hover_text(full_path).clicked() {
                                requests.push(EditorRequest::OpenRecent(idx));
                                ui.close();
                            }
                        }
                    }
                });
            } else {
                ui.add_enabled(false, Button::new("Open Recent"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            ui.separator();

            if editor.file_ops_available {
                if ui.button("Save").clicked() {
                    requests.push(EditorRequest::Save);
                    ui.close();
                }
            } else {
                ui.add_enabled(false, Button::new("Save"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            if editor.file_ops_available {
                if ui.button("Save As…").clicked() {
                    requests.push(EditorRequest::SaveAs);
                    ui.close();
                }
            } else {
                ui.add_enabled(false, Button::new("Save As…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            file_op_item(
                ui,
                editor.file_ops_available,
                "Save As Project Folder (for git)…",
                "Save as a folder of plain files that git can track",
                EditorRequest::SaveAsFolder,
                requests,
            );

            let history_ready = editor.file_ops_available && editor.ui.versions.has_file;
            let history = ui.add_enabled(history_ready, Button::new("Version History…"));
            let history = if editor.file_ops_available {
                history.on_disabled_hover_text("Save the course first to start its history.")
            } else {
                history.on_disabled_hover_text(FILE_OPS_DISABLED_MSG)
            };
            if history.clicked() {
                requests.push(EditorRequest::ShowVersionHistory);
                ui.close();
            }

            ui.separator();

            if ui.button("Preferences…").clicked() {
                editor.open_preferences();
                ui.close();
            }
            if ui.button("Slide Size…").clicked() {
                editor.open_slide_size_dialog();
                ui.close();
            }

            if ui.button("Welcome screen").clicked() {
                editor.ui.show_welcome = true;
                ui.close();
            }

            ui.separator();

            if ui.button("Import PowerPoint…").clicked() {
                requests.push(EditorRequest::PickPptx);
                ui.close();
            }
            ui.menu_button("Import", |ui| {
                if ui.button("Questions from CSV…").clicked() {
                    requests.push(EditorRequest::ImportQuestionsCsv);
                    ui.close();
                }
            });

            if ui.button("Add font…").clicked() {
                requests.push(EditorRequest::PickFonts);
                ui.close();
            }
            if ui.button("Manage fonts…").clicked() {
                editor.ui.show_fonts_window = true;
                ui.close();
            }

            if editor.file_ops_available {
                if ui.button("Export Translation…").clicked() {
                    editor.ui.show_export_translation = true;
                    ui.close();
                }
                if ui.button("Import Translation…").clicked() {
                    requests.push(EditorRequest::ImportTranslation);
                    ui.close();
                }
            } else {
                ui.add_enabled(false, Button::new("Export Translation…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
                ui.add_enabled(false, Button::new("Import Translation…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            ui.separator();

            ui.menu_button("Export", |ui| {
                if ui.button("Video (MP4)…").clicked() {
                    editor.open_export_dialog();
                    ui.close();
                }
                if ui.button("PDF for review…").clicked() {
                    editor.open_export_review(crate::ReviewFormat::Pdf);
                    ui.close();
                }
                if ui.button("Word for review…").clicked() {
                    editor.open_export_review(crate::ReviewFormat::Word);
                    ui.close();
                }
            });

            ui.separator();

            if editor.file_ops_available {
                let btn =
                    Button::new("Publish…").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+Shift+P"));
                if ui.add(btn).clicked() {
                    editor.open_publish_dialog();
                    ui.close();
                }
            } else {
                ui.add_enabled(false, Button::new("Publish…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            ui.separator();

            // Registry items under File/
            for cmd in registry() {
                if let Some(sub) = cmd.menu.strip_prefix("File/") {
                    render_registry_menu_item(editor, ui, cmd, sub, &mut action_to_run);
                }
            }

            ui.separator();
            if ui.button("Quit").clicked() {
                requests.push(EditorRequest::Quit);
                ui.close();
            }
        });

        // 2. Other categories from the registry
        for category in categories {
            if category == "File" {
                continue;
            }
            if category == "Edit" {
                ui.menu_button("Edit", |ui| {
                    show_edit_menu(editor, ui, &mut action_to_run);
                });
                continue;
            }
            ui.menu_button(category, |ui| {
                render_category_menu_items(editor, ui, category, &mut action_to_run, requests);
            });
        }

        // View menu (Theme preference selection)
        ui.menu_button("View", |ui| {
            let mut masters_on = editor.ui.master_view;
            if ui.checkbox(&mut masters_on, "Slide Masters").changed() {
                editor.toggle_master_view();
                ui.close();
            }
            ui.checkbox(&mut editor.ui.show_media_library, "Media Library");
            ui.separator();
            ui.menu_button("Theme", |ui| {
                let current = editor.theme_pref;
                for (pref, label) in [
                    (coursecraft_render::ThemePref::Dark, "Dark"),
                    (coursecraft_render::ThemePref::Light, "Light"),
                    (coursecraft_render::ThemePref::System, "Match system"),
                ] {
                    if ui.radio(current == pref, label).clicked() {
                        editor.theme_pref = pref;
                        requests.push(EditorRequest::SetTheme(pref));
                        ui.close();
                    }
                }
            });
            ui.separator();
            ui.checkbox(&mut editor.ui.snap_to_guides, "Snap to Guides");
            ui.separator();
            ui.checkbox(&mut editor.ui.rulers.show_rulers, "Rulers");
            ui.checkbox(&mut editor.ui.rulers.show_grid, "Grid");
            ui.checkbox(&mut editor.ui.rulers.snap_to_grid, "Snap to Grid");
            ui.checkbox(&mut editor.ui.rulers.show_guides, "Guides");
        });

        // 3. Help menu
        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard shortcuts").clicked() {
                editor.ui.show_shortcuts_dialog = true;
                ui.close();
            }
            if editor.automation_available {
                ui.separator();
                let mut enabled = editor.automation_enabled;
                if ui.checkbox(&mut enabled, "Allow automation").changed() {
                    editor.automation_enabled = enabled;
                    requests.push(EditorRequest::SetAutomation(enabled));
                }
            }
            ui.separator();
            if ui.button("Connect an AI agent…").clicked() {
                editor.open_agent_settings();
                ui.close();
            }
            ui.separator();
            show_project_help_items(editor, ui);
        });
    });

    if let Some((cmd, params)) = action_to_run {
        editor.run(cmd, params);
    }
}

/// Help items that report a problem or open the project's pages on GitHub.
fn show_project_help_items(editor: &mut Editor, ui: &mut Ui) {
    use crate::project_links::{REPO_URL, releases_url};
    use crate::report::{open_feature_request, open_link};
    if ui.button("Report a Problem…").clicked() {
        editor.open_report();
        ui.close();
    }
    if ui.button("Suggest a Feature…").clicked() {
        open_feature_request(ui.ctx());
        ui.close();
    }
    if ui.button("CourseCraft on GitHub").clicked() {
        open_link(ui.ctx(), REPO_URL);
        ui.close();
    }
    if ui.button("Check for Updates").clicked() {
        open_link(ui.ctx(), &releases_url());
        ui.close();
    }
}

fn show_edit_menu(
    editor: &mut Editor,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, serde_json::Value)>,
) {
    for id in ["edit.undo", "edit.redo"] {
        if let Some(cmd) = registry().iter().find(|cmd| cmd.id == id) {
            render_registry_menu_item(editor, ui, cmd, cmd.label, action_to_run);
        }
    }
    ui.separator();

    let has_objects = editor.selection.object.is_some();
    let has_slide = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some();
    let navigator = editor.ui.last_active_area == crate::ActiveArea::Navigator;
    let master_view = editor.ui.master_view;
    let can_cut_slide = editor.session.course.slides().count() > 1;

    let cut_enabled =
        !master_view && ((navigator && has_slide && can_cut_slide) || (!navigator && has_objects));
    let cut = Button::new("Cut").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+X"));
    if ui
        .add_enabled(cut_enabled, cut)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else if navigator {
            "Can't cut the only slide"
        } else {
            "Select an object first"
        })
        .clicked()
    {
        crate::clipboard::cut(editor);
        ui.close();
    }

    let copy_enabled = !master_view && ((navigator && has_slide) || (!navigator && has_objects));
    let copy = Button::new("Copy").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+C"));
    if ui
        .add_enabled(copy_enabled, copy)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else {
            "Select something first"
        })
        .clicked()
    {
        crate::clipboard::copy(editor);
        ui.close();
    }

    let paste_enabled =
        !master_view && (editor.ui.clipboard.is_some() || editor.file_ops_available);
    let paste = Button::new("Paste").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+V"));
    if ui
        .add_enabled(paste_enabled, paste)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else {
            "Nothing to paste"
        })
        .clicked()
    {
        if editor.file_ops_available {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        } else {
            crate::clipboard::paste_private(editor);
        }
        ui.close();
    }

    let select_all = Button::new("Select All").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+A"));
    if ui.add(select_all).clicked() {
        editor.select_all_objects();
        ui.close();
    }

    ui.separator();

    for cmd in registry() {
        if let Some(label) = cmd.menu.strip_prefix("Edit/")
            && cmd.id != "edit.undo"
            && cmd.id != "edit.redo"
        {
            render_registry_menu_item(editor, ui, cmd, label, action_to_run);
        }
    }
}

fn render_category_menu_items(
    editor: &mut Editor,
    ui: &mut Ui,
    category: &str,
    action_to_run: &mut Option<(&'static str, serde_json::Value)>,
    requests: &mut Vec<EditorRequest>,
) {
    let prefix = format!("{category}/");
    for cmd in registry() {
        if let Some(label) = cmd.menu.strip_prefix(&prefix) {
            render_registry_menu_item(editor, ui, cmd, label, action_to_run);
        }
    }
    render_extra_category_items(editor, ui, category, requests, action_to_run);
}

fn can_reorder(editor: &Editor, delta: isize) -> bool {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return false;
    };
    let ids: Vec<&str> = match editor.selection.layer.as_deref() {
        Some(lid) => slide
            .layer(lid)
            .map(|layer| layer.objects.iter().map(|o| o.id.as_str()).collect())
            .unwrap_or_default(),
        None => slide.objects.iter().map(|o| o.id.as_str()).collect(),
    };
    let selected = editor.selection.ids();
    let selected: std::collections::BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    if selected.is_empty() {
        return false;
    }
    if delta > 0 {
        ids.iter().enumerate().any(|(i, id)| {
            selected.contains(id) && ids.get(i + 1).is_some_and(|n| !selected.contains(n))
        })
    } else {
        ids.iter().enumerate().any(|(i, id)| {
            selected.contains(id) && i > 0 && ids.get(i - 1).is_some_and(|p| !selected.contains(p))
        })
    }
}

fn render_reorder_menu_item(editor: &mut Editor, ui: &mut Ui) {
    let can_forward = can_reorder(editor, 1);
    let can_backward = can_reorder(editor, -1);
    let forward_reason = if editor.selection.object.is_none() {
        "Select an object first"
    } else {
        "Object is already at the front"
    };
    let backward_reason = if editor.selection.object.is_none() {
        "Select an object first"
    } else {
        "Object is already at the back"
    };

    if can_forward {
        if ui.button("Bring to front").clicked() {
            editor.reorder_selection_to(true);
            ui.close();
        }
        if ui.button("Bring forward").clicked() {
            editor.reorder_selection(1);
            ui.close();
        }
    } else {
        ui.add_enabled(false, Button::new("Bring to front"))
            .on_disabled_hover_text(forward_reason);
        ui.add_enabled(false, Button::new("Bring forward"))
            .on_disabled_hover_text(forward_reason);
    }

    if can_backward {
        if ui.button("Send backward").clicked() {
            editor.reorder_selection(-1);
            ui.close();
        }
        if ui.button("Send to back").clicked() {
            editor.reorder_selection_to(false);
            ui.close();
        }
    } else {
        ui.add_enabled(false, Button::new("Send backward"))
            .on_disabled_hover_text(backward_reason);
        ui.add_enabled(false, Button::new("Send to back"))
            .on_disabled_hover_text(backward_reason);
    }
}

/// Stacking and edit items for an object row's context menu.
///
/// The canvas and the timeline row menu share this path so their items,
/// shortcuts and disabled rules stay identical.
pub(crate) fn show_object_row_menu(editor: &mut Editor, ui: &mut Ui) {
    let has_selection = editor.selection.object.is_some();
    let master_view = editor.ui.master_view;

    let cut_enabled = has_selection && !master_view;
    let cut = Button::new("Cut").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+X"));
    if ui
        .add_enabled(cut_enabled, cut)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else {
            "Select an object first"
        })
        .clicked()
    {
        crate::clipboard::cut(editor);
        ui.close();
    }

    let copy_enabled = has_selection && !master_view;
    let copy = Button::new("Copy").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+C"));
    if ui
        .add_enabled(copy_enabled, copy)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else {
            "Select an object first"
        })
        .clicked()
    {
        crate::clipboard::copy(editor);
        ui.close();
    }

    let paste_enabled = paste_enabled(editor);
    let paste = Button::new("Paste").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+V"));
    if ui
        .add_enabled(paste_enabled, paste)
        .on_disabled_hover_text(if master_view {
            "Not available in Master View"
        } else {
            "Nothing copied yet"
        })
        .clicked()
    {
        crate::clipboard::paste_private(editor);
        ui.close();
    }

    let duplicate = Button::new("Duplicate").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+D"));
    if ui
        .add_enabled(has_selection, duplicate)
        .on_disabled_hover_text("Select an object first")
        .clicked()
    {
        editor.duplicate_selection();
        ui.close();
    }

    let delete = Button::new("Delete").shortcut_text(shortcut_label(ui.ctx(), "Delete"));
    if ui
        .add_enabled(has_selection, delete)
        .on_disabled_hover_text("Select an object first")
        .clicked()
    {
        editor.delete_selection();
        ui.close();
    }

    let path_selected = editor
        .selection
        .object
        .as_ref()
        .and_then(|id| {
            editor
                .session
                .course
                .slide(&editor.selection.slide)
                .and_then(|slide| slide.object(id))
        })
        .is_some_and(|(object, _)| {
            matches!(object.kind, coursecraft_course::ObjectKind::Path { .. })
        });
    if path_selected && ui.button("Edit points").clicked() {
        if let Some(id) = editor.selection.object.clone() {
            crate::canvas::start_path_edit(editor, &id);
        }
        ui.close();
    }

    ui.separator();
    render_group_ungroup_items(editor, ui, true);
    ui.separator();
    ui.menu_button("Order", |ui| {
        render_reorder_menu_item(editor, ui);
    });
    ui.menu_button("Align", |ui| {
        let mut action_to_run: Option<(&'static str, Json)> = None;
        render_align_actions(editor, ui, &mut action_to_run);
        ui.separator();
        ui.checkbox(&mut editor.ui.align_to_slide, "Align to Slide");
        if let Some((cmd, params)) = action_to_run {
            editor.run(cmd, params);
        }
    });
    ui.separator();

    let select_all = Button::new("Select all").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+A"));
    if ui.add(select_all).clicked() {
        editor.select_all_objects();
        ui.close();
    }
}

fn render_group_ungroup_items(editor: &mut Editor, ui: &mut Ui, shortcuts: bool) {
    let group_params = adapter(editor, "object.group");
    let group = if shortcuts {
        Button::new("Group").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+G"))
    } else {
        Button::new("Group")
    };
    if ui
        .add_enabled(group_params.is_some(), group)
        .on_disabled_hover_text("Select at least two objects")
        .clicked()
    {
        if let Some(params) = group_params {
            editor.run("object.group", params);
        }
        ui.close();
    }

    let ungroup_params = adapter(editor, "object.ungroup");
    let ungroup = if shortcuts {
        Button::new("Ungroup").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+Shift+G"))
    } else {
        Button::new("Ungroup")
    };
    if ui
        .add_enabled(ungroup_params.is_some(), ungroup)
        .on_disabled_hover_text("This object isn't in a group")
        .clicked()
    {
        if let Some(params) = ungroup_params {
            editor.run("object.ungroup", params);
        }
        ui.close();
    }
}

/// The canvas context menu used while a placement tool, crop or inline text
/// editing owns the canvas. It keeps the menu that existed before the fuller
/// object menu: group/ungroup, then stacking and duplicate/delete when an object
/// is selected, with no selection change on right-click.
pub(crate) fn show_canvas_special_menu(editor: &mut Editor, ui: &mut Ui) {
    render_group_ungroup_items(editor, ui, false);
    if editor.selection.object.is_some() {
        ui.separator();
        render_reorder_menu_item(editor, ui);
        ui.separator();
        if ui.button("Duplicate").clicked() {
            editor.duplicate_selection();
            ui.close();
        }
        if ui.button("Delete").clicked() {
            editor.delete_selection();
            ui.close();
        }
    }
}

/// Whether a menu Paste item can run: the private clipboard holds a payload and
/// Master View is not active.
#[must_use]
pub(crate) fn paste_enabled(editor: &Editor) -> bool {
    editor.ui.clipboard.is_some() && !editor.ui.master_view
}

/// Items for the canvas background context menu: paste and select all.
pub(crate) fn show_canvas_background_menu(editor: &mut Editor, ui: &mut Ui) {
    let can_paste = paste_enabled(editor);
    let paste = Button::new("Paste").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+V"));
    if ui
        .add_enabled(can_paste, paste)
        .on_disabled_hover_text(if editor.ui.master_view {
            "Not available in Master View"
        } else {
            "Nothing copied yet"
        })
        .clicked()
    {
        let point = editor.ui.canvas_context_pos;
        crate::clipboard::paste_private_at(editor, point);
        ui.close();
    }

    let select_all = Button::new("Select all").shortcut_text(shortcut_label(ui.ctx(), "Ctrl+A"));
    if ui.add(select_all).clicked() {
        editor.select_all_objects();
        ui.close();
    }
}

/// Commands that only open an editor window, so they dispatch no params.
fn render_open_window_menu_item(
    editor: &mut Editor,
    ui: &mut Ui,
    cmd: &'static coursecraft_engine::Command,
    label: &str,
) {
    let mut btn = Button::new(label);
    if let Some(sc) = cmd.shortcut {
        btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
    }
    if ui.add(btn).clicked() {
        match cmd.id {
            "course.find" => crate::find::open(editor),
            "course.import_outline" => editor.open_outline_dialog(),
            _ => editor.ui.show_theme_window = true,
        }
        ui.close();
    }
}

fn render_registry_menu_item(
    editor: &mut Editor,
    ui: &mut Ui,
    cmd: &'static coursecraft_engine::Command,
    label: &str,
    action_to_run: &mut Option<(&'static str, serde_json::Value)>,
) {
    if cmd.id == "object.reorder" {
        render_reorder_menu_item(editor, ui);
        return;
    }

    if matches!(
        cmd.id,
        "course.find" | "course.import_outline" | "course.set_theme"
    ) {
        render_open_window_menu_item(editor, ui, cmd, label);
        return;
    }

    if cmd.id == "object.duplicate" || cmd.id == "object.delete" {
        let enabled = editor.selection.object.is_some();
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui
            .add_enabled(enabled, btn)
            .on_disabled_hover_text("Select an object first")
            .clicked()
        {
            if cmd.id == "object.duplicate" {
                editor.duplicate_selection();
            } else {
                editor.delete_selection();
            }
            ui.close();
        }
        return;
    }

    if cmd.id == "course.update" || cmd.id == "course.set_player" || cmd.id == "course.set_quiz" {
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui.add(btn).clicked() {
            editor.ui.show_course_settings_window = true;
            ui.close();
        }
        return;
    }

    if cmd.id == "variable.add" {
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui.add(btn).clicked() {
            editor.ui.bottom_tab = crate::BottomTab::Variables;
            ui.close();
        }
        return;
    }

    if cmd.id == "trigger.add" {
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui.add(btn).clicked() {
            crate::toolbar::insert_trigger(editor);
            ui.close();
        }
        return;
    }

    if cmd.id == "results.insert" || cmd.id == "certificate.insert" {
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui.add(btn).clicked() {
            if cmd.id == "results.insert" {
                crate::toolbar::insert_results_slide(editor);
            } else {
                crate::toolbar::insert_certificate_slide(editor);
            }
            ui.close();
        }
        return;
    }

    if cmd.id == "scene.delete" {
        let scene_id = editor.session.course.scenes.iter().find_map(|scene| {
            if scene
                .slides
                .iter()
                .any(|slide| slide.id == editor.selection.slide)
            {
                Some(scene.id.clone())
            } else {
                None
            }
        });
        let enabled = scene_id.is_some() && editor.session.course.scenes.len() > 1;
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui
            .add_enabled(enabled, btn)
            .on_disabled_hover_text(adapter_reason(editor, cmd.id))
            .clicked()
            && let Some(id) = scene_id
        {
            editor.ui.pending_delete_scene = Some(id);
            ui.close();
        }
        return;
    }

    if let Some(params) = adapter(editor, cmd.id) {
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        if ui.add(btn).clicked() {
            *action_to_run = Some((cmd.id, params));
            ui.close();
        }
    } else {
        let reason = adapter_reason(editor, cmd.id);
        let mut btn = Button::new(label);
        if let Some(sc) = cmd.shortcut {
            btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
        }
        ui.add_enabled(false, btn).on_disabled_hover_text(reason);
    }
}

/// Parameters for one align action, or `None` when nothing is selected.
#[must_use]
pub fn align_params(editor: &Editor, edge: &str) -> Option<Json> {
    let slide = &editor.selection.slide;
    let ids = editor.selection.ids();
    if slide.is_empty() || ids.is_empty() {
        return None;
    }
    editor.session.course.slide(slide)?;
    Some(serde_json::json!({
        "slide": slide,
        "objects": ids,
        "edge": edge,
        "to": if editor.ui.align_to_slide { "slide" } else { "selection" },
    }))
}

/// Parameters for distribute, or `None` when fewer than three units are selected.
/// A group counts as one unit.
#[must_use]
pub fn distribute_params(editor: &Editor, axis: &str) -> Option<Json> {
    let slide = &editor.selection.slide;
    let ids = editor.selection.ids();
    if slide.is_empty() || selection_unit_count(editor) < 3 {
        return None;
    }
    editor.session.course.slide(slide)?;
    Some(serde_json::json!({
        "slide": slide,
        "objects": ids,
        "axis": axis,
    }))
}

/// How many rigid units the object selection forms: each ungrouped object and
/// each distinct group counts once.
#[must_use]
pub fn selection_unit_count(editor: &Editor) -> usize {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return 0;
    };
    let mut groups: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut units = 0usize;
    for id in editor.selection.ids() {
        let Some((object, _)) = slide.object(&id) else {
            continue;
        };
        match &object.group {
            Some(group) => {
                if groups.insert(group.clone()) {
                    units = units.saturating_add(1);
                }
            }
            None => units = units.saturating_add(1),
        }
    }
    units
}

/// Parameters for match size, or `None` when fewer than two objects are selected.
#[must_use]
pub fn match_size_params(editor: &Editor, dimension: &str) -> Option<Json> {
    let slide = &editor.selection.slide;
    let ids = editor.selection.ids();
    if slide.is_empty() || ids.len() < 2 {
        return None;
    }
    editor.session.course.slide(slide)?;
    Some(serde_json::json!({
        "slide": slide,
        "objects": ids,
        "dimension": dimension,
    }))
}

fn arrange_menu_item(
    ui: &mut Ui,
    label: &str,
    command: &'static str,
    params: Option<Json>,
    reason: &'static str,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    if let Some(params) = params {
        if ui.button(label).clicked() {
            *action_to_run = Some((command, params));
            ui.close();
        }
    } else {
        ui.add_enabled(false, Button::new(label))
            .on_disabled_hover_text(reason);
    }
}

fn render_align_actions(
    editor: &mut Editor,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    for (label, edge) in [
        ("Align Left", "left"),
        ("Align Center", "center"),
        ("Align Right", "right"),
        ("Align Top", "top"),
        ("Align Middle", "middle"),
        ("Align Bottom", "bottom"),
    ] {
        arrange_menu_item(
            ui,
            label,
            "object.align",
            align_params(editor, edge),
            "Select an object first",
            action_to_run,
        );
    }
    ui.separator();
    for (label, axis) in [
        ("Distribute Horizontally", "horizontal"),
        ("Distribute Vertically", "vertical"),
    ] {
        arrange_menu_item(
            ui,
            label,
            "object.distribute",
            distribute_params(editor, axis),
            "Select at least three objects",
            action_to_run,
        );
    }
    ui.separator();
    for (label, dimension) in [("Match Width", "width"), ("Match Height", "height")] {
        arrange_menu_item(
            ui,
            label,
            "object.match_size",
            match_size_params(editor, dimension),
            "Select at least two objects",
            action_to_run,
        );
    }
}

fn render_arrange_extras(
    editor: &mut Editor,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    ui.separator();
    render_align_actions(editor, ui, action_to_run);
    ui.separator();
    ui.checkbox(&mut editor.ui.align_to_slide, "Align to Slide");
}

fn render_extra_category_items(
    editor: &mut Editor,
    ui: &mut Ui,
    category: &str,
    requests: &mut Vec<EditorRequest>,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    if category == "Arrange" {
        render_arrange_extras(editor, ui, action_to_run);
    }

    if category == "Project" {
        ui.separator();
        if ui.button("Course Settings").clicked() {
            editor.ui.show_course_settings_window = true;
            ui.close();
        }
        if ui.button("Theme…").clicked() {
            editor.ui.show_theme_window = true;
            ui.close();
        }
        if ui.button("AI Settings…").clicked() {
            editor.open_ai_settings();
            ui.close();
        }
        if ui.button("Suggest Alt Text…").clicked() {
            editor.open_alt_text_dialog();
            ui.close();
        }
        if ui.button("Translate with AI…").clicked() {
            editor.open_translate_dialog();
            ui.close();
        }
        let template_slide = editor.selection.slide.clone();
        let can_template = editor.session.course.play_slide(&template_slide).is_some();
        if ui
            .add_enabled(can_template, Button::new("Save as Template"))
            .on_disabled_hover_text("Select a slide first")
            .clicked()
        {
            *action_to_run = Some((
                "template.save",
                serde_json::json!({ "slide": template_slide }),
            ));
            ui.close();
        }
    }

    if category == "Insert" {
        ui.separator();
        if ui.button("Text Box").clicked() {
            crate::toolbar::insert_text_box(editor);
            ui.close();
        }
        if ui.button("Table").clicked() {
            crate::toolbar::insert_table(editor);
            ui.close();
        }
        if ui.button("Button").clicked() {
            crate::toolbar::insert_button(editor);
            ui.close();
        }
        if ui.button("Shape").clicked() {
            editor.ui.show_shape_gallery_window = true;
            ui.close();
        }
        if ui.button("Image").clicked() {
            crate::toolbar::insert_image(editor, requests);
            ui.close();
        }
        if ui.button("Audio").clicked() {
            crate::toolbar::insert_audio(editor, requests);
            ui.close();
        }
        if ui.button("Video").clicked() {
            crate::toolbar::insert_video(editor, requests);
            ui.close();
        }
        if ui.button("Lottie…").clicked() {
            crate::toolbar::insert_lottie(editor, requests);
            ui.close();
        }
        ui.menu_button("Input", |ui| {
            if ui.button("Text entry").clicked() {
                crate::input_insert::insert_text_entry(editor);
                ui.close();
            }
            if ui.button("Slider").clicked() {
                crate::input_insert::insert_slider(editor);
                ui.close();
            }
            if ui.button("Dial").clicked() {
                crate::input_insert::insert_dial(editor);
                ui.close();
            }
            if ui.button("Checkbox").clicked() {
                crate::input_insert::insert_checkbox(editor);
                ui.close();
            }
            if ui.button("Radio buttons").clicked() {
                crate::input_insert::insert_radio_group(editor);
                ui.close();
            }
        });
        ui.menu_button("Interaction", |ui| {
            if ui.button("Marker").clicked() {
                crate::interaction_insert::insert_marker(editor);
                ui.close();
            }
            if ui.button("Scrolling panel").clicked() {
                crate::interaction_insert::insert_scroll_panel(editor);
                ui.close();
            }
            if ui.button("Timer").clicked() {
                crate::interaction_insert::insert_timer(editor);
                ui.close();
            }
            if ui.button("Progress meter").clicked() {
                crate::interaction_insert::insert_progress_meter(editor);
                ui.close();
            }
            if ui.button("Tabs").clicked() {
                crate::interaction_insert::insert_tabs(editor);
                ui.close();
            }
            if ui.button("Accordion").clicked() {
                crate::interaction_insert::insert_accordion(editor);
                ui.close();
            }
        });
        if ui.button("Web object…").clicked() {
            editor.ui.web_error = None;
            editor.ui.show_web_window = true;
            ui.close();
        }
        ui.separator();
        if ui.button("Record Narration…").clicked() {
            editor.open_record_narration();
            ui.close();
        }
        if ui.button("Narration from Text…").clicked() {
            editor.open_speech_dialog();
            ui.close();
        }
        ui.separator();
        if ui.button("Course from Document…").clicked() {
            editor.open_document_dialog();
            ui.close();
        }
    }
}

/// A File menu item that needs the desktop's file access: enabled with a hover
/// hint when available, disabled with the shared explanation when not.
fn file_op_item(
    ui: &mut egui::Ui,
    available: bool,
    label: &str,
    hint: &str,
    request: EditorRequest,
    requests: &mut Vec<EditorRequest>,
) {
    if !available {
        ui.add_enabled(false, Button::new(label))
            .on_disabled_hover_text("Opening and saving files is desktop-only in this version");
    } else if ui.button(label).on_hover_text(hint).clicked() {
        requests.push(request);
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts_correctly() {
        let sc_z = parse_shortcut("Ctrl+Z").expect("parse Ctrl+Z");
        assert_eq!(sc_z.modifiers, Modifiers::COMMAND);
        assert_eq!(sc_z.logical_key, Key::Z);

        let sc_y = parse_shortcut("Ctrl+Y").expect("parse Ctrl+Y");
        assert_eq!(sc_y.modifiers, Modifiers::COMMAND);
        assert_eq!(sc_y.logical_key, Key::Y);

        let sc_m = parse_shortcut("Ctrl+M").expect("parse Ctrl+M");
        assert_eq!(sc_m.modifiers, Modifiers::COMMAND);
        assert_eq!(sc_m.logical_key, Key::M);

        let sc_del = parse_shortcut("Delete").expect("parse Delete");
        assert_eq!(sc_del.modifiers, Modifiers::NONE);
        assert_eq!(sc_del.logical_key, Key::Delete);
    }
}
