//! Course settings: the "Resources & glossary" section.
//!
//! Rows hold local text buffers that resync from the course whenever it
//! changes. Edits commit through the `resource.*` and `glossary.*` commands.

use coursecraft_course::{Course, ResourceTarget};
use egui::{Response, TextEdit, Ui};
use serde_json::{Value as Json, json};

/// One resource row's buffers.
#[derive(Debug, Clone, PartialEq, Default)]
struct ResourceRow {
    id: String,
    title: String,
    is_file: bool,
    url: String,
    src: String,
}

/// One glossary row's buffers.
#[derive(Debug, Clone, PartialEq, Default)]
struct GlossaryRow {
    id: String,
    term: String,
    definition: String,
}

/// Buffers for both lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResourceBuffers {
    resources: Vec<ResourceRow>,
    glossary: Vec<GlossaryRow>,
}

/// What the section asks the editor to do after the frame.
#[derive(Debug, Default)]
pub struct ResourceActions {
    /// Engine commands to run, in order.
    pub runs: Vec<(&'static str, Json)>,
    /// A resource id whose file should be chosen.
    pub pick_file: Option<String>,
}

fn committed(response: &Response, ui: &Ui) -> bool {
    response.lost_focus() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
}

impl ResourceBuffers {
    /// Copy the lists from the course.
    pub fn sync(&mut self, course: &Course) {
        self.resources = course
            .resources
            .iter()
            .map(|resource| {
                let (is_file, url, src) = match &resource.target {
                    ResourceTarget::Url { url } => (false, url.clone(), String::new()),
                    ResourceTarget::File { src } => (true, String::new(), src.clone()),
                };
                ResourceRow {
                    id: resource.id.clone(),
                    title: resource.title.clone(),
                    is_file,
                    url,
                    src,
                }
            })
            .collect();
        self.glossary = course
            .glossary
            .iter()
            .map(|entry| GlossaryRow {
                id: entry.id.clone(),
                term: entry.term.clone(),
                definition: entry.definition.clone(),
            })
            .collect();
    }

    /// Draw the section and collect what the author asked for.
    pub fn show(&mut self, ui: &mut Ui) -> ResourceActions {
        let mut actions = ResourceActions::default();
        egui::CollapsingHeader::new(egui::RichText::new("Resources & glossary").heading())
            .id_salt("course_settings_resources")
            .default_open(false)
            .show(ui, |ui| self.rows(ui, &mut actions));
        actions
    }

    fn rows(&mut self, ui: &mut Ui, actions: &mut ResourceActions) {
        ui.label("Resources");
        let count = self.resources.len();
        for (index, row) in self.resources.iter_mut().enumerate() {
            show_resource_row(ui, row, index, count, actions);
        }
        if ui.button("Add resource").clicked() {
            actions.runs.push((
                "resource.add",
                json!({
                    "title": "New resource",
                    "target": { "kind": "url", "url": "https://example.com" }
                }),
            ));
        }
        ui.add_space(6.0);
        ui.label("Glossary");
        let count = self.glossary.len();
        for (index, row) in self.glossary.iter_mut().enumerate() {
            show_glossary_row(ui, row, index, count, actions);
        }
        if ui.button("Add term").clicked() {
            actions
                .runs
                .push(("glossary.add", json!({ "term": "New term" })));
        }
    }
}

fn order_buttons(
    ui: &mut Ui,
    kind: &'static str,
    id: &str,
    index: usize,
    count: usize,
    actions: &mut ResourceActions,
) {
    let (move_id, remove_id) = if kind == "resource" {
        ("resource.move", "resource.remove")
    } else {
        ("glossary.move", "glossary.remove")
    };
    if ui
        .add_enabled(index > 0, egui::Button::new("↑"))
        .on_hover_text("Move up")
        .clicked()
    {
        actions
            .runs
            .push((move_id, json!({ "id": id, "to": index.saturating_sub(1) })));
    }
    if ui
        .add_enabled(index.saturating_add(1) < count, egui::Button::new("↓"))
        .on_hover_text("Move down")
        .clicked()
    {
        actions
            .runs
            .push((move_id, json!({ "id": id, "to": index.saturating_add(1) })));
    }
    if ui.button("✕").on_hover_text("Remove").clicked() {
        actions.runs.push((remove_id, json!({ "id": id })));
    }
}

fn show_resource_row(
    ui: &mut Ui,
    row: &mut ResourceRow,
    index: usize,
    count: usize,
    actions: &mut ResourceActions,
) {
    ui.horizontal(|ui| {
        let title = ui.add(
            TextEdit::singleline(&mut row.title)
                .id_salt(("resource_title", index))
                .desired_width(110.0)
                .hint_text("Title"),
        );
        if committed(&title, ui) && !row.title.trim().is_empty() {
            actions.runs.push((
                "resource.update",
                json!({ "id": row.id, "title": row.title.trim() }),
            ));
        }
        let mut is_file = row.is_file;
        egui::ComboBox::from_id_salt(("resource_kind", index))
            .selected_text(if is_file { "File" } else { "Link" })
            .width(60.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut is_file, false, "Link");
                ui.selectable_value(&mut is_file, true, "File");
            });
        if is_file != row.is_file {
            row.is_file = is_file;
            if is_file {
                actions.pick_file = Some(row.id.clone());
            } else {
                actions.runs.push((
                    "resource.update",
                    json!({
                        "id": row.id,
                        "target": { "kind": "url", "url": "https://example.com" }
                    }),
                ));
            }
        }
        if row.is_file {
            ui.label(if row.src.is_empty() {
                "(none)"
            } else {
                &row.src
            });
            if ui.button("Choose file…").clicked() {
                actions.pick_file = Some(row.id.clone());
            }
        } else {
            let url = ui.add(
                TextEdit::singleline(&mut row.url)
                    .id_salt(("resource_url", index))
                    .desired_width(150.0)
                    .hint_text("https://…"),
            );
            if committed(&url, ui) {
                actions.runs.push((
                    "resource.update",
                    json!({
                        "id": row.id,
                        "target": { "kind": "url", "url": row.url.trim() }
                    }),
                ));
            }
        }
        order_buttons(ui, "resource", &row.id, index, count, actions);
    });
}

fn show_glossary_row(
    ui: &mut Ui,
    row: &mut GlossaryRow,
    index: usize,
    count: usize,
    actions: &mut ResourceActions,
) {
    ui.horizontal(|ui| {
        let term = ui.add(
            TextEdit::singleline(&mut row.term)
                .id_salt(("glossary_term", index))
                .desired_width(110.0)
                .hint_text("Term"),
        );
        if committed(&term, ui) && !row.term.trim().is_empty() {
            actions.runs.push((
                "glossary.update",
                json!({ "id": row.id, "term": row.term.trim() }),
            ));
        }
        let definition = ui.add(
            TextEdit::multiline(&mut row.definition)
                .id_salt(("glossary_definition", index))
                .desired_width(190.0)
                .desired_rows(2)
                .hint_text("Definition"),
        );
        if committed(&definition, ui) {
            actions.runs.push((
                "glossary.update",
                json!({ "id": row.id, "definition": row.definition }),
            ));
        }
        order_buttons(ui, "glossary", &row.id, index, count, actions);
    });
}
