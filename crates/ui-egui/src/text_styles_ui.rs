//! Named text styles and the theme palette in the editor. Everything here
//! only builds commands for the engine (`text_style.*`, `theme.set_palette*`);
//! the editor runs them, so each click is one undo step.

use crate::dialog::Dialog;
use coursecraft_course::{
    Course, MAX_PALETTE_COLORS, MAX_TEXT_STYLES, Object, ObjectKind, StyleLink, style_link,
    style_of_text_box,
};
use egui::{Context, Id, RichText, TextEdit, Ui};
use serde_json::{Value as Json, json};

use crate::properties::{
    color_swatch_picker, inspector_row, inspector_section_header, is_committed,
};

/// A command id and its params, as the inspector collects them.
pub(crate) type Action = (&'static str, Json);

fn manage_open_id() -> Id {
    Id::unique("coursecraft_manage_text_styles_open")
}

/// Open or close the "Manage styles" window.
fn set_manage_open(ctx: &Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(manage_open_id(), open));
}

fn manage_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(manage_open_id()))
        .unwrap_or(false)
}

/// A text buffer that survives frames while a field is being edited and
/// re-reads the course value once the field is left.
fn buffered_field(ui: &mut Ui, key: Id, current: &str, width: f32) -> (String, bool) {
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| current.to_owned());
    let response = ui.add(
        TextEdit::singleline(&mut text)
            .id_salt(key)
            .desired_width(width),
    );
    let committed = is_committed(&response, ui);
    if response.changed() && !committed {
        ui.data_mut(|d| d.insert_temp(key, text.clone()));
    } else if !response.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    (text, committed)
}

// ------------------------------------------------------------ Format panel

fn link_note(link: StyleLink) -> Option<&'static str> {
    match link {
        StyleLink::Modified => Some("modified"),
        StyleLink::Missing => Some("style missing"),
        StyleLink::None | StyleLink::Matches => None,
    }
}

/// The "Text style" row of the Format panel for a text box: a style dropdown,
/// the "modified" mark, and a menu with the style actions. One row, so the
/// panel stays short.
pub(crate) fn show_style_section(
    ui: &mut Ui,
    course: &Course,
    slide_id: &str,
    object: &Object,
) -> Option<Action> {
    let ObjectKind::Text { text_style, .. } = &object.kind else {
        return None;
    };
    let link = style_link(course, object);
    let linked_id = text_style
        .as_deref()
        .filter(|id| course.text_styles.iter().any(|s| s.id == *id));
    let mut action = None;
    inspector_row(ui, "Text style", |ui| {
        action = style_dropdown(ui, course, slide_id, object, link);
        if let Some(note) = link_note(link) {
            let color = coursecraft_render::palette(ui.ctx()).text_secondary;
            ui.label(RichText::new(note).small().italics().color(color));
        }
        if let Some(next) = style_menu(ui, course, slide_id, object, linked_id) {
            action = Some(next);
        }
    });
    action
}

fn style_dropdown(
    ui: &mut Ui,
    course: &Course,
    slide_id: &str,
    object: &Object,
    _link: StyleLink,
) -> Option<Action> {
    let ObjectKind::Text { text_style, .. } = &object.kind else {
        return None;
    };
    let mut action = None;
    let shown = text_style
        .as_deref()
        .and_then(|id| course.text_styles.iter().find(|s| s.id == id))
        .map_or("None", |s| s.name.as_str());
    egui::ComboBox::from_id_salt(("text_style_pick", &object.id))
        .selected_text(shown)
        .width(110.0)
        .show_ui(ui, |ui| {
            for named in &course.text_styles {
                let selected = text_style.as_deref() == Some(named.id.as_str());
                if ui.selectable_label(selected, &named.name).clicked() && !selected {
                    action = Some((
                        "text_style.apply",
                        json!({"id": named.id, "slide": slide_id, "objects": [object.id]}),
                    ));
                    ui.close();
                }
            }
            if course.text_styles.is_empty() {
                ui.label("No styles yet");
            }
        });
    action
}

fn style_menu(
    ui: &mut Ui,
    course: &Course,
    slide_id: &str,
    object: &Object,
    linked_id: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    let full = course.text_styles.len() >= MAX_TEXT_STYLES;
    ui.menu_button("⋯", |ui| {
        let new = ui
            .add_enabled(!full, egui::Button::new("New style from selection"))
            .on_disabled_hover_text("A course holds at most 50 text styles");
        if new.clicked() {
            let name = format!("Style {}", course.text_styles.len() + 1);
            action = Some((
                "text_style.create",
                json!({"name": name, "from_object": {"slide": slide_id, "object": object.id}}),
            ));
            ui.close();
        }
        if let Some(id) = linked_id {
            if ui.button("Update style from selection").clicked() {
                action = update_from_selection(id, object);
                ui.close();
            }
            if ui.button("Reset to style").clicked() {
                action = Some((
                    "text_style.apply",
                    json!({"id": id, "slide": slide_id, "objects": [object.id]}),
                ));
                ui.close();
            }
        }
        if ui.button("Manage styles…").clicked() {
            set_manage_open(ui.ctx(), true);
            ui.close();
        }
    });
    action
}

fn update_from_selection(id: &str, object: &Object) -> Option<Action> {
    let (style, paragraphs) = style_of_text_box(&object.kind)?;
    let style = serde_json::to_value(style).ok()?;
    let paragraphs = serde_json::to_value(paragraphs).ok()?;
    Some((
        "text_style.update",
        json!({"id": id, "style": style, "paragraphs": paragraphs}),
    ))
}

// ----------------------------------------------------------- Manage window

/// The small "Text styles" window: rename and delete.
pub fn show_manage_styles_window(editor: &mut crate::Editor, ctx: &Context) {
    if std::mem::take(&mut editor.ui.open_text_styles_window) {
        set_manage_open(ctx, true);
    }
    if !manage_open(ctx) {
        return;
    }
    let mut close = false;
    let mut actions: Vec<Action> = Vec::new();
    Dialog::new("Text styles").width(340.0).show(ctx, |ui| {
        let styles = &editor.session.course.text_styles;
        if styles.is_empty() {
            ui.label(
                "No text styles yet. Select a text box and choose \"New style from selection\".",
            );
        }
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for named in styles {
                    ui.horizontal(|ui| {
                        let key = Id::unique(("text_style_name", &named.id));
                        let (name, committed) = buffered_field(ui, key, &named.name, 200.0);
                        if committed && !name.trim().is_empty() && name.trim() != named.name {
                            actions.push((
                                "text_style.update",
                                json!({"id": named.id, "name": name.trim()}),
                            ));
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(("text_style.delete", json!({"id": named.id})));
                        }
                    });
                }
            });
        ui.add_space(8.0);
        if crate::primary_button(ui, "Close").clicked() {
            close = true;
        }
    });
    for (id, params) in actions {
        editor.run(id, params);
    }
    if close {
        set_manage_open(ctx, false);
    }
}

// ------------------------------------------------------------- Palette UI

/// The palette part of the Theme dialog: add, rename, recolour, delete.
pub(crate) fn show_palette_section(ui: &mut Ui, course: &Course) -> Vec<Action> {
    let mut actions = Vec::new();
    inspector_section_header(ui, "Palette");
    let palette = &course.theme.palette;
    if palette.is_empty() {
        ui.small("No palette colors yet");
    }
    for entry in palette {
        ui.horizontal(|ui| palette_row(ui, course, &entry.id, &mut actions));
    }
    let full = palette.len() >= MAX_PALETTE_COLORS;
    if ui
        .add_enabled(!full, egui::Button::new("Add color"))
        .on_disabled_hover_text("A palette holds at most 12 colors")
        .clicked()
    {
        actions.push(("theme.set_palette", palette_with_new(course)));
    }
    ui.small("Changing a palette color recolors every use of it in the course.");
    actions
}

fn palette_json(course: &Course) -> Vec<Json> {
    course
        .theme
        .palette
        .iter()
        .map(|c| json!({"id": c.id, "name": c.name, "color": c.color.0}))
        .collect()
}

fn palette_with_new(course: &Course) -> Json {
    let mut colors = palette_json(course);
    colors.push(json!({
        "name": format!("Color {}", course.theme.palette.len() + 1),
        "color": "#808080",
    }));
    json!({ "colors": colors })
}

fn palette_row(ui: &mut Ui, course: &Course, id: &str, actions: &mut Vec<Action>) {
    let Some(entry) = course.theme.palette.iter().find(|c| c.id == id) else {
        return;
    };
    ui.spacing_mut().item_spacing.x = 6.0;
    let hex_key = Id::unique(("palette_hex", id));
    let mut hex = ui
        .data(|d| d.get_temp::<String>(hex_key))
        .unwrap_or_else(|| entry.color.0.clone());
    if color_swatch_picker(ui, &mut hex, ("palette_swatch", id.to_owned()), 18.0, 4) {
        ui.data_mut(|d| d.remove::<String>(hex_key));
        if hex != entry.color.0 {
            actions.push(("theme.set_palette_color", json!({"id": id, "color": hex})));
        }
    } else {
        ui.data_mut(|d| d.insert_temp(hex_key, hex));
    }
    let (name, committed) =
        buffered_field(ui, Id::unique(("palette_name", id)), &entry.name, 120.0);
    if committed && !name.trim().is_empty() && name.trim() != entry.name {
        let colors: Vec<Json> = palette_json(course)
            .into_iter()
            .map(|mut c| {
                if c["id"] == id {
                    c["name"] = json!(name.trim());
                }
                c
            })
            .collect();
        actions.push(("theme.set_palette", json!({ "colors": colors })));
    }
    palette_hex_field(ui, entry.color.0.as_str(), id, actions);
    if ui.button("Delete").clicked() {
        let colors: Vec<Json> = palette_json(course)
            .into_iter()
            .filter(|c| c["id"] != id)
            .collect();
        actions.push(("theme.set_palette", json!({ "colors": colors })));
    }
}

/// A hex text box for a palette colour; committing recolours the course.
fn palette_hex_field(ui: &mut Ui, current: &str, id: &str, actions: &mut Vec<Action>) {
    let key = Id::unique(("palette_hex_text", id));
    let (text, committed) = buffered_field(ui, key, current, 80.0);
    let valid = coursecraft_course::normalize_hex(&text).is_some();
    if committed && valid && coursecraft_course::normalize_hex(&text).as_deref() != Some(current) {
        actions.push((
            "theme.set_palette_color",
            json!({"id": id, "color": text.trim()}),
        ));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use coursecraft_course::Course;
    use coursecraft_engine::Session;
    use serde_json::json;

    use super::{palette_with_new, update_from_selection};

    const HELLO: &str = include_str!("../../../examples/hello-course/course.json");

    #[test]
    fn the_panel_commands_run_and_are_one_undo_step_each() {
        let mut s = Session::new(Course::from_json(HELLO).unwrap());
        let slide = s.course.slides().next().unwrap().id.clone();
        s.execute(
            "object.add",
            &json!({"slide": slide, "id": "t", "frame": {"x": 0, "y": 0, "w": 300, "h": 80},
                "kind": {"type": "text", "text": "Hi"}}),
        )
        .unwrap();
        let made = s
            .execute(
                "text_style.create",
                &json!({"name": "Style 1", "from_object": {"slide": slide, "object": "t"}}),
            )
            .unwrap();
        let id = made["id"].as_str().unwrap().to_owned();
        s.execute(
            "object.update",
            &json!({"slide": slide, "object": "t",
                "kind": {"type": "text", "text": "Hi", "style": {"size": 44.0}}}),
        )
        .unwrap();
        let object = s
            .course
            .slide(&slide)
            .unwrap()
            .object("t")
            .unwrap()
            .0
            .clone();
        let (cmd, params) = update_from_selection(&id, &object).expect("a text box");
        assert_eq!(cmd, "text_style.update");
        let before = s.undo_len();
        s.execute(cmd, &params).unwrap();
        assert_eq!(s.undo_len(), before + 1);
        assert!((s.course.text_styles[0].style.size - 44.0).abs() < f32::EPSILON);

        let params = palette_with_new(&s.course);
        s.execute("theme.set_palette", &params).unwrap();
        assert_eq!(s.course.theme.palette.len(), 1);
    }
}
