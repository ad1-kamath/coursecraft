//! Editor menu bar, command adapters, and keyboard shortcut handling.

use crate::shortcuts::{ShortcutAction, shortcut_for_action, shortcut_for_command};
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

/// Format `"{base} ({formatted})"` with the platform-aware shortcut label,
/// or plain `base` when `shortcut` is `None` or empty.
#[must_use]
pub fn tooltip_with_shortcut(ctx: &egui::Context, base: &str, shortcut: Option<&str>) -> String {
    match shortcut.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sc) => {
            let formatted = shortcut_label(ctx, sc);
            if formatted.is_empty() {
                base.to_owned()
            } else {
                format!("{base} ({formatted})")
            }
        }
        None => base.to_owned(),
    }
}

/// Render a menu button with an optional right-aligned shortcut hint, a hover
/// tooltip echoing the shortcut, and an optional disabled hover reason.
pub fn menu_button(
    ui: &mut Ui,
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
    disabled_reason: Option<&str>,
) -> egui::Response {
    let shortcut = shortcut.map(str::trim).filter(|s| !s.is_empty());
    let mut btn = Button::new(label);
    if let Some(sc) = shortcut {
        btn = btn.shortcut_text(shortcut_label(ui.ctx(), sc));
    }
    let mut resp = if let Some(reason) = disabled_reason {
        ui.add_enabled(enabled, btn).on_disabled_hover_text(reason)
    } else if enabled {
        ui.add(btn)
    } else {
        ui.add_enabled(false, btn)
            .on_disabled_hover_text("Not available right now")
    };
    if let Some(sc) = shortcut {
        resp = resp.on_hover_text(tooltip_with_shortcut(ui.ctx(), label, Some(sc)));
    }
    resp
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
    if trimmed.contains(" / ") {
        return trimmed
            .split(" / ")
            .map(|part| format_single_shortcut(part, is_mac, &has_glyph))
            .collect::<Vec<_>>()
            .join(" / ");
    }
    format_single_shortcut(trimmed, is_mac, &has_glyph)
}

fn format_single_shortcut(
    shortcut: &str,
    is_mac: bool,
    has_glyph: &dyn Fn(char) -> bool,
) -> String {
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
            if key_part.len() == 1 || is_function_key(key_part) {
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

fn is_function_key(s: &str) -> bool {
    let Some(rest) = s.strip_prefix(['f', 'F']) else {
        return false;
    };
    (1..=2).contains(&rest.len())
        && rest.bytes().all(|b| b.is_ascii_digit())
        && rest.parse::<u8>().is_ok_and(|n| (1..=20).contains(&n))
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
    /// Open the History window.
    OpenHistory,
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
        "edit.history" => Some(MenuRoute::OpenHistory),
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
        "group.set_interactive" => {
            if editor.ui.editing_state.is_some() {
                return None;
            }
            let (slide_id, group_id, is_interactive) = selected_group_interactive_info(editor)?;
            Some(serde_json::json!({
                "slide": slide_id,
                "group": group_id,
                "interactive": !is_interactive,
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
        return "Not available in slide masters";
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
        "group.set_interactive" => {
            if editor.ui.master_view {
                "Not available in slide masters"
            } else if editor.ui.editing_state.is_some() {
                crate::STATE_LOCK_TOOLTIP
            } else {
                "Select a group first"
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
            if menu_button(
                ui,
                "New",
                Some(shortcut_for_action(ShortcutAction::NewCourse)),
                true,
                None,
            )
            .clicked()
            {
                requests.push(EditorRequest::New);
                ui.close();
            }

            let desktop_file_ops = editor.file_ops_available && !cfg!(target_arch = "wasm32");

            if menu_button(
                ui,
                "Open…",
                Some(shortcut_for_action(ShortcutAction::OpenCourse)),
                editor.file_ops_available,
                Some(FILE_OPS_DISABLED_MSG),
            )
            .clicked()
            {
                requests.push(EditorRequest::Open);
                ui.close();
            }

            file_op_item(
                ui,
                desktop_file_ops,
                "Open folder…",
                "Open a project folder, such as one kept in git",
                EditorRequest::OpenFolder,
                requests,
            );

            if desktop_file_ops {
                ui.menu_button("Open recent", |ui| {
                    if editor.recent_files.is_empty() {
                        let p = coursecraft_render::palette(ui.ctx());
                        ui.label(
                            coursecraft_render::small("No recent files").color(p.text_disabled),
                        );
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
                ui.add_enabled(false, Button::new("Open recent"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG);
            }

            ui.separator();

            if menu_button(
                ui,
                "Save",
                Some(shortcut_for_action(ShortcutAction::Save)),
                editor.file_ops_available,
                Some(FILE_OPS_DISABLED_MSG),
            )
            .clicked()
            {
                requests.push(EditorRequest::Save);
                ui.close();
            }

            if menu_button(
                ui,
                "Save as…",
                Some(shortcut_for_action(ShortcutAction::SaveAs)),
                editor.file_ops_available,
                Some(FILE_OPS_DISABLED_MSG),
            )
            .clicked()
            {
                requests.push(EditorRequest::SaveAs);
                ui.close();
            }

            file_op_item(
                ui,
                desktop_file_ops,
                "Save as project folder (for git)…",
                "Save as a folder of plain files that git can track",
                EditorRequest::SaveAsFolder,
                requests,
            );

            let history_ready = desktop_file_ops && editor.ui.versions.has_file;
            let history = ui.add_enabled(history_ready, Button::new("Version history…"));
            let history = if desktop_file_ops {
                history.on_disabled_hover_text("Save the course first to start its history.")
            } else {
                history.on_disabled_hover_text(FILE_OPS_DISABLED_MSG)
            };
            if history.clicked() {
                requests.push(EditorRequest::ShowVersionHistory);
                ui.close();
            }

            ui.separator();

            if menu_button(
                ui,
                "Preferences…",
                Some(shortcut_for_action(ShortcutAction::Preferences)),
                true,
                None,
            )
            .clicked()
            {
                editor.open_preferences();
                ui.close();
            }
            if ui.button("Slide size…").clicked() {
                editor.open_slide_size_dialog();
                ui.close();
            }

            if ui.button(tl!("Welcome screen")).clicked() {
                editor.ui.show_welcome = true;
                // The Start page, not the section of the File page that was open last.
                editor.ui.file_page.section = crate::file_page::FileSection::Start;
                ui.close();
            }

            ui.separator();

            ui.menu_button("Import", |ui| {
                if ui.button("PowerPoint (.pptx)…").clicked() {
                    requests.push(EditorRequest::PickPptx);
                    ui.close();
                }
                if ui.button("Course from document…").clicked() {
                    editor.open_document_dialog();
                    ui.close();
                }
                if ui.button("Slides from outline…").clicked() {
                    editor.open_outline_dialog();
                    ui.close();
                }
                if ui.button("Questions from spreadsheet (CSV)…").clicked() {
                    requests.push(EditorRequest::ImportQuestionsCsv);
                    ui.close();
                }
                if ui
                    .add_enabled(desktop_file_ops, Button::new("Translation (XLIFF)…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG)
                    .clicked()
                {
                    requests.push(EditorRequest::ImportTranslation);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(desktop_file_ops, Button::new("Media files…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG)
                    .clicked()
                {
                    requests.push(EditorRequest::PickMediaFiles);
                    ui.close();
                }
                if ui.button("Font…").clicked() {
                    requests.push(EditorRequest::PickFonts);
                    ui.close();
                }
            });

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
                if ui
                    .add_enabled(desktop_file_ops, Button::new("Translation (XLIFF)…"))
                    .on_disabled_hover_text(FILE_OPS_DISABLED_MSG)
                    .clicked()
                {
                    editor.ui.show_export_translation = true;
                    ui.close();
                }
            });

            if ui.button("Manage fonts…").clicked() {
                editor.ui.show_fonts_window = true;
                ui.close();
            }

            ui.separator();

            if menu_button(
                ui,
                "Publish…",
                Some(shortcut_for_action(ShortcutAction::Publish)),
                editor.file_ops_available,
                Some(FILE_OPS_DISABLED_MSG),
            )
            .clicked()
            {
                editor.open_publish_dialog();
                ui.close();
            }

            ui.separator();

            // Registry items under File/
            for cmd in registry() {
                if let Some(sub) = cmd.menu.strip_prefix("File/")
                    && !sub.starts_with("Import/")
                {
                    render_registry_menu_item(editor, ui, cmd, sub, &mut action_to_run);
                }
            }

            ui.separator();
            if ui.button("Quit").clicked() {
                requests.push(EditorRequest::Quit);
                ui.close();
            }
        });

        // 2. Other categories from the registry. Insert lives on the ribbon's
        // Insert and Slides tabs while the ribbon is on.
        for category in categories {
            if category == "File" || (category == "Insert" && editor.ui.ribbon.visible) {
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

        // Slide menu
        ui.menu_button("Slide", |ui| {
            if ui.button("Slide size…").clicked() {
                editor.open_slide_size_dialog();
                ui.close();
            }
        });

        // View menu (Theme preference selection)
        ui.menu_button("View", |ui| {
            let mut masters_on = editor.ui.master_view;
            if ui.checkbox(&mut masters_on, "Slide masters").changed() {
                editor.toggle_master_view();
                ui.close();
            }
            ui.checkbox(&mut editor.ui.show_media_library, "Media library");
            ui.checkbox(&mut editor.ui.ribbon.visible, tl!("Ribbon"))
                .on_hover_text(tl!(
                    "Commands in tabs above the slide. Off shows the older toolbar."
                ));
            ui.checkbox(&mut editor.ui.dock.enabled, tl!("Docked Panels"))
                .on_hover_text(tl!(
                    "Move, tab, float and close the panels. Off keeps them fixed."
                ));
            panels_menu(editor, ui);
            if menu_button(
                ui,
                "Course map",
                Some(shortcut_for_action(ShortcutAction::CourseMap)),
                !editor.ui.master_view,
                Some("Not available in slide masters"),
            )
            .clicked()
            {
                editor.ui.story_view = !editor.ui.story_view;
                ui.close();
            }
            ui.separator();
            if menu_button(
                ui,
                "Zoom to fit",
                Some(shortcut_for_action(ShortcutAction::ZoomFit)),
                true,
                None,
            )
            .clicked()
            {
                editor.ui.zoom = crate::ZoomLevel::Fit;
                ui.close();
            }
            if menu_button(
                ui,
                "Zoom in",
                Some(shortcut_for_action(ShortcutAction::ZoomIn)),
                true,
                None,
            )
            .clicked()
            {
                editor.zoom_in();
                ui.close();
            }
            if menu_button(
                ui,
                "Zoom out",
                Some(shortcut_for_action(ShortcutAction::ZoomOut)),
                true,
                None,
            )
            .clicked()
            {
                editor.zoom_out();
                ui.close();
            }
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
            ui.checkbox(&mut editor.ui.snap_to_guides, "Snap to guides");
            ui.separator();
            let mut show_rulers = editor.session.course.editor.show_rulers;
            if ui.checkbox(&mut show_rulers, "Rulers").changed() {
                editor.run(
                    "view.set",
                    serde_json::json!({ "show_rulers": show_rulers }),
                );
            }
            let mut show_grid = editor.session.course.editor.show_grid;
            if ui.checkbox(&mut show_grid, "Grid").changed() {
                editor.run("view.set", serde_json::json!({ "show_grid": show_grid }));
            }
            let mut snap_to_grid = editor.session.course.editor.snap_to_grid;
            if ui.checkbox(&mut snap_to_grid, "Snap to grid").changed() {
                editor.run(
                    "view.set",
                    serde_json::json!({ "snap_to_grid": snap_to_grid }),
                );
            }
            let mut show_guides = editor.session.course.editor.show_guides;
            if ui.checkbox(&mut show_guides, "Guides").changed() {
                editor.run(
                    "view.set",
                    serde_json::json!({ "show_guides": show_guides }),
                );
            }
            ui.separator();
            let (hidden_ids, locked_ids) =
                crate::timeline::hidden_and_locked_ids(&editor.session.course, &editor.selection);
            if ui
                .add_enabled(
                    !hidden_ids.is_empty(),
                    Button::new("Show all hidden objects"),
                )
                .on_disabled_hover_text("No hidden objects on this slide")
                .clicked()
            {
                let slide_id = editor.selection.slide.clone();
                editor.run(
                    "object.hide_in_editor",
                    serde_json::json!({
                        "slide": slide_id,
                        "objects": hidden_ids,
                        "hidden": false,
                    }),
                );
                ui.close();
            }
            if ui
                .add_enabled(!locked_ids.is_empty(), Button::new("Unlock all objects"))
                .on_disabled_hover_text("No locked objects on this slide")
                .clicked()
            {
                let slide_id = editor.selection.slide.clone();
                editor.run(
                    "object.lock",
                    serde_json::json!({
                        "slide": slide_id,
                        "objects": locked_ids,
                        "locked": false,
                    }),
                );
                ui.close();
            }
        });

        // 3. Preview menu
        ui.menu_button("Preview", |ui| {
            show_preview_menu_items(editor, ui, requests);
        });

        // 4. Help menu
        ui.menu_button("Help", |ui| {
            if menu_button(
                ui,
                "Keyboard shortcuts",
                Some(shortcut_for_action(ShortcutAction::KeyboardShortcuts)),
                true,
                None,
            )
            .clicked()
            {
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
            show_project_help_items(editor, ui, requests);
        });
    });

    if let Some((cmd, params)) = action_to_run {
        if cmd == "scene.add" {
            // A new scene from the menu starts with a slide, like the toolbar.
            crate::toolbar::add_new_scene(editor);
        } else {
            editor.run(cmd, params);
        }
    }
}

/// View › Panels: show or hide each docked panel, and reset the arrangement.
pub(crate) fn panels_menu(editor: &mut Editor, ui: &mut Ui) {
    let docked = editor.ui.dock.enabled;
    ui.add_enabled_ui(docked, |ui| {
        ui.menu_button(tl!("Panels"), |ui| {
            for panel in crate::dock::Panel::ALL {
                if panel == crate::dock::Panel::Canvas {
                    continue;
                }
                let mut open = crate::dock::is_open(editor, panel);
                if ui.checkbox(&mut open, tl!(panel.label())).changed() {
                    crate::dock::set_open(editor, panel, open);
                }
            }
            ui.separator();
            if ui.button(tl!("Reset Panel Layout")).clicked() {
                crate::dock::reset(editor);
                ui.close();
            }
        })
        .response
        .on_disabled_hover_text(tl!("Turn on Docked Panels first"));
    });
}

/// Shared menu items for both the top-level **Preview** menu and the toolbar's
/// Preview caret popup.
pub(crate) fn show_preview_menu_items(
    editor: &mut Editor,
    ui: &mut Ui,
    requests: &mut Vec<EditorRequest>,
) {
    let master_view = editor.ui.master_view;
    let slide_id = editor.selection.slide.clone();
    let slide_empty = slide_id.is_empty();
    let on_bank_slide = !slide_empty && editor.session.course.bank_slide(&slide_id).is_some();

    let can_preview_current = !master_view && !on_bank_slide && !slide_empty;
    let current_disabled_reason = if master_view {
        "Not available in slide masters"
    } else if on_bank_slide {
        "Question bank slides can't be previewed directly"
    } else {
        "Select a slide first"
    };

    if menu_button(
        ui,
        "Preview this slide",
        Some(shortcut_for_action(ShortcutAction::PreviewThisSlide)),
        can_preview_current,
        Some(current_disabled_reason),
    )
    .clicked()
    {
        crate::toolbar::preview_slide(editor, requests);
        ui.close();
    }

    if menu_button(
        ui,
        "Preview from here",
        Some(shortcut_for_action(ShortcutAction::PreviewFromHere)),
        can_preview_current,
        Some(current_disabled_reason),
    )
    .clicked()
    {
        editor.open_preview_from(&slide_id);
        requests.push(EditorRequest::Preview { slide: slide_id });
        ui.close();
    }

    let selected_non_bank_count = editor
        .selection
        .slide_ids()
        .iter()
        .filter(|id| !id.is_empty() && editor.session.course.bank_slide(id).is_none())
        .count();
    let can_preview_selected = !master_view && selected_non_bank_count >= 2;
    let selected_disabled_reason = if master_view {
        "Not available in slide masters"
    } else {
        "Select two or more slides to preview them"
    };
    if menu_button(
        ui,
        "Preview selected slides",
        None,
        can_preview_selected,
        Some(selected_disabled_reason),
    )
    .clicked()
    {
        let slide = editor.selection.slide.clone();
        editor.open_preview_selected();
        requests.push(EditorRequest::Preview { slide });
        ui.close();
    }

    if menu_button(
        ui,
        "Preview whole course",
        Some(shortcut_for_action(ShortcutAction::PreviewWholeCourse)),
        !master_view,
        Some("Not available in slide masters"),
    )
    .clicked()
    {
        editor.open_preview_at(None);
        requests.push(EditorRequest::Preview {
            slide: String::new(),
        });
        ui.close();
    }
}

/// Help items that report a problem or open the project's pages on GitHub.
fn show_project_help_items(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    use crate::project_links::REPO_URL;
    use crate::report::{open_feature_request, open_link};

    if ui.button("Report a problem…").clicked() {
        editor.open_report();
        ui.close();
    }
    if ui.button("Suggest a feature…").clicked() {
        open_feature_request(ui.ctx());
        ui.close();
    }
    if ui.button("CourseCraft on GitHub").clicked() {
        open_link(ui.ctx(), REPO_URL);
        ui.close();
    }
    if editor.updates_available && ui.button("Check for updates…").clicked() {
        requests.push(EditorRequest::CheckForUpdates);
        ui.close();
    }
}

fn format_undo_redo_label(prefix: &str, name: &str) -> String {
    const KEEP_CAPS: &[&str] = &[
        "AI",
        "CSV",
        "XLIFF",
        "Lottie",
        "PowerPoint",
        "PDF",
        "MP4",
        "Word",
        "SCORM",
        "LTI",
        "CourseCraft",
    ];
    let first_word = name
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("");
    if KEEP_CAPS.contains(&first_word) {
        return format!("{prefix} {name}");
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {
            format!("{prefix} {}{}", first.to_ascii_lowercase(), chars.as_str())
        }
        _ => format!("{prefix} {name}"),
    }
}

/// Why file and publish commands are disabled where there is no file system.
pub(crate) const FILE_OPS_DISABLED_MSG: &str =
    "Opening and saving files is desktop-only in this version";

/// Whether Cut, Copy and Paste can run now, each with its disabled reason.
/// The Edit menu and the ribbon's Clipboard group share these rules.
pub(crate) struct ClipboardRules {
    pub cut: Result<(), &'static str>,
    pub copy: Result<(), &'static str>,
    pub paste: Result<(), &'static str>,
}

pub(crate) fn clipboard_rules(editor: &Editor) -> ClipboardRules {
    let has_objects = editor.selection.object.is_some();
    let has_slide = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some();
    let navigator = editor.ui.last_active_area == crate::ActiveArea::Navigator;
    if editor.ui.master_view {
        let reason = Err("Not available in slide masters");
        return ClipboardRules {
            cut: reason,
            copy: reason,
            paste: reason,
        };
    }
    let can_cut_slide = editor.session.course.slides().count() > 1;
    let cut = if (navigator && has_slide && can_cut_slide) || (!navigator && has_objects) {
        Ok(())
    } else if navigator {
        Err("Can't cut the only slide")
    } else {
        Err("Select an object first")
    };
    let copy = if (navigator && has_slide) || (!navigator && has_objects) {
        Ok(())
    } else {
        Err("Select something first")
    };
    let paste = if editor.ui.clipboard.is_some() || editor.file_ops_available {
        Ok(())
    } else {
        Err("Nothing to paste")
    };
    ClipboardRules { cut, copy, paste }
}

/// Paste: through the system clipboard on the desktop, else the editor's own.
pub(crate) fn paste_now(editor: &mut Editor, ctx: &egui::Context) {
    if editor.file_ops_available {
        crate::clipboard::request_system_paste(editor, ctx, None);
    } else {
        crate::clipboard::paste_private(editor);
    }
}

fn show_edit_menu(
    editor: &mut Editor,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, serde_json::Value)>,
) {
    for id in ["edit.undo", "edit.redo"] {
        if let Some(cmd) = registry().iter().find(|cmd| cmd.id == id) {
            let dynamic_label;
            let label = match id {
                "edit.undo" => {
                    if let Some(name) = editor.session.undo_names().first() {
                        dynamic_label = format_undo_redo_label("Undo", name);
                        dynamic_label.as_str()
                    } else {
                        cmd.label
                    }
                }
                "edit.redo" => {
                    if let Some(name) = editor.session.redo_names().first() {
                        dynamic_label = format_undo_redo_label("Redo", name);
                        dynamic_label.as_str()
                    } else {
                        cmd.label
                    }
                }
                _ => cmd.label,
            };
            render_registry_menu_item(editor, ui, cmd, label, action_to_run);
        }
    }
    ui.separator();

    let rules = clipboard_rules(editor);
    if menu_button(
        ui,
        "Cut",
        Some(shortcut_for_action(ShortcutAction::Cut)),
        rules.cut.is_ok(),
        rules.cut.err(),
    )
    .clicked()
    {
        crate::clipboard::cut(editor);
        ui.close();
    }

    if menu_button(
        ui,
        "Copy",
        Some(shortcut_for_action(ShortcutAction::Copy)),
        rules.copy.is_ok(),
        rules.copy.err(),
    )
    .clicked()
    {
        crate::clipboard::copy(editor);
        ui.close();
    }

    if menu_button(
        ui,
        "Paste",
        Some(shortcut_for_action(ShortcutAction::Paste)),
        rules.paste.is_ok(),
        rules.paste.err(),
    )
    .clicked()
    {
        paste_now(editor, ui.ctx());
        ui.close();
    }

    if menu_button(
        ui,
        "Select all",
        Some(shortcut_for_action(ShortcutAction::SelectAll)),
        true,
        None,
    )
    .clicked()
    {
        editor.select_all_objects();
        ui.close();
    }

    ui.separator();
    if menu_button(
        ui,
        "Check spelling…",
        Some(shortcut_for_action(ShortcutAction::CheckSpelling)),
        true,
        None,
    )
    .clicked()
    {
        crate::spell_check::open(editor);
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

/// The Arrange menu's entries (order, group, align, distribute), for the ribbon.
pub(crate) fn arrange_menu_items(
    editor: &mut Editor,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, serde_json::Value)>,
    requests: &mut Vec<EditorRequest>,
) {
    render_category_menu_items(editor, ui, "Arrange", action_to_run, requests);
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
    let cut_reason = if master_view {
        "Not available in slide masters"
    } else {
        "Select an object first"
    };
    if menu_button(
        ui,
        "Cut",
        Some(shortcut_for_action(ShortcutAction::Cut)),
        cut_enabled,
        Some(cut_reason),
    )
    .clicked()
    {
        crate::clipboard::cut(editor);
        ui.close();
    }

    let copy_enabled = has_selection && !master_view;
    let copy_reason = if master_view {
        "Not available in slide masters"
    } else {
        "Select an object first"
    };
    if menu_button(
        ui,
        "Copy",
        Some(shortcut_for_action(ShortcutAction::Copy)),
        copy_enabled,
        Some(copy_reason),
    )
    .clicked()
    {
        crate::clipboard::copy(editor);
        ui.close();
    }

    let paste_enabled = paste_enabled(editor);
    let paste_reason = if master_view {
        "Not available in slide masters"
    } else {
        "Nothing copied yet"
    };
    if menu_button(
        ui,
        "Paste",
        Some(shortcut_for_action(ShortcutAction::Paste)),
        paste_enabled,
        Some(paste_reason),
    )
    .clicked()
    {
        crate::clipboard::paste_private(editor);
        ui.close();
    }

    if menu_button(
        ui,
        "Duplicate",
        shortcut_for_command("object.duplicate"),
        has_selection,
        Some("Select an object first"),
    )
    .clicked()
    {
        editor.duplicate_selection();
        ui.close();
    }

    if menu_button(
        ui,
        "Delete",
        shortcut_for_command("object.delete"),
        has_selection,
        Some("Select an object first"),
    )
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

    let single_non_master = has_selection && !editor.selection.is_multi() && !master_view;
    let edit_states = Button::new("Edit states…");
    if ui
        .add_enabled(single_non_master, edit_states)
        .on_disabled_hover_text(if master_view {
            "Master objects don't have states"
        } else if editor.selection.is_multi() {
            "Select a single object"
        } else {
            "Select an object first"
        })
        .clicked()
    {
        if let Some(state_name) = editor.ui.preview_state.clone().filter(|s| s != "normal")
            && let Some(obj_id) = editor.selection.object.clone()
        {
            let slide_id = editor.selection.slide.clone();
            editor.enter_edit_state(&slide_id, &obj_id, &state_name);
        }
        crate::dock::request_bottom_tab(editor, crate::BottomTab::States);
        ui.close();
    }

    ui.separator();
    let state_edit_locked = editor.ui.editing_state.is_some();
    ui.add_enabled_ui(!state_edit_locked, |ui| {
        render_group_ungroup_items(editor, ui, true);
        ui.separator();
        ui.menu_button("Order", |ui| {
            render_reorder_menu_item(editor, ui);
        });
        ui.menu_button("Align", |ui| {
            let mut action_to_run: Option<(&'static str, Json)> = None;
            render_align_actions(editor, ui, &mut action_to_run);
            ui.separator();
            ui.checkbox(&mut editor.ui.align_to_slide, "Align to slide");
            if let Some((cmd, params)) = action_to_run {
                editor.run(cmd, params);
            }
        });
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
    ui.separator();

    if menu_button(
        ui,
        "Select all",
        Some(shortcut_for_action(ShortcutAction::SelectAll)),
        true,
        None,
    )
    .clicked()
    {
        editor.select_all_objects();
        ui.close();
    }
}

pub(crate) fn selected_group_interactive_info(editor: &Editor) -> Option<(String, String, bool)> {
    if editor.ui.master_view || !editor.selection.is_one_group(&editor.session.course) {
        return None;
    }
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    let primary = editor.selection.object.as_deref()?;
    let (obj, _) = slide.object(primary)?;
    let group_id = obj.group.clone()?;
    let is_interactive = slide.interactive_group(&group_id).is_some();
    Some((slide.id.clone(), group_id, is_interactive))
}

fn render_group_ungroup_items(editor: &mut Editor, ui: &mut Ui, shortcuts: bool) {
    let group_params = adapter(editor, "object.group");
    let group_sc = shortcuts
        .then(|| shortcut_for_command("object.group"))
        .flatten();
    if menu_button(
        ui,
        "Group",
        group_sc,
        group_params.is_some(),
        Some("Select at least two objects"),
    )
    .clicked()
    {
        if let Some(params) = group_params {
            editor.run("object.group", params);
        }
        ui.close();
    }

    let ungroup_params = adapter(editor, "object.ungroup");
    let ungroup_sc = shortcuts
        .then(|| shortcut_for_command("object.ungroup"))
        .flatten();
    if menu_button(
        ui,
        "Ungroup",
        ungroup_sc,
        ungroup_params.is_some(),
        Some("This object isn't in a group"),
    )
    .clicked()
    {
        if let Some(params) = ungroup_params {
            editor.run("object.ungroup", params);
        }
        ui.close();
    }

    if let Some((slide_id, group_id, is_interactive)) = selected_group_interactive_info(editor) {
        let label = if is_interactive {
            tl!("Remove button behaviour")
        } else {
            tl!("Make button from group")
        };
        if menu_button(ui, label, None, true, None).clicked() {
            editor.run(
                "group.set_interactive",
                serde_json::json!({
                    "slide": slide_id,
                    "group": group_id,
                    "interactive": !is_interactive,
                }),
            );
            ui.close();
        }
    }
}

/// Context menu for a selected `StateExtra` inside Canvas Edit-State Mode.
pub(crate) fn show_state_extra_menu(editor: &mut Editor, ui: &mut Ui) {
    let Some(session) = editor.ui.editing_state.clone() else {
        return;
    };
    let Some(extra_id) = session.selected_extra.clone() else {
        return;
    };
    let info = editor
        .session
        .course
        .slide(&session.slide_id)
        .and_then(|slide| slide.object(&session.object_id))
        .and_then(|(obj, _)| obj.states.iter().find(|st| st.name == session.state_name))
        .and_then(|st| {
            st.extras
                .iter()
                .position(|e| e.id == extra_id)
                .map(|idx| (st.extras[idx].clone(), idx, st.extras.len()))
        });
    let Some((extra, idx, count)) = info else {
        return;
    };

    if ui.button("Duplicate extra").clicked() {
        let mut cloned = extra;
        cloned.id.clear();
        cloned.frame.x += 16.0;
        cloned.frame.y += 16.0;
        if let Some(res) = editor.run(
            "object.state_add_extra",
            serde_json::json!({
                "slide": session.slide_id,
                "object": session.object_id,
                "state": session.state_name,
                "extra": cloned,
            }),
        ) && let Some(new_id) = res.get("id").and_then(Json::as_str)
            && let Some(active) = editor.ui.editing_state.as_mut()
        {
            active.selected_extra = Some(new_id.to_owned());
        }
        ui.close();
    }

    let can_forward = idx + 1 < count;
    if ui
        .add_enabled(can_forward, Button::new("Bring forward"))
        .on_disabled_hover_text("Already in front")
        .clicked()
    {
        editor.run(
            "object.state_reorder_extra",
            serde_json::json!({
                "slide": session.slide_id,
                "object": session.object_id,
                "state": session.state_name,
                "extra": extra_id,
                "index": idx + 1,
            }),
        );
        ui.close();
    }

    let can_backward = idx > 0;
    if ui
        .add_enabled(can_backward, Button::new("Send backward"))
        .on_disabled_hover_text("Already at back")
        .clicked()
    {
        editor.run(
            "object.state_reorder_extra",
            serde_json::json!({
                "slide": session.slide_id,
                "object": session.object_id,
                "state": session.state_name,
                "extra": extra_id,
                "index": idx - 1,
            }),
        );
        ui.close();
    }

    ui.separator();
    if menu_button(
        ui,
        "Delete extra",
        shortcut_for_command("object.delete"),
        true,
        None,
    )
    .clicked()
    {
        editor.delete_selection();
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
    let paste_reason = if editor.ui.master_view {
        "Not available in slide masters"
    } else {
        "Nothing copied yet"
    };
    if menu_button(
        ui,
        "Paste",
        Some(shortcut_for_action(ShortcutAction::Paste)),
        can_paste,
        Some(paste_reason),
    )
    .clicked()
    {
        let point = editor.ui.canvas_context_pos;
        crate::clipboard::paste_private_at(editor, point);
        ui.close();
    }

    if menu_button(
        ui,
        "Select all",
        Some(shortcut_for_action(ShortcutAction::SelectAll)),
        true,
        None,
    )
    .clicked()
    {
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
    if menu_button(ui, label, cmd.shortcut, true, None).clicked() {
        match cmd.id {
            "course.find" => crate::find::open(editor),
            "course.import_outline" => editor.open_outline_dialog(),
            "edit.history" => editor.ui.show_history = true,
            _ => editor.ui.show_theme_window = true,
        }
        ui.close();
    }
}

fn render_delete_slide_menu_item(
    editor: &mut Editor,
    ui: &mut Ui,
    cmd: &'static coursecraft_engine::Command,
    label: &str,
) {
    let total = editor.session.course.slides().count();
    let multi = !editor.selection.also_slides.is_empty() && editor.selection.slide_ids().len() > 1;
    let selected_count = if multi {
        editor.selection.slide_ids().len()
    } else {
        1
    };
    let enabled = adapter(editor, cmd.id).is_some() && total > selected_count;
    let reason = if total <= selected_count {
        "A course needs at least one slide"
    } else {
        adapter_reason(editor, cmd.id)
    };
    if menu_button(ui, label, cmd.shortcut, enabled, Some(reason)).clicked() {
        editor.delete_selected_slides();
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
        "course.find" | "course.import_outline" | "course.set_theme" | "edit.history"
    ) {
        render_open_window_menu_item(editor, ui, cmd, label);
        return;
    }

    if cmd.id == "slide.delete" {
        render_delete_slide_menu_item(editor, ui, cmd, label);
        return;
    }

    if cmd.id == "object.duplicate" || cmd.id == "object.delete" {
        let enabled = editor.selection.object.is_some();
        if menu_button(
            ui,
            label,
            cmd.shortcut,
            enabled,
            Some("Select an object first"),
        )
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
        if menu_button(ui, label, cmd.shortcut, true, None).clicked() {
            editor.ui.show_course_settings_window = true;
            ui.close();
        }
        return;
    }

    if cmd.id == "variable.add" {
        if menu_button(ui, label, cmd.shortcut, true, None).clicked() {
            crate::dock::request_bottom_tab(editor, crate::BottomTab::Variables);
            ui.close();
        }
        return;
    }

    if cmd.id == "trigger.add" {
        let master = editor.ui.master_view;
        let reason = master.then_some("Not available in Master View");
        if menu_button(ui, label, cmd.shortcut, !master, reason).clicked() {
            crate::toolbar::insert_trigger(editor);
            ui.close();
        }
        return;
    }

    if cmd.id == "results.insert" || cmd.id == "certificate.insert" {
        let master = editor.ui.master_view;
        let reason = master.then_some("Not available in Master View");
        if menu_button(ui, label, cmd.shortcut, !master, reason).clicked() {
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
        if menu_button(
            ui,
            label,
            cmd.shortcut,
            enabled,
            Some(adapter_reason(editor, cmd.id)),
        )
        .clicked()
            && let Some(id) = scene_id
        {
            editor.ui.pending_delete_scene = Some(id);
            ui.close();
        }
        return;
    }

    let params = adapter(editor, cmd.id);
    let enabled = params.is_some();
    let reason = adapter_reason(editor, cmd.id);
    if menu_button(ui, label, cmd.shortcut, enabled, Some(reason)).clicked()
        && let Some(params) = params
    {
        *action_to_run = Some((cmd.id, params));
        ui.close();
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
        ("Align left", "left"),
        ("Align center", "center"),
        ("Align right", "right"),
        ("Align top", "top"),
        ("Align middle", "middle"),
        ("Align bottom", "bottom"),
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
        ("Distribute horizontally", "horizontal"),
        ("Distribute vertically", "vertical"),
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
    for (label, dimension) in [("Match width", "width"), ("Match height", "height")] {
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
    if let Some((slide_id, group_id, is_interactive)) = selected_group_interactive_info(editor) {
        let label = if is_interactive {
            tl!("Remove button behaviour")
        } else {
            tl!("Make button from group")
        };
        if menu_button(ui, label, None, true, None).clicked() {
            *action_to_run = Some((
                "group.set_interactive",
                serde_json::json!({
                    "slide": slide_id,
                    "group": group_id,
                    "interactive": !is_interactive,
                }),
            ));
            ui.close();
        }
    }
    ui.separator();
    render_align_actions(editor, ui, action_to_run);
    ui.separator();
    ui.checkbox(&mut editor.ui.align_to_slide, "Align to slide");
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
        if ui.button("Theme…").clicked() {
            editor.ui.show_theme_window = true;
            ui.close();
        }
        if ui.button("AI settings…").clicked() {
            editor.open_ai_settings();
            ui.close();
        }
        if ui.button("Suggest alt text…").clicked() {
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
            .add_enabled(can_template, Button::new("Save as template"))
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
        let state_locked = editor.ui.editing_state.is_some();
        let state_tip = "Not available inside a state";
        ui.separator();
        if ui.button("Text box").clicked() {
            crate::toolbar::insert_text_box(editor);
            ui.close();
        }
        let master = editor.ui.master_view;
        if ui
            .add_enabled(!state_locked && !master, egui::Button::new("Table"))
            .on_disabled_hover_text(if master {
                "Not available in Master View"
            } else {
                state_tip
            })
            .clicked()
        {
            crate::toolbar::insert_table(editor);
            ui.close();
        }
        if ui
            .add_enabled(!state_locked, egui::Button::new("Button"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            crate::toolbar::insert_button(editor);
            ui.close();
        }
        if ui.button("Shape").clicked() {
            editor.ui.shape_gallery_category = None;
            editor.ui.show_shape_gallery_window = true;
            ui.close();
        }
        if ui.button("Image").clicked() {
            crate::toolbar::insert_image(editor, requests);
            ui.close();
        }
        if ui
            .add_enabled(!state_locked, egui::Button::new("Audio"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            crate::toolbar::insert_audio(editor, requests);
            ui.close();
        }
        if ui
            .add_enabled(!state_locked, egui::Button::new("Video"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            crate::toolbar::insert_video(editor, requests);
            ui.close();
        }
        if ui
            .add_enabled(!state_locked, egui::Button::new("Lottie…"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            crate::toolbar::insert_lottie(editor, requests);
            ui.close();
        }
        ui.add_enabled_ui(!state_locked, |ui| {
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
            })
            .response
            .on_disabled_hover_text(state_tip);
        });
        if ui
            .add_enabled(!state_locked, egui::Button::new("Interaction…"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            editor.ui.show_interaction_gallery = true;
            ui.close();
        }
        if ui
            .add_enabled(!state_locked, egui::Button::new("Web object…"))
            .on_disabled_hover_text(state_tip)
            .clicked()
        {
            editor.ui.web_error = None;
            editor.ui.show_web_window = true;
            ui.close();
        }
        ui.separator();
        let audio_off = state_locked || master;
        let audio_tip = if master {
            "Not available in Master View"
        } else {
            state_tip
        };
        if ui
            .add_enabled(!audio_off, egui::Button::new("Record narration…"))
            .on_disabled_hover_text(audio_tip)
            .clicked()
        {
            editor.open_record_narration();
            ui.close();
        }
        if ui
            .add_enabled(!audio_off, egui::Button::new("Narration from text…"))
            .on_disabled_hover_text(audio_tip)
            .clicked()
        {
            editor.open_speech_dialog();
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
