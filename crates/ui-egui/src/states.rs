//! States side panel section: lists normal and custom states with add, edit, delete, and initial state.

use crate::Editor;
use crate::properties::{color_swatch_picker, is_committed, mark_color_picked};
use coursecraft_course::{Color, Course, ObjectKind, StateDef};
use coursecraft_render::parse_color;
use egui::{Rect, Response, RichText, TextEdit, Ui};
use serde_json::Value as Json;
use std::collections::HashMap;

pub const SUGGESTED_STATE_NAMES: &[&str] =
    &["visited", "selected", "disabled", "hover", "down", "hidden"];

/// Edit buffers for a single custom state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateEditBuffer {
    pub name: String,
    pub fill_hex: String,
    pub text_override: String,
    pub src_override: String,

    pub fill_rect: Option<Rect>,
    pub text_rect: Option<Rect>,
    pub src_rect: Option<Rect>,
    pub delete_rect: Option<Rect>,
    pub set_initial_rect: Option<Rect>,
    pub preview_rect: Option<Rect>,
}

/// Interaction and edit state for the States panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatesState {
    pub last_synced_revision: crate::SyncRevision,
    pub last_synced_selection: crate::Selection,

    pub new_state_name: String,
    pub edit_buffers: HashMap<String, StateEditBuffer>,

    // Tracked rects for test interaction
    pub new_state_name_rect: Option<Rect>,
    pub add_button_rect: Option<Rect>,
    pub state_row_rects: HashMap<String, Rect>,
}

impl StatesState {
    /// Re-sync edit buffers with current course data if selection or revision changed.
    pub fn sync(&mut self, course: &Course, selection: &crate::Selection, revision: u64) {
        if self.last_synced_revision.is(revision) && &self.last_synced_selection == selection {
            return;
        }
        self.last_synced_revision.set(revision);
        self.last_synced_selection = selection.clone();

        let Some(slide) = course.slide(&selection.slide) else {
            self.edit_buffers.clear();
            return;
        };
        let Some(ref obj_id) = selection.object else {
            self.edit_buffers.clear();
            return;
        };
        let Some((obj, _)) = slide.object(obj_id) else {
            self.edit_buffers.clear();
            return;
        };

        // Retain only buffers for existing states
        let current_names: Vec<String> = obj.states.iter().map(|s| s.name.clone()).collect();
        self.edit_buffers.retain(|k, _| current_names.contains(k));

        for state in &obj.states {
            let buf = self.edit_buffers.entry(state.name.clone()).or_default();
            buf.name = state.name.clone();
            buf.fill_hex = state.fill.as_ref().map(|c| c.0.clone()).unwrap_or_default();
            buf.text_override = state.text.clone().unwrap_or_default();
            buf.src_override = state.src.clone().unwrap_or_default();
        }
    }
}

fn show_state_color_field(
    ui: &mut Ui,
    label: &str,
    color_hex: &mut String,
    id_salt: &str,
) -> (Response, bool) {
    ui.horizontal(|ui| {
        let p = coursecraft_render::palette(ui.ctx());
        ui.label(label);
        let picked = color_swatch_picker(ui, color_hex, id_salt, 16.0, 2);
        let r = ui.add(
            TextEdit::singleline(color_hex)
                .id_salt(id_salt)
                .desired_width(70.0),
        );
        if picked {
            mark_color_picked(&r);
        }
        let is_valid = color_hex.trim().is_empty()
            || parse_color(&Color(color_hex.trim().to_owned())).is_some();
        if !is_valid {
            ui.colored_label(p.danger, "Invalid");
        }
        (r, is_valid)
    })
    .inner
}

/// Render the States panel section in the right column.
pub fn show_states_section(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    editor.ui.states.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
    );

    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        ui.label("No slide selected");
        return;
    };
    let Some(ref obj_id) = editor.selection.object else {
        ui.label("No object selected");
        return;
    };
    let Some((obj, _)) = slide.object(obj_id) else {
        ui.label("Object not found");
        return;
    };

    let slide_id = slide.id.clone();
    let obj_id_str = obj.id.clone();
    let initial_state = obj.initial_state.clone();
    let obj_kind = obj.kind.clone();
    let states_copy = obj.states.clone();

    // 1. Add State form
    ui.horizontal(|ui| {
        let r_name = ui.add(
            TextEdit::singleline(&mut editor.ui.states.new_state_name)
                .id_salt("state_add_name_edit")
                .desired_width(110.0)
                .hint_text("state name"),
        );
        editor.ui.states.new_state_name_rect = Some(r_name.rect);

        let trimmed = editor.ui.states.new_state_name.trim();
        let is_valid = !trimmed.is_empty()
            && trimmed != "normal"
            && !states_copy.iter().any(|s| s.name == trimmed);

        let add_btn = ui.add_enabled(is_valid, egui::Button::new("+ Add"));
        editor.ui.states.add_button_rect = Some(add_btn.rect);

        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
            && (r_name.has_focus() || r_name.lost_focus());
        if (add_btn.clicked() || (enter && is_valid)) && is_valid {
            let default_fill = match &obj_kind {
                ObjectKind::Button { fill, .. } | ObjectKind::Shape { fill, .. } => {
                    Some(fill.clone())
                }
                ObjectKind::Path { path } if path.closed => path.fill.clone(),
                _ => None,
            };
            let state_def = StateDef {
                name: trimmed.to_owned(),
                fill: default_fill,
                text: None,
                src: None,
            };
            if let Ok(state_val) = serde_json::to_value(&state_def) {
                actions.push((
                    "object.add_state",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id_str,
                        "state": state_val,
                    }),
                ));
            }
            editor.ui.states.new_state_name.clear();
        }
    });

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("states_suggestion_combo")
            .selected_text("Preset name…")
            .width(130.0)
            .show_ui(ui, |ui| {
                for name in SUGGESTED_STATE_NAMES {
                    if ui.selectable_label(false, *name).clicked() {
                        editor.ui.states.new_state_name = (*name).to_owned();
                    }
                }
            });
    });

    ui.add_space(4.0);
    ui.separator();

    // 2. Built-in "normal" state
    let normal_is_initial = initial_state == "normal";
    let normal_previewing = editor.ui.preview_state.as_deref() == Some("normal");

    let normal_row = ui
        .horizontal(|ui| {
            ui.label(RichText::new("normal").strong());
            if normal_is_initial {
                ui.label(RichText::new("(initial)").italics());
            } else {
                let btn = ui.button("Set Initial");
                if btn.clicked() {
                    actions.push((
                        "object.update",
                        serde_json::json!({
                            "slide": slide_id,
                            "object": obj_id_str,
                            "initial_state": "normal",
                        }),
                    ));
                }
            }

            if crate::widgets::icon_toggle(
                ui,
                coursecraft_render::Icon::Eye,
                normal_previewing,
                "Preview state",
            )
            .clicked()
            {
                editor.ui.preview_state = if normal_previewing {
                    None
                } else {
                    Some("normal".to_owned())
                };
            }
        })
        .response;
    editor
        .ui
        .states
        .state_row_rects
        .insert("normal".to_owned(), normal_row.rect);

    ui.add_space(4.0);

    // 3. Custom states
    for state in &states_copy {
        let is_initial = initial_state == state.name;
        let is_previewing = editor.ui.preview_state.as_deref() == Some(&state.name);

        let buf = editor
            .ui
            .states
            .edit_buffers
            .entry(state.name.clone())
            .or_default();

        let state_name = state.name.clone();

        let row_resp = ui
            .group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&state_name).strong());
                    if is_initial {
                        ui.label(RichText::new("(initial)").italics());
                    }
                });

                ui.horizontal(|ui| {
                    if !is_initial {
                        let btn = ui.button("Set Initial");
                        buf.set_initial_rect = Some(btn.rect);
                        if btn.clicked() {
                            actions.push((
                                "object.update",
                                serde_json::json!({
                                    "slide": slide_id,
                                    "object": obj_id_str,
                                    "initial_state": state_name,
                                }),
                            ));
                        }
                    }

                    let prev_btn = crate::widgets::icon_toggle(
                        ui,
                        coursecraft_render::Icon::Eye,
                        is_previewing,
                        "Preview state",
                    );
                    buf.preview_rect = Some(prev_btn.rect);
                    if prev_btn.clicked() {
                        editor.ui.preview_state = if is_previewing {
                            None
                        } else {
                            Some(state_name.clone())
                        };
                    }

                    let del_btn =
                        crate::quiet_button(ui, "Delete state", crate::QuietTone::Danger, true);
                    buf.delete_rect = Some(del_btn.rect);
                    if del_btn.clicked() {
                        actions.push((
                            "object.delete_state",
                            serde_json::json!({
                                "slide": slide_id,
                                "object": obj_id_str,
                                "name": state_name,
                            }),
                        ));
                    }
                });

                // Fill edit (for Button / Shape)
                let state_fill_allowed = match &obj_kind {
                    ObjectKind::Button { .. } | ObjectKind::Shape { .. } => true,
                    ObjectKind::Path { path } => path.closed,
                    _ => false,
                };
                if state_fill_allowed {
                    let id_salt = format!("state_fill_{state_name}");
                    let (r_color, is_valid_hex) =
                        show_state_color_field(ui, "Fill", &mut buf.fill_hex, &id_salt);
                    buf.fill_rect = Some(r_color.rect);
                    if is_committed(&r_color, ui) && is_valid_hex {
                        let new_fill = if buf.fill_hex.trim().is_empty() {
                            None
                        } else {
                            Some(Color(buf.fill_hex.trim().to_owned()))
                        };
                        if new_fill != state.fill {
                            let mut updated = state.clone();
                            updated.fill = new_fill;
                            if let Ok(state_val) = serde_json::to_value(&updated) {
                                actions.push((
                                    "object.update_state",
                                    serde_json::json!({
                                        "slide": slide_id,
                                        "object": obj_id_str,
                                        "state": state_val,
                                    }),
                                ));
                            }
                        }
                    }
                }

                // Text override edit (for Text / Button / text-bearing shapes)
                let allows_state_text = match obj_kind {
                    ObjectKind::Text { .. } | ObjectKind::Button { .. } => true,
                    ObjectKind::Shape { shape, .. } => coursecraft_course::shape_allows_text(shape),
                    _ => false,
                };
                if allows_state_text {
                    ui.horizontal(|ui| {
                        ui.label("Text");
                        let r_text = ui.add(
                            TextEdit::singleline(&mut buf.text_override)
                                .id_salt(format!("state_text_{state_name}"))
                                .desired_width(120.0),
                        );
                        buf.text_rect = Some(r_text.rect);
                        let enter =
                            r_text.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if r_text.lost_focus() || enter {
                            let new_text = if buf.text_override.trim().is_empty() {
                                None
                            } else {
                                Some(buf.text_override.clone())
                            };
                            if new_text != state.text {
                                let mut updated = state.clone();
                                updated.text = new_text;
                                if let Ok(state_val) = serde_json::to_value(&updated) {
                                    actions.push((
                                        "object.update_state",
                                        serde_json::json!({
                                            "slide": slide_id,
                                            "object": obj_id_str,
                                            "state": state_val,
                                        }),
                                    ));
                                }
                            }
                        }
                    });
                }

                // Src override edit (for Image)
                if matches!(obj_kind, ObjectKind::Image { .. }) {
                    ui.horizontal(|ui| {
                        ui.label("Src");
                        let r_src = ui.add(
                            TextEdit::singleline(&mut buf.src_override)
                                .id_salt(format!("state_src_{state_name}"))
                                .desired_width(120.0),
                        );
                        buf.src_rect = Some(r_src.rect);
                        let enter =
                            r_src.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if r_src.lost_focus() || enter {
                            let new_src = if buf.src_override.trim().is_empty() {
                                None
                            } else {
                                Some(buf.src_override.trim().to_owned())
                            };
                            if new_src != state.src {
                                let mut updated = state.clone();
                                updated.src = new_src;
                                if let Ok(state_val) = serde_json::to_value(&updated) {
                                    actions.push((
                                        "object.update_state",
                                        serde_json::json!({
                                            "slide": slide_id,
                                            "object": obj_id_str,
                                            "state": state_val,
                                        }),
                                    ));
                                }
                            }
                        }
                    });
                }
            })
            .response;

        editor
            .ui
            .states
            .state_row_rects
            .insert(state_name, row_resp.rect);
    }
}
