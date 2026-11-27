//! States side panel section: lists normal and custom states with add, edit, delete, and initial state.

use crate::Editor;
use crate::properties::{color_swatch_picker, is_committed, mark_color_picked};
use coursecraft_course::{
    Action, Color, Condition, Course, Event, Object, ObjectKind, Size, Slide, StateDef, Trigger,
    Value,
};
use coursecraft_render::{
    DrawItem, Highlight, SlideTransform, compose_object_items, icon, paint, parse_color,
};
use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, pos2, vec2,
};
use serde_json::Value as Json;
use std::collections::HashMap;

pub const SUGGESTED_STATE_NAMES: &[&str] = &["visited", "selected", "disabled", "hover", "down"];

const MAX_STATE_NAME_LEN: usize = 64;
const CARD_WIDTH: f32 = 140.0;
const CARD_HEIGHT: f32 = 128.0;
const THUMB_WIDTH: f32 = 120.0;
const THUMB_HEIGHT: f32 = 72.0;

/// Edit buffers for a single custom state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateEditBuffer {
    pub name: String,
    pub fill_hex: String,
    pub text_override: String,

    pub fill_rect: Option<Rect>,
    pub text_rect: Option<Rect>,
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
    pub show_new_state_input: bool,
    pub focus_override_bar: bool,
    pub renaming_state: Option<String>,
    pub rename_buffer: String,
    pub rename_just_started: bool,
    pub card_strip_focused: bool,
    pub edit_buffers: HashMap<String, StateEditBuffer>,
    pub delete_error: Option<(String, String)>,

    // Tracked rects for test interaction
    pub new_state_name_rect: Option<Rect>,
    pub add_button_rect: Option<Rect>,
    pub state_row_rects: HashMap<String, Rect>,

    /// Composed state cards by (object id, state name): see [`ThumbCard`].
    pub thumb_cards: HashMap<(String, String), ThumbCard>,
}

impl StatesState {
    /// Re-sync edit buffers with current course data if selection or revision changed.
    pub fn sync(&mut self, course: &Course, selection: &crate::Selection, revision: u64) {
        if self.last_synced_revision.is(revision) && &self.last_synced_selection == selection {
            return;
        }
        self.last_synced_revision.set(revision);
        self.last_synced_selection = selection.clone();
        self.delete_error = None;

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
                // Wide enough for "#RRGGBB" after the field's own padding.
                .desired_width(86.0),
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

fn push_add_state(
    actions: &mut Vec<(&'static str, Json)>,
    slide_id: &str,
    obj_id: &str,
    obj_kind: &ObjectKind,
    name: &str,
) {
    let default_fill = match obj_kind {
        ObjectKind::Button { fill, .. } | ObjectKind::Shape { fill, .. } => Some(fill.clone()),
        ObjectKind::Path { path } if path.closed => path.fill.clone(),
        _ => None,
    };
    let state_def = StateDef {
        name: name.to_owned(),
        fill: default_fill,
        text: None,
        src: None,
        ..Default::default()
    };
    if let Ok(state_val) = serde_json::to_value(&state_def) {
        actions.push((
            "object.add_state",
            serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
                "state": state_val,
            }),
        ));
    }
}

fn validate_new_state_name(trimmed: &str, existing: &[StateDef]) -> Result<(), String> {
    if trimmed.is_empty() {
        return Err("State name cannot be empty".to_owned());
    }
    if trimmed.eq_ignore_ascii_case("normal") {
        return Err("'normal' is a built-in state name".to_owned());
    }
    if trimmed.eq_ignore_ascii_case("hidden") {
        return Err("Hidden is built in — choose it in Initial or a trigger".to_owned());
    }
    if trimmed.chars().count() > MAX_STATE_NAME_LEN {
        return Err(format!(
            "State name cannot exceed {MAX_STATE_NAME_LEN} characters"
        ));
    }
    if existing.iter().any(|s| s.name == trimmed) {
        return Err(format!("State '{trimmed}' already exists"));
    }
    Ok(())
}

fn condition_references_state(cond: &Condition, obj_id: &str, state_name: &str) -> bool {
    match cond {
        Condition::ObjectState { object, state, .. } => object == obj_id && state == state_name,
        Condition::All(list) | Condition::Any(list) => list
            .iter()
            .any(|c| condition_references_state(c, obj_id, state_name)),
        Condition::Not(inner) => condition_references_state(inner, obj_id, state_name),
        Condition::Compare { .. } => false,
    }
}

fn trigger_references_state(trig: &Trigger, obj_id: &str, state_name: &str) -> bool {
    let in_event = matches!(
        &trig.when,
        Event::StateChanges { object, to: Some(to) } if object == obj_id && to == state_name
    );
    let in_action = matches!(
        &trig.action,
        Action::ChangeState { object, state } if object == obj_id && state == state_name
    );
    let in_cond = trig
        .condition
        .as_ref()
        .is_some_and(|c| condition_references_state(c, obj_id, state_name));
    in_event || in_action || in_cond
}

fn state_delete_blocker(
    slide: &Slide,
    obj_id: &str,
    state_name: &str,
    initial_state: &str,
) -> Option<String> {
    if initial_state == state_name {
        return Some(format!(
            "Cannot delete '{state_name}': it is the object's initial state"
        ));
    }
    let used_by_trigger = slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .any(|t| trigger_references_state(t, obj_id, state_name));
    if used_by_trigger {
        return Some(format!(
            "Cannot delete '{state_name}': referenced by a trigger"
        ));
    }
    let used_by_question = slide
        .question
        .as_ref()
        .and_then(|q| q.correct_when.as_ref())
        .is_some_and(|c| condition_references_state(c, obj_id, state_name));
    if used_by_question {
        return Some(format!(
            "Cannot delete '{state_name}': referenced by the slide question"
        ));
    }
    None
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
    if let Some(ref preview) = editor.ui.preview_state
        && !obj.has_state(preview)
    {
        editor.ui.preview_state = None;
    }

    let delete_blockers: HashMap<String, String> = states_copy
        .iter()
        .filter_map(|s| {
            state_delete_blocker(slide, &obj_id_str, &s.name, &initial_state)
                .map(|reason| (s.name.clone(), reason))
        })
        .collect();

    // 1. Add State form
    ui.horizontal(|ui| {
        let r_name = ui.add(
            TextEdit::singleline(&mut editor.ui.states.new_state_name)
                .id_salt("state_add_name_edit")
                .desired_width(110.0)
                .hint_text("state name"),
        );
        editor.ui.states.new_state_name_rect = Some(r_name.rect);

        let trimmed = editor.ui.states.new_state_name.trim().to_owned();
        let validation = validate_new_state_name(&trimmed, &states_copy);
        let is_valid = validation.is_ok();

        let add_btn = ui
            .add_enabled(is_valid, egui::Button::new("+ Add"))
            .on_disabled_hover_text(
                validation
                    .as_ref()
                    .err()
                    .map_or("Enter a state name", String::as_str),
            );
        editor.ui.states.add_button_rect = Some(add_btn.rect);

        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
            && (r_name.has_focus() || r_name.lost_focus());
        if (add_btn.clicked() || enter) && is_valid {
            push_add_state(actions, &slide_id, &obj_id_str, &obj_kind, &trimmed);
            editor.ui.states.new_state_name.clear();
        } else if enter && let Err(err) = validation {
            editor.ui.last_error = Some((err, editor.ui.current_time));
        }
    });

    ui.horizontal_wrapped(|ui| {
        for (state_name, button_label) in [
            ("hover", "+ Hover"),
            ("selected", "+ Selected"),
            ("visited", "+ Visited"),
            ("disabled", "+ Disabled"),
        ] {
            let already_added = states_copy.iter().any(|s| s.name == state_name);
            if ui
                .add_enabled(
                    !already_added,
                    egui::Button::new(button_label).min_size(egui::vec2(0.0, 24.0)),
                )
                .on_disabled_hover_text("This state is already added")
                .clicked()
            {
                push_add_state(actions, &slide_id, &obj_id_str, &obj_kind, state_name);
            }
        }
    });

    crate::properties::inspector_row(ui, "Preset", |ui| {
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, "states_suggestion_combo", rem_w)
            .selected_text("Preset name…")
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
                let btn = ui.button("Set initial");
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

        let blocker = delete_blockers.get(&state_name).cloned();
        let delete_err = editor
            .ui
            .states
            .delete_error
            .as_ref()
            .and_then(|(s, msg)| (s == &state_name).then(|| msg.clone()));

        let mut pick_state_image = false;
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
                        let btn = ui.button("Set initial");
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
                        if let Some(reason) = blocker.clone() {
                            editor.ui.states.delete_error = Some((state_name.clone(), reason));
                        }
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
                if let Some(msg) = delete_err {
                    let p = coursecraft_render::palette(ui.ctx());
                    ui.label(coursecraft_render::caption(msg).color(p.danger));
                } else if let Some(reason) = blocker {
                    let p = coursecraft_render::palette(ui.ctx());
                    ui.label(coursecraft_render::caption(reason).color(p.text_secondary));
                }

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

                // Image override picker (for Image)
                if matches!(obj_kind, ObjectKind::Image { .. }) {
                    let p = coursecraft_render::palette(ui.ctx());
                    let current_src = state.src.as_deref().filter(|s| !s.trim().is_empty());
                    let is_unsafe =
                        current_src.is_some_and(|s| !coursecraft_format::is_safe_relative_path(s));

                    ui.horizontal(|ui| {
                        if let Some(src) = current_src {
                            if is_unsafe {
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(src).color(p.danger))
                                            .truncate(),
                                    )
                                    .on_hover_text(src);
                                    ui.label(
                                        RichText::new("File not in the course. Choose image…")
                                            .color(p.danger)
                                            .size(coursecraft_render::SIZE_BODY),
                                    );
                                });
                            } else {
                                let thumb_size = egui::vec2(28.0, 28.0);
                                let (thumb_rect, thumb_resp) =
                                    ui.allocate_exact_size(thumb_size, egui::Sense::hover());
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                                    p.hover,
                                );
                                let source = crate::MemoryMediaSource::from_store(&editor.media);
                                if let coursecraft_render::MediaView::Ready { texture, .. } = editor
                                    .media_cache
                                    .texture_for(ui.ctx(), src, &source, (28, 28))
                                {
                                    ui.painter().image(
                                        texture.id(),
                                        thumb_rect,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().text(
                                        thumb_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        coursecraft_render::icon(
                                            coursecraft_render::Icon::Image,
                                            coursecraft_render::ICON_SM,
                                        )
                                        .text()
                                        .to_owned(),
                                        egui::FontId::new(
                                            coursecraft_render::ICON_SM,
                                            egui::FontFamily::Name("phosphor".into()),
                                        ),
                                        p.text_secondary,
                                    );
                                }
                                thumb_resp.on_hover_text(src);

                                let file_name = src.rsplit('/').next().unwrap_or(src);
                                ui.add(egui::Label::new(file_name).truncate())
                                    .on_hover_text(src);
                            }
                        } else {
                            ui.label(
                                RichText::new("Same as normal")
                                    .color(p.text_secondary)
                                    .size(coursecraft_render::SIZE_BODY),
                            );
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        let choose_label = if current_src.is_some() && !is_unsafe {
                            "Replace…"
                        } else {
                            "Choose image…"
                        };
                        if ui.button(choose_label).clicked() {
                            pick_state_image = true;
                        }

                        ui.menu_button("From project…", |ui| {
                            let images: Vec<String> = editor
                                .media
                                .iter()
                                .map(|(name, _)| name)
                                .filter(|name| coursecraft_course::is_image_src(name))
                                .cloned()
                                .collect();
                            if images.is_empty() {
                                ui.label(
                                    RichText::new("No images in project")
                                        .color(p.text_secondary)
                                        .size(coursecraft_render::SIZE_BODY),
                                );
                            } else {
                                for name in images {
                                    let selected = current_src == Some(name.as_str());
                                    if ui.selectable_label(selected, &name).clicked() {
                                        let mut updated = state.clone();
                                        updated.src = Some(name);
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
                                        ui.close();
                                    }
                                }
                            }
                        });

                        if state.src.is_some()
                            && crate::quiet_button(ui, "Clear", crate::QuietTone::Secondary, true)
                                .clicked()
                        {
                            let mut updated = state.clone();
                            updated.src = None;
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
                    });
                }
            })
            .response;

        editor
            .ui
            .states
            .state_row_rects
            .insert(state_name.clone(), row_resp.rect);

        if pick_state_image {
            editor.pending_image_pick = Some(crate::PendingImagePick::StateImage {
                slide: slide_id.clone(),
                object: obj_id_str.clone(),
                state: state_name,
            });
            editor.request(crate::EditorRequest::PickImage);
        }
    }
}

/// The value an input object reads on the authoring canvas: the variable's
/// default, or `None` when the course has no such variable yet.
pub(crate) fn course_input_value<'a>(course: &'a Course, obj: &Object) -> Option<&'a Value> {
    obj.kind
        .input_variable()
        .and_then(|(name, _)| course.variable(name).map(|def| &def.value))
}

/// Compact "States" section in the Format inspector: `Initial` combo + `Open States tab (N)` link.
pub fn show_compact_states_section(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let Some(ref obj_id) = editor.selection.object else {
        return;
    };
    let Some((obj, _)) = slide.object(obj_id) else {
        return;
    };

    let slide_id = slide.id.clone();
    let obj_id_str = obj.id.clone();
    let mut initial_state = obj.initial_state.clone();
    let mut available_states: Vec<String> = vec!["normal".to_owned(), "hidden".to_owned()];
    for s in &obj.states {
        if !available_states.iter().any(|existing| existing == &s.name) {
            available_states.push(s.name.clone());
        }
    }

    crate::properties::inspector_row(ui, "Initial", |ui| {
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, "prop_initial_state", rem_w)
            .selected_text(&initial_state)
            .show_ui(ui, |ui| {
                for state in &available_states {
                    if ui
                        .selectable_value(&mut initial_state, state.clone(), state)
                        .clicked()
                        && initial_state != obj.initial_state
                    {
                        actions.push((
                            "object.update",
                            serde_json::json!({
                                "slide": slide_id,
                                "object": obj_id_str,
                                "initial_state": state,
                            }),
                        ));
                    }
                }
            });
    });

    let custom_count = obj.states.len();
    let open_label = if custom_count == 0 {
        "Open States tab".to_owned()
    } else {
        format!("Open States tab ({custom_count})")
    };
    crate::properties::inspector_row(ui, "States", |ui| {
        if ui
            .add(egui::Button::new(open_label).min_size(vec2(0.0, 24.0)))
            .clicked()
        {
            crate::dock::request_bottom_tab(editor, crate::BottomTab::States);
        }
    });
}

fn paint_checkerboard(painter: &egui::Painter, rect: Rect, is_dark: bool) {
    let cell = 6.0;
    let a = if is_dark {
        Color32::from_gray(46)
    } else {
        Color32::from_gray(236)
    };
    let b = if is_dark {
        Color32::from_gray(58)
    } else {
        Color32::from_gray(220)
    };
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for r in 0..rows {
        for c in 0..cols {
            let x0 = rect.min.x + c as f32 * cell;
            let y0 = rect.min.y + r as f32 * cell;
            let cell_rect = Rect::from_min_max(
                pos2(x0, y0),
                pos2((x0 + cell).min(rect.max.x), (y0 + cell).min(rect.max.y)),
            );
            let fill = if (r + c) % 2 == 0 { a } else { b };
            painter.rect_filled(cell_rect, CornerRadius::ZERO, fill);
        }
    }
}

fn object_needs_checkerboard(obj: &Object, state_name: &str) -> bool {
    if matches!(obj.kind, ObjectKind::Image { .. }) {
        return true;
    }
    if state_name != "normal"
        && let Some(st) = obj.states.iter().find(|s| s.name == state_name)
        && let Some(ref fill) = st.fill
        && let Some(c) = parse_color(fill)
    {
        return c.a() < 255;
    }
    match &obj.kind {
        ObjectKind::Button { fill, .. } | ObjectKind::Shape { fill, .. } => {
            parse_color(fill).is_some_and(|c| c.a() < 255)
        }
        ObjectKind::Path { path } => path
            .fill
            .as_ref()
            .and_then(parse_color)
            .is_some_and(|c| c.a() < 255),
        _ => false,
    }
}

/// Compose the parent object + any `StateExtra` items for a state thumbnail card.
pub(crate) fn compose_thumbnail_items(
    course: &Course,
    obj: &Object,
    state_name: &str,
) -> Vec<DrawItem> {
    compose_object_items(
        obj,
        state_name,
        Some(&course.theme),
        course_input_value(course, obj),
    )
}

/// Cards kept composed; a stale or oversized cache is dropped whole.
const MAX_CACHED_CARDS: usize = 64;

/// One state card's composed items, with what they were composed from.
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbCard {
    revision: u64,
    media_generation: u64,
    obj: Object,
    /// The items moved to the origin, and the size they fit in; `None` when
    /// the card draws nothing.
    items: Option<(Vec<DrawItem>, f32, f32)>,
}

fn compose_thumb_card(editor: &Editor, obj: &Object, state_name: &str) -> ThumbCard {
    let mut card = ThumbCard {
        revision: editor.session.revision,
        media_generation: editor.media.generation(),
        obj: obj.clone(),
        items: None,
    };
    let mut items = compose_thumbnail_items(&editor.session.course, obj, state_name);
    if items.is_empty() {
        return card;
    }
    crate::canvas::missing_images_as_placeholders(
        &mut items,
        &editor.media,
        Some(editor.media.index()),
    );
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for it in &items {
        min_x = min_x.min(it.rect.min.x);
        min_y = min_y.min(it.rect.min.y);
        max_x = max_x.max(it.rect.max.x);
        max_y = max_y.max(it.rect.max.y);
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return card;
    }
    let raw_w = (max_x - min_x).max(1.0);
    let raw_h = (max_y - min_y).max(1.0);
    let pad = (raw_w.max(raw_h) * 0.10).max(2.0);
    let origin_x = min_x - pad;
    let origin_y = min_y - pad;
    let bounds_w = (raw_w + pad * 2.0).max(1.0);
    let bounds_h = (raw_h + pad * 2.0).max(1.0);
    for it in &mut items {
        it.rect = it.rect.translate(vec2(-origin_x, -origin_y));
    }
    card.items = Some((items, bounds_w, bounds_h));
    card
}

fn paint_state_thumbnail(
    editor: &mut Editor,
    ui: &mut Ui,
    slide: &Slide,
    obj: &Object,
    state_name: &str,
    thumb_rect: Rect,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let is_dark = ui.style().visuals.dark_mode;
    let clipped_rect = thumb_rect.intersect(ui.clip_rect());
    if !clipped_rect.is_positive() {
        return;
    }
    let painter = ui.painter_at(clipped_rect);
    crate::content_region::mark_content_region(ui.ctx(), ui.layer_id(), clipped_rect);
    let bg_color =
        parse_color(editor.session.course.slide_background(slide)).unwrap_or(Color32::WHITE);
    painter.rect_filled(
        thumb_rect,
        CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
        bg_color,
    );

    if object_needs_checkerboard(obj, state_name) {
        paint_checkerboard(&painter, thumb_rect, is_dark);
    }

    #[cfg(test)]
    crate::guard_counters::note_state_thumbnail_paint(&obj.id, state_name, obj);

    if let ObjectKind::Audio { src, .. } = &obj.kind {
        painter.text(
            pos2(thumb_rect.center().x, thumb_rect.center().y - 8.0),
            Align2::CENTER_CENTER,
            icon(coursecraft_render::Icon::SpeakerHigh, 20.0).text(),
            FontId::new(20.0, egui::FontFamily::Name("phosphor".into())),
            p.text_secondary,
        );
        let file_name = src.rsplit('/').next().unwrap_or(src);
        painter.text(
            pos2(thumb_rect.center().x, thumb_rect.center().y + 14.0),
            Align2::CENTER_CENTER,
            file_name,
            FontId::proportional(10.0),
            p.text_secondary,
        );
        return;
    }

    // Composing a card is not free, and a card changes rarely: keep the
    // composed items until the course, the media or the object change.
    let key = (obj.id.clone(), state_name.to_owned());
    let revision = editor.session.revision;
    let media_generation = editor.media.generation();
    let fresh = editor.ui.states.thumb_cards.get(&key).is_some_and(|c| {
        c.revision == revision && c.media_generation == media_generation && c.obj == *obj
    });
    if !fresh {
        if editor.ui.states.thumb_cards.len() > MAX_CACHED_CARDS {
            editor.ui.states.thumb_cards.clear();
        }
        let card = compose_thumb_card(editor, obj, state_name);
        editor.ui.states.thumb_cards.insert(key.clone(), card);
    }
    let Some(card) = editor.ui.states.thumb_cards.get(&key) else {
        return;
    };
    let Some((items, bounds_w, bounds_h)) = &card.items else {
        return;
    };

    let Some(t) = SlideTransform::fit(
        thumb_rect,
        Size {
            w: bounds_w.round().max(1.0) as u32,
            h: bounds_h.round().max(1.0) as u32,
        },
    ) else {
        return;
    };
    let source = crate::MemoryMediaSource::from_store(&editor.media);
    paint(
        &painter,
        &t,
        items,
        Highlight::default(),
        Some((&mut editor.media_cache, &source)),
        Some(&editor.session.course.theme),
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn start_inline_rename(editor: &mut Editor, state_name: &str) {
    editor.ui.states.renaming_state = Some(state_name.to_owned());
    editor.ui.states.rename_buffer = state_name.to_owned();
    editor.ui.states.rename_just_started = true;
}

/// Return `true` if `state` has any visual override or explicit `clear_*` flag set.
pub(crate) fn state_has_overrides(state: &StateDef) -> bool {
    state.fill.is_some()
        || state.text.is_some()
        || state.src.is_some()
        || state.fill_style.is_some()
        || state.clear_fill_style
        || state.outline.is_some()
        || state.clear_outline
        || state.text_style.is_some()
        || state.runs.is_some()
        || state.paragraphs.is_some()
        || state.autofit.is_some()
        || state.opacity.is_some()
        || state.shadow.is_some()
        || state.clear_shadow
        || state.glow.is_some()
        || state.clear_glow
        || state.crop.is_some()
        || state.clear_crop
        || state.image_shape.is_some()
        || state.clear_image_shape
        || state.rotation.is_some()
        || state.flip_h.is_some()
        || state.flip_v.is_some()
}

fn select_card_state(editor: &mut Editor, obj: &Object, state_name: &str) {
    editor.ui.states.card_strip_focused = true;
    if state_name == "normal" {
        editor.exit_edit_state();
        editor.ui.preview_state = if obj.initial_state != "normal" {
            Some("normal".to_owned())
        } else {
            None
        };
    } else {
        if let Some(session) = editor.ui.editing_state.as_mut() {
            session.state_name = state_name.to_owned();
            session.selected_extra = None;
        }
        editor.ui.preview_state = Some(state_name.to_owned());
    }
}

#[allow(clippy::too_many_arguments)]
fn show_state_card(
    editor: &mut Editor,
    ui: &mut Ui,
    slide: &Slide,
    obj: &Object,
    state_name: &str,
    display_name: &str,
    is_initial: bool,
    is_active: bool,
    custom_index: Option<usize>,
    total_custom: usize,
    actions: &mut Vec<(&'static str, Json)>,
) -> Rect {
    let is_custom = custom_index.is_some();
    let p = coursecraft_render::palette(ui.ctx());
    let (card_rect, card_resp) =
        ui.allocate_exact_size(vec2(CARD_WIDTH, CARD_HEIGHT), Sense::click());
    card_resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{display_name} state"))
    });

    let fill = if is_active { p.hover } else { p.raised };
    let stroke = if is_active {
        Stroke::new(2.0, p.accent)
    } else {
        Stroke::new(1.0, p.border)
    };
    ui.painter().rect(
        card_rect,
        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
        fill,
        stroke,
        StrokeKind::Inside,
    );

    let thumb_rect = Rect::from_min_size(
        pos2(card_rect.min.x + 10.0, card_rect.min.y + 8.0),
        vec2(THUMB_WIDTH, THUMB_HEIGHT),
    );
    if thumb_rect.intersects(ui.clip_rect()) {
        paint_state_thumbnail(editor, ui, slide, obj, state_name, thumb_rect);
    }

    let footer_rect = Rect::from_min_max(
        pos2(card_rect.min.x + 8.0, thumb_rect.max.y + 6.0),
        pos2(card_rect.max.x - 8.0, card_rect.max.y - 6.0),
    );
    let mut menu_clicked = false;
    let mut label_clicked = false;
    let is_renaming = is_custom && editor.ui.states.renaming_state.as_deref() == Some(state_name);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(footer_rect));
    child.horizontal(|ui| {
        let label_w = if is_initial { 48.0 } else { 88.0 };
        if is_renaming {
            let r_rename = ui.add(
                TextEdit::singleline(&mut editor.ui.states.rename_buffer)
                    .id_salt(format!("state_card_rename_{state_name}"))
                    .desired_width(label_w),
            );
            if editor.ui.states.rename_just_started {
                r_rename.request_focus();
                editor.ui.states.rename_just_started = false;
            }
            let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            let enter = r_rename.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if esc {
                editor.ui.states.renaming_state = None;
                editor.ui.states.rename_just_started = false;
            } else if enter || r_rename.lost_focus() {
                let trimmed = editor.ui.states.rename_buffer.trim().to_owned();
                if trimmed == state_name {
                    editor.ui.states.renaming_state = None;
                } else {
                    let other_states: Vec<StateDef> = obj
                        .states
                        .iter()
                        .filter(|s| s.name != state_name)
                        .cloned()
                        .collect();
                    match validate_new_state_name(&trimmed, &other_states) {
                        Ok(()) => {
                            actions.push((
                                "object.rename_state",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": obj.id,
                                    "from": state_name,
                                    "to": trimmed,
                                }),
                            ));
                            if editor.ui.preview_state.as_deref() == Some(state_name) {
                                editor.ui.preview_state = Some(trimmed.clone());
                            }
                            if let Some(session) = editor.ui.editing_state.as_mut()
                                && session.state_name == state_name
                            {
                                session.state_name = trimmed;
                            }
                            editor.ui.states.renaming_state = None;
                        }
                        Err(err) => {
                            if enter {
                                editor.ui.last_error = Some((err, editor.ui.current_time));
                            } else {
                                editor.ui.states.renaming_state = None;
                            }
                        }
                    }
                }
            }
        } else {
            let label_resp = ui
                .add_sized(
                    vec2(label_w, 24.0),
                    egui::Label::new(RichText::new(display_name).strong())
                        .truncate()
                        .sense(Sense::click()),
                )
                .on_hover_text(display_name);
            if is_custom && label_resp.double_clicked() {
                start_inline_rename(editor, state_name);
                label_clicked = true;
            } else if label_resp.clicked() {
                label_clicked = true;
                select_card_state(editor, obj, state_name);
            }
        }

        if is_initial && !is_renaming {
            egui::Frame::new()
                .fill(p.hover)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(4, 1))
                .show(ui, |ui| {
                    ui.label(RichText::new("Initial").size(10.0).color(p.text_secondary));
                });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let menu_btn = ui
                .scope(|ui| {
                    ui.spacing_mut().interact_size = vec2(24.0, 24.0);
                    ui.menu_button(
                        RichText::new("⋯").size(coursecraft_render::SIZE_BODY),
                        |ui| {
                            menu_clicked = true;
                            if is_custom && ui.button("Edit state").clicked() {
                                editor.enter_edit_state(&slide.id, &obj.id, state_name);
                                ui.close();
                            }
                            if is_custom && ui.button("Duplicate").clicked() {
                                actions.push((
                                    "object.duplicate_state",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "name": state_name,
                                    }),
                                ));
                                ui.close();
                            }
                            if is_custom && ui.button("Rename…").clicked() {
                                start_inline_rename(editor, state_name);
                                ui.close();
                            }
                            if ui
                                .add_enabled(!is_initial, egui::Button::new("Set as initial"))
                                .on_disabled_hover_text("Already the initial state")
                                .clicked()
                            {
                                actions.push((
                                    "object.update",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "initial_state": state_name,
                                    }),
                                ));
                                ui.close();
                            }
                            if let Some(idx) = custom_index {
                                if ui
                                    .add_enabled(idx > 0, egui::Button::new("Move left"))
                                    .on_disabled_hover_text("Already first custom state")
                                    .clicked()
                                {
                                    actions.push((
                                        "object.reorder_state",
                                        serde_json::json!({
                                            "slide": slide.id,
                                            "object": obj.id,
                                            "name": state_name,
                                            "index": idx - 1,
                                        }),
                                    ));
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        idx + 1 < total_custom,
                                        egui::Button::new("Move right"),
                                    )
                                    .on_disabled_hover_text("Already last custom state")
                                    .clicked()
                                {
                                    actions.push((
                                        "object.reorder_state",
                                        serde_json::json!({
                                            "slide": slide.id,
                                            "object": obj.id,
                                            "name": state_name,
                                            "index": idx + 1,
                                        }),
                                    ));
                                    ui.close();
                                }
                                ui.separator();
                                let blocker = state_delete_blocker(
                                    slide,
                                    &obj.id,
                                    state_name,
                                    &obj.initial_state,
                                );
                                let del_btn = ui
                                    .add_enabled(blocker.is_none(), egui::Button::new("Delete"))
                                    .on_disabled_hover_text(
                                        blocker.as_deref().unwrap_or("Cannot delete state"),
                                    );
                                if del_btn.clicked() {
                                    actions.push((
                                        "object.delete_state",
                                        serde_json::json!({
                                            "slide": slide.id,
                                            "object": obj.id,
                                            "name": state_name,
                                        }),
                                    ));
                                    ui.close();
                                }
                            }
                        },
                    )
                })
                .inner;
            menu_btn.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, "State actions")
            });
            menu_btn.response.on_hover_text("State actions");
        });
    });

    if is_custom && card_resp.double_clicked() && !menu_clicked && !label_clicked {
        editor.enter_edit_state(&slide.id, &obj.id, state_name);
    } else if card_resp.clicked() && !menu_clicked && !label_clicked {
        select_card_state(editor, obj, state_name);
    }

    card_rect
}

/// Render the `BottomTab::States` panel.
pub fn show_states_tab(editor: &mut Editor, ui: &mut Ui, actions: &mut Vec<(&'static str, Json)>) {
    if editor.ui.states.last_synced_selection.object.is_some()
        && editor.ui.states.last_synced_selection != editor.selection
    {
        editor.ui.preview_state = None;
        editor.ui.states.show_new_state_input = false;
        editor.ui.states.renaming_state = None;
        editor.ui.states.card_strip_focused = false;
    }
    editor.ui.states.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
    );

    let p = coursecraft_render::palette(ui.ctx());

    if editor.selection.is_multi() {
        let count = 1 + editor.selection.also.len();
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(format!(
                    "Select a single object to edit its states ({count} selected)."
                ))
                .color(p.text_secondary),
            );
        });
        return;
    }

    let Some(slide) = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .cloned()
    else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select an object on the slide to view and edit its states.")
                    .color(p.text_secondary),
            );
        });
        return;
    };
    let Some(ref obj_id) = editor.selection.object else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select an object on the slide to view and edit its states.")
                    .color(p.text_secondary),
            );
        });
        return;
    };
    let Some((obj_ref, _)) = slide.object(obj_id) else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select an object on the slide to view and edit its states.")
                    .color(p.text_secondary),
            );
        });
        return;
    };
    let obj = obj_ref.clone();

    if let Some(ref preview) = editor.ui.preview_state
        && !obj.has_state(preview)
    {
        editor.ui.preview_state = None;
    }

    let obj_display_name = if obj.name.trim().is_empty() {
        obj.id.as_str()
    } else {
        obj.name.as_str()
    };
    let total_states = 1 + obj.states.len();
    let count_label = if total_states == 1 {
        "1 state".to_owned()
    } else {
        format!("{total_states} states")
    };

    // Header strip (32 pt height, PANEL_MARGIN horizontal padding)
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(
            coursecraft_render::PANEL_MARGIN as i8,
            4,
        ))
        .show(ui, |ui| {
            ui.set_min_height(32.0);
            let wide_header = ui.available_width() >= 900.0;
            let show_edit_btn = |ui: &mut Ui, editor: &mut Editor| {
                let custom_active = editor
                    .ui
                    .preview_state
                    .as_deref()
                    .is_some_and(|s| s != "normal");
                if ui
                    .add_enabled(
                        custom_active,
                        egui::Button::new("Edit state").min_size(vec2(0.0, 32.0)),
                    )
                    .on_disabled_hover_text("Select a custom state to edit its overrides")
                    .clicked()
                    && let Some(state_name) =
                        editor.ui.preview_state.clone().filter(|s| s != "normal")
                {
                    editor.enter_edit_state(&slide.id, &obj.id, &state_name);
                }
            };
            let show_identity = |ui: &mut Ui, actions: &mut Vec<(&'static str, Json)>| {
                ui.scope(|ui| {
                    ui.set_max_width(140.0);
                    ui.add(egui::Label::new(RichText::new(obj_display_name).strong()).truncate())
                        .on_hover_text(obj_display_name);
                });
                ui.label(RichText::new(&count_label).color(p.text_secondary));

                ui.separator();
                ui.label("Initial");
                let mut initial = obj.initial_state.clone();
                let mut initial_choices = vec!["normal".to_owned(), "hidden".to_owned()];
                for s in &obj.states {
                    if !initial_choices.iter().any(|c| c == &s.name) {
                        initial_choices.push(s.name.clone());
                    }
                }
                crate::widgets::styled_combo(ui, "states_tab_initial_combo", 104.0)
                    .selected_text(&initial)
                    .show_ui(ui, |ui| {
                        for choice in &initial_choices {
                            if ui
                                .selectable_value(&mut initial, choice.clone(), choice)
                                .clicked()
                                && initial != obj.initial_state
                            {
                                actions.push((
                                    "object.update",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "initial_state": choice,
                                    }),
                                ));
                            }
                        }
                    });
                if obj.initial_state == "hidden" {
                    egui::Frame::new()
                        .fill(p.hover)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Starts hidden")
                                    .size(coursecraft_render::SIZE_BODY)
                                    .color(p.text_secondary),
                            );
                        });
                }
            };
            let show_chips =
                |ui: &mut Ui, editor: &mut Editor, actions: &mut Vec<(&'static str, Json)>| {
                    for (state_name, button_label) in [
                        ("hover", "+ Hover"),
                        ("down", "+ Down"),
                        ("selected", "+ Selected"),
                        ("visited", "+ Visited"),
                        ("disabled", "+ Disabled"),
                    ] {
                        let already_added = obj.states.iter().any(|s| s.name == state_name);
                        if ui
                            .add_enabled(
                                !already_added,
                                egui::Button::new(button_label).min_size(vec2(0.0, 24.0)),
                            )
                            .on_disabled_hover_text("Already added")
                            .clicked()
                        {
                            push_add_state(actions, &slide.id, &obj.id, &obj.kind, state_name);
                            editor.ui.preview_state = Some(state_name.to_owned());
                        }
                    }
                    if ui
                        .add(egui::Button::new("+ New state").min_size(vec2(0.0, 24.0)))
                        .clicked()
                    {
                        editor.ui.states.show_new_state_input =
                            !editor.ui.states.show_new_state_input;
                    }
                };
            if wide_header {
                ui.horizontal(|ui| {
                    show_identity(ui, actions);
                    ui.separator();
                    show_chips(ui, editor, actions);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        show_edit_btn(ui, editor);
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    show_identity(ui, actions);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        show_edit_btn(ui, editor);
                    });
                });
                ui.horizontal(|ui| {
                    show_chips(ui, editor, actions);
                });
            }

            if editor.ui.states.show_new_state_input {
                ui.horizontal(|ui| {
                    let r_name = ui.add(
                        TextEdit::singleline(&mut editor.ui.states.new_state_name)
                            .id_salt("states_tab_new_name_edit")
                            .desired_width(140.0)
                            .hint_text("state name"),
                    );
                    editor.ui.states.new_state_name_rect = Some(r_name.rect);
                    let trimmed = editor.ui.states.new_state_name.trim().to_owned();
                    let is_hidden = trimmed.eq_ignore_ascii_case("hidden");
                    let validation = if is_hidden {
                        Err("Hidden is built in — choose it in Initial or a trigger".to_owned())
                    } else {
                        validate_new_state_name(&trimmed, &obj.states)
                    };
                    let is_valid = validation.is_ok();
                    let add_btn = ui
                        .add_enabled(is_valid, egui::Button::new("Add").min_size(vec2(0.0, 24.0)))
                        .on_disabled_hover_text(
                            validation
                                .as_ref()
                                .err()
                                .map_or("Enter a state name", String::as_str),
                        );
                    editor.ui.states.add_button_rect = Some(add_btn.rect);
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && (r_name.has_focus() || r_name.lost_focus());
                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape)) && r_name.has_focus();
                    if esc {
                        editor.ui.states.show_new_state_input = false;
                    } else if (add_btn.clicked() || enter) && is_valid {
                        push_add_state(actions, &slide.id, &obj.id, &obj.kind, &trimmed);
                        editor.ui.preview_state = Some(trimmed);
                        editor.ui.states.new_state_name.clear();
                        editor.ui.states.show_new_state_input = false;
                    } else if enter && let Err(err) = validation {
                        editor.ui.last_error = Some((err, editor.ui.current_time));
                    } else if is_hidden {
                        ui.label(
                            RichText::new("Hidden is built in — choose it in Initial or a trigger")
                                .color(p.text_secondary),
                        );
                    }
                });
            }
        });

    ui.separator();

    let active_custom_state = editor
        .ui
        .preview_state
        .as_deref()
        .and_then(|name| obj.states.iter().find(|s| s.name == name))
        .cloned();

    let avail_w = ui.available_width();
    let override_w = if active_custom_state.is_some() {
        (avail_w * 0.34)
            .clamp(180.0, 320.0)
            .min((avail_w - 184.0).max(140.0))
    } else {
        0.0
    };
    let strip_w = (avail_w
        - override_w
        - if active_custom_state.is_some() {
            24.0
        } else {
            0.0
        })
    .max(160.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(strip_w, CARD_HEIGHT + 14.0),
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("states_card_strip")
                    .auto_shrink([false, true])
                    .max_height(CARD_HEIGHT + 14.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(coursecraft_render::PANEL_MARGIN);
                            let normal_active = match editor.ui.preview_state.as_deref() {
                                Some("normal") => true,
                                Some(_) => false,
                                None => obj.initial_state == "normal",
                            };
                            let normal_rect = show_state_card(
                                editor,
                                ui,
                                &slide,
                                &obj,
                                "normal",
                                "Normal",
                                obj.initial_state == "normal",
                                normal_active,
                                None,
                                obj.states.len(),
                                actions,
                            );
                            editor
                                .ui
                                .states
                                .state_row_rects
                                .insert("normal".to_owned(), normal_rect);

                            let total_custom = obj.states.len();
                            for (idx, state) in obj.states.iter().enumerate() {
                                let is_active =
                                    editor.ui.preview_state.as_deref() == Some(&state.name);
                                let rect = show_state_card(
                                    editor,
                                    ui,
                                    &slide,
                                    &obj,
                                    &state.name,
                                    &state.name,
                                    obj.initial_state == state.name,
                                    is_active,
                                    Some(idx),
                                    total_custom,
                                    actions,
                                );
                                editor
                                    .ui
                                    .states
                                    .state_row_rects
                                    .insert(state.name.clone(), rect);
                            }

                            if obj.states.is_empty() {
                                let (add_rect, add_resp) = ui.allocate_exact_size(
                                    vec2(CARD_WIDTH, CARD_HEIGHT),
                                    Sense::click(),
                                );
                                ui.painter().rect_stroke(
                                    add_rect,
                                    CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                                    Stroke::new(1.0, p.border),
                                    StrokeKind::Inside,
                                );
                                ui.painter().text(
                                    add_rect.center(),
                                    Align2::CENTER_CENTER,
                                    "+ Add state",
                                    FontId::proportional(coursecraft_render::SIZE_BODY),
                                    p.text_secondary,
                                );
                                if add_resp.clicked() {
                                    editor.ui.states.show_new_state_input = true;
                                }
                            }
                            ui.add_space(coursecraft_render::PANEL_MARGIN);
                        });
                    });
            },
        );

        // Slice 1 inline override bar beside the card strip when a custom state is selected
        if let Some(state) = active_custom_state {
            let active_name = state.name.clone();
            ui.separator();
            ui.vertical(|ui| {
                ui.set_max_width(override_w);
                let buf = editor
                    .ui
                    .states
                    .edit_buffers
                    .entry(state.name.clone())
                    .or_default();
                let mut pick_state_image = false;

                let state_fill_allowed = match &obj.kind {
                    ObjectKind::Button { .. } | ObjectKind::Shape { .. } => true,
                    ObjectKind::Path { path } => path.closed,
                    _ => false,
                };
                if state_fill_allowed {
                    let id_salt = format!("state_tab_fill_{active_name}");
                    let (r_color, is_valid_hex) =
                        show_state_color_field(ui, "Fill", &mut buf.fill_hex, &id_salt);
                    buf.fill_rect = Some(r_color.rect);
                    if editor.ui.states.focus_override_bar {
                        r_color.request_focus();
                        editor.ui.states.focus_override_bar = false;
                    }
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
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "state": state_val,
                                    }),
                                ));
                            }
                        }
                    }
                }

                let allows_state_text = match &obj.kind {
                    ObjectKind::Text { .. } | ObjectKind::Button { .. } => true,
                    ObjectKind::Shape { shape, .. } => {
                        coursecraft_course::shape_allows_text(*shape)
                    }
                    _ => false,
                };
                if allows_state_text {
                    ui.horizontal(|ui| {
                        ui.label("Text");
                        let text_w = (ui.available_width() - 56.0).clamp(80.0, 140.0);
                        let r_text = ui.add(
                            TextEdit::singleline(&mut buf.text_override)
                                .id_salt(format!("state_tab_text_{active_name}"))
                                .desired_width(text_w),
                        );
                        buf.text_rect = Some(r_text.rect);
                        if editor.ui.states.focus_override_bar {
                            r_text.request_focus();
                            editor.ui.states.focus_override_bar = false;
                        }
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
                                            "slide": slide.id,
                                            "object": obj.id,
                                            "state": state_val,
                                        }),
                                    ));
                                }
                            }
                        }
                    });
                }

                if matches!(obj.kind, ObjectKind::Image { .. }) {
                    let current_src = state.src.as_deref().filter(|s| !s.trim().is_empty());
                    let is_unsafe =
                        current_src.is_some_and(|s| !coursecraft_format::is_safe_relative_path(s));
                    if let Some(src) = current_src {
                        if is_unsafe {
                            ui.add(egui::Label::new(RichText::new(src).color(p.danger)).truncate())
                                .on_hover_text(src);
                            ui.label(
                                RichText::new("File not in the course. Choose image…")
                                    .color(p.danger)
                                    .size(coursecraft_render::SIZE_BODY),
                            );
                        } else {
                            let file_name = src.rsplit('/').next().unwrap_or(src);
                            ui.add(egui::Label::new(file_name).truncate())
                                .on_hover_text(src);
                        }
                    } else {
                        ui.label(
                            RichText::new("Same as normal")
                                .color(p.text_secondary)
                                .size(coursecraft_render::SIZE_BODY),
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        let choose_label = if current_src.is_some() && !is_unsafe {
                            "Replace…"
                        } else {
                            "Choose image…"
                        };
                        if ui
                            .add(egui::Button::new(choose_label).min_size(vec2(0.0, 24.0)))
                            .clicked()
                        {
                            pick_state_image = true;
                        }
                        ui.menu_button("From project…", |ui| {
                            let images: Vec<String> = editor
                                .media
                                .iter()
                                .map(|(name, _)| name)
                                .filter(|name| coursecraft_course::is_image_src(name))
                                .cloned()
                                .collect();
                            if images.is_empty() {
                                ui.label(
                                    RichText::new("No images in project")
                                        .color(p.text_secondary)
                                        .size(coursecraft_render::SIZE_BODY),
                                );
                            } else {
                                for name in images {
                                    let selected = current_src == Some(name.as_str());
                                    if ui.selectable_label(selected, &name).clicked() {
                                        let mut updated = state.clone();
                                        updated.src = Some(name);
                                        if let Ok(state_val) = serde_json::to_value(&updated) {
                                            actions.push((
                                                "object.update_state",
                                                serde_json::json!({
                                                    "slide": slide.id,
                                                    "object": obj.id,
                                                    "state": state_val,
                                                }),
                                            ));
                                        }
                                        ui.close();
                                    }
                                }
                            }
                        });
                        if state.src.is_some()
                            && crate::quiet_button(ui, "Clear", crate::QuietTone::Secondary, true)
                                .clicked()
                        {
                            let mut updated = state.clone();
                            updated.src = None;
                            if let Ok(state_val) = serde_json::to_value(&updated) {
                                actions.push((
                                    "object.update_state",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "state": state_val,
                                    }),
                                ));
                            }
                        }
                    });
                }

                editor.ui.states.focus_override_bar = false;
                if pick_state_image {
                    editor.pending_image_pick = Some(crate::PendingImagePick::StateImage {
                        slide: slide.id.clone(),
                        object: obj.id.clone(),
                        state: active_name,
                    });
                    editor.request(crate::EditorRequest::PickImage);
                }
            });
        }
    });

    // Keyboard navigation when focus is inside the card strip
    if editor.ui.states.card_strip_focused
        && editor.ui.states.renaming_state.is_none()
        && !editor.ui.states.show_new_state_input
        && editor.ui.inline_edit.is_none()
        && editor.ui.crop_edit.is_none()
        && editor.ui.editing_state.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
    {
        let mut names = Vec::with_capacity(1 + obj.states.len());
        names.push("normal");
        for s in &obj.states {
            names.push(s.name.as_str());
        }
        let cur_idx = editor
            .ui
            .preview_state
            .as_deref()
            .and_then(|p| names.iter().position(|&n| n == p))
            .unwrap_or(0);
        let cur_name = names[cur_idx];

        let left = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft));
        let right = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        let f2 = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2));
        let enter = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let del = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        });
        let esc = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));

        if right && cur_idx + 1 < names.len() {
            select_card_state(editor, &obj, names[cur_idx + 1]);
        } else if left && cur_idx > 0 {
            select_card_state(editor, &obj, names[cur_idx - 1]);
        } else if f2 && cur_name != "normal" {
            start_inline_rename(editor, cur_name);
        } else if enter && cur_name != "normal" {
            editor.ui.states.focus_override_bar = true;
        } else if del && cur_name != "normal" {
            if state_delete_blocker(&slide, &obj.id, cur_name, &obj.initial_state).is_none() {
                actions.push((
                    "object.delete_state",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "name": cur_name,
                    }),
                ));
            }
        } else if esc {
            editor.ui.preview_state = None;
        }
    }
}
