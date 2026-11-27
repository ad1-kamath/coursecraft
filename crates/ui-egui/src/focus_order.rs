//! The Focus Order dialog (Home › Slide › Focus Order).
//!
//! It lists every object and interactive group of the current slide that can take
//! keyboard focus, in the order the learner's Tab key visits them. The list comes from
//! [`coursecraft_render::focus_order_candidates`], the same rules the player uses.
//! The list is grouped by layer: the base layer first, then each layer as its own section.
//! The author moves items up and down or drags them within a section, then OK writes
//! `focus_order` 1..n for every item in one undo step. Only items that take keyboard
//! focus are listed: the published player reads text and pictures to a screen reader in
//! drawing order, not in `focus_order` (see `docs/spec/triggers.md`, Screen readers).

use serde_json::{Value as Json, json};

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::Slide;
use coursecraft_render::{FocusCandidate, focus_order_candidates};

/// One row of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusOrderItem {
    /// Object id, or interactive group id when `is_group`.
    pub id: String,
    pub is_group: bool,
    /// What the list shows: the name, plus the layer for layer objects.
    pub label: String,
    /// The owning layer, or `None` for the base layer. Items only move within their layer.
    pub layer: Option<String>,
}

/// State of the Focus Order window.
#[derive(Debug, Clone, Default)]
pub struct FocusOrderState {
    pub open: bool,
    /// The slide the list belongs to.
    pub slide: String,
    /// The course revision the list was built from. A different revision closes the window.
    pub revision: u64,
    pub items: Vec<FocusOrderItem>,
    /// The row Move Up and Move Down act on.
    pub selected: Option<usize>,
    /// The list was changed since it was opened or reset.
    pub dirty: bool,
    /// "Reset to default order" was pressed: OK clears every `focus_order`.
    pub cleared: bool,
    /// The row being dragged.
    drag_from: Option<usize>,
}

fn item_of(slide: &Slide, candidate: FocusCandidate) -> FocusOrderItem {
    FocusOrderItem {
        label: crate::triggers::object_label(slide, &candidate.id),
        id: candidate.id,
        is_group: candidate.is_group,
        layer: candidate.layer,
    }
}

/// The candidates grouped by layer: the base layer, then each layer in slide order. Within a
/// group the order is the player's Tab order.
fn grouped_items(slide: &Slide, candidates: Vec<FocusCandidate>) -> Vec<FocusOrderItem> {
    let section = |layer: &Option<String>| match layer {
        None => 0,
        Some(id) => slide
            .layers
            .iter()
            .position(|l| &l.id == id)
            .map_or(usize::MAX, |i| i.saturating_add(1)),
    };
    let mut items: Vec<FocusOrderItem> = candidates
        .into_iter()
        .map(|candidate| item_of(slide, candidate))
        .collect();
    // Stable: the order inside a section is kept.
    items.sort_by_key(|item| section(&item.layer));
    items
}

impl FocusOrderState {
    /// Open the window for `slide`, listing its items in the current order.
    pub fn open_for(&mut self, slide: &Slide) {
        *self = Self {
            open: true,
            slide: slide.id.clone(),
            items: grouped_items(slide, focus_order_candidates(slide)),
            ..Self::default()
        };
    }

    /// Whether the row at `index` has a row above it in its own layer.
    #[must_use]
    pub fn can_move_up(&self, index: usize) -> bool {
        match (index.checked_sub(1), self.items.get(index)) {
            (Some(above), Some(item)) => {
                self.items.get(above).is_some_and(|a| a.layer == item.layer)
            }
            _ => false,
        }
    }

    /// Whether the row at `index` has a row below it in its own layer.
    #[must_use]
    pub fn can_move_down(&self, index: usize) -> bool {
        match (
            self.items.get(index),
            self.items.get(index.saturating_add(1)),
        ) {
            (Some(item), Some(below)) => item.layer == below.layer,
            _ => false,
        }
    }

    /// Move the row at `index` one place up. Returns whether anything moved.
    pub fn move_up(&mut self, index: usize) -> bool {
        match index.checked_sub(1) {
            Some(to) => self.move_to(index, to),
            None => false,
        }
    }

    /// Move the row at `index` one place down. Returns whether anything moved.
    pub fn move_down(&mut self, index: usize) -> bool {
        self.move_to(index, index.saturating_add(1))
    }

    /// Move the row at `from` so it ends up at `to`, inside its own layer. Returns whether
    /// anything moved.
    pub fn move_to(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.items.len() || to >= self.items.len() {
            return false;
        }
        if self.items.get(from).map(|i| &i.layer) != self.items.get(to).map(|i| &i.layer) {
            return false;
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        if self.selected == Some(from) {
            self.selected = Some(to);
        }
        self.dirty = true;
        self.cleared = false;
        true
    }

    /// Show the default order again: no author numbers, base layer first, then
    /// each layer. OK then clears every `focus_order` on the slide.
    pub fn reset_to_default(&mut self, slide: &Slide) {
        let mut plain = slide.clone();
        for object in &mut plain.objects {
            object.focus_order = None;
        }
        for group in &mut plain.groups {
            group.focus_order = None;
        }
        for layer in &mut plain.layers {
            for object in &mut layer.objects {
                object.focus_order = None;
            }
            for group in &mut layer.groups {
                group.focus_order = None;
            }
        }
        self.items = grouped_items(slide, focus_order_candidates(&plain));
        self.selected = None;
        self.dirty = true;
        self.cleared = true;
    }

    /// The commands OK runs, as one batch: `focus_order` 1..n in list order, or
    /// null for every item after a reset.
    #[must_use]
    pub fn steps(&self) -> Vec<(String, Json)> {
        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let order = if self.cleared {
                    Json::Null
                } else {
                    json!(index.saturating_add(1))
                };
                if item.is_group {
                    (
                        "group.update".to_owned(),
                        json!({ "slide": self.slide, "group": item.id, "focus_order": order }),
                    )
                } else {
                    (
                        "object.update".to_owned(),
                        json!({ "slide": self.slide, "object": item.id, "focus_order": order }),
                    )
                }
            })
            .collect()
    }
}

impl Editor {
    /// Open the Focus Order window for the selected slide.
    pub fn open_focus_order(&mut self) {
        if self.ui.master_view {
            self.set_status(tl!("Not available in slide masters"));
            return;
        }
        let Some(slide) = self.session.course.slide(&self.selection.slide).cloned() else {
            self.set_status(tl!("Select a slide first"));
            return;
        };
        self.ui.focus_order.open_for(&slide);
        self.ui.focus_order.revision = self.session.revision;
    }

    /// Write the dialog's order as one undo step.
    pub fn apply_focus_order(&mut self) -> bool {
        let steps = self.ui.focus_order.steps();
        if steps.is_empty() {
            return true;
        }
        self.run_batch(&steps).is_some()
    }
}

/// Draw the Focus Order window.
pub fn show_focus_order_window(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.focus_order.open {
        return;
    }
    // The slide changed, vanished or was edited (an undo, a delete) under the window: its list
    // is stale and OK would fail as a whole, so close it.
    let slide = editor
        .session
        .course
        .slide(&editor.ui.focus_order.slide)
        .cloned();
    let stale = editor.selection.slide != editor.ui.focus_order.slide
        || editor.session.revision != editor.ui.focus_order.revision;
    let Some(slide) = slide.filter(|_| !stale) else {
        editor.ui.focus_order.open = false;
        editor.set_status(tl!("Focus Order closed because the slide changed"));
        return;
    };
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    Dialog::new(tl!("Focus Order"))
        .id(egui::Id::unique("focus_order_dialog"))
        .open(&mut open)
        .width(380.0)
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                ui.label(tl!(
                    "Tab visits these in order. Drag a row, or use Move Up and Move Down."
                ));
                ui.add_space(4.0);
                if editor.ui.focus_order.items.is_empty() {
                    ui.label(tl!("Nothing on this slide can take keyboard focus."));
                }
                draw_rows(ui, &mut editor.ui.focus_order, &slide);
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    let state = &mut editor.ui.focus_order;
                    let selected = state.selected;
                    if ui
                        .add_enabled(
                            selected.is_some_and(|i| state.can_move_up(i)),
                            egui::Button::new(tl!("Move Up")),
                        )
                        .on_disabled_hover_text(tl!(
                            "Select a row below the first row of its layer"
                        ))
                        .clicked()
                        && let Some(i) = selected
                    {
                        state.move_up(i);
                    }
                    if ui
                        .add_enabled(
                            selected.is_some_and(|i| state.can_move_down(i)),
                            egui::Button::new(tl!("Move Down")),
                        )
                        .on_disabled_hover_text(tl!("Select a row above the last row of its layer"))
                        .clicked()
                        && let Some(i) = selected
                    {
                        state.move_down(i);
                    }
                    if ui.button(tl!("Reset to default order")).clicked() {
                        state.reset_to_default(&slide);
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(editor.ui.focus_order.dirty, egui::Button::new(tl!("OK")))
                        .on_disabled_hover_text(tl!("Change the order first"))
                        .clicked()
                    {
                        ok = true;
                    }
                    if ui.button(tl!("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if ok && editor.apply_focus_order() {
        open = false;
    }
    if cancel {
        open = false;
    }
    if !open {
        editor.ui.focus_order.open = false;
    }
}

/// The heading of one section of the list: the base layer or a layer's name.
fn section_title(slide: &Slide, layer: &Option<String>) -> String {
    match layer {
        None => tl!("Base layer").to_owned(),
        Some(id) => crate::triggers::layer_name(slide, id),
    }
}

fn draw_rows(ui: &mut egui::Ui, state: &mut FocusOrderState, slide: &Slide) {
    let mut dropped: Option<(usize, f32)> = None;
    let mut rects: Vec<(usize, egui::Rect)> = Vec::new();
    for index in 0..state.items.len() {
        let Some(item) = state.items.get(index) else {
            continue;
        };
        let starts_section = index == 0
            || state
                .items
                .get(index.saturating_sub(1))
                .is_some_and(|prev| prev.layer != item.layer);
        if starts_section {
            ui.add_space(if index == 0 { 0.0 } else { 6.0 });
            ui.label(egui::RichText::new(section_title(slide, &item.layer)).strong());
        }
        let text = format!("{}.  {}", index.saturating_add(1), item.label);
        let response = ui.add(
            egui::Button::selectable(state.selected == Some(index), text)
                .sense(egui::Sense::click_and_drag())
                .min_size(egui::vec2(ui.available_width(), 0.0)),
        );
        rects.push((index, response.rect));
        if response.clicked() {
            state.selected = Some(index);
        }
        if response.drag_started() {
            state.drag_from = Some(index);
            state.selected = Some(index);
        }
        if response.drag_stopped()
            && let Some(from) = state.drag_from.take()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            dropped = Some((from, pos.y));
        }
    }
    // Dropped on the row nearest the pointer, once every row has a rect.
    if let Some((from, y)) = dropped
        && let Some(to) = row_at(&rects, y)
    {
        state.move_to(from, to);
    }
}

/// The index of the row whose rect is nearest to pointer height `y`.
fn row_at(rects: &[(usize, egui::Rect)], y: f32) -> Option<usize> {
    rects
        .iter()
        .min_by(|a, b| {
            let da = (a.1.center().y - y).abs();
            let db = (b.1.center().y - y).abs();
            da.total_cmp(&db)
        })
        .map(|&(index, _)| index)
}
