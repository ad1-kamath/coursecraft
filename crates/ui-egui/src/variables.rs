//! Variables bottom panel: table of variables, inline value editing, add and delete.

use coursecraft_course::{Course, Value, ValueKind};
use egui::{DragValue, RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value as Json;
use std::collections::HashMap;

/// Human-readable name for a ValueKind.
#[must_use]
pub fn value_kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Bool => "Boolean",
        ValueKind::Number => "Number",
        ValueKind::Text => "Text",
    }
}

pub const ALL_VALUE_KINDS: [ValueKind; 3] = [ValueKind::Bool, ValueKind::Number, ValueKind::Text];

/// Interaction and edit state for the Variables panel.
#[derive(Debug, Clone, PartialEq)]
pub struct VariablesState {
    pub last_synced_revision: u64,

    // Add variable form
    pub new_name: String,
    pub new_kind: ValueKind,
    pub new_val_bool: bool,
    pub new_val_num: f64,
    pub new_val_text: String,

    // In-place edit buffers
    pub edit_num: HashMap<String, f64>,
    pub edit_text: HashMap<String, String>,
}

impl Default for VariablesState {
    fn default() -> Self {
        Self {
            last_synced_revision: u64::MAX,
            new_name: String::new(),
            new_kind: ValueKind::Number,
            new_val_bool: false,
            new_val_num: 0.0,
            new_val_text: String::new(),
            edit_num: HashMap::new(),
            edit_text: HashMap::new(),
        }
    }
}

impl VariablesState {
    /// Resync edit buffers with course variables on revision change.
    pub fn sync(&mut self, course: &Course, revision: u64) {
        if self.last_synced_revision == revision {
            return;
        }
        self.last_synced_revision = revision;

        for v in &course.variables {
            match &v.value {
                Value::Number(n) => {
                    self.edit_num.insert(v.name.clone(), *n);
                }
                Value::Text(t) => {
                    self.edit_text.insert(v.name.clone(), t.clone());
                }
                Value::Bool(_) => {}
            }
        }
    }
}

/// Render the Variables bottom panel.
pub fn show_variables_tab(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    editor
        .ui
        .variables
        .sync(&editor.session.course, editor.session.revision);

    let vars = editor.session.course.variables.clone();

    // 1. Add Variable bar (header button hidden when empty so there is only one primary action)
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Add Variable").color(p.text_secondary));

        ui.add(
            TextEdit::singleline(&mut editor.ui.variables.new_name)
                .hint_text("Name")
                .desired_width(120.0),
        );

        egui::ComboBox::from_id_salt("var_new_type_combo")
            .selected_text(value_kind_name(editor.ui.variables.new_kind))
            .show_ui(ui, |ui| {
                for kind in ALL_VALUE_KINDS {
                    ui.selectable_value(
                        &mut editor.ui.variables.new_kind,
                        kind,
                        value_kind_name(kind),
                    );
                }
            });

        match editor.ui.variables.new_kind {
            ValueKind::Bool => {
                ui.checkbox(&mut editor.ui.variables.new_val_bool, "Initial: True/False");
            }
            ValueKind::Number => {
                ui.add(
                    DragValue::new(&mut editor.ui.variables.new_val_num)
                        .speed(1.0)
                        .prefix("Value "),
                );
            }
            ValueKind::Text => {
                ui.add(
                    TextEdit::singleline(&mut editor.ui.variables.new_val_text)
                        .hint_text("Initial text")
                        .desired_width(140.0),
                );
            }
        }

        let name_valid = !editor.ui.variables.new_name.trim().is_empty();
        if !vars.is_empty() {
            let add_btn = egui::Button::new(coursecraft_render::body("+ Add").color(p.accent_text))
                .fill(egui::Color32::TRANSPARENT);
            if ui.add_enabled(name_valid, add_btn).clicked() {
                let name = editor.ui.variables.new_name.trim().to_owned();
                let val = match editor.ui.variables.new_kind {
                    ValueKind::Bool => Value::Bool(editor.ui.variables.new_val_bool),
                    ValueKind::Number => Value::Number(editor.ui.variables.new_val_num),
                    ValueKind::Text => Value::Text(editor.ui.variables.new_val_text.clone()),
                };

                actions.push((
                    "variable.add",
                    serde_json::json!({
                        "name": name,
                        "value": val,
                    }),
                ));
                editor.ui.variables.new_name.clear();
                editor.ui.variables.new_val_text.clear();
                editor.ui.variables.new_val_num = 0.0;
                editor.ui.variables.new_val_bool = false;
            }
        }
    });

    ui.separator();

    // 2. Variables table
    ScrollArea::vertical()
        .id_salt("variables_table_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if vars.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new(
                            "No variables yet. Variables remember choices and scores across slides.",
                        )
                        .color(p.text_secondary)
                        .size(coursecraft_render::SIZE_BODY),
                    );
                    ui.add_space(12.0);
                    if crate::primary_button(ui, "+ Add variable").clicked() {
                        let name = if editor.ui.variables.new_name.trim().is_empty() {
                            "var1".to_owned()
                        } else {
                            editor.ui.variables.new_name.trim().to_owned()
                        };
                        let val = match editor.ui.variables.new_kind {
                            ValueKind::Bool => Value::Bool(editor.ui.variables.new_val_bool),
                            ValueKind::Number => Value::Number(editor.ui.variables.new_val_num),
                            ValueKind::Text => {
                                Value::Text(editor.ui.variables.new_val_text.clone())
                            }
                        };
                        actions.push((
                            "variable.add",
                            serde_json::json!({
                                "name": name,
                                "value": val,
                            }),
                        ));
                        editor.ui.variables.new_name.clear();
                        editor.ui.variables.new_val_text.clear();
                        editor.ui.variables.new_val_num = 0.0;
                        editor.ui.variables.new_val_bool = false;
                    }
                });
                return;
            }

            egui::Grid::new("variables_grid")
                .striped(true)
                .min_col_width(80.0)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Type").strong());
                    ui.label(RichText::new("Initial Value").strong());
                    ui.label(RichText::new("Actions").strong());
                    ui.end_row();

                    for var in &vars {
                        ui.label(&var.name);
                        ui.label(value_kind_name(var.value.kind()));

                        match &var.value {
                            Value::Bool(b) => {
                                let mut val = *b;
                                if ui.checkbox(&mut val, "").changed() && val != *b {
                                    actions.push((
                                        "variable.set_value",
                                        serde_json::json!({
                                            "name": var.name,
                                            "value": val,
                                        }),
                                    ));
                                }
                            }
                            Value::Number(orig_n) => {
                                let entry = editor
                                    .ui
                                    .variables
                                    .edit_num
                                    .entry(var.name.clone())
                                    .or_insert(*orig_n);
                                let r = ui.add(DragValue::new(entry).speed(1.0));
                                let enter =
                                    r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if (r.drag_stopped() || r.lost_focus() || enter)
                                    && entry.is_finite()
                                    && *entry != *orig_n
                                {
                                    actions.push((
                                        "variable.set_value",
                                        serde_json::json!({
                                            "name": var.name,
                                            "value": *entry,
                                        }),
                                    ));
                                }
                            }
                            Value::Text(orig_t) => {
                                let entry = editor
                                    .ui
                                    .variables
                                    .edit_text
                                    .entry(var.name.clone())
                                    .or_insert_with(|| orig_t.clone());
                                let r = ui.add(TextEdit::singleline(entry).desired_width(180.0));
                                let enter =
                                    r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if (r.lost_focus() || enter) && entry != orig_t {
                                    actions.push((
                                        "variable.set_value",
                                        serde_json::json!({
                                            "name": var.name,
                                            "value": entry.as_str(),
                                        }),
                                    ));
                                }
                            }
                        }

                        if crate::destructive_action_button(ui, "Delete")
                            .on_hover_text("Delete variable")
                            .clicked()
                        {
                            actions.push((
                                "variable.delete",
                                serde_json::json!({
                                    "name": var.name,
                                }),
                            ));
                        }

                        ui.end_row();
                    }
                });
        });
}
