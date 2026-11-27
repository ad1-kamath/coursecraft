//! The contextual tabs (docs/ui/contextual-tabs.md): which ones the selection
//! shows, and the controls that act on the selected object.

use coursecraft_course::{Object, ObjectKind, Rect, Slide};
use egui::{Response, Ui};
use serde_json::json;

use super::spec::{CONTEXT_TABS, Context, ContextOp, ContextTab, FrameField, Item};
use crate::{Editor, EditorRequest};

/// The contexts the selection needs, in strip order.
#[must_use]
pub fn contexts(editor: &Editor) -> Vec<Context> {
    let mut found = Vec::new();
    if editor.ui.story_view {
        return found;
    }
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return found;
    };
    for id in editor.selection.ids() {
        let Some((object, _)) = slide.object(&id) else {
            continue;
        };
        if let Some(context) = context_of(object) {
            found.push(context);
        }
    }
    if editor.ui.inline_edit.is_some() {
        found.push(Context::Drawing);
    }
    // A button shows Button Tools instead of Drawing Tools.
    if found.contains(&Context::Button) {
        found.retain(|c| *c != Context::Drawing);
    }
    if !editor.ui.master_view && slide.question.is_some() {
        found.push(Context::Question);
    }
    let mut ordered = Vec::new();
    for tab in CONTEXT_TABS {
        if found.contains(&tab.context) && !ordered.contains(&tab.context) {
            ordered.push(tab.context);
        }
    }
    ordered
}

fn context_of(object: &Object) -> Option<Context> {
    Some(match object.kind {
        ObjectKind::Text { .. }
        | ObjectKind::Shape { .. }
        | ObjectKind::Path { .. }
        | ObjectKind::Marker { .. }
        | ObjectKind::Slider { .. }
        | ObjectKind::Dial { .. }
        | ObjectKind::TextEntry { .. }
        | ObjectKind::Checkbox { .. }
        | ObjectKind::RadioGroup { .. } => Context::Drawing,
        ObjectKind::Button { .. } => Context::Button,
        ObjectKind::Image { .. } => Context::Picture,
        ObjectKind::Video { .. } => Context::Video,
        ObjectKind::Audio { .. } => Context::Audio,
        ObjectKind::Table { .. } => Context::Table,
        _ => return None,
    })
}

/// The contextual tabs to draw now, in strip order.
#[must_use]
pub fn visible_tabs(editor: &Editor) -> Vec<&'static ContextTab> {
    let contexts = contexts(editor);
    CONTEXT_TABS
        .iter()
        .filter(|t| contexts.contains(&t.context))
        .collect()
}

/// Keep the open tab valid, and open an object's tab when it was just
/// inserted (selecting an existing object leaves the tab alone).
pub fn follow_selection(editor: &mut Editor) {
    let visible = visible_tabs(editor);
    if super::spec::context_tab_of(editor.ui.ribbon.tab).is_some()
        && !visible.iter().any(|t| t.tab.id == editor.ui.ribbon.tab)
    {
        editor.ui.ribbon.tab = "home";
    }
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        editor.ui.ribbon.seen_objects = None;
        return;
    };
    let ids: Vec<String> = slide.local_ids().map(str::to_owned).collect();
    let selected = editor.selection.ids();
    let inserted = match (&editor.ui.ribbon.seen_objects, selected.as_slice()) {
        (Some((seen_slide, seen)), [one]) if *seen_slide == slide.id => {
            !seen.contains(one) && ids.contains(one)
        }
        _ => false,
    };
    // A collapsed ribbon stays collapsed: the author chose to hide it.
    if inserted
        && !editor.ui.ribbon.collapsed
        && let Some(context) = selected
            .first()
            .and_then(|id| slide.object(id))
            .and_then(|(o, _)| context_of(o))
        && let Some(tab) = CONTEXT_TABS.iter().find(|t| t.context == context)
    {
        editor.ui.ribbon.tab = tab.tab.id;
    }
    editor.ui.ribbon.seen_objects = Some((slide.id.clone(), ids));
}

/// The one selected object of the current slide.
fn one(editor: &Editor) -> Option<(&Slide, &Object)> {
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    match editor.selection.ids().as_slice() {
        [id] => slide.object(id).map(|(o, _)| (slide, o)),
        _ => None,
    }
}

const ONE: &str = "Select one object";

/// Whether `op` can run now, with the reason when it cannot.
pub fn available(editor: &Editor, op: ContextOp) -> Result<(), &'static str> {
    if editor.ui.editing_state.is_some() && !matches!(op, ContextOp::QuestionPanel) {
        return Err(super::STATE);
    }
    let kind = one(editor).map(|(_, o)| &o.kind);
    let need = |ok: bool, reason: &'static str| if ok { Ok(()) } else { Err(reason) };
    match op {
        ContextOp::Frame(_) => need(kind.is_some(), ONE),
        ContextOp::EditPoints => need(
            matches!(kind, Some(ObjectKind::Path { .. })),
            "Select one freeform shape",
        ),
        ContextOp::Crop | ContextOp::ResetPicture => need(
            matches!(kind, Some(ObjectKind::Image { .. })),
            "Select one picture",
        ),
        ContextOp::ReplaceMedia => need(
            matches!(
                kind,
                Some(
                    ObjectKind::Image { .. } | ObjectKind::Video { .. } | ObjectKind::Audio { .. }
                )
            ),
            "Select one picture, video or audio",
        ),
        ContextOp::ImportCaptions => need(
            matches!(
                kind,
                Some(ObjectKind::Video { .. } | ObjectKind::Audio { .. })
            ),
            "Select one video or audio",
        ),
        ContextOp::TableHeaderRow
        | ContextOp::TableInsertAbove
        | ContextOp::TableInsertBelow
        | ContextOp::TableInsertLeft
        | ContextOp::TableInsertRight => table(editor).map(|_| ()).ok_or("Select one table"),
        ContextOp::TableDeleteRow => match table(editor) {
            None => Err("Select one table"),
            Some(t) if t.rows < 2 => Err("A table must have at least one row"),
            Some(_) => Ok(()),
        },
        ContextOp::TableDeleteColumn => match table(editor) {
            None => Err("Select one table"),
            Some(t) if t.columns < 2 => Err("A table must have at least one column"),
            Some(_) => Ok(()),
        },
        ContextOp::TableSplit => match table(editor) {
            None => Err("Select one table"),
            Some(t) if !t.merged => Err("Select a merged cell first"),
            Some(_) => Ok(()),
        },
        ContextOp::QuestionPanel | ContextOp::QuestionShuffle => need(
            !editor.ui.master_view
                && editor
                    .session
                    .course
                    .slide(&editor.selection.slide)
                    .is_some_and(|s| s.question.is_some()),
            "This slide has no question",
        ),
    }
}

/// Whether a toggle-like control shows as on.
#[must_use]
pub fn selected(editor: &Editor, op: ContextOp) -> bool {
    match op {
        ContextOp::TableHeaderRow => table(editor).is_some_and(|t| t.header_row),
        ContextOp::QuestionShuffle => editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|s| s.question.as_ref())
            .is_some_and(|q| q.shuffle),
        _ => false,
    }
}

/// The selected table and its selected cell.
struct TableAt {
    slide: String,
    object: String,
    rows: usize,
    columns: usize,
    header_row: bool,
    /// The selected cell, or the row and column past the end.
    row: usize,
    column: usize,
    /// The selected cell spans more than one row or column.
    merged: bool,
}

fn table(editor: &Editor) -> Option<TableAt> {
    let (slide, object) = one(editor)?;
    let ObjectKind::Table {
        columns,
        rows,
        header_row,
        ..
    } = &object.kind
    else {
        return None;
    };
    let cell = editor.ui.table_cell;
    let merged = cell
        .and_then(|(r, c)| rows.get(r).and_then(|line| line.cells.get(c)))
        .is_some_and(|cell| cell.row_span > 1 || cell.col_span > 1);
    Some(TableAt {
        slide: slide.id.clone(),
        object: object.id.clone(),
        rows: rows.len(),
        columns: columns.len(),
        header_row: *header_row,
        row: cell.map_or(rows.len(), |(r, _)| r.min(rows.len())),
        column: cell.map_or(columns.len(), |(_, c)| c.min(columns.len())),
        merged,
    })
}

/// Run an enabled contextual control.
pub fn run(editor: &mut Editor, op: ContextOp, _requests: &mut Vec<EditorRequest>) {
    if available(editor, op).is_err() {
        return;
    }
    match op {
        // The field commits its own value.
        ContextOp::Frame(_) => {}
        ContextOp::EditPoints => {
            if let Some((_, o)) = one(editor) {
                let id = o.id.clone();
                crate::canvas::start_path_edit(editor, &id);
            }
        }
        ContextOp::Crop => {
            if let Some((_, o)) = one(editor) {
                let id = o.id.clone();
                editor.start_crop(&id);
            }
        }
        ContextOp::ReplaceMedia | ContextOp::ImportCaptions => {
            let Some((_, o)) = one(editor) else {
                return;
            };
            let request = match (&o.kind, op) {
                (_, ContextOp::ImportCaptions) => EditorRequest::PickCaptions,
                (ObjectKind::Image { .. }, _) => EditorRequest::PickImage,
                (ObjectKind::Video { .. }, _) => EditorRequest::PickVideo,
                _ => EditorRequest::PickAudio,
            };
            editor.pending_image_pick = Some(crate::PendingImagePick::ReplaceObject(o.id.clone()));
            editor.request(request);
        }
        ContextOp::ResetPicture => reset_picture(editor),
        ContextOp::TableHeaderRow => {
            if let Some(t) = table(editor) {
                editor.run(
                    "table.set_header_row",
                    json!({ "slide": t.slide, "object": t.object, "header_row": !t.header_row }),
                );
            }
        }
        ContextOp::TableInsertAbove
        | ContextOp::TableInsertBelow
        | ContextOp::TableInsertLeft
        | ContextOp::TableInsertRight
        | ContextOp::TableDeleteRow
        | ContextOp::TableDeleteColumn
        | ContextOp::TableSplit => table_edit(editor, op),
        ContextOp::QuestionPanel => {
            crate::dock::request_bottom_tab(editor, crate::BottomTab::Question);
        }
        ContextOp::QuestionShuffle => {
            let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
                return;
            };
            let Some(mut question) = slide.question.clone() else {
                return;
            };
            question.shuffle = !question.shuffle;
            let slide = slide.id.clone();
            editor.run(
                "question.set",
                json!({ "slide": slide, "question": question }),
            );
        }
    }
}

fn table_edit(editor: &mut Editor, op: ContextOp) {
    let Some(t) = table(editor) else {
        return;
    };
    let (cmd, index) = match op {
        ContextOp::TableInsertAbove => ("table.insert_row", t.row),
        ContextOp::TableInsertBelow => ("table.insert_row", (t.row + 1).min(t.rows)),
        ContextOp::TableInsertLeft => ("table.insert_column", t.column),
        ContextOp::TableInsertRight => ("table.insert_column", (t.column + 1).min(t.columns)),
        ContextOp::TableDeleteRow => ("table.delete_row", t.row.min(t.rows.saturating_sub(1))),
        ContextOp::TableDeleteColumn => (
            "table.delete_column",
            t.column.min(t.columns.saturating_sub(1)),
        ),
        ContextOp::TableSplit => {
            editor.run(
                "table.split",
                json!({ "slide": t.slide, "object": t.object, "row": t.row, "column": t.column }),
            );
            return;
        }
        _ => return,
    };
    editor.run(
        cmd,
        json!({ "slide": t.slide, "object": t.object, "index": index }),
    );
}

/// Clear the crop and give the picture its own size, keeping its top left.
fn reset_picture(editor: &mut Editor) {
    let Some((slide, o)) = one(editor) else {
        return;
    };
    let ObjectKind::Image { src, .. } = &o.kind else {
        return;
    };
    let (slide, id, frame, src) = (slide.id.clone(), o.id.clone(), o.frame, src.clone());
    let mut params = json!({ "slide": slide, "object": id, "crop": null });
    if let Some((w, h)) = editor.natural_image_size(&src)
        && w.is_finite()
        && h.is_finite()
        && w >= 1.0
        && h >= 1.0
    {
        params["frame"] = json!(Rect { w, h, ..frame });
    }
    editor.run("object.set_crop", params);
}

/// Largest offset or size the ribbon's number fields offer, in slide pixels.
const MAX_SPAN: f32 = 20_000.0;

/// A label and a number field of the selected object's frame; a committed
/// edit runs `object.set_frame` once.
pub fn number_field(
    editor: &mut Editor,
    ui: &mut Ui,
    item: &'static Item,
    field: FrameField,
    width: f32,
    enabled: bool,
) -> Response {
    let target = one(editor).map(|(s, o)| (s.id.clone(), o.id.clone(), o.frame));
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let p = coursecraft_render::palette(ui.ctx());
        let color = if enabled { p.text } else { p.text_disabled };
        ui.label(egui::RichText::new(tl!(item.label)).size(11.0).color(color));
        let frame = target.as_ref().map_or(Rect::default(), |(_, _, f)| *f);
        let (current, range) = match field {
            FrameField::X => (frame.x, -MAX_SPAN..=MAX_SPAN),
            FrameField::Y => (frame.y, -MAX_SPAN..=MAX_SPAN),
            FrameField::Width => (frame.w, 1.0..=MAX_SPAN),
            FrameField::Height => (frame.h, 1.0..=MAX_SPAN),
        };
        ui.add_enabled_ui(enabled, |ui| {
            ui.set_width(width);
            crate::grid_panel::committed_value(
                ui,
                egui::Id::unique(("ribbon_frame", item.id)),
                current,
                range,
            )
        })
        .inner
    });
    if let (Some(value), Some((slide, object, frame))) = (inner.inner, target) {
        let frame = match field {
            FrameField::X => Rect { x: value, ..frame },
            FrameField::Y => Rect { y: value, ..frame },
            FrameField::Width => Rect { w: value, ..frame },
            FrameField::Height => Rect { h: value, ..frame },
        };
        editor.run(
            "object.set_frame",
            json!({ "slide": slide, "object": object, "frame": frame }),
        );
    }
    inner.response
}

/// The caption colour of a context.
#[must_use]
pub fn caption_color(context: Context) -> egui::Color32 {
    match context {
        Context::Drawing => egui::Color32::from_rgb(0xD9, 0x8C, 0x1F),
        Context::Button => egui::Color32::from_rgb(0x2F, 0x9E, 0x6E),
        Context::Picture => egui::Color32::from_rgb(0x8E, 0x5B, 0xD0),
        Context::Video | Context::Audio => egui::Color32::from_rgb(0xC9, 0x4F, 0x7C),
        Context::Table => egui::Color32::from_rgb(0x3D, 0x7E, 0xD6),
        Context::Question => egui::Color32::from_rgb(0xC0, 0x5A, 0x2E),
    }
}
