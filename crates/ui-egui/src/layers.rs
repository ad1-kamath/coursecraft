//! Slide Layers panel: base pinned at the bottom, front-to-back rows, drag
//! reorder, rename, duplicate/delete, eye and Dim, and the Layer Settings window.

use crate::Editor;
use coursecraft_course::{Course, Layer, RevisitMode, SeekMode, Slide};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, DragValue, FontFamily, FontId, Key, Modifiers, Rect,
    RichText, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};
use serde_json::{Map, Value as Json};
use std::collections::{BTreeSet, HashMap};

/// Width of the Slide Layers panel in the bottom panel.
pub const LAYERS_PANEL_WIDTH: f32 = 260.0;
/// Narrower width used when the bottom panel is tight.
pub const LAYERS_PANEL_NARROW_WIDTH: f32 = 200.0;
/// Below this bottom-panel width the layers panel narrows rather than overlap.
pub const LAYERS_PANEL_NARROW_BELOW: f32 = 900.0;
/// Height of one layer row.
pub const LAYER_ROW_HEIGHT: f32 = 30.0;
const LAYERS_HEADER_HEIGHT: f32 = 28.0;

/// In-place rename of a layer row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRename {
    pub slide: String,
    pub layer: String,
    pub buffer: String,
    pub original: String,
    pub just_entered: bool,
}

/// A row drag in the Slide Layers panel.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerRowDrag {
    pub layer: String,
    pub start_pointer_y: f32,
    pub moved: bool,
    /// Insertion slot in the displayed (front-to-back) layer list.
    pub target_slot: usize,
}

/// Editor-only view state and interaction state for the Slide Layers panel.
#[derive(Debug, Clone, PartialEq)]
pub struct LayersState {
    pub last_synced_revision: crate::SyncRevision,
    pub last_synced_selection: crate::Selection,

    /// Layers whose eye is off. Never course data.
    pub hidden_layers: BTreeSet<String>,
    /// Dim everything except a focused layer. Never course data.
    pub dim_other_layers: bool,
    /// The slide the eye states belong to; changing slide clears them.
    eye_slide: String,

    pub rename: Option<LayerRename>,
    pub active_row_drag: Option<LayerRowDrag>,

    // Tracked rects for test interaction
    pub panel_rect: Option<Rect>,
    pub header_rect: Option<Rect>,
    pub base_row_rect: Option<Rect>,
    pub layer_row_rects: HashMap<String, Rect>,
    pub eye_rects: HashMap<String, Rect>,
    pub gear_rects: HashMap<String, Rect>,
    pub add_button_rect: Option<Rect>,
    pub duplicate_button_rect: Option<Rect>,
    pub delete_button_rect: Option<Rect>,
    pub dim_rect: Option<Rect>,
    pub rename_edit_rect: Option<Rect>,
}

impl Default for LayersState {
    fn default() -> Self {
        Self {
            last_synced_revision: crate::SyncRevision::default(),
            last_synced_selection: crate::Selection::default(),
            hidden_layers: BTreeSet::new(),
            dim_other_layers: true,
            eye_slide: String::new(),
            rename: None,
            active_row_drag: None,
            panel_rect: None,
            header_rect: None,
            base_row_rect: None,
            layer_row_rects: HashMap::new(),
            eye_rects: HashMap::new(),
            gear_rects: HashMap::new(),
            add_button_rect: None,
            duplicate_button_rect: None,
            delete_button_rect: None,
            dim_rect: None,
            rename_edit_rect: None,
        }
    }
}

impl LayersState {
    /// Re-sync with the course and clear per-slide view state when the slide changes.
    pub fn sync(&mut self, course: &Course, selection: &crate::Selection, revision: u64) {
        if self.last_synced_revision.is(revision) && &self.last_synced_selection == selection {
            return;
        }
        let slide_changed = self.last_synced_selection.slide != selection.slide;
        self.last_synced_revision.set(revision);
        self.last_synced_selection = selection.clone();

        if slide_changed && self.eye_slide != selection.slide {
            self.hidden_layers.clear();
            self.eye_slide = selection.slide.clone();
            if self
                .rename
                .as_ref()
                .is_some_and(|rename| rename.slide != selection.slide)
            {
                self.rename = None;
            }
            self.active_row_drag = None;
        }

        if let Some(rename) = &self.rename {
            let still_there = course
                .slide(&rename.slide)
                .and_then(|slide| slide.layer(&rename.layer))
                .is_some();
            if !still_there {
                self.rename = None;
            }
        }
        if let Some(drag) = &self.active_row_drag {
            let still_there = course
                .slide(&selection.slide)
                .and_then(|slide| slide.layer(&drag.layer))
                .is_some();
            if !still_there {
                self.active_row_drag = None;
            }
        }
    }

    /// Whether a layer's eye is off.
    #[must_use]
    pub fn is_hidden(&self, layer_id: &str) -> bool {
        self.hidden_layers.contains(layer_id)
    }

    /// Ids of unfocused, visible layers to draw on the canvas as well.
    ///
    /// Empty on the base view: layers only appear beside a focused layer.
    #[must_use]
    pub fn extra_layer_ids(&self, slide: &Slide, focus: Option<&str>) -> Vec<String> {
        let Some(focus) = focus else {
            return Vec::new();
        };
        slide
            .layers
            .iter()
            .filter(|layer| layer.id != focus && !self.is_hidden(&layer.id))
            .map(|layer| layer.id.clone())
            .collect()
    }
}

/// The layer a settings window is editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSettingsTarget {
    pub slide: String,
    pub layer: String,
}

/// Buffers and tracked rects for the Layer Settings window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayerSettingsState {
    pub open: Option<LayerSettingsTarget>,
    pub slide: String,
    pub layer: String,
    pub opened_revision: u64,
    pub original_name: String,
    pub original_duration_ms: u32,
    pub original_hide_other_layers: bool,
    pub original_pause_base_timeline: bool,
    pub original_prevent_base_clicks: bool,
    pub original_hide_base_objects: bool,
    pub original_hide_when_timeline_ends: bool,
    pub original_allow_seek: SeekMode,
    pub original_revisit: RevisitMode,
    pub name: String,
    pub duration_ms: u32,
    pub hide_other_layers: bool,
    pub pause_base_timeline: bool,
    pub prevent_base_clicks: bool,
    pub hide_base_objects: bool,
    pub hide_when_timeline_ends: bool,
    pub allow_seek: SeekMode,
    pub revisit: RevisitMode,

    // Tracked rects for test interaction
    pub name_rect: Option<Rect>,
    pub duration_rect: Option<Rect>,
    pub hide_other_rect: Option<Rect>,
    pub pause_base_rect: Option<Rect>,
    pub prevent_clicks_rect: Option<Rect>,
    pub hide_base_rect: Option<Rect>,
    pub hide_when_ends_rect: Option<Rect>,
    pub allow_seek_rect: Option<Rect>,
    pub revisit_rect: Option<Rect>,
    pub ok_rect: Option<Rect>,
    pub cancel_rect: Option<Rect>,
}

impl LayerSettingsState {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    fn close(&mut self) {
        self.open = None;
        self.slide.clear();
        self.layer.clear();
        self.name_rect = None;
        self.duration_rect = None;
        self.hide_other_rect = None;
        self.pause_base_rect = None;
        self.prevent_clicks_rect = None;
        self.hide_base_rect = None;
        self.hide_when_ends_rect = None;
        self.allow_seek_rect = None;
        self.revisit_rect = None;
        self.ok_rect = None;
        self.cancel_rect = None;
    }
}

fn load_layer_settings(
    state: &mut LayerSettingsState,
    slide: &Slide,
    layer: &Layer,
    revision: u64,
) {
    state.open = Some(LayerSettingsTarget {
        slide: slide.id.clone(),
        layer: layer.id.clone(),
    });
    state.slide = slide.id.clone();
    state.layer = layer.id.clone();
    state.opened_revision = revision;
    state.original_name = layer.name.clone();
    state.original_duration_ms = layer.duration_ms;
    state.original_hide_other_layers = layer.hide_other_layers;
    state.original_pause_base_timeline = layer.pause_base_timeline;
    state.original_prevent_base_clicks = layer.prevent_base_clicks;
    state.original_hide_base_objects = layer.hide_base_objects;
    state.original_hide_when_timeline_ends = layer.hide_when_timeline_ends;
    state.original_allow_seek = layer.allow_seek;
    state.original_revisit = layer.revisit;
    state.name = layer.name.clone();
    state.duration_ms = layer.duration_ms;
    state.hide_other_layers = layer.hide_other_layers;
    state.pause_base_timeline = layer.pause_base_timeline;
    state.prevent_base_clicks = layer.prevent_base_clicks;
    state.hide_base_objects = layer.hide_base_objects;
    state.hide_when_timeline_ends = layer.hide_when_timeline_ends;
    state.allow_seek = layer.allow_seek;
    state.revisit = layer.revisit;
    state.name_rect = None;
    state.duration_rect = None;
    state.hide_other_rect = None;
    state.pause_base_rect = None;
    state.prevent_clicks_rect = None;
    state.hide_base_rect = None;
    state.hide_when_ends_rect = None;
    state.allow_seek_rect = None;
    state.revisit_rect = None;
    state.ok_rect = None;
    state.cancel_rect = None;
}

/// Open the Layer Settings window for a layer on the current slide.
pub fn open_layer_settings(editor: &mut Editor, slide_id: &str, layer_id: &str) {
    let Some(slide) = editor.session.course.slide(slide_id) else {
        return;
    };
    let Some(layer) = slide.layer(layer_id) else {
        return;
    };
    load_layer_settings(
        &mut editor.ui.layer_settings,
        slide,
        layer,
        editor.session.revision,
    );
}

fn start_layer_rename(editor: &mut Editor, slide_id: &str, layer_id: &str) {
    let Some(layer) = editor
        .session
        .course
        .slide(slide_id)
        .and_then(|slide| slide.layer(layer_id))
    else {
        return;
    };
    editor.selection.layer = Some(layer_id.to_owned());
    editor.selection.repair(&editor.session.course);
    editor.ui.layers.rename = Some(LayerRename {
        slide: slide_id.to_owned(),
        layer: layer_id.to_owned(),
        buffer: layer.name.clone(),
        original: layer.name.clone(),
        just_entered: true,
    });
}

/// Render the Slide Layers panel that sits on the right side of the bottom panel.
pub fn show_layers_panel(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    editor.ui.layers.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
    );

    // Escape cancels a row drag before the canvas clears the selection.
    if editor.ui.layers.active_row_drag.is_some()
        && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        editor.ui.layers.active_row_drag = None;
    }

    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let slide_id = slide.id.clone();
    let selected_layer = editor.selection.layer.clone();
    // Front to back: the top row is the frontmost layer.
    let layers: Vec<(String, String)> = slide
        .layers
        .iter()
        .rev()
        .map(|layer| (layer.id.clone(), layer.name.clone()))
        .collect();

    let p = coursecraft_render::palette(ui.ctx());
    let panel_rect = ui.available_rect_before_wrap();
    editor.ui.layers.panel_rect = Some(panel_rect);
    editor.ui.layers.layer_row_rects.clear();
    editor.ui.layers.eye_rects.clear();
    editor.ui.layers.gear_rects.clear();
    editor.ui.layers.rename_edit_rect = None;

    let header_rect = Rect::from_min_size(
        panel_rect.min,
        vec2(panel_rect.width(), LAYERS_HEADER_HEIGHT),
    );
    editor.ui.layers.header_rect = Some(header_rect);
    painter_header(ui, header_rect, &p);

    let footer_height = if layers.is_empty() { 76.0 } else { 38.0 };
    let footer_rect = Rect::from_min_max(
        pos2(
            panel_rect.min.x,
            (panel_rect.max.y - footer_height).max(header_rect.max.y),
        ),
        panel_rect.max,
    );
    let base_row_rect = Rect::from_min_max(
        pos2(panel_rect.min.x, footer_rect.min.y - LAYER_ROW_HEIGHT),
        pos2(panel_rect.max.x, footer_rect.min.y),
    );
    editor.ui.layers.base_row_rect = Some(base_row_rect);

    let rows_rect = Rect::from_min_max(
        pos2(panel_rect.min.x, header_rect.max.y),
        pos2(panel_rect.max.x, base_row_rect.min.y),
    );
    if rows_rect.height() > 0.0 {
        let mut rows_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rows_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        rows_ui.set_clip_rect(rows_rect.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt("slide_layers_scroll")
            .auto_shrink([false, false])
            .show(&mut rows_ui, |ui| {
                for (layer_id, layer_name) in &layers {
                    show_layer_row(
                        editor,
                        ui,
                        &slide_id,
                        layer_id,
                        layer_name,
                        rows_rect.width(),
                        selected_layer.as_deref() == Some(layer_id.as_str()),
                        actions,
                    );
                }
            });
    }

    handle_row_drag(editor, ui, &layers, rows_rect, actions);

    show_base_row(editor, ui, base_row_rect, selected_layer.is_none(), &p);

    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    footer_ui.set_clip_rect(footer_rect.intersect(ui.clip_rect()));
    show_footer(
        editor,
        &mut footer_ui,
        &slide_id,
        selected_layer.as_deref(),
        layers.is_empty(),
        actions,
    );
}

fn painter_header(ui: &Ui, rect: Rect, p: &coursecraft_render::Palette) {
    let painter = ui.painter_at(rect);
    painter.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        "Slide Layers",
        FontId::new(
            coursecraft_render::SIZE_CAPTION,
            FontFamily::Name("inter-semibold".into()),
        ),
        p.text_secondary,
    );
}

fn show_base_row(
    editor: &mut Editor,
    ui: &mut Ui,
    rect: Rect,
    selected: bool,
    p: &coursecraft_render::Palette,
) {
    let painter = ui.painter_at(rect.expand2(vec2(0.0, 0.0)));
    painter.line_segment(
        [
            pos2(rect.left(), rect.top()),
            pos2(rect.right(), rect.top()),
        ],
        Stroke::new(1.0, p.border),
    );
    let response = ui.interact(
        rect,
        ui.scope_id().with("slide_layers_base_row"),
        Sense::click(),
    );
    if selected {
        painter.rect_filled(rect, CornerRadius::ZERO, p.accent_subtle);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::ZERO, p.hover);
    }
    painter.text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Base layer",
        FontId::proportional(coursecraft_render::SIZE_BODY),
        if selected { p.text } else { p.text_secondary },
    );
    if response.clicked() {
        editor.selection.layer = None;
        editor.selection.repair(&editor.session.course);
    }
}

#[allow(clippy::too_many_arguments)]
fn show_layer_row(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    layer_id: &str,
    layer_name: &str,
    row_width: f32,
    selected: bool,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let renaming = editor
        .ui
        .layers
        .rename
        .as_ref()
        .is_some_and(|rename| rename.slide == slide_id && rename.layer == layer_id);
    let (row_rect, _) = ui.allocate_exact_size(vec2(row_width, LAYER_ROW_HEIGHT), Sense::hover());
    let response = ui.interact(
        row_rect,
        ui.scope_id().with(("layer_row", slide_id, layer_id)),
        Sense::click(),
    );
    editor
        .ui
        .layers
        .layer_row_rects
        .insert(layer_id.to_owned(), row_rect);

    let is_dragged = editor
        .ui
        .layers
        .active_row_drag
        .as_ref()
        .is_some_and(|drag| drag.layer == layer_id);
    let painter = ui.painter_at(row_rect);
    if is_dragged {
        painter.rect_filled(row_rect, CornerRadius::ZERO, p.hover);
    } else if selected {
        painter.rect_filled(row_rect, CornerRadius::ZERO, p.accent_subtle);
    } else if response.hovered() {
        painter.rect_filled(row_rect, CornerRadius::ZERO, p.hover);
    }

    if renaming {
        show_rename_editor(editor, ui, row_rect, slide_id, layer_id, actions);
        return;
    }

    let eye_rect = Rect::from_center_size(
        pos2(row_rect.left() + 15.0, row_rect.center().y),
        vec2(26.0, 26.0),
    );
    let gear_rect = Rect::from_center_size(
        pos2(row_rect.right() - 15.0, row_rect.center().y),
        vec2(26.0, 26.0),
    );
    let name_rect = Rect::from_min_max(
        pos2(eye_rect.right() + 4.0, row_rect.top()),
        pos2(gear_rect.left() - 4.0, row_rect.bottom()),
    );
    painter.text(
        name_rect.left_center(),
        Align2::LEFT_CENTER,
        layer_name,
        FontId::proportional(coursecraft_render::SIZE_BODY),
        if selected { p.text } else { p.text_secondary },
    );

    let focus_layer = editor.selection.layer.as_deref();
    let focused = focus_layer == Some(layer_id);
    let hidden = editor.ui.layers.is_hidden(layer_id);

    let pointer = ui.input(|input| input.pointer.interact_pos());
    let primary_pressed = ui.input(|input| input.pointer.primary_pressed());
    let over_eye = pointer.is_some_and(|pos| eye_rect.contains(pos));
    let over_gear = pointer.is_some_and(|pos| gear_rect.contains(pos));

    // The eye and gear sit inside the row, so handle their presses here rather
    // than adding overlapping click widgets. The row only selects elsewhere.
    if primary_pressed {
        if over_gear {
            open_layer_settings(editor, slide_id, layer_id);
        } else if over_eye && !focused {
            if hidden {
                editor.ui.layers.hidden_layers.remove(layer_id);
            } else {
                editor.ui.layers.hidden_layers.insert(layer_id.to_owned());
            }
        }
    }

    if !over_eye && !over_gear {
        if response.double_clicked() {
            start_layer_rename(editor, slide_id, layer_id);
        } else if response.clicked() {
            editor.selection.layer = Some(layer_id.to_owned());
            editor.selection.repair(&editor.session.course);
        }
        if response.secondary_clicked() || response.container_secondary_clicked() {
            editor.selection.layer = Some(layer_id.to_owned());
            editor.selection.repair(&editor.session.course);
        }
    }

    let index = editor
        .session
        .course
        .slide(slide_id)
        .and_then(|slide| slide.layers.iter().position(|layer| layer.id == layer_id));
    let layer_count = editor
        .session
        .course
        .slide(slide_id)
        .map_or(0, |slide| slide.layers.len());
    if let Some(index) = index {
        response.context_menu(|ui| {
            show_layer_row_menu(editor, ui, slide_id, layer_id, index, layer_count, actions);
        });
    }

    if primary_pressed
        && let Some(pos) = pointer
        && row_rect.contains(pos)
        && !over_eye
        && !over_gear
        && editor.ui.layers.rename.is_none()
    {
        editor.selection.layer = Some(layer_id.to_owned());
        editor.selection.repair(&editor.session.course);
        editor.ui.layers.active_row_drag = Some(LayerRowDrag {
            layer: layer_id.to_owned(),
            start_pointer_y: pos.y,
            moved: false,
            target_slot: 0,
        });
    }

    // Eye: the focused layer always reads as on and cannot be turned off.
    editor
        .ui
        .layers
        .eye_rects
        .insert(layer_id.to_owned(), eye_rect);
    if ui.is_rect_visible(eye_rect) {
        let icon = if focused || !hidden {
            coursecraft_render::Icon::Eye
        } else {
            coursecraft_render::Icon::EyeSlash
        };
        let color = if focused || !hidden {
            p.text_secondary
        } else {
            p.text_disabled
        };
        paint_icon(ui, eye_rect, icon, color);
    }
    let eye_tooltip = if focused {
        "The focused layer stays visible"
    } else if hidden {
        "Show layer"
    } else {
        "Hide layer"
    };
    ui.interact(
        eye_rect,
        ui.scope_id().with(("layer_eye", slide_id, layer_id)),
        Sense::hover(),
    )
    .on_hover_text(eye_tooltip)
    .on_hover_cursor(CursorIcon::PointingHand);

    // Gear opens Layer Settings.
    editor
        .ui
        .layers
        .gear_rects
        .insert(layer_id.to_owned(), gear_rect);
    if ui.is_rect_visible(gear_rect) {
        paint_icon(
            ui,
            gear_rect,
            coursecraft_render::Icon::Gear,
            p.text_secondary,
        );
    }
    ui.interact(
        gear_rect,
        ui.scope_id().with(("layer_gear", slide_id, layer_id)),
        Sense::hover(),
    )
    .on_hover_text("Layer settings")
    .on_hover_cursor(CursorIcon::PointingHand);
}

fn paint_icon(ui: &Ui, rect: Rect, icon: coursecraft_render::Icon, color: Color32) {
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(
        icon.codepoint().to_string(),
        FontId::new(
            coursecraft_render::ICON_SM,
            FontFamily::Name("phosphor".into()),
        ),
        color,
    );
    painter.galley(
        pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
}

#[allow(clippy::too_many_arguments)]
fn show_rename_editor(
    editor: &mut Editor,
    ui: &mut Ui,
    row_rect: Rect,
    slide_id: &str,
    layer_id: &str,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(rename) = editor.ui.layers.rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = ui.scope_id().with(("layer_rename", slide_id, layer_id));
    let edit_rect = Rect::from_min_max(
        pos2(row_rect.left() + 8.0, row_rect.top() + 3.0),
        pos2(row_rect.right() - 8.0, row_rect.bottom() - 3.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(edit_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let mut output = TextEdit::singleline(&mut buffer)
        .id(edit_id)
        .desired_width(edit_rect.width())
        .margin(egui::Margin::same(1))
        .show(&mut child);
    editor.ui.layers.rename_edit_rect = Some(output.response.rect);

    if rename.just_entered {
        output.response.request_focus();
        let chars = buffer.chars().count();
        output
            .state
            .cursor
            .set_char_range(Some(egui::text_selection::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(chars),
            )));
        output.state.store(ui.ctx(), output.response.id);
        if let Some(state) = editor.ui.layers.rename.as_mut() {
            state.just_entered = false;
        }
    }

    if let Some(state) = editor.ui.layers.rename.as_mut() {
        state.buffer = buffer.clone();
    }

    let enter = output.response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
    let escaped = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    let lost = output.response.lost_focus();
    let clicked_outside = ui.input(|input| {
        input.pointer.primary_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| !row_rect.contains(pos))
    });

    if escaped {
        editor.ui.layers.rename = None;
    } else if enter || lost || clicked_outside {
        let name = buffer.trim().to_owned();
        if !name.is_empty() && name != rename.original {
            actions.push((
                "layer.update",
                serde_json::json!({
                    "slide": slide_id,
                    "layer": layer_id,
                    "name": name,
                }),
            ));
        }
        editor.ui.layers.rename = None;
    }
}

fn show_layer_row_menu(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    layer_id: &str,
    index: usize,
    layer_count: usize,
    actions: &mut Vec<(&'static str, Json)>,
) {
    if ui.button("Rename").clicked() {
        start_layer_rename(editor, slide_id, layer_id);
        ui.close();
    }
    if ui.button("Layer settings…").clicked() {
        open_layer_settings(editor, slide_id, layer_id);
        ui.close();
    }
    if ui.button("Duplicate").clicked() {
        actions.push((
            "layer.duplicate",
            serde_json::json!({ "slide": slide_id, "layer": layer_id }),
        ));
        ui.close();
    }
    let can_forward = index.saturating_add(1) < layer_count;
    if ui
        .add_enabled(can_forward, egui::Button::new("Bring forward"))
        .clicked()
    {
        actions.push((
            "layer.move",
            serde_json::json!({
                "slide": slide_id,
                "layer": layer_id,
                "to": index.saturating_add(1),
            }),
        ));
        ui.close();
    }
    let can_backward = index > 0;
    if ui
        .add_enabled(can_backward, egui::Button::new("Send backward"))
        .clicked()
    {
        actions.push((
            "layer.move",
            serde_json::json!({
                "slide": slide_id,
                "layer": layer_id,
                "to": index.saturating_sub(1),
            }),
        ));
        ui.close();
    }
    if ui.button("Delete").clicked() {
        actions.push((
            "layer.delete",
            serde_json::json!({ "slide": slide_id, "layer": layer_id }),
        ));
        ui.close();
    }
}

fn show_footer(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    selected_layer: Option<&str>,
    no_layers: bool,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    if no_layers {
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Layers show extra content on top of the slide, like popups and feedback.",
            )
            .color(p.text_secondary)
            .size(coursecraft_render::SIZE_SMALL),
        );
        ui.add_space(4.0);
    }
    let mut footer = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(ui.available_rect_before_wrap())
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    footer.spacing_mut().item_spacing.x = 4.0;

    let add = crate::quiet_button(&mut footer, "+ New layer", crate::QuietTone::Accent, true);
    editor.ui.layers.add_button_rect = Some(add.rect);
    if add.clicked() {
        actions.push(("layer.add", serde_json::json!({ "slide": slide_id })));
    }

    let duplicate = crate::widgets::icon_button_enabled(
        &mut footer,
        coursecraft_render::Icon::Copy,
        selected_layer.is_some(),
        "Duplicate layer",
    );
    editor.ui.layers.duplicate_button_rect = Some(duplicate.rect);
    if duplicate.clicked()
        && let Some(layer_id) = selected_layer
    {
        actions.push((
            "layer.duplicate",
            serde_json::json!({ "slide": slide_id, "layer": layer_id }),
        ));
    }

    let delete = crate::widgets::icon_button_danger(
        &mut footer,
        coursecraft_render::Icon::Trash,
        selected_layer.is_some(),
        "Delete layer",
    );
    editor.ui.layers.delete_button_rect = Some(delete.rect);
    if delete.clicked()
        && let Some(layer_id) = selected_layer
    {
        actions.push((
            "layer.delete",
            serde_json::json!({ "slide": slide_id, "layer": layer_id }),
        ));
    }

    footer.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let dim = ui.checkbox(&mut editor.ui.layers.dim_other_layers, "Dim");
        editor.ui.layers.dim_rect = Some(dim.rect);
        dim.on_hover_text("Dim everything except the focused layer");
    });
}

fn drop_slot(display: &[(String, String)], row_rects: &HashMap<String, Rect>, y: f32) -> usize {
    for (index, (id, _)) in display.iter().enumerate() {
        if let Some(rect) = row_rects.get(id)
            && y < rect.center().y
        {
            return index;
        }
    }
    display.len()
}

fn drop_line_y(
    display: &[(String, String)],
    row_rects: &HashMap<String, Rect>,
    slot: usize,
) -> Option<f32> {
    if let Some((id, _)) = display.get(slot)
        && let Some(rect) = row_rects.get(id)
    {
        return Some(rect.min.y);
    }
    display
        .last()
        .and_then(|(id, _)| row_rects.get(id))
        .map(|rect| rect.max.y)
}

fn handle_row_drag(
    editor: &mut Editor,
    ui: &Ui,
    display: &[(String, String)],
    total_area: Rect,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(mut drag) = editor.ui.layers.active_row_drag.take() else {
        return;
    };
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let down = ui.input(|input| input.pointer.primary_down());

    if down {
        if let Some(pos) = pointer {
            if (pos.y - drag.start_pointer_y).abs() >= 4.0 {
                drag.moved = true;
            }
            if drag.moved {
                drag.target_slot = drop_slot(display, &editor.ui.layers.layer_row_rects, pos.y);
                if let Some(y) =
                    drop_line_y(display, &editor.ui.layers.layer_row_rects, drag.target_slot)
                {
                    let p = coursecraft_render::palette(ui.ctx());
                    ui.painter().line_segment(
                        [pos2(total_area.min.x, y), pos2(total_area.max.x, y)],
                        Stroke::new(2.0, p.accent),
                    );
                }
            }
        }
        editor.ui.layers.active_row_drag = Some(drag);
    } else if drag.moved {
        let mut remaining: Vec<String> = display
            .iter()
            .map(|(id, _)| id.clone())
            .filter(|id| id != &drag.layer)
            .collect();
        let before = display
            .iter()
            .take(drag.target_slot)
            .filter(|(id, _)| id == &drag.layer)
            .count();
        let at = drag.target_slot.saturating_sub(before).min(remaining.len());
        remaining.insert(at, drag.layer.clone());
        // The engine's index counts from the back, so reverse the displayed list.
        if let Some(new_index) = remaining.iter().rev().position(|id| id == &drag.layer) {
            let current = editor
                .session
                .course
                .slide(&editor.selection.slide)
                .and_then(|slide| slide.layers.iter().position(|layer| layer.id == drag.layer));
            if current != Some(new_index) {
                actions.push((
                    "layer.move",
                    serde_json::json!({
                        "slide": editor.selection.slide,
                        "layer": drag.layer,
                        "to": new_index,
                    }),
                ));
            }
        }
    }
}

fn seek_label(mode: SeekMode) -> &'static str {
    match mode {
        SeekMode::Auto => "Automatic",
        SeekMode::Yes => "Yes",
        SeekMode::No => "No",
    }
}

fn revisit_label(mode: RevisitMode) -> &'static str {
    match mode {
        RevisitMode::Auto => "Automatic",
        RevisitMode::Reset => "Start over",
        RevisitMode::Resume => "Resume where it left off",
    }
}

fn seek_value(mode: SeekMode) -> &'static str {
    match mode {
        SeekMode::Auto => "auto",
        SeekMode::Yes => "yes",
        SeekMode::No => "no",
    }
}

fn revisit_value(mode: RevisitMode) -> &'static str {
    match mode {
        RevisitMode::Auto => "auto",
        RevisitMode::Reset => "reset",
        RevisitMode::Resume => "resume",
    }
}

/// Render the Layer Settings modal window.
pub fn show_layer_settings_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.layer_settings.is_open() {
        return;
    }

    let mut state = editor.ui.layer_settings.clone();
    let target = state.open.clone();
    let Some(target) = target else {
        return;
    };

    // A slide change, undo/redo or any other edit while open closes the window,
    // as does losing the layer. Nothing here can panic on a vanished layer.
    if editor.selection.slide != target.slide || state.opened_revision != editor.session.revision {
        state.close();
        editor.ui.layer_settings = state;
        return;
    }
    let layer = editor
        .session
        .course
        .slide(&target.slide)
        .and_then(|slide| slide.layer(&target.layer));
    if layer.is_none() {
        state.close();
        editor.ui.layer_settings = state;
        return;
    }

    let title = format!("Layer Settings: {}", state.original_name);
    let mut open = true;
    let mut close = false;
    let mut commit = false;
    let mut escape = false;

    let max_height = (ctx.content_rect().height() - 40.0).max(220.0);
    egui::Window::new(title)
        .default_width(360.0)
        .max_height(max_height)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let name = ui.add(
                    TextEdit::singleline(&mut state.name)
                        .desired_width(ui.available_width().max(80.0)),
                );
                state.name_rect = Some(name.rect);
            });

            ui.horizontal(|ui| {
                ui.label("Duration");
                let duration = ui.add(
                    DragValue::new(&mut state.duration_ms)
                        .speed(50.0)
                        .range(100..=86_400_000)
                        .suffix(" ms"),
                );
                state.duration_rect = Some(duration.rect);
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label(
                RichText::new("While this layer shows:")
                    .strong()
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
            let hide = ui.checkbox(&mut state.hide_other_layers, "Hide other layers");
            state.hide_other_rect = Some(hide.rect);
            let hide_base = ui.checkbox(
                &mut state.hide_base_objects,
                "Hide objects on the base layer",
            );
            state.hide_base_rect = Some(hide_base.rect);
            let pause = ui.checkbox(&mut state.pause_base_timeline, "Pause the base timeline");
            state.pause_base_rect = Some(pause.rect);
            let prevent = ui.checkbox(&mut state.prevent_base_clicks, "Stop clicks on the base layer");
            state.prevent_clicks_rect = Some(prevent.rect);

            ui.add_space(6.0);
            ui.separator();
            ui.label(
                RichText::new("When its timeline ends:")
                    .strong()
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
            let ends = ui.checkbox(&mut state.hide_when_timeline_ends, "Hide this layer");
            state.hide_when_ends_rect = Some(ends.rect);

            ui.add_space(6.0);
            ui.separator();
            ui.label(
                RichText::new("Learner seeking:")
                    .strong()
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
            ui.horizontal(|ui| {
                ui.label("Allow seeking");
                let combo = egui::ComboBox::from_id_salt("layer_allow_seek")
                    .selected_text(seek_label(state.allow_seek))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.allow_seek, SeekMode::Auto, "Automatic");
                        ui.selectable_value(&mut state.allow_seek, SeekMode::Yes, "Yes");
                        ui.selectable_value(&mut state.allow_seek, SeekMode::No, "No");
                    });
                state.allow_seek_rect = Some(combo.response.rect);
                combo.response.on_hover_text(
                    "Automatic means the seekbar controls this layer only if it pauses the base timeline.",
                );
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label(
                RichText::new("When shown again:")
                    .strong()
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
            ui.horizontal(|ui| {
                ui.label("When revisiting");
                let combo = egui::ComboBox::from_id_salt("layer_revisit")
                    .selected_text(revisit_label(state.revisit))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.revisit, RevisitMode::Auto, "Automatic");
                        ui.selectable_value(&mut state.revisit, RevisitMode::Reset, "Start over");
                        ui.selectable_value(
                            &mut state.revisit,
                            RevisitMode::Resume,
                            "Resume where it left off",
                        );
                    });
                state.revisit_rect = Some(combo.response.rect);
            });

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                let cancel = ui.button("Cancel");
                state.cancel_rect = Some(cancel.rect);
                if cancel.clicked() {
                    close = true;
                }
                let ok_enabled = !state.name.trim().is_empty();
                let p = coursecraft_render::palette(ui.ctx());
                let ok = ui.add_enabled(
                    ok_enabled,
                    egui::Button::new(RichText::new("OK").color(Color32::WHITE))
                        .fill(p.accent)
                        .corner_radius(coursecraft_render::RADIUS_MD as u8)
                        .min_size(vec2(0.0, coursecraft_render::CONTROL_PRIMARY)),
                );
                state.ok_rect = Some(ok.rect);
                if ok.clicked() {
                    commit = true;
                }
            });

            escape = ui.input(|input| input.key_pressed(Key::Escape));
        });

    if !open {
        close = true;
    }
    if escape {
        close = true;
    }

    if commit {
        let name = state.name.trim().to_owned();
        let mut changes = Map::new();
        if name != state.original_name {
            changes.insert("name".to_owned(), Json::String(name));
        }
        if state.duration_ms != state.original_duration_ms {
            changes.insert("duration_ms".to_owned(), Json::from(state.duration_ms));
        }
        if state.hide_other_layers != state.original_hide_other_layers {
            changes.insert(
                "hide_other_layers".to_owned(),
                Json::Bool(state.hide_other_layers),
            );
        }
        if state.pause_base_timeline != state.original_pause_base_timeline {
            changes.insert(
                "pause_base_timeline".to_owned(),
                Json::Bool(state.pause_base_timeline),
            );
        }
        if state.prevent_base_clicks != state.original_prevent_base_clicks {
            changes.insert(
                "prevent_base_clicks".to_owned(),
                Json::Bool(state.prevent_base_clicks),
            );
        }
        if state.hide_base_objects != state.original_hide_base_objects {
            changes.insert(
                "hide_base_objects".to_owned(),
                Json::Bool(state.hide_base_objects),
            );
        }
        if state.hide_when_timeline_ends != state.original_hide_when_timeline_ends {
            changes.insert(
                "hide_when_timeline_ends".to_owned(),
                Json::Bool(state.hide_when_timeline_ends),
            );
        }
        if state.allow_seek != state.original_allow_seek {
            changes.insert(
                "allow_seek".to_owned(),
                Json::String(seek_value(state.allow_seek).to_owned()),
            );
        }
        if state.revisit != state.original_revisit {
            changes.insert(
                "revisit".to_owned(),
                Json::String(revisit_value(state.revisit).to_owned()),
            );
        }
        if !changes.is_empty() {
            changes.insert("slide".to_owned(), Json::String(target.slide.clone()));
            changes.insert("layer".to_owned(), Json::String(target.layer.clone()));
            editor.run("layer.update", Json::Object(changes));
        }
        close = true;
    }

    if close {
        state.close();
    }
    editor.ui.layer_settings = state;
}
