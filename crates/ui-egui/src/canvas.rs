//! Canvas center view: slide display, selection, outline, handles, zoom, and layer pills.

use crate::{DrawTool, Editor, PlacementTool};
use coursecraft_course::{
    ConnectorStyle, Course, Glue, HotspotShape, LineEnd, Object, ObjectKind, Path, PathStyle,
    Point, QuestionKind, ShapeKind, shape_info, shape_label,
};
use coursecraft_render::{
    DrawItem, Highlight, Hit, SlideTransform, Visual, compose_authoring_layers, compose_object,
    paint,
};
use egui::{
    Align2, Area, Color32, CornerRadius, CursorIcon, FontId, Id, Painter, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use serde_json::Value as Json;

pub const LAYERS_STRIP_HEIGHT: f32 = 0.0;

/// Zoom level for the canvas display.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ZoomLevel {
    #[default]
    Fit,
    Percent50,
    Percent75,
    Percent100,
    Percent150,
    Percent200,
    /// Any other scale, reached by pinching.
    Custom(f32),
}

/// The closest a pinch can zoom out and in.
const PINCH_ZOOM_RANGE: (f32, f32) = (0.25, 4.0);

impl ZoomLevel {
    pub const ALL: [ZoomLevel; 6] = [
        ZoomLevel::Fit,
        ZoomLevel::Percent50,
        ZoomLevel::Percent75,
        ZoomLevel::Percent100,
        ZoomLevel::Percent150,
        ZoomLevel::Percent200,
    ];

    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Fit => "Fit".to_owned(),
            Self::Percent50 => "50%".to_owned(),
            Self::Percent75 => "75%".to_owned(),
            Self::Percent100 => "100%".to_owned(),
            Self::Percent150 => "150%".to_owned(),
            Self::Percent200 => "200%".to_owned(),
            Self::Custom(scale) => format!("{:.0}%", scale * 100.0),
        }
    }

    #[must_use]
    pub fn fixed_scale(&self) -> Option<f32> {
        match self {
            Self::Fit => None,
            Self::Percent50 => Some(0.5),
            Self::Percent75 => Some(0.75),
            Self::Percent100 => Some(1.0),
            Self::Percent150 => Some(1.5),
            Self::Percent200 => Some(2.0),
            Self::Custom(scale) => Some(*scale),
        }
    }

    #[must_use]
    pub fn zoom_in(&self) -> Self {
        match self {
            Self::Fit => Self::Percent100,
            Self::Percent50 => Self::Percent75,
            Self::Percent75 => Self::Percent100,
            Self::Percent100 => Self::Percent150,
            Self::Percent150 => Self::Percent200,
            Self::Percent200 => Self::Percent200,
            Self::Custom(scale) => Self::ALL
                .into_iter()
                .find(|level| level.fixed_scale().is_some_and(|preset| preset > *scale))
                .unwrap_or(*self),
        }
    }

    #[must_use]
    pub fn zoom_out(&self) -> Self {
        match self {
            Self::Fit => Self::Percent75,
            Self::Percent50 => Self::Percent50,
            Self::Percent75 => Self::Percent50,
            Self::Percent100 => Self::Percent75,
            Self::Percent150 => Self::Percent100,
            Self::Percent200 => Self::Percent150,
            Self::Custom(scale) => Self::ALL
                .into_iter()
                .rev()
                .find(|level| level.fixed_scale().is_some_and(|preset| preset < *scale))
                .unwrap_or(*self),
        }
    }
}

/// Which of the 8 resize handles is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

/// Drag operation on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub enum DragOp {
    Move,
    Resize {
        handle: ResizeHandle,
        rotation: f32,
    },
    Rotate {
        centre: Pos2,
        start_angle: f32,
        orig_rotation: f32,
    },
    Marquee,
    RegionMove,
    RegionResize(ResizeHandle),
    MotionPathPoint {
        path_id: String,
        point_index: usize,
        orig_points: Vec<coursecraft_course::Point>,
        relative: bool,
        obj_origin: Pos2,
    },
    Adjust {
        index: usize,
        original: Vec<f32>,
    },
    /// Dragging one point of a path (an end handle or a point-edit handle).
    PathPoint {
        index: usize,
        original: Vec<coursecraft_course::PathPoint>,
        two_point: bool,
    },
    /// Dragging an inner table column (`column` true) or row border.
    TableBorder {
        column: bool,
        before: usize,
        original: Vec<f32>,
    },
}

/// Active drag state on the canvas.
#[derive(Debug, Clone)]
pub struct CanvasDrag {
    pub op: DragOp,
    pub object_id: String,
    pub orig_frame: Rect,
    pub start_screen: Pos2,
    /// Original frames of every object being moved or resized.
    pub frames: Vec<(String, Rect)>,
}

/// An in-progress drag that sizes a new object before it is placed.
///
/// The press point is kept in slide units, so a window resize mid-drag only
/// changes the screen mapping, not where the frame started.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementDrag {
    /// Press point in slide coordinates; the frame grows from here.
    pub start_slide: Pos2,
    /// The frame a release would create, in slide units, clamped to the slide.
    pub frame: Rect,
}

/// Live state of a draw tool before it commits one `object.add`.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawSession {
    pub tool: DrawTool,
    /// Committed points in slide coordinates. Drag tools keep the current end here.
    pub points: Vec<Pos2>,
    /// Press position for the current gesture.
    pub press: Option<Pos2>,
    pub dragging: bool,
    pub moved: bool,
    /// Target the connector's start end snapped to when the gesture began.
    pub start_glue: Option<Glue>,
}

/// Point-editing mode for one path object.
#[derive(Debug, Clone, PartialEq)]
pub struct PathEdit {
    pub slide: String,
    pub object: String,
    pub selected_point: Option<usize>,
}

impl PathEdit {
    #[must_use]
    pub fn new(slide: impl Into<String>, object: impl Into<String>) -> Self {
        Self {
            slide: slide.into(),
            object: object.into(),
            selected_point: None,
        }
    }
}

/// In-place text editing of a text object, button label or shape text on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineEdit {
    pub slide: String,
    pub object: String,
    pub buffer: String,
    pub original: String,
    pub is_button: bool,
    pub is_shape: bool,
    /// A table cell being edited, as `(row, column)`.
    pub cell: Option<(usize, usize)>,
    /// Select all and focus on the first frame after entering.
    pub just_entered: bool,
}

/// Enter in-place editing for a text object, button or shape on the current slide.
fn start_inline_edit(editor: &mut Editor, object_id: &str) {
    let slide_id = editor.selection.slide.clone();
    let Some(slide) = editor.session.course.slide(&slide_id) else {
        return;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return;
    };
    let (buffer, is_button, is_shape) = match &object.kind {
        coursecraft_course::ObjectKind::Text { text, .. } => (text.clone(), false, false),
        coursecraft_course::ObjectKind::Button { label, .. } => (label.clone(), true, false),
        coursecraft_course::ObjectKind::Shape { shape, text, .. }
            if coursecraft_course::shape_allows_text(*shape) =>
        {
            (
                text.as_ref().map(|t| t.text.clone()).unwrap_or_default(),
                false,
                true,
            )
        }
        _ => return,
    };
    editor.selection.set_many([object_id.to_owned()]);
    editor.ui.inline_edit = Some(InlineEdit {
        slide: slide_id,
        object: object_id.to_owned(),
        original: buffer.clone(),
        buffer,
        is_button,
        is_shape,
        cell: None,
        just_entered: true,
    });
}

/// The slide-space rect of one table cell, including its spans.
pub(crate) fn table_cell_slide_rect(
    object: &coursecraft_course::Object,
    row: usize,
    column: usize,
) -> Option<Rect> {
    let ObjectKind::Table { columns, rows, .. } = &object.kind else {
        return None;
    };
    let cell = rows.get(row)?.cells.get(column)?;
    let col_offsets = coursecraft_course::table_fraction_offsets(columns);
    let heights: Vec<f32> = rows.iter().map(|line| line.height).collect();
    let row_offsets = coursecraft_course::table_fraction_offsets(&heights);
    let col_end = column.saturating_add(usize::from(cell.col_span.max(1)));
    let row_end = row.saturating_add(usize::from(cell.row_span.max(1)));
    let (left, _) = *col_offsets.get(column)?;
    let (_, right) = *col_offsets.get(col_end.saturating_sub(1))?;
    let (top, _) = *row_offsets.get(row)?;
    let (_, bottom) = *row_offsets.get(row_end.saturating_sub(1))?;
    let rect = Rect::from_min_max(
        pos2(
            object.frame.x + left * object.frame.w,
            object.frame.y + top * object.frame.h,
        ),
        pos2(
            object.frame.x + right * object.frame.w,
            object.frame.y + bottom * object.frame.h,
        ),
    );
    rect.is_positive().then_some(rect)
}

/// Find the table cell under a slide-space point, returning its merge anchor.
fn table_cell_at_point(object: &coursecraft_course::Object, point: Pos2) -> Option<(usize, usize)> {
    let ObjectKind::Table { columns, rows, .. } = &object.kind else {
        return None;
    };
    if !point.x.is_finite()
        || !point.y.is_finite()
        || object.frame.w <= 0.0
        || object.frame.h <= 0.0
    {
        return None;
    }
    let x = (point.x - object.frame.x) / object.frame.w;
    let y = (point.y - object.frame.y) / object.frame.h;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let col_offsets = coursecraft_course::table_fraction_offsets(columns);
    let heights: Vec<f32> = rows.iter().map(|line| line.height).collect();
    let row_offsets = coursecraft_course::table_fraction_offsets(&heights);
    let column = col_offsets.iter().position(|(_, end)| x <= *end + 1e-6)?;
    let row = row_offsets.iter().position(|(_, end)| y <= *end + 1e-6)?;
    coursecraft_course::table_cell_owners(rows, columns.len())
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
}

/// The next visible cell after `(row, column)`, wrapping within the table.
fn table_next_cell(
    object: &coursecraft_course::Object,
    row: usize,
    column: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    let ObjectKind::Table { columns, rows, .. } = &object.kind else {
        return None;
    };
    let owners = coursecraft_course::table_cell_owners(rows, columns.len());
    let mut visible = Vec::new();
    for (row_index, line) in owners.iter().enumerate() {
        for (column_index, owner) in line.iter().enumerate() {
            if *owner == Some((row_index, column_index)) {
                visible.push((row_index, column_index));
            }
        }
    }
    if visible.is_empty() {
        return None;
    }
    let current = owners
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
        .unwrap_or((row, column));
    let index = visible.iter().position(|cell| *cell == current)?;
    let next = if forward {
        (index + 1) % visible.len()
    } else if index == 0 {
        visible.len().saturating_sub(1)
    } else {
        index - 1
    };
    visible.get(next).copied()
}

/// Enter in-place editing of a table cell.
fn start_table_cell_edit(editor: &mut Editor, object_id: &str, row: usize, column: usize) {
    let slide_id = editor.selection.slide.clone();
    let Some(slide) = editor.session.course.slide(&slide_id) else {
        return;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return;
    };
    let ObjectKind::Table { rows, .. } = &object.kind else {
        return;
    };
    let Some(cell) = rows.get(row).and_then(|line| line.cells.get(column)) else {
        return;
    };
    editor.selection.set_many([object_id.to_owned()]);
    editor.ui.table_cell = Some((row, column));
    editor.ui.inline_edit = Some(InlineEdit {
        slide: slide_id,
        object: object_id.to_owned(),
        original: cell.text.clone(),
        buffer: cell.text.clone(),
        is_button: false,
        is_shape: false,
        cell: Some((row, column)),
        just_entered: true,
    });
}

/// Enter point editing for a path object.
pub fn start_path_edit(editor: &mut Editor, object_id: &str) {
    editor.selection.set_many([object_id.to_owned()]);
    editor.ui.path_edit = Some(PathEdit::new(
        editor.selection.slide.clone(),
        object_id.to_owned(),
    ));
}

/// Commit normalized path points through `object.set_path`, using `frame`.
fn commit_path_points(
    editor: &mut Editor,
    object_id: &str,
    frame: Rect,
    points: &[coursecraft_course::PathPoint],
) {
    let slide_points: Vec<Point> = points
        .iter()
        .map(|point| Point {
            x: frame.min.x + point.x * frame.width(),
            y: frame.min.y + point.y * frame.height(),
        })
        .collect();
    if slide_points.len() < coursecraft_course::PATH_MIN_POINTS
        || slide_points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return;
    }
    editor.run(
        "object.set_path",
        serde_json::json!({
            "slide": editor.selection.slide,
            "object": object_id,
            "points": slide_points,
        }),
    );
}

/// Add or remove a point on the selected path from a double-click.
fn path_edit_double_click(
    editor: &mut Editor,
    object_id: &str,
    slide_pos: Pos2,
    t: &SlideTransform,
) -> bool {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return false;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return false;
    };
    let ObjectKind::Path { path } = &object.kind else {
        return false;
    };
    let frame = Rect::from_min_size(
        pos2(object.frame.x, object.frame.y),
        vec2(object.frame.w.max(1.0), object.frame.h.max(1.0)),
    );
    let screen = t.to_screen_pos(slide_pos);
    for (index, point) in path.points.iter().enumerate() {
        let centre = t.to_screen_pos(pos2(
            frame.min.x + point.x * frame.width(),
            frame.min.y + point.y * frame.height(),
        ));
        if Rect::from_center_size(centre, vec2(12.0, 12.0)).contains(screen) {
            if path.points.len() <= coursecraft_course::PATH_MIN_POINTS {
                return true;
            }
            let mut points = path.points.clone();
            points.remove(index);
            if let Some(edit) = editor.ui.path_edit.as_mut() {
                edit.selected_point = None;
            }
            commit_path_points(editor, object_id, frame, &points);
            return true;
        }
    }

    let slide_points: Vec<coursecraft_render::ShapePoint> = path
        .points
        .iter()
        .map(|point| {
            (
                frame.min.x + point.x * frame.width(),
                frame.min.y + point.y * frame.height(),
            )
        })
        .collect();
    let flat =
        coursecraft_render::flatten_path(&slide_points, path.style, path.connector, path.closed);
    let Some((segment, _, distance)) =
        coursecraft_render::path_closest(&flat, (slide_pos.x, slide_pos.y))
    else {
        return false;
    };
    let scale = t.scale();
    let threshold = if scale.is_finite() && scale > 0.0 {
        6.0 / scale
    } else {
        6.0
    };
    if !distance.is_finite() || distance > threshold {
        return false;
    }
    if path.points.len() >= coursecraft_course::PATH_MAX_POINTS {
        return true;
    }
    let mut points = path.points.clone();
    let inserted = coursecraft_course::PathPoint {
        x: ((slide_pos.x - frame.min.x) / frame.width()).clamp(0.0, 1.0),
        y: ((slide_pos.y - frame.min.y) / frame.height()).clamp(0.0, 1.0),
    };
    points.insert(segment.saturating_add(1).min(points.len()), inserted);
    if let Some(edit) = editor.ui.path_edit.as_mut() {
        edit.selected_point = Some(segment.saturating_add(1));
    }
    commit_path_points(editor, object_id, frame, &points);
    true
}

/// Delete the selected point of the active path edit, if it would leave two.
#[must_use]
pub fn delete_selected_path_point(editor: &mut Editor) -> bool {
    let Some(edit) = editor.ui.path_edit.clone() else {
        return false;
    };
    if editor.selection.object.as_deref() != Some(edit.object.as_str()) {
        editor.ui.path_edit = None;
        return false;
    }
    let Some(index) = edit.selected_point else {
        return false;
    };
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return false;
    };
    let Some((object, _)) = slide.object(&edit.object) else {
        return false;
    };
    let ObjectKind::Path { path } = &object.kind else {
        return false;
    };
    if path.points.len() <= coursecraft_course::PATH_MIN_POINTS || index >= path.points.len() {
        return true;
    }
    let frame = Rect::from_min_size(
        pos2(object.frame.x, object.frame.y),
        vec2(object.frame.w.max(1.0), object.frame.h.max(1.0)),
    );
    let mut points = path.points.clone();
    points.remove(index);
    if let Some(edit) = editor.ui.path_edit.as_mut() {
        edit.selected_point = None;
    }
    commit_path_points(editor, &edit.object, frame, &points);
    true
}

/// A shape text style that contrasts with the shape's fill, centred both ways.
fn default_shape_text_style(fill: &coursecraft_course::Color) -> coursecraft_course::TextStyle {
    let dark = coursecraft_course::Color("#1F1F1F".to_owned());
    let light = coursecraft_course::Color("#FFFFFF".to_owned());
    let color = match coursecraft_course::parse_color(fill) {
        Some(fill) if fill.a > 0 => {
            let luminance = coursecraft_course::relative_luminance(fill);
            if luminance < 0.45 { light } else { dark }
        }
        _ => dark,
    };
    coursecraft_course::TextStyle {
        color,
        align: coursecraft_course::Align::Center,
        valign: coursecraft_course::VAlign::Middle,
        ..coursecraft_course::TextStyle::default()
    }
}

/// Commit the active in-place edit through the same command the inspector uses.
/// Enter edits the selected text-bearing object in place, like a double click.
/// Text fields and other modals keep their own Enter.
fn enter_edits_selection(editor: &mut Editor, ui: &Ui) {
    if editor.ui.inline_edit.is_some()
        || editor.ui.crop_edit.is_some()
        || ui.ctx().egui_wants_keyboard_input()
    {
        return;
    }
    let enter =
        ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.command && !i.modifiers.ctrl);
    if enter && let Some(id) = editor.selection.object.clone() {
        start_inline_edit(editor, &id);
    }
}

fn commit_inline_edit(editor: &mut Editor) {
    let Some(edit) = editor.ui.inline_edit.take() else {
        return;
    };
    if edit.buffer == edit.original {
        return;
    }
    let Some(slide) = editor.session.course.slide(&edit.slide) else {
        return;
    };
    let Some((object, _)) = slide.object(&edit.object) else {
        return;
    };
    match &object.kind {
        coursecraft_course::ObjectKind::Text { .. } => {
            editor.run(
                "text.set",
                serde_json::json!({
                    "slide": edit.slide,
                    "object": edit.object,
                    "text": edit.buffer,
                }),
            );
        }
        coursecraft_course::ObjectKind::Button {
            fill, fill_style, ..
        } => {
            let fill = fill.0.clone();
            let fill_style = serde_json::to_value(fill_style).unwrap_or(serde_json::Value::Null);
            editor.run(
                "object.update",
                serde_json::json!({
                    "slide": edit.slide,
                    "object": edit.object,
                    "kind": {
                        "type": "button",
                        "label": edit.buffer,
                        "fill": fill,
                        "fill_style": fill_style,
                    },
                }),
            );
        }
        coursecraft_course::ObjectKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            text,
            adjust,
        } if coursecraft_course::shape_allows_text(*shape) => {
            let shape_text = match text {
                Some(text) => {
                    let old_plain: String = if text.runs.is_empty() {
                        text.text.clone()
                    } else {
                        text.runs.iter().map(|run| run.text.as_str()).collect()
                    };
                    let runs =
                        coursecraft_course::apply_text_edit(&text.runs, &old_plain, &edit.buffer);
                    let paragraphs = coursecraft_course::align_paragraphs(
                        &old_plain,
                        &text.paragraphs,
                        &edit.buffer,
                    );
                    coursecraft_course::ShapeText {
                        text: edit.buffer.clone(),
                        style: text.style.clone(),
                        runs,
                        paragraphs,
                        autofit: text.autofit,
                    }
                }
                None => coursecraft_course::ShapeText {
                    text: edit.buffer.clone(),
                    style: default_shape_text_style(fill),
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: coursecraft_course::Autofit::None,
                },
            };
            let kind = coursecraft_course::ObjectKind::Shape {
                shape: *shape,
                fill: fill.clone(),
                fill_style: fill_style.clone(),
                outline: outline.clone(),
                text: Some(shape_text),
                adjust: adjust.clone(),
            };
            if let Ok(kind) = serde_json::to_value(kind) {
                editor.run(
                    "object.update",
                    serde_json::json!({
                        "slide": edit.slide,
                        "object": edit.object,
                        "kind": kind,
                    }),
                );
            }
        }
        coursecraft_course::ObjectKind::Table { rows, .. } => {
            let Some((row, column)) = edit.cell else {
                return;
            };
            let Some(cell) = rows.get(row).and_then(|line| line.cells.get(column)) else {
                return;
            };
            let old_plain = if cell.runs.is_empty() {
                cell.text.clone()
            } else {
                cell.runs.iter().map(|run| run.text.as_str()).collect()
            };
            let runs = coursecraft_course::apply_text_edit(&cell.runs, &old_plain, &edit.buffer);
            let paragraphs =
                coursecraft_course::align_paragraphs(&old_plain, &cell.paragraphs, &edit.buffer);
            editor.run(
                "table.set_cell",
                serde_json::json!({
                    "slide": edit.slide,
                    "object": edit.object,
                    "row": row,
                    "column": column,
                    "text": edit.buffer,
                    "runs": runs,
                    "paragraphs": paragraphs,
                }),
            );
        }
        _ => {}
    }
}

fn cancel_inline_edit(editor: &mut Editor) {
    editor.ui.inline_edit = None;
}

/// Return the canvas rect within the central panel area.
#[must_use]
pub fn canvas_rect(central_rect: Rect) -> Rect {
    central_rect
}

/// Compute the canvas fit rect reserving zoom HUD space.
#[must_use]
pub fn canvas_fit_rect(canvas_rect: Rect) -> Rect {
    let zoom_hud_h = 32.0;
    let margin_x = 32.0;
    let margin_top = 32.0;
    let margin_bottom = zoom_hud_h + 16.0 + coursecraft_render::SPACE_XL;
    Rect::from_min_max(
        pos2(canvas_rect.min.x + margin_x, canvas_rect.min.y + margin_top),
        pos2(
            canvas_rect.max.x - margin_x,
            canvas_rect.max.y - margin_bottom,
        ),
    )
}

/// Compute the screen rect of an object on a slide, given the central panel rect.
#[must_use]
pub fn object_screen_rect(
    course: &Course,
    slide_id: &str,
    object_id: &str,
    central_rect: Rect,
) -> Option<Rect> {
    let slide = course.slide(slide_id)?;
    let (obj, _) = slide.object(object_id)?;
    let c_rect = canvas_rect(central_rect);
    let fit_rect = canvas_fit_rect(c_rect);
    let transform = SlideTransform::fit(fit_rect, course.slide_size)?;
    let obj_slide_rect = Rect::from_min_size(
        pos2(obj.frame.x, obj.frame.y),
        vec2(obj.frame.w, obj.frame.h),
    );
    Some(transform.to_screen_rect(obj_slide_rect))
}

/// Resize a frame with a drag in its own axes, keeping the opposite edge or
/// corner still in local coordinates.
fn resize_frame(orig: Rect, handle: ResizeHandle, delta: egui::Vec2, shift: bool) -> Rect {
    let moves_left = matches!(
        handle,
        ResizeHandle::TopLeft | ResizeHandle::Left | ResizeHandle::BottomLeft
    );
    let moves_right = matches!(
        handle,
        ResizeHandle::TopRight | ResizeHandle::Right | ResizeHandle::BottomRight
    );
    let moves_top = matches!(
        handle,
        ResizeHandle::TopLeft | ResizeHandle::Top | ResizeHandle::TopRight
    );
    let moves_bottom = matches!(
        handle,
        ResizeHandle::BottomLeft | ResizeHandle::Bottom | ResizeHandle::BottomRight
    );
    let mut min = orig.min;
    let mut max = orig.max;
    if moves_left {
        min.x = (orig.min.x + delta.x).min(orig.max.x - 4.0);
    }
    if moves_right {
        max.x = (orig.max.x + delta.x).max(orig.min.x + 4.0);
    }
    if moves_top {
        min.y = (orig.min.y + delta.y).min(orig.max.y - 4.0);
    }
    if moves_bottom {
        max.y = (orig.max.y + delta.y).max(orig.min.y + 4.0);
    }
    let raw = Rect::from_min_max(min, max);
    if shift {
        aspect_bbox(orig, raw, handle)
    } else {
        raw
    }
}

/// The local point that must stay still while `handle` is dragged.
fn resize_anchor(orig: Rect, handle: ResizeHandle) -> Pos2 {
    let x = match handle {
        ResizeHandle::TopLeft | ResizeHandle::Left | ResizeHandle::BottomLeft => orig.max.x,
        ResizeHandle::TopRight | ResizeHandle::Right | ResizeHandle::BottomRight => orig.min.x,
        ResizeHandle::Top | ResizeHandle::Bottom => orig.center().x,
    };
    let y = match handle {
        ResizeHandle::TopLeft | ResizeHandle::Top | ResizeHandle::TopRight => orig.max.y,
        ResizeHandle::BottomLeft | ResizeHandle::Bottom | ResizeHandle::BottomRight => orig.min.y,
        ResizeHandle::Left | ResizeHandle::Right => orig.center().y,
    };
    pos2(x, y)
}

/// A rotated object resizes in its own axes. The drag delta is rotated back into
/// frame space, then the result is translated so the opposite edge or corner
/// stays fixed in slide space.
fn resize_frame_rotated(
    orig: Rect,
    handle: ResizeHandle,
    slide_delta: egui::Vec2,
    shift: bool,
    rotation: f32,
) -> Rect {
    let radians = if rotation.is_finite() {
        -rotation.to_radians()
    } else {
        0.0
    };
    let (sin, cos) = radians.sin_cos();
    let local_delta = vec2(
        slide_delta.x * cos - slide_delta.y * sin,
        slide_delta.x * sin + slide_delta.y * cos,
    );
    let raw = resize_frame(orig, handle, local_delta, shift);
    if rotation == 0.0 || !rotation.is_finite() {
        return raw;
    }
    let anchor = resize_anchor(orig, handle);
    let old_anchor = coursecraft_render::rotate_point(anchor, orig.center(), rotation);
    let new_anchor = coursecraft_render::rotate_point(anchor, raw.center(), rotation);
    let correction = old_anchor - new_anchor;
    Rect::from_min_size(raw.min + correction, raw.size())
}

fn compute_drag_frame(drag: &CanvasDrag, curr_screen: Pos2, shift: bool, scale: f32) -> Rect {
    let delta_x = (curr_screen.x - drag.start_screen.x) / scale;
    let delta_y = (curr_screen.y - drag.start_screen.y) / scale;

    match drag.op {
        DragOp::Move | DragOp::RegionMove => {
            let mut nx = drag.orig_frame.min.x + delta_x;
            let mut ny = drag.orig_frame.min.y + delta_y;
            if shift {
                nx = (nx / 10.0).round() * 10.0;
                ny = (ny / 10.0).round() * 10.0;
            }
            Rect::from_min_size(pos2(nx, ny), drag.orig_frame.size())
        }
        DragOp::Resize { handle, rotation } => resize_frame_rotated(
            drag.orig_frame,
            handle,
            vec2(delta_x, delta_y),
            shift,
            rotation,
        ),
        DragOp::RegionResize(handle) => {
            resize_frame(drag.orig_frame, handle, vec2(delta_x, delta_y), shift)
        }
        DragOp::Rotate { .. }
        | DragOp::MotionPathPoint { .. }
        | DragOp::Adjust { .. }
        | DragOp::PathPoint { .. }
        | DragOp::TableBorder { .. }
        | DragOp::Marquee => drag.orig_frame,
    }
}

fn get_handle_centers(screen_rect: Rect) -> [(ResizeHandle, Pos2); 8] {
    [
        (ResizeHandle::TopLeft, screen_rect.left_top()),
        (
            ResizeHandle::Top,
            pos2(screen_rect.center().x, screen_rect.top()),
        ),
        (ResizeHandle::TopRight, screen_rect.right_top()),
        (
            ResizeHandle::Right,
            pos2(screen_rect.right(), screen_rect.center().y),
        ),
        (ResizeHandle::BottomRight, screen_rect.right_bottom()),
        (
            ResizeHandle::Bottom,
            pos2(screen_rect.center().x, screen_rect.bottom()),
        ),
        (ResizeHandle::BottomLeft, screen_rect.left_bottom()),
        (
            ResizeHandle::Left,
            pos2(screen_rect.left(), screen_rect.center().y),
        ),
    ]
}

/// Screen-space handle centres on a rotated frame.
fn rotated_handle_centers(screen_rect: Rect, rotation: f32) -> [(ResizeHandle, Pos2); 8] {
    let centre = screen_rect.center();
    get_handle_centers(screen_rect).map(|(handle, point)| {
        (
            handle,
            coursecraft_render::rotate_point(point, centre, rotation),
        )
    })
}

/// The small rotation handle above a rotated frame's top edge.
fn rotate_handle_center(screen_rect: Rect, rotation: f32) -> Pos2 {
    let centre = screen_rect.center();
    let top = coursecraft_render::rotate_point(
        pos2(screen_rect.center().x, screen_rect.top()),
        centre,
        rotation,
    );
    let radians = rotation.to_radians();
    top + vec2(radians.sin(), -radians.cos()) * 20.0
}

fn selection_frames(editor: &Editor) -> Vec<(String, Rect)> {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return Vec::new();
    };
    editor
        .selection
        .ids()
        .into_iter()
        .filter_map(|id| {
            slide.object(&id).map(|(object, _)| {
                (
                    id,
                    Rect::from_min_size(
                        pos2(object.frame.x, object.frame.y),
                        vec2(object.frame.w, object.frame.h),
                    ),
                )
            })
        })
        .collect()
}

fn frames_bbox(frames: &[(String, Rect)]) -> Option<Rect> {
    let mut iter = frames.iter().map(|(_, rect)| *rect);
    let first = iter.next()?;
    Some(iter.fold(first, |acc, rect| acc.union(rect)))
}

/// The union of the selected frames' rotated bounding boxes.
fn frames_rotated_bbox(editor: &Editor, frames: &[(String, Rect)]) -> Option<Rect> {
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    let mut bbox: Option<Rect> = None;
    for (id, frame) in frames {
        let rotation = slide.object(id).map_or(0.0, |(object, _)| {
            coursecraft_course::normalize_rotation(object.rotation)
        });
        let centre = frame.center();
        let corners = [
            frame.left_top(),
            frame.right_top(),
            frame.right_bottom(),
            frame.left_bottom(),
        ];
        let mut min = pos2(f32::INFINITY, f32::INFINITY);
        let mut max = pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for corner in corners {
            let corner = coursecraft_render::rotate_point(corner, centre, rotation);
            min.x = min.x.min(corner.x);
            min.y = min.y.min(corner.y);
            max.x = max.x.max(corner.x);
            max.y = max.y.max(corner.y);
        }
        let bounds = Rect::from_min_max(min, max);
        bbox = Some(bbox.map_or(bounds, |current| current.union(bounds)));
    }
    bbox
}

fn finite_frame(frame: &coursecraft_course::Rect) -> bool {
    [frame.x, frame.y, frame.w, frame.h]
        .iter()
        .all(|value| value.is_finite())
}

/// Slide-space frames of the objects the moving selection can snap to.
fn snap_targets(editor: &Editor, moving: &[(String, Rect)]) -> Vec<Rect> {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return Vec::new();
    };
    let objects: Vec<&coursecraft_course::Object> = match editor.selection.layer.as_deref() {
        Some(lid) => slide
            .layer(lid)
            .map(|layer| layer.objects.iter().collect())
            .unwrap_or_default(),
        None => slide.objects.iter().collect(),
    };
    objects
        .into_iter()
        .filter(|object| !moving.iter().any(|(id, _)| id == &object.id))
        .filter(|object| finite_frame(&object.frame))
        .map(|object| {
            let frame = Rect::from_min_size(
                pos2(object.frame.x, object.frame.y),
                vec2(object.frame.w, object.frame.h),
            );
            let rotation = coursecraft_course::normalize_rotation(object.rotation);
            if rotation == 0.0 {
                return frame;
            }
            let centre = frame.center();
            let corners = [
                frame.left_top(),
                frame.right_top(),
                frame.right_bottom(),
                frame.left_bottom(),
            ];
            let mut min = pos2(f32::INFINITY, f32::INFINITY);
            let mut max = pos2(f32::NEG_INFINITY, f32::NEG_INFINITY);
            for corner in corners {
                let corner = coursecraft_render::rotate_point(corner, centre, rotation);
                min.x = min.x.min(corner.x);
                min.y = min.y.min(corner.y);
                max.x = max.x.max(corner.x);
                max.y = max.y.max(corner.y);
            }
            Rect::from_min_max(min, max)
        })
        .collect()
}

fn slide_space_rect(editor: &Editor) -> Rect {
    let size = editor.session.course.slide_size;
    Rect::from_min_size(pos2(0.0, 0.0), vec2(size.w as f32, size.h as f32))
}

fn draw_snap_guides(editor: &Editor, painter: &Painter, t: &SlideTransform, ui: &Ui) {
    if editor.ui.snap_guides.is_empty() {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let stroke = Stroke::new(1.0, p.accent);
    for guide in &editor.ui.snap_guides {
        if guide.vertical {
            let start = t.to_screen_pos(pos2(guide.at, guide.start));
            let end = t.to_screen_pos(pos2(guide.at, guide.end));
            painter.line_segment([start, end], stroke);
        } else {
            let start = t.to_screen_pos(pos2(guide.start, guide.at));
            let end = t.to_screen_pos(pos2(guide.end, guide.at));
            painter.line_segment([start, end], stroke);
        }
    }
}

fn scale_frames(orig_bbox: Rect, new_bbox: Rect, frames: &[(String, Rect)]) -> Vec<(String, Rect)> {
    let bw = orig_bbox.width();
    let bh = orig_bbox.height();
    if !bw.is_finite() || !bh.is_finite() || bw.abs() < 0.001 || bh.abs() < 0.001 {
        return frames.to_vec();
    }
    let sx = new_bbox.width() / bw;
    let sy = new_bbox.height() / bh;
    if !sx.is_finite() || !sy.is_finite() {
        return frames.to_vec();
    }
    frames
        .iter()
        .map(|(id, frame)| {
            let x = new_bbox.min.x + (frame.min.x - orig_bbox.min.x) * sx;
            let y = new_bbox.min.y + (frame.min.y - orig_bbox.min.y) * sy;
            let w = (frame.width() * sx).max(4.0);
            let h = (frame.height() * sy).max(4.0);
            let rect = if x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() {
                Rect::from_min_size(pos2(x, y), vec2(w, h))
            } else {
                *frame
            };
            (id.clone(), rect)
        })
        .collect()
}

/// Keep the original proportions while resizing with Shift. A corner follows
/// whichever direction changed more and keeps the opposite corner still. A side
/// handle keeps the opposite side still and grows evenly along that side.
fn aspect_bbox(orig: Rect, new: Rect, handle: ResizeHandle) -> Rect {
    let ow = orig.width();
    let oh = orig.height();
    if !ow.is_finite() || !oh.is_finite() || ow.abs() < 0.001 || oh.abs() < 0.001 {
        return new;
    }
    let sx = new.width() / ow;
    let sy = new.height() / oh;
    if !sx.is_finite() || !sy.is_finite() {
        return new;
    }
    let scale = match handle {
        ResizeHandle::Left | ResizeHandle::Right => sx,
        ResizeHandle::Top | ResizeHandle::Bottom => sy,
        _ if (sx - 1.0).abs() >= (sy - 1.0).abs() => sx,
        _ => sy,
    };
    let scale = scale.max(0.001);
    let w = (ow * scale).max(4.0);
    let h = (oh * scale).max(4.0);
    let x = match handle {
        ResizeHandle::TopLeft | ResizeHandle::Left | ResizeHandle::BottomLeft => orig.max.x - w,
        ResizeHandle::Top | ResizeHandle::Bottom => orig.center().x - w * 0.5,
        _ => orig.min.x,
    };
    let y = match handle {
        ResizeHandle::TopLeft | ResizeHandle::Top | ResizeHandle::TopRight => orig.max.y - h,
        ResizeHandle::Left | ResizeHandle::Right => orig.center().y - h * 0.5,
        _ => orig.min.y,
    };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// Test hook: resizing a rotated frame keeps the opposite corner in slide space.
#[cfg(test)]
pub(crate) fn rotated_resize_for_test(
    orig: Rect,
    handle: ResizeHandle,
    delta: egui::Vec2,
    shift: bool,
    rotation: f32,
) -> Rect {
    resize_frame_rotated(orig, handle, delta, shift, rotation)
}

/// Test hook: the frame a Shift+drag of `handle` by `delta` slide pixels gives.
#[cfg(test)]
pub(crate) fn shift_resize_for_test(orig: Rect, handle: ResizeHandle, delta: egui::Vec2) -> Rect {
    let min = pos2(
        if matches!(
            handle,
            ResizeHandle::TopLeft | ResizeHandle::Left | ResizeHandle::BottomLeft
        ) {
            orig.min.x + delta.x
        } else {
            orig.min.x
        },
        if matches!(
            handle,
            ResizeHandle::TopLeft | ResizeHandle::Top | ResizeHandle::TopRight
        ) {
            orig.min.y + delta.y
        } else {
            orig.min.y
        },
    );
    let max = pos2(
        if matches!(
            handle,
            ResizeHandle::TopRight | ResizeHandle::Right | ResizeHandle::BottomRight
        ) {
            orig.max.x + delta.x
        } else {
            orig.max.x
        },
        if matches!(
            handle,
            ResizeHandle::BottomLeft | ResizeHandle::Bottom | ResizeHandle::BottomRight
        ) {
            orig.max.y + delta.y
        } else {
            orig.max.y
        },
    );
    aspect_bbox(orig, Rect::from_min_max(min, max), handle)
}

/// Apply a trackpad pinch (or Ctrl+scroll) over the canvas. Returns the scroll
/// offset that keeps the point under the pointer still.
fn pinch_canvas_zoom(
    editor: &mut Editor,
    ui: &Ui,
    viewport: Rect,
    available: egui::Vec2,
    slide: egui::Vec2,
) -> Option<egui::Vec2> {
    let delta = ui.input(|i| i.zoom_delta());
    if !delta.is_finite() || delta <= 0.0 || (delta - 1.0).abs() < 0.0001 {
        return None;
    }
    if editor.ui.canvas_drag.is_some() || !ui.rect_contains_pointer(viewport) {
        return None;
    }
    let pointer = ui.input(|i| i.pointer.hover_pos())?;
    let slide_rect = editor.ui.last_slide_rect?;
    let current = editor
        .ui
        .zoom
        .fixed_scale()
        .unwrap_or_else(|| slide_rect.width() / slide.x);
    if !current.is_finite() || current <= 0.0 {
        return None;
    }
    let scale = (current * delta).clamp(PINCH_ZOOM_RANGE.0, PINCH_ZOOM_RANGE.1);
    if scale == current {
        return None;
    }
    editor.ui.zoom = ZoomLevel::Custom(scale);

    let in_slide = (pointer - slide_rect.min) / current;
    let (content, _) = canvas_content_size(available, slide, scale);
    let origin = (content - slide * scale) * 0.5;
    Some(origin + in_slide * scale - (pointer - viewport.min))
}

/// The scrollable size of the canvas at `scale`, and that scale's slide size.
fn canvas_content_size(
    available: egui::Vec2,
    slide: egui::Vec2,
    scale: f32,
) -> (egui::Vec2, egui::Vec2) {
    let scaled = slide * scale;
    (
        vec2(
            (scaled.x + 64.0).max(available.x),
            (scaled.y + 96.0).max(available.y),
        ),
        scaled,
    )
}

fn screen_rect_to_slide(t: &SlideTransform, rect: Rect) -> Rect {
    let a = t.to_slide_pos(rect.min);
    let b = t.to_slide_pos(rect.max);
    Rect::from_min_max(
        pos2(a.x.min(b.x), a.y.min(b.y)),
        pos2(a.x.max(b.x), a.y.max(b.y)),
    )
}

fn show_layer_pills(editor: &mut Editor, ui: &mut Ui, t: &SlideTransform) {
    // A master has no layers, so there is nothing to switch between or add.
    if editor.ui.master_view {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let pills_rect = Rect::from_min_size(
        pos2(t.slide_rect().min.x, t.slide_rect().min.y - 24.0 - 8.0),
        vec2(t.slide_rect().width(), 24.0),
    );

    let mut pills_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(pills_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    pills_ui.spacing_mut().item_spacing.x = 4.0;

    let is_base = editor.selection.layer.is_none();
    let base_text = coursecraft_render::small("Base").color(if is_base {
        p.accent_text
    } else {
        p.text_secondary
    });
    let base_btn = egui::Button::new(base_text)
        .fill(if is_base {
            p.accent_subtle
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(12)
        .min_size(vec2(0.0, 24.0));

    if pills_ui.add(base_btn).clicked() {
        editor.selection.layer = None;
        editor.selection.repair(&editor.session.course);
    }

    if let Some(slide) = editor.session.course.slide(&editor.selection.slide) {
        for layer in &slide.layers {
            let is_sel = editor.selection.layer.as_deref() == Some(&layer.id);
            let layer_text = coursecraft_render::small(&layer.name).color(if is_sel {
                p.accent_text
            } else {
                p.text_secondary
            });
            let layer_btn = egui::Button::new(layer_text)
                .fill(if is_sel {
                    p.accent_subtle
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(12)
                .min_size(vec2(0.0, 24.0));

            if pills_ui.add(layer_btn).clicked() {
                editor.selection.layer = Some(layer.id.clone());
                editor.selection.repair(&editor.session.course);
            }
        }
    }

    let plus_btn = egui::Button::new(
        coursecraft_render::icon(coursecraft_render::Icon::Plus, coursecraft_render::ICON_SM)
            .color(p.accent_text),
    )
    .fill(Color32::TRANSPARENT)
    .corner_radius(12)
    .min_size(vec2(24.0, 24.0));

    if pills_ui.add(plus_btn).on_hover_text("Add layer").clicked() {
        editor.run(
            "layer.add",
            serde_json::json!({ "slide": editor.selection.slide }),
        );
    }
}

/// Movement in screen points below which a press is a click, not a drag.
const PLACEMENT_CLICK_SLOP: f32 = 4.0;
/// Smallest frame a placement drag creates, in slide units.
const PLACEMENT_MIN_SIZE: f32 = 8.0;

fn finite_pos(pos: Pos2) -> bool {
    pos.x.is_finite() && pos.y.is_finite()
}

fn finite_rect(rect: Rect) -> bool {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
        .iter()
        .all(|value| value.is_finite())
}

/// The default placement size for a shape kind.
fn shape_size(kind: ShapeKind) -> (f32, f32) {
    match shape_info(kind) {
        Some(info) if info.tall => (120.0, 200.0),
        Some(info) if info.square => (160.0, 160.0),
        _ => (200.0, 120.0),
    }
}

/// The kind and default size a placement tool creates.
fn placement_kind_and_size(editor: &Editor, tool: &PlacementTool) -> (Json, (f32, f32)) {
    match tool {
        PlacementTool::TextBox => (
            serde_json::json!({
                "type": "text",
                "text": "Text",
                "style": {
                    "color": editor.session.course.theme.text.0,
                }
            }),
            (300.0, 60.0),
        ),
        PlacementTool::Button => (
            serde_json::json!({
                "type": "button",
                "label": "Button",
                "fill": editor.session.course.theme.accent.0,
            }),
            (200.0, 56.0),
        ),
        PlacementTool::Shape(kind) => (
            serde_json::json!({
                "type": "shape",
                "shape": kind,
                "fill": "#D0D7DE"
            }),
            shape_size(*kind),
        ),
        PlacementTool::Draw(_) => (
            serde_json::json!({
                "type": "path",
                "points": [{"x": 0.0, "y": 0.5}, {"x": 1.0, "y": 0.5}],
                "outline": {"color": editor.session.course.theme.text.0, "width": 2.0},
            }),
            (160.0, 1.0),
        ),
        PlacementTool::Image { src, natural_size } => {
            let slide_size = editor.session.course.slide_size;
            let (slide_w, slide_h) = (slide_size.w as f32, slide_size.h as f32);
            let (mut w, mut h) = *natural_size;
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
                w = 320.0;
                h = 180.0;
            }
            if (w > slide_w || h > slide_h) && slide_w > 0.0 && slide_h > 0.0 {
                let scale = (slide_w / w).min(slide_h / h);
                w = (w * scale).round().max(PLACEMENT_MIN_SIZE);
                h = (h * scale).round().max(PLACEMENT_MIN_SIZE);
            }
            (
                serde_json::json!({
                    "type": "image",
                    "src": src,
                    "alt": ""
                }),
                (w, h),
            )
        }
        PlacementTool::Audio { src } => (
            serde_json::json!({
                "type": "audio",
                "src": src,
                "autoplay": false,
            }),
            (48.0, 48.0),
        ),
        PlacementTool::Video { src } => (
            serde_json::json!({
                "type": "video",
                "src": src,
                "autoplay": false,
            }),
            (640.0, 360.0),
        ),
        PlacementTool::Lottie { src, natural_size } => {
            let slide_size = editor.session.course.slide_size;
            let (slide_w, slide_h) = (slide_size.w as f32, slide_size.h as f32);
            let (mut w, mut h) = *natural_size;
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
                w = 320.0;
                h = 180.0;
            }
            if (w > slide_w || h > slide_h) && slide_w > 0.0 && slide_h > 0.0 {
                let scale = (slide_w / w).min(slide_h / h);
                w = (w * scale).round().max(PLACEMENT_MIN_SIZE);
                h = (h * scale).round().max(PLACEMENT_MIN_SIZE);
            }
            (
                serde_json::json!({
                    "type": "lottie",
                    "src": src,
                }),
                (w, h),
            )
        }
    }
}

/// The aspect ratio a drag is held to, if any.
///
/// Images always keep their natural ratio, as crop mode does. Shift squares
/// shapes and audio, fixes video at 16:9, and holds text and buttons to their
/// default ratio.
fn placement_aspect(tool: &PlacementTool, shift: bool) -> Option<f32> {
    fn ratio(w: f32, h: f32) -> Option<f32> {
        if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
            Some(w / h)
        } else {
            None
        }
    }
    match tool {
        PlacementTool::Image { natural_size, .. } | PlacementTool::Lottie { natural_size, .. } => {
            ratio(natural_size.0, natural_size.1)
        }
        PlacementTool::Video { .. } if shift => ratio(16.0, 9.0),
        PlacementTool::Shape(kind) if shift => {
            let (w, h) = shape_size(*kind);
            ratio(w, h)
        }
        PlacementTool::Audio { .. } if shift => Some(1.0),
        PlacementTool::TextBox if shift => ratio(300.0, 60.0),
        PlacementTool::Button if shift => ratio(200.0, 56.0),
        _ => None,
    }
}

/// Build the frame a drag from `start` to `end` creates.
///
/// Both points are clamped to the slide. The frame keeps at least
/// [`PLACEMENT_MIN_SIZE`] on each axis when the slide has room, and stays
/// inside the slide even when the pointer goes outside it. A `ratio` makes the
/// frame keep that width-to-height ratio.
fn placement_frame_from_drag(
    start: Pos2,
    end: Pos2,
    slide: Rect,
    ratio: Option<f32>,
) -> Option<Rect> {
    if !finite_pos(start) || !finite_pos(end) || !finite_rect(slide) {
        return None;
    }
    let slide_w = slide.width();
    let slide_h = slide.height();
    if slide_w <= 0.0 || slide_h <= 0.0 {
        return None;
    }

    let min_x = PLACEMENT_MIN_SIZE.min(slide_w);
    let min_y = PLACEMENT_MIN_SIZE.min(slide_h);
    let start_x = start.x.clamp(slide.min.x, slide.max.x);
    let start_y = start.y.clamp(slide.min.y, slide.max.y);
    let end_x = end.x.clamp(slide.min.x, slide.max.x);
    let end_y = end.y.clamp(slide.min.y, slide.max.y);

    let sign_x = if end_x < start_x { -1.0 } else { 1.0 };
    let sign_y = if end_y < start_y { -1.0 } else { 1.0 };

    // Move the anchor inward when it sits too close to the edge for a frame
    // that respects both the minimum size and the slide bounds.
    let anchor_x = if sign_x > 0.0 {
        start_x.min(slide.max.x - min_x)
    } else {
        start_x.max(slide.min.x + min_x)
    };
    let anchor_y = if sign_y > 0.0 {
        start_y.min(slide.max.y - min_y)
    } else {
        start_y.max(slide.min.y + min_y)
    };
    let avail_w = if sign_x > 0.0 {
        slide.max.x - anchor_x
    } else {
        anchor_x - slide.min.x
    };
    let avail_h = if sign_y > 0.0 {
        slide.max.y - anchor_y
    } else {
        anchor_y - slide.min.y
    };
    if !avail_w.is_finite() || !avail_h.is_finite() || avail_w <= 0.0 || avail_h <= 0.0 {
        return None;
    }

    let raw_w = (end_x - start_x).abs();
    let raw_h = (end_y - start_y).abs();
    let (w, h) = if let Some(aspect) = ratio.filter(|value| value.is_finite() && *value > 0.0) {
        // Choose one uniform scale from whichever drag dimension is larger.
        let mut w = raw_w
            .max(raw_h * aspect)
            .max(min_x)
            .max(min_y * aspect)
            .max(0.0);
        let mut h = w / aspect;
        let fit = (avail_w / w).min(avail_h / h).min(1.0);
        if !fit.is_finite() || fit <= 0.0 {
            return None;
        }
        w *= fit;
        h *= fit;
        (w, h)
    } else {
        (
            raw_w.max(min_x).min(avail_w).max(0.0),
            raw_h.max(min_y).min(avail_h).max(0.0),
        )
    };
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }

    let frame = Rect::from_two_pos(
        pos2(anchor_x, anchor_y),
        pos2(anchor_x + sign_x * w, anchor_y + sign_y * h),
    );
    if !finite_rect(frame) {
        return None;
    }
    Some(frame)
}

/// The centered default frame a plain click creates.
fn default_placement_frame(editor: &Editor, slide_pos: Pos2, w: f32, h: f32) -> Rect {
    let slide_size = editor.session.course.slide_size;
    let slide_w = slide_size.w.max(1) as f32;
    let slide_h = slide_size.h.max(1) as f32;
    let w = if w.is_finite() && w > 0.0 {
        w
    } else {
        PLACEMENT_MIN_SIZE
    };
    let h = if h.is_finite() && h > 0.0 {
        h
    } else {
        PLACEMENT_MIN_SIZE
    };
    let pos_x = if slide_pos.x.is_finite() {
        slide_pos.x
    } else {
        0.0
    };
    let pos_y = if slide_pos.y.is_finite() {
        slide_pos.y
    } else {
        0.0
    };
    let x = if w <= slide_w {
        (pos_x - w / 2.0).round().clamp(0.0, slide_w - w)
    } else {
        0.0
    };
    let y = if h <= slide_h {
        (pos_y - h / 2.0).round().clamp(0.0, slide_h - h)
    } else {
        0.0
    };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// A fresh default name for a new shape, `Right arrow 1` style.
fn next_shape_name(editor: &Editor, kind: ShapeKind) -> String {
    let base = shape_label(kind);
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return base.to_owned();
    };
    for n in 1..=10_000u32 {
        let candidate = format!("{base} {n}");
        let taken = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            .any(|object| object.name == candidate);
        if !taken {
            return candidate;
        }
    }
    base.to_owned()
}

/// Create the object with `frame` through the same command a click uses.
fn place_object(editor: &mut Editor, tool: &PlacementTool, frame: Rect) {
    let is_text = matches!(tool, PlacementTool::TextBox);
    let (kind, _) = placement_kind_and_size(editor, tool);
    let mut params = serde_json::json!({
        "slide": editor.selection.slide,
        "kind": kind,
        "frame": {
            "x": frame.min.x.round(),
            "y": frame.min.y.round(),
            "w": frame.width().round().max(1.0),
            "h": frame.height().round().max(1.0)
        }
    });
    if let PlacementTool::Shape(kind) = tool {
        params["name"] = serde_json::json!(next_shape_name(editor, *kind));
    }
    if let Some(ref lid) = editor.selection.layer {
        params["layer"] = serde_json::json!(lid);
    }
    let result = editor.run("object.add", params);
    if is_text
        && let Some(id) = result
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    {
        start_inline_edit(editor, &id);
    }
}

/// Snap a placement frame's moving edges, keeping the anchored corner still.
///
/// Snapping is skipped when it would shrink the frame below the minimum, and
/// only guides for edges the drag actually moves are kept.
fn snap_placement_frame(
    editor: &Editor,
    raw: Rect,
    t: &SlideTransform,
    moves_left: bool,
    moves_right: bool,
    moves_top: bool,
    moves_bottom: bool,
) -> (Rect, Vec<crate::snap::SnapGuide>) {
    let scale = t.scale();
    if !editor.ui.snap_to_guides || !scale.is_finite() || scale <= 0.0 {
        return (raw, Vec::new());
    }
    let slide = slide_space_rect(editor);
    let targets = snap_targets(editor, &[]);
    let result = crate::snap::snap_rect(raw, &targets, slide, 6.0 / scale);

    let mut frame = raw;
    let mut guides = Vec::new();
    let x_guide = if moves_left || moves_right {
        result.guides.iter().find(|guide| guide.vertical)
    } else {
        None
    };
    if let Some(guide) = x_guide {
        let mut candidate = frame;
        if moves_right {
            candidate.max.x += result.offset.x;
        } else {
            candidate.min.x += result.offset.x;
        }
        let edge = if moves_right {
            candidate.max.x
        } else {
            candidate.min.x
        };
        if (edge - guide.at).abs() <= 0.01
            && candidate.width() >= PLACEMENT_MIN_SIZE.min(slide.width())
            && candidate.min.x >= slide.min.x - 0.01
            && candidate.max.x <= slide.max.x + 0.01
        {
            frame = candidate;
            guides.push(*guide);
        }
    }
    let y_guide = if moves_top || moves_bottom {
        result.guides.iter().find(|guide| !guide.vertical)
    } else {
        None
    };
    if let Some(guide) = y_guide {
        let mut candidate = frame;
        if moves_bottom {
            candidate.max.y += result.offset.y;
        } else {
            candidate.min.y += result.offset.y;
        }
        let edge = if moves_bottom {
            candidate.max.y
        } else {
            candidate.min.y
        };
        if (edge - guide.at).abs() <= 0.01
            && candidate.height() >= PLACEMENT_MIN_SIZE.min(slide.height())
            && candidate.min.y >= slide.min.y - 0.01
            && candidate.max.y <= slide.max.y + 0.01
        {
            frame = candidate;
            guides.push(*guide);
        }
    }
    (frame, guides)
}

/// A fresh default name for a drawn path, `Line 1` style.
fn next_draw_name(editor: &Editor, base: &str) -> String {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return base.to_owned();
    };
    for n in 1..=10_000u32 {
        let candidate = format!("{base} {n}");
        let taken = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            .any(|object| object.name == candidate);
        if !taken {
            return candidate;
        }
    }
    base.to_owned()
}

/// Snap a drag end to 45° steps around `start`.
fn snap_45(start: Pos2, end: Pos2) -> Pos2 {
    let delta = end - start;
    let len = delta.length();
    if !len.is_finite() || len <= 0.0 {
        return end;
    }
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (delta.y.atan2(delta.x) / step).round() * step;
    pos2(start.x + len * angle.cos(), start.y + len * angle.sin())
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if !len2.is_finite() || len2 <= 1e-8 {
        return (point - a).length();
    }
    let t = ((point - a).dot(ab) / len2).clamp(0.0, 1.0);
    (point - (a + ab * t)).length()
}

/// Ramer-Douglas-Peucker simplification, iterative so hostile point counts
/// cannot exhaust the stack.
fn simplify_rdp(points: &[Pos2], epsilon: f32) -> Vec<Pos2> {
    let count = points.len();
    if count <= 2 || !epsilon.is_finite() || epsilon <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; count];
    if let Some(first) = keep.first_mut() {
        *first = true;
    }
    if let Some(last) = keep.last_mut() {
        *last = true;
    }
    let mut stack: Vec<(usize, usize)> = vec![(0, count.saturating_sub(1))];
    let mut guard = 0usize;
    while let Some((start, end)) = stack.pop() {
        guard = guard.saturating_add(1);
        if guard > count.saturating_mul(4).saturating_add(64) || end <= start + 1 {
            continue;
        }
        let (Some(&a), Some(&b)) = (points.get(start), points.get(end)) else {
            continue;
        };
        let mut farthest = None;
        let mut farthest_d = 0.0f32;
        for (index, &point) in points.iter().enumerate().take(end).skip(start + 1) {
            let d = distance_to_segment(point, a, b);
            if d > farthest_d {
                farthest_d = d;
                farthest = Some(index);
            }
        }
        if farthest_d > epsilon
            && let Some(index) = farthest
            && let Some(slot) = keep.get_mut(index)
        {
            *slot = true;
            stack.push((start, index));
            stack.push((index, end));
        }
    }
    points
        .iter()
        .enumerate()
        .filter(|(index, _)| keep.get(*index).copied().unwrap_or(false))
        .map(|(_, point)| *point)
        .collect()
}

/// Keep at most `max` points, preserving the first and last.
fn cap_points(points: &[Pos2], max: usize) -> Vec<Pos2> {
    if points.len() <= max || max < 2 {
        return points.to_vec();
    }
    let last = points.len().saturating_sub(1);
    (0..max)
        .filter_map(|i| {
            let index = if max == 1 {
                0
            } else {
                i.saturating_mul(last) / (max - 1)
            };
            points.get(index).copied()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
struct ConnectorSnap {
    glue: Glue,
    point: Pos2,
}

fn is_connector(object: &Object) -> bool {
    matches!(&object.kind, ObjectKind::Path { path } if path.connector.is_some())
}

/// The four connection sites of a frame, in slide coordinates.
fn connector_sites(frame: coursecraft_course::Rect) -> [Pos2; 4] {
    [
        pos2(frame.x + frame.w * 0.5, frame.y),
        pos2(frame.x + frame.w, frame.y + frame.h * 0.5),
        pos2(frame.x + frame.w * 0.5, frame.y + frame.h),
        pos2(frame.x, frame.y + frame.h * 0.5),
    ]
}

fn same_owner_objects(editor: &Editor) -> Vec<&Object> {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return Vec::new();
    };
    match editor.selection.layer.as_deref() {
        Some(layer) => slide
            .layer(layer)
            .map(|layer| layer.objects.iter().collect())
            .unwrap_or_default(),
        None => slide.objects.iter().collect(),
    }
}

/// Snap the pointer to the nearest site dot within 12 screen px, ignoring
/// connectors and the object being edited. `Alt` disables snapping.
fn connector_snap(
    editor: &Editor,
    t: &SlideTransform,
    pointer: Pos2,
    exclude: Option<&str>,
    alt: bool,
) -> Option<ConnectorSnap> {
    if alt {
        return None;
    }
    let mut best: Option<(f32, ConnectorSnap)> = None;
    for object in same_owner_objects(editor) {
        if exclude == Some(object.id.as_str()) || is_connector(object) {
            continue;
        }
        for (site, point) in connector_sites(object.frame).into_iter().enumerate() {
            let screen = t.to_screen_pos(point);
            let distance = (screen - pointer).length();
            if distance.is_finite()
                && distance <= 12.0
                && best.as_ref().is_none_or(|(best, _)| distance < *best)
            {
                #[allow(clippy::cast_possible_truncation)]
                let site = site as u8;
                best = Some((
                    distance,
                    ConnectorSnap {
                        glue: Glue {
                            object: object.id.clone(),
                            site,
                        },
                        point,
                    },
                ));
            }
        }
    }
    best.map(|(_, snap)| snap)
}

/// The object whose four site dots should show: the nearest snap target, else
/// the topmost object under or near the pointer in the same owner.
fn connector_site_target(
    editor: &Editor,
    t: &SlideTransform,
    pointer: Pos2,
    exclude: Option<&str>,
) -> Option<(String, coursecraft_course::Rect)> {
    let objects = same_owner_objects(editor);
    let snap = connector_snap(editor, t, pointer, exclude, false);
    if let Some(snap) = snap
        && let Some(object) = objects.iter().find(|object| object.id == snap.glue.object)
    {
        return Some((object.id.clone(), object.frame));
    }
    let near = 24.0;
    objects
        .iter()
        .rev()
        .filter(|object| exclude != Some(object.id.as_str()) && !is_connector(object))
        .find(|object| {
            let frame = coursecraft_course::Rect {
                x: object.frame.x,
                y: object.frame.y,
                w: object.frame.w.max(1.0),
                h: object.frame.h.max(1.0),
            };
            t.to_screen_rect(Rect::from_min_size(
                pos2(frame.x, frame.y),
                vec2(frame.w, frame.h),
            ))
            .expand(near)
            .contains(pointer)
        })
        .map(|object| (object.id.clone(), object.frame))
}

fn fresh_path_id(editor: &Editor) -> String {
    let slide = editor.session.course.slide(&editor.selection.slide);
    (1u64..)
        .map(|n| format!("path{n}"))
        .find(|candidate| slide.is_none_or(|slide| !slide.local_ids().any(|id| id == candidate)))
        .unwrap_or_else(|| "path".to_owned())
}

/// Commit a drawn path through one `object.add` command.
fn commit_draw(
    editor: &mut Editor,
    tool: DrawTool,
    points: &[Pos2],
    closed: bool,
    start_glue: Option<Glue>,
    end_glue: Option<Glue>,
) {
    let mut points: Vec<Pos2> = points
        .iter()
        .copied()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .collect();
    points = cap_points(&points, coursecraft_course::PATH_MAX_POINTS);
    if points.len() < coursecraft_course::PATH_MIN_POINTS {
        return;
    }
    let slide_points: Vec<Point> = points
        .iter()
        .map(|point| Point {
            x: point.x,
            y: point.y,
        })
        .collect();
    let Some((frame, fractions)) = coursecraft_course::path_frame_and_points(&slide_points) else {
        return;
    };
    let theme = &editor.session.course.theme;
    let style = if tool == DrawTool::Curve {
        PathStyle::Curved
    } else {
        PathStyle::Straight
    };
    let connector = match tool {
        DrawTool::ElbowConnector => Some(ConnectorStyle::Elbow),
        DrawTool::CurvedConnector => Some(ConnectorStyle::Curved),
        DrawTool::Line
        | DrawTool::Arrow
        | DrawTool::Curve
        | DrawTool::Freeform
        | DrawTool::Scribble => None,
    };
    let wants_glue = connector.is_some() && (start_glue.is_some() || end_glue.is_some());
    let path = Path {
        points: fractions,
        style,
        closed,
        fill: closed.then(|| theme.accent.clone()),
        outline: coursecraft_course::Outline {
            color: theme.text.clone(),
            width: 2.0,
            dash: coursecraft_course::DashStyle::Solid,
        },
        start_end: LineEnd::None,
        end_end: if tool == DrawTool::Arrow {
            LineEnd::Arrow
        } else {
            LineEnd::None
        },
        connector,
        start_glue: start_glue.clone(),
        end_glue: end_glue.clone(),
    };
    let name = next_draw_name(editor, tool.base_name());
    if wants_glue {
        // Add first, then attach both ends through `object.set_path` in the same
        // batch, so the placement is one undo step.
        let id = fresh_path_id(editor);
        let mut plain = path.clone();
        plain.start_glue = None;
        plain.end_glue = None;
        let kind = ObjectKind::Path { path: plain };
        let Ok(kind_json) = serde_json::to_value(&kind) else {
            return;
        };
        let mut add = serde_json::json!({
            "slide": editor.selection.slide,
            "id": id,
            "kind": kind_json,
            "frame": {
                "x": frame.x,
                "y": frame.y,
                "w": frame.w,
                "h": frame.h,
            },
            "name": name,
        });
        if let Some(ref lid) = editor.selection.layer {
            add["layer"] = serde_json::json!(lid);
        }
        let attach = serde_json::json!({
            "slide": editor.selection.slide,
            "object": id,
            "points": slide_points,
            "start_glue": start_glue,
            "end_glue": end_glue,
        });
        let steps = vec![
            ("object.add".to_owned(), add),
            ("object.set_path".to_owned(), attach),
        ];
        let placed = editor.run_batch(&steps).is_some();
        editor.ui.placement_tool = None;
        editor.ui.draw = None;
        if placed {
            editor.select_object_group(&id);
        }
        return;
    }

    let kind = ObjectKind::Path { path };
    let Ok(kind_json) = serde_json::to_value(&kind) else {
        return;
    };
    let mut params = serde_json::json!({
        "slide": editor.selection.slide,
        "kind": kind_json,
        "frame": {
            "x": frame.x,
            "y": frame.y,
            "w": frame.w,
            "h": frame.h,
        },
        "name": name,
    });
    if let Some(ref lid) = editor.selection.layer {
        params["layer"] = serde_json::json!(lid);
    }
    let result = editor.run("object.add", params);
    editor.ui.placement_tool = None;
    editor.ui.draw = None;
    if let Some(id) = result
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
    {
        editor.select_object_group(id);
    }
}

/// Finish a curve, freeform or scribble session.
fn finish_draw_session(editor: &mut Editor, closed: bool) {
    let Some(session) = editor.ui.draw.clone() else {
        return;
    };
    if session.points.len() < coursecraft_course::PATH_MIN_POINTS {
        editor.ui.draw = None;
        return;
    }
    commit_draw(editor, session.tool, &session.points, closed, None, None);
}

/// Read and apply input for an armed draw tool.
#[allow(clippy::cognitive_complexity)]
fn process_draw_tool(editor: &mut Editor, t: &SlideTransform, ui: &Ui, response: &egui::Response) {
    let Some(PlacementTool::Draw(tool)) = editor.ui.placement_tool.clone() else {
        return;
    };
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let slide_pos = pointer.map(|pos| t.to_slide_pos(pos));
    let inside = pointer.is_some_and(|pos| t.slide_rect().contains(pos));
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let shift = ui.input(|i| i.modifiers.shift);
    let alt = ui.input(|i| i.modifiers.alt);
    let is_connector_tool = matches!(tool, DrawTool::ElbowConnector | DrawTool::CurvedConnector);
    let double = ui.input(|i| {
        i.pointer
            .button_double_clicked(egui::PointerButton::Primary)
            || i.pointer
                .button_triple_clicked(egui::PointerButton::Primary)
    });
    let scale = t.scale();
    let close_radius = if scale.is_finite() && scale > 0.0 {
        8.0 / scale
    } else {
        8.0
    };

    if tool.is_drag() {
        if primary_pressed
            && inside
            && let Some(raw_start) = slide_pos
            && finite_pos(raw_start)
        {
            let snap = if is_connector_tool {
                pointer.and_then(|pointer| connector_snap(editor, t, pointer, None, alt))
            } else {
                None
            };
            let start = snap.as_ref().map_or(raw_start, |snap| snap.point);
            editor.ui.draw = Some(DrawSession {
                tool,
                points: vec![start, start],
                press: Some(start),
                dragging: true,
                moved: false,
                start_glue: snap.map(|snap| snap.glue),
            });
        }
        let Some(mut session) = editor.ui.draw.clone() else {
            return;
        };
        if session.tool != tool {
            editor.ui.draw = None;
            return;
        }
        let Some(start) = session.press else {
            editor.ui.draw = None;
            return;
        };
        if let Some(current) = slide_pos
            && finite_pos(current)
            && let Some(slot) = session.points.get_mut(1)
        {
            *slot = current;
            session.moved = (current - start).length() >= PLACEMENT_CLICK_SLOP;
        }
        if primary_released || !primary_down {
            let raw_end = slide_pos
                .filter(|pos| finite_pos(*pos))
                .or_else(|| session.points.get(1).copied())
                .unwrap_or(start);
            let end_snap = if is_connector_tool {
                pointer.and_then(|pointer| connector_snap(editor, t, pointer, None, alt))
            } else {
                None
            };
            let moved = session.moved
                || ((raw_end - start).length() >= PLACEMENT_CLICK_SLOP
                    && (raw_end - start).length().is_finite());
            let end = if let Some(snap) = &end_snap {
                snap.point
            } else if moved && shift {
                snap_45(start, raw_end)
            } else if !moved {
                pos2(start.x + 160.0, start.y)
            } else {
                raw_end
            };
            if (end - start).length() > 1e-4 {
                commit_draw(
                    editor,
                    tool,
                    &[start, end],
                    false,
                    session.start_glue.clone(),
                    end_snap.map(|snap| snap.glue),
                );
            } else {
                editor.ui.draw = None;
                editor.ui.placement_tool = None;
            }
        } else {
            editor.ui.draw = Some(session);
            ui.ctx().request_repaint();
        }
        return;
    }

    if tool == DrawTool::Scribble {
        if primary_pressed
            && inside
            && let Some(start) = slide_pos
            && finite_pos(start)
        {
            editor.ui.draw = Some(DrawSession {
                tool,
                points: vec![start],
                press: Some(start),
                dragging: true,
                moved: false,
                start_glue: None,
            });
        }
        let Some(mut session) = editor.ui.draw.clone() else {
            return;
        };
        if session.tool != tool {
            editor.ui.draw = None;
            return;
        }
        if let (Some(current), true) = (slide_pos, session.dragging)
            && finite_pos(current)
            && session
                .points
                .last()
                .is_none_or(|last| (*last - current).length() >= 1.0)
            && session.points.len() < coursecraft_course::PATH_MAX_POINTS.saturating_mul(2)
        {
            session.points.push(current);
            session.moved = true;
        }
        if primary_released || !primary_down {
            editor.ui.draw = None;
            editor.ui.placement_tool = None;
            let simplified = simplify_rdp(&session.points, 1.5);
            let simplified = cap_points(&simplified, coursecraft_course::PATH_MAX_POINTS);
            if simplified.len() >= coursecraft_course::PATH_MIN_POINTS {
                commit_draw(editor, tool, &simplified, false, None, None);
            }
        } else {
            editor.ui.draw = Some(session);
            ui.ctx().request_repaint();
        }
        return;
    }

    // Curve and Freeform: click to add points, with a freehand drag for Freeform.
    let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
    if !wants_keyboard && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        finish_draw_session(editor, false);
        return;
    }
    if !wants_keyboard
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
        && let Some(session) = editor.ui.draw.as_mut()
    {
        session.points.pop();
        session.moved = false;
        return;
    }

    if primary_pressed && inside {
        let Some(point) = slide_pos else {
            return;
        };
        if !finite_pos(point) {
            return;
        }
        match editor.ui.draw.as_mut() {
            Some(session) if session.tool == tool => {
                session.press = Some(point);
                session.dragging = true;
                session.moved = false;
                if tool == DrawTool::Freeform
                    && session
                        .points
                        .last()
                        .is_none_or(|last| (*last - point).length() >= 1.0)
                {
                    session.points.push(point);
                }
            }
            _ => {
                let mut points = Vec::new();
                if tool == DrawTool::Freeform {
                    points.push(point);
                }
                editor.ui.draw = Some(DrawSession {
                    tool,
                    points,
                    press: Some(point),
                    dragging: true,
                    moved: false,
                    start_glue: None,
                });
            }
        }
        ui.ctx().request_repaint();
    }

    if let (Some(session), Some(current)) = (editor.ui.draw.as_mut(), slide_pos)
        && session.dragging
        && tool == DrawTool::Freeform
        && finite_pos(current)
        && session
            .points
            .last()
            .is_none_or(|last| (*last - current).length() >= 1.0)
        && session.points.len() < coursecraft_course::PATH_MAX_POINTS.saturating_mul(2)
    {
        session.points.push(current);
        session.moved = true;
    }

    if primary_released || !primary_down {
        let Some(mut session) = editor.ui.draw.take() else {
            return;
        };
        if double {
            // Finish without adding the double-click's second point.
            if session.points.len() >= coursecraft_course::PATH_MIN_POINTS {
                commit_draw(editor, tool, &session.points, false, None, None);
            }
            return;
        }
        let Some(current) = slide_pos.filter(|pos| finite_pos(*pos)) else {
            editor.ui.draw = Some(session);
            return;
        };
        let closing = session.points.len() >= 3
            && session
                .points
                .first()
                .is_some_and(|first| (*first - current).length() <= close_radius);
        session.dragging = false;
        if closing {
            commit_draw(editor, tool, &session.points, true, None, None);
            return;
        }
        if !session.moved
            && session
                .points
                .last()
                .is_none_or(|last| (*last - current).length() >= 1.0)
        {
            session.points.push(current);
        }
        if session.points.len() >= coursecraft_course::PATH_MAX_POINTS.saturating_mul(2) {
            session.points = cap_points(&session.points, coursecraft_course::PATH_MAX_POINTS);
        }
        editor.ui.draw = Some(session);
        ui.ctx().request_repaint();
    }
    let _ = response;
}

/// Follow an armed placement press until release, then place the object.
fn process_placement_drag(editor: &mut Editor, t: &SlideTransform, ui: &Ui) {
    let Some(drag) = editor.ui.placement_drag.clone() else {
        return;
    };
    let Some(tool) = editor.ui.placement_tool.clone() else {
        editor.ui.placement_drag = None;
        return;
    };
    ui.ctx().request_repaint();

    let pointer = ui
        .input(|i| i.pointer.interact_pos())
        .unwrap_or_else(|| t.to_screen_pos(drag.start_slide));
    let (primary_down, primary_released, shift, command_held) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.modifiers.shift,
            i.modifiers.command,
        )
    });

    let start_screen = t.to_screen_pos(drag.start_slide);
    let moved = (pointer - start_screen).length();
    let is_click = !moved.is_finite() || moved < PLACEMENT_CLICK_SLOP;
    let slide = slide_space_rect(editor);
    if !slide.contains(drag.start_slide) {
        // A press outside the slide never places anything. A drag leaves the
        // tool armed; a plain click outside still cancels it as before.
        if primary_released || !primary_down {
            editor.ui.placement_drag = None;
            editor.ui.snap_guides.clear();
            if is_click {
                editor.ui.placement_tool = None;
            }
        }
        return;
    }
    let end_slide = t.to_slide_pos(pointer);
    let ratio = placement_aspect(&tool, shift);
    let computed = placement_frame_from_drag(drag.start_slide, end_slide, slide, ratio);

    let placed_frame = if is_click {
        None
    } else if let Some(raw) = computed {
        if editor.ui.snap_to_guides && !command_held {
            Some(snap_placement_frame(
                editor,
                raw,
                t,
                end_slide.x < drag.start_slide.x,
                end_slide.x >= drag.start_slide.x,
                end_slide.y < drag.start_slide.y,
                end_slide.y >= drag.start_slide.y,
            ))
        } else {
            Some((raw, Vec::new()))
        }
    } else {
        None
    };

    if primary_released || !primary_down {
        editor.ui.placement_drag = None;
        editor.ui.snap_guides.clear();
        editor.ui.placement_tool = None;
        if is_click {
            let (_, (w, h)) = placement_kind_and_size(editor, &tool);
            let frame = default_placement_frame(editor, drag.start_slide, w, h);
            place_object(editor, &tool, frame);
        } else if let Some((frame, _)) = placed_frame {
            place_object(editor, &tool, frame);
        }
        return;
    }

    if is_click {
        editor.ui.snap_guides.clear();
        if let Some(state) = editor.ui.placement_drag.as_mut() {
            state.frame = Rect::from_min_size(drag.start_slide, vec2(0.0, 0.0));
        }
        return;
    }

    if let Some((frame, guides)) = placed_frame {
        editor.ui.snap_guides = guides;
        if let Some(state) = editor.ui.placement_drag.as_mut() {
            state.frame = frame;
        }
    } else {
        editor.ui.snap_guides.clear();
    }
}

fn pick_object(
    course: &Course,
    selection: &crate::Selection,
    locked: &std::collections::BTreeSet<String>,
    slide_pos: Pos2,
    items: &[DrawItem],
) -> Option<(String, Rect)> {
    let slide = course.slide(&selection.slide)?;
    for item in items.iter().rev() {
        if let Some(layer_id) = selection.layer.as_deref() {
            let in_layer = matches!(&item.hit, Hit::Object(id)
                if slide
                    .layer(layer_id)
                    .is_some_and(|l| l.objects.iter().any(|o| &o.id == id)));
            if !in_layer {
                continue;
            }
        }
        if let Hit::Object(ref id) = item.hit
            && !locked.contains(id)
            && coursecraft_render::item_hit_test(item, slide_pos)
        {
            return Some((id.clone(), item.rect));
        }
    }
    None
}

/// A table inner border near `start_pos` (screen), as `(object, column?, before, fractions)`.
fn table_border_hit(
    editor: &Editor,
    t: &SlideTransform,
    start_pos: Pos2,
) -> Option<(String, bool, usize, Vec<f32>)> {
    if editor.selection.is_multi() {
        return None;
    }
    let object_id = editor.selection.object.clone()?;
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    let (object, _) = slide.object(&object_id)?;
    let ObjectKind::Table { columns, rows, .. } = &object.kind else {
        return None;
    };
    let frame = Rect::from_min_size(
        pos2(object.frame.x, object.frame.y),
        vec2(object.frame.w, object.frame.h),
    );
    if !frame.is_positive() {
        return None;
    }
    let point = t.to_slide_pos(start_pos);
    if !point.x.is_finite() || !point.y.is_finite() || !frame.contains(point) {
        return None;
    }
    let scale = t.scale();
    let threshold = if scale.is_finite() && scale > 0.0 {
        6.0 / scale
    } else {
        6.0
    };
    let columns_offsets = coursecraft_course::table_fraction_offsets(columns);
    let mut best: Option<(f32, bool, usize, Vec<f32>)> = None;
    for boundary in 1..columns_offsets.len() {
        let Some((_, fraction)) = columns_offsets.get(boundary.saturating_sub(1)) else {
            continue;
        };
        let x = frame.left() + fraction * frame.width();
        let distance = (point.x - x).abs();
        if distance <= threshold && best.as_ref().is_none_or(|(d, _, _, _)| distance < *d) {
            best = Some((distance, true, boundary, columns.clone()));
        }
    }
    let heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    let row_offsets = coursecraft_course::table_fraction_offsets(&heights);
    for boundary in 1..row_offsets.len() {
        let Some((_, fraction)) = row_offsets.get(boundary.saturating_sub(1)) else {
            continue;
        };
        let y = frame.top() + fraction * frame.height();
        let distance = (point.y - y).abs();
        if distance <= threshold && best.as_ref().is_none_or(|(d, _, _, _)| distance < *d) {
            best = Some((distance, false, boundary, heights.clone()));
        }
    }
    best.map(|(_, column, before, fractions)| (object_id, column, before, fractions))
}

#[allow(clippy::cognitive_complexity)]
fn start_drag_if_pressed(
    editor: &mut Editor,
    start_pos: Pos2,
    t: &SlideTransform,
    items: &[DrawItem],
    modifiers: egui::Modifiers,
) {
    editor.ui.last_active_area = crate::ActiveArea::Canvas;
    // A locked object has no canvas handles: pressing the canvas leaves it.
    if editor
        .selection
        .ids()
        .iter()
        .any(|id| editor.ui.locked_objects.contains(id))
    {
        editor.selection.clear_objects();
    }
    let mut path_point_hit = None;
    if !editor.selection.is_multi()
        && let Some(ref sel_id) = editor.selection.object
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(sel_id)
        && let ObjectKind::Path { path } = &obj.kind
    {
        let editing = editor
            .ui
            .path_edit
            .as_ref()
            .is_some_and(|edit| &edit.object == sel_id);
        let connector = path.connector.is_some();
        let last_index = path.points.len().saturating_sub(1);
        if editing || path.points.len() == 2 || connector {
            for (index, point) in path.points.iter().enumerate() {
                if !editing && connector && index != 0 && index != last_index {
                    continue;
                }
                let centre = t.to_screen_pos(pos2(
                    obj.frame.x + point.x * obj.frame.w,
                    obj.frame.y + point.y * obj.frame.h,
                ));
                let hit_rect = Rect::from_center_size(centre, vec2(12.0, 12.0));
                if hit_rect.contains(start_pos) {
                    if editing && let Some(edit) = editor.ui.path_edit.as_mut() {
                        edit.selected_point = Some(index);
                    }
                    path_point_hit = Some((
                        sel_id.clone(),
                        index,
                        path.points.clone(),
                        path.points.len() == 2,
                        Rect::from_min_size(
                            pos2(obj.frame.x, obj.frame.y),
                            vec2(obj.frame.w, obj.frame.h),
                        ),
                    ));
                    break;
                }
            }
        }
    }
    if let Some((id, index, original, two_point, orig_frame)) = path_point_hit {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::PathPoint {
                index,
                original,
                two_point,
            },
            object_id: id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
        });
        return;
    }
    let mut path_handle_hit = None;
    if !editor.selection.is_multi()
        && let Some(ref sel_id) = editor.selection.object
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(sel_id)
    {
        for path in &obj.motion_paths {
            for (idx, pt) in path.points.iter().enumerate() {
                if path.relative && idx == 0 {
                    continue;
                }
                let slide_pt = if path.relative {
                    pos2(obj.frame.x + pt.x, obj.frame.y + pt.y)
                } else {
                    pos2(pt.x, pt.y)
                };
                let screen_pt = t.to_screen_pos(slide_pt);
                let h_rect = Rect::from_center_size(screen_pt, vec2(10.0, 10.0));
                if h_rect.contains(start_pos) {
                    path_handle_hit = Some((
                        sel_id.clone(),
                        path.id.clone(),
                        idx,
                        path.points.clone(),
                        path.relative,
                        pos2(obj.frame.x, obj.frame.y),
                    ));
                    break;
                }
            }
            if path_handle_hit.is_some() {
                break;
            }
        }
    }

    if path_handle_hit.is_none()
        && let Some((object_id, column, before, original)) = table_border_hit(editor, t, start_pos)
    {
        let frame = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|slide| slide.object(&object_id))
            .map(|(object, _)| {
                Rect::from_min_size(
                    pos2(object.frame.x, object.frame.y),
                    vec2(object.frame.w, object.frame.h),
                )
            })
            .unwrap_or(Rect::ZERO);
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::TableBorder {
                column,
                before,
                original,
            },
            object_id,
            orig_frame: frame,
            start_screen: start_pos,
            frames: Vec::new(),
        });
        return;
    }

    let mut adjust_handle_hit = None;
    if path_handle_hit.is_none()
        && editor.ui.preview_state.is_none()
        && !editor.selection.is_multi()
        && let Some(ref sel_id) = editor.selection.object
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(sel_id)
        && let coursecraft_course::ObjectKind::Shape { shape, adjust, .. } = &obj.kind
        && coursecraft_course::shape_is_adjustable(*shape)
    {
        let hits = coursecraft_render::adjust_handles(*shape, obj.frame.w, obj.frame.h, adjust);
        for (index, (x, y)) in hits.iter().enumerate() {
            let screen = t.to_screen_pos(pos2(obj.frame.x + x, obj.frame.y + y));
            let hit_rect = Rect::from_center_size(screen, vec2(12.0, 12.0));
            if hit_rect.contains(start_pos) {
                adjust_handle_hit = Some((sel_id.clone(), index, adjust.clone()));
                break;
            }
        }
    }

    let frames = selection_frames(editor);
    let bbox = frames.iter().fold(None, |acc: Option<Rect>, (id, _)| {
        let Some(item) = items
            .iter()
            .find(|item| item.hit == Hit::Object(id.clone()))
        else {
            return acc;
        };
        let bounds = coursecraft_render::item_bounds(item);
        Some(acc.map_or(bounds, |current| current.union(bounds)))
    });
    let bbox = bbox.or_else(|| frames_bbox(&frames));
    let primary_item = if editor.selection.is_multi() {
        None
    } else {
        editor.selection.object.as_ref().and_then(|id| {
            items
                .iter()
                .find(|item| item.hit == Hit::Object(id.clone()))
        })
    };
    let mut handle_hit = None;
    let mut rotate_hit = None;

    if path_handle_hit.is_none()
        && adjust_handle_hit.is_none()
        && let Some(item) = primary_item
    {
        let screen_rect = t.to_screen_rect(item.rect);
        let handle = rotate_handle_center(screen_rect, item.rotation);
        if Rect::from_center_size(handle, vec2(14.0, 14.0)).contains(start_pos) {
            let centre = screen_rect.center();
            let start_angle = (start_pos.y - centre.y).atan2(start_pos.x - centre.x);
            rotate_hit = Some((centre, start_angle, item.rotation));
        }
    }

    if path_handle_hit.is_none()
        && adjust_handle_hit.is_none()
        && rotate_hit.is_none()
        && let Some(bbox) = bbox
    {
        let (centers, orig) = match primary_item {
            Some(item) => {
                let screen_rect = t.to_screen_rect(item.rect);
                (
                    rotated_handle_centers(screen_rect, item.rotation),
                    item.rect,
                )
            }
            None => (get_handle_centers(t.to_screen_rect(bbox)), bbox),
        };
        for (h, c) in centers {
            let h_rect = Rect::from_center_size(c, vec2(8.0, 8.0));
            if h_rect.contains(start_pos) {
                handle_hit = Some((h, orig));
                break;
            }
        }
    }

    let mut region_handle_hit = None;
    let mut region_hit = None;

    if path_handle_hit.is_none()
        && handle_hit.is_none()
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some(ref q) = slide.question
        && q.kind == QuestionKind::Hotspot
    {
        if let Some(ref sel_r_id) = editor.ui.question.selected_region
            && let Some(region) = q.regions.iter().find(|r| &r.id == sel_r_id)
        {
            let r_rect = Rect::from_min_size(pos2(region.x, region.y), vec2(region.w, region.h));
            let screen_rect = t.to_screen_rect(r_rect);
            for (h, c) in get_handle_centers(screen_rect) {
                let h_rect = Rect::from_center_size(c, vec2(8.0, 8.0));
                if h_rect.contains(start_pos) {
                    region_handle_hit = Some((h, sel_r_id.clone(), r_rect));
                    break;
                }
            }
        }
        if region_handle_hit.is_none() && t.slide_rect().contains(start_pos) {
            let slide_pos = t.to_slide_pos(start_pos);
            for region in q.regions.iter().rev() {
                let r_rect =
                    Rect::from_min_size(pos2(region.x, region.y), vec2(region.w, region.h));
                if r_rect.contains(slide_pos) {
                    region_hit = Some((region.id.clone(), r_rect));
                    break;
                }
            }
        }
    }

    if let Some((sel_id, path_id, point_index, orig_points, relative, obj_origin)) = path_handle_hit
    {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::MotionPathPoint {
                path_id,
                point_index,
                orig_points,
                relative,
                obj_origin,
            },
            object_id: sel_id,
            orig_frame: Rect::ZERO,
            start_screen: start_pos,
            frames: Vec::new(),
        });
    } else if let Some((sel_id, index, original)) = adjust_handle_hit {
        let orig_frame = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|slide| slide.object(&sel_id))
            .map(|(obj, _)| {
                Rect::from_min_size(
                    pos2(obj.frame.x, obj.frame.y),
                    vec2(obj.frame.w, obj.frame.h),
                )
            })
            .unwrap_or(Rect::ZERO);
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::Adjust { index, original },
            object_id: sel_id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
        });
    } else if let Some((centre, start_angle, orig_rotation)) = rotate_hit {
        let primary = editor.selection.object.clone().unwrap_or_default();
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::Rotate {
                centre,
                start_angle,
                orig_rotation,
            },
            object_id: primary,
            orig_frame: primary_item.map_or(Rect::ZERO, |item| item.rect),
            start_screen: start_pos,
            frames: Vec::new(),
        });
    } else if let Some((h, orig_frame)) = handle_hit {
        let primary = editor
            .selection
            .object
            .clone()
            .or_else(|| frames.first().map(|(id, _)| id.clone()))
            .unwrap_or_default();
        let rotation = primary_item.map_or(0.0, |item| item.rotation);
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::Resize {
                handle: h,
                rotation,
            },
            object_id: primary,
            orig_frame,
            start_screen: start_pos,
            frames,
        });
    } else if let Some((h, region_id, orig_frame)) = region_handle_hit {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::RegionResize(h),
            object_id: region_id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
        });
    } else if let Some((region_id, orig_frame)) = region_hit {
        editor.ui.question.selected_region = Some(region_id.clone());
        editor.selection.clear_objects();
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::RegionMove,
            object_id: region_id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
        });
    } else if t.slide_rect().contains(start_pos) {
        let slide_pos = t.to_slide_pos(start_pos);
        if let Some((id, orig_frame)) = pick_object(
            &editor.session.course,
            &editor.selection,
            &editor.ui.locked_objects,
            slide_pos,
            items,
        ) {
            if modifiers.shift {
                // Toggle on release, so a click does not also start a drag.
                return;
            }
            if !editor.selection.contains(&id) {
                editor.select_object_group(&id);
            }
            // Moving a glued connector would immediately be undone by the
            // engine's re-route, so the press only selects it.
            let glued_connector = editor
                .session
                .course
                .slide(&editor.selection.slide)
                .and_then(|slide| slide.object(&id))
                .is_some_and(|(object, _)| {
                    matches!(&object.kind, ObjectKind::Path { path }
                        if path.connector.is_some()
                            && (path.start_glue.is_some() || path.end_glue.is_some()))
                });
            if glued_connector {
                return;
            }
            let frames = selection_frames(editor);
            editor.ui.canvas_drag = Some(CanvasDrag {
                op: DragOp::Move,
                object_id: id,
                orig_frame,
                start_screen: start_pos,
                frames,
            });
        } else {
            editor.ui.canvas_drag = Some(CanvasDrag {
                op: DragOp::Marquee,
                object_id: String::new(),
                orig_frame: Rect::ZERO,
                start_screen: start_pos,
                frames: Vec::new(),
            });
        }
    } else {
        // Dragging the canvas backdrop also draws a marquee.
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::Marquee,
            object_id: String::new(),
            orig_frame: Rect::ZERO,
            start_screen: start_pos,
            frames: Vec::new(),
        });
    }
}

#[allow(clippy::cognitive_complexity)]
fn process_active_drag(editor: &mut Editor, items: &mut [DrawItem], t: &SlideTransform, ui: &Ui) {
    let Some(drag) = editor.ui.canvas_drag.clone() else {
        return;
    };
    ui.ctx().request_repaint();

    let current_pointer = ui.input(|i| i.pointer.interact_pos());
    let shift_held = ui.input(|i| i.modifiers.shift);
    let alt_held = ui.input(|i| i.modifiers.alt);
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());

    let curr_screen = current_pointer.unwrap_or(drag.start_screen);

    if let DragOp::PathPoint {
        index,
        ref original,
        two_point,
    } = drag.op
    {
        let frame = drag.orig_frame;
        if !finite_rect(frame) || frame.width() <= 0.0 || frame.height() <= 0.0 {
            editor.ui.canvas_drag = None;
            return;
        }
        let connector_glues = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|slide| slide.object(&drag.object_id))
            .and_then(|(object, _)| match &object.kind {
                ObjectKind::Path { path } if path.connector.is_some() && path.points.len() == 2 => {
                    Some((path.start_glue.clone(), path.end_glue.clone()))
                }
                _ => None,
            });
        if let Some(slide_pos) = current_pointer.map(|pos| t.to_slide_pos(pos))
            && finite_pos(slide_pos)
        {
            let mut target = slide_pos;
            if two_point && shift_held {
                let other_index = if index == 0 { 1 } else { 0 };
                if let Some(other) = original.get(other_index) {
                    let other = pos2(
                        frame.min.x + other.x * frame.width(),
                        frame.min.y + other.y * frame.height(),
                    );
                    target = snap_45(other, target);
                }
            }
            let mut dragged_glue = None;
            if connector_glues.is_some()
                && let Some(snap) = current_pointer.and_then(|pointer| {
                    connector_snap(editor, t, pointer, Some(drag.object_id.as_str()), alt_held)
                })
            {
                target = snap.point;
                dragged_glue = Some(snap.glue);
            }
            if primary_released || !primary_down {
                editor.ui.canvas_drag = None;
                let points: Vec<Point> = original
                    .iter()
                    .enumerate()
                    .map(|(point_index, point)| {
                        if point_index == index {
                            Point {
                                x: target.x,
                                y: target.y,
                            }
                        } else {
                            Point {
                                x: frame.min.x + point.x * frame.width(),
                                y: frame.min.y + point.y * frame.height(),
                            }
                        }
                    })
                    .collect();
                if points
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite())
                {
                    let params = if let Some((start_glue, end_glue)) = connector_glues {
                        let (start_glue, end_glue) = if index == 0 {
                            (dragged_glue, end_glue)
                        } else if index + 1 == original.len() {
                            (start_glue, dragged_glue)
                        } else {
                            (start_glue, end_glue)
                        };
                        serde_json::json!({
                            "slide": editor.selection.slide,
                            "object": drag.object_id,
                            "points": points,
                            "start_glue": start_glue,
                            "end_glue": end_glue,
                        })
                    } else {
                        serde_json::json!({
                            "slide": editor.selection.slide,
                            "object": drag.object_id,
                            "points": points,
                        })
                    };
                    editor.run("object.set_path", params);
                }
            } else {
                let normalized = coursecraft_course::PathPoint {
                    x: (target.x - frame.min.x) / frame.width(),
                    y: (target.y - frame.min.y) / frame.height(),
                };
                for item in items.iter_mut() {
                    if item.hit == Hit::Object(drag.object_id.clone())
                        && let Visual::Path(path) = &mut item.visual
                        && let Some(slot) = path.points.get_mut(index)
                    {
                        *slot = normalized;
                    }
                }
            }
        }
        return;
    }

    if let DragOp::Adjust {
        index,
        ref original,
    } = drag.op
    {
        let shape = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|slide| slide.object(&drag.object_id))
            .and_then(|(obj, _)| match &obj.kind {
                coursecraft_course::ObjectKind::Shape { shape, .. } => {
                    Some((*shape, obj.frame.w, obj.frame.h))
                }
                _ => None,
            });
        let Some((kind, w, h)) = shape else {
            editor.ui.canvas_drag = None;
            return;
        };
        let slide_point = t.to_slide_pos(curr_screen);
        let point = (
            slide_point.x - drag.orig_frame.min.x,
            slide_point.y - drag.orig_frame.min.y,
        );
        let preview = coursecraft_render::adjust_from_point(kind, w, h, original, index, point);

        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if preview != *original {
                let kind_json = editor
                    .session
                    .course
                    .slide(&editor.selection.slide)
                    .and_then(|slide| slide.object(&drag.object_id))
                    .and_then(|(obj, _)| serde_json::to_value(&obj.kind).ok())
                    .map(|mut kind| {
                        kind["adjust"] = serde_json::json!(preview);
                        kind
                    });
                if let Some(kind) = kind_json {
                    editor.run(
                        "object.update",
                        serde_json::json!({
                            "slide": editor.selection.slide,
                            "object": drag.object_id,
                            "kind": kind,
                        }),
                    );
                }
            }
        } else {
            for item in items.iter_mut() {
                if item.hit == Hit::Object(drag.object_id.clone())
                    && let Visual::Shape { adjust, .. } = &mut item.visual
                {
                    *adjust = preview.clone();
                }
            }
        }
        return;
    }

    if let DragOp::MotionPathPoint {
        ref path_id,
        point_index,
        ref orig_points,
        relative,
        obj_origin,
    } = drag.op
    {
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            let current_slide_pos = t.to_slide_pos(curr_screen);
            let mut new_points = orig_points.clone();
            let new_pt = if relative {
                coursecraft_course::Point {
                    x: (current_slide_pos.x - obj_origin.x).round(),
                    y: (current_slide_pos.y - obj_origin.y).round(),
                }
            } else {
                coursecraft_course::Point {
                    x: current_slide_pos.x.round(),
                    y: current_slide_pos.y.round(),
                }
            };
            if point_index < new_points.len() {
                new_points[point_index] = new_pt;
                editor.run(
                    "object.update_motion_path",
                    serde_json::json!({
                        "slide": editor.selection.slide,
                        "object": drag.object_id,
                        "path": path_id,
                        "points": new_points,
                    }),
                );
            }
        }
        return;
    }

    if let DragOp::Rotate {
        centre,
        start_angle,
        orig_rotation,
    } = drag.op
    {
        let current_angle = (curr_screen.y - centre.y).atan2(curr_screen.x - centre.x);
        let mut degrees = orig_rotation + (current_angle - start_angle).to_degrees();
        if shift_held {
            degrees = (degrees / 15.0).round() * 15.0;
        }
        let rotation = coursecraft_course::normalize_rotation(degrees);
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            let orig = coursecraft_course::normalize_rotation(orig_rotation);
            if (rotation - orig).abs() > 0.001 {
                editor.run(
                    "object.set_rotation",
                    serde_json::json!({
                        "slide": editor.selection.slide,
                        "object": drag.object_id,
                        "degrees": rotation,
                    }),
                );
            }
        } else {
            for item in items.iter_mut() {
                if item.hit == Hit::Object(drag.object_id.clone()) {
                    item.rotation = rotation;
                }
            }
        }
        return;
    }

    if let DragOp::TableBorder {
        column,
        before,
        ref original,
    } = drag.op
    {
        let frame = drag.orig_frame;
        if !frame.is_positive() || before == 0 || before >= original.len() {
            editor.ui.canvas_drag = None;
            return;
        }
        let start = t.to_slide_pos(drag.start_screen);
        let current = t.to_slide_pos(curr_screen);
        if !start.x.is_finite()
            || !start.y.is_finite()
            || !current.x.is_finite()
            || !current.y.is_finite()
        {
            editor.ui.canvas_drag = None;
            return;
        }
        let moved = if column {
            (current.x - start.x) / frame.width()
        } else {
            (current.y - start.y) / frame.height()
        };
        let mut values = original.clone();
        let left_index = before.saturating_sub(1);
        let (Some(left), Some(right)) =
            (values.get(left_index).copied(), values.get(before).copied())
        else {
            editor.ui.canvas_drag = None;
            return;
        };
        let pair = left + right;
        if pair > coursecraft_course::TABLE_MIN_FRACTION * 2.0 && moved.is_finite() {
            let next_left = (left + moved).clamp(
                coursecraft_course::TABLE_MIN_FRACTION,
                pair - coursecraft_course::TABLE_MIN_FRACTION,
            );
            if let Some(slot) = values.get_mut(left_index) {
                *slot = next_left;
            }
            if let Some(slot) = values.get_mut(before) {
                *slot = pair - next_left;
            }
        }
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            let params = if column {
                serde_json::json!({
                    "slide": editor.selection.slide,
                    "object": drag.object_id,
                    "columns": values,
                })
            } else {
                serde_json::json!({
                    "slide": editor.selection.slide,
                    "object": drag.object_id,
                    "rows": values,
                })
            };
            editor.run("table.resize", params);
        } else {
            for item in items.iter_mut() {
                if item.hit != Hit::Object(drag.object_id.clone()) {
                    continue;
                }
                if let Visual::Table(table) = &mut item.visual {
                    if column {
                        table.columns = values.clone();
                    } else {
                        for (row, height) in table.rows.iter_mut().zip(values.iter().copied()) {
                            row.height = height;
                        }
                    }
                }
            }
        }
        return;
    }

    let delta = curr_screen - drag.start_screen;
    let is_move = delta.length() >= 3.0;

    if matches!(drag.op, DragOp::Marquee) {
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if is_move {
                let marquee = Rect::from_two_pos(drag.start_screen, curr_screen);
                let slide_rect = screen_rect_to_slide(t, marquee);
                let mut ids = if shift_held {
                    editor.selection.ids()
                } else {
                    Vec::new()
                };
                let slide = editor.session.course.slide(&editor.selection.slide);
                if let Some(slide) = slide {
                    for item in items.iter() {
                        let Hit::Object(ref id) = item.hit else {
                            continue;
                        };
                        let on_layer = match editor.selection.layer.as_deref() {
                            Some(lid) => slide
                                .layer(lid)
                                .is_some_and(|l| l.objects.iter().any(|o| &o.id == id)),
                            None => slide.objects.iter().any(|o| &o.id == id),
                        };
                        if on_layer
                            && !editor.ui.locked_objects.contains(id)
                            && coursecraft_render::item_bounds(item).intersects(slide_rect)
                            && !ids.contains(id)
                        {
                            ids.push(id.clone());
                        }
                    }
                }
                editor.selection.set_many(ids);
                editor.expand_selection_groups();
                editor.ui.group_focus = None;
            }
        }
        return;
    }

    if matches!(drag.op, DragOp::RegionMove | DragOp::RegionResize(_)) {
        let scale = t.scale();
        let preview_rect = if is_move {
            compute_drag_frame(&drag, curr_screen, shift_held, scale)
        } else {
            drag.orig_frame
        };
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;

            if is_move {
                let fx = preview_rect.min.x.round();
                let fy = preview_rect.min.y.round();
                let fw = preview_rect.width().round().max(4.0);
                let fh = preview_rect.height().round().max(4.0);

                let orig_f = drag.orig_frame;
                if fx != orig_f.min.x
                    || fy != orig_f.min.y
                    || fw != orig_f.width()
                    || fh != orig_f.height()
                {
                    editor.run(
                        "question.update_region",
                        serde_json::json!({
                            "slide": editor.selection.slide,
                            "id": drag.object_id,
                            "x": fx,
                            "y": fy,
                            "w": fw,
                            "h": fh,
                        }),
                    );
                }
            }
        }
        return;
    }

    let scale = t.scale();
    let is_resize = matches!(drag.op, DragOp::Resize { .. });
    let mut resize_bbox: Option<Rect> = None;
    let mut preview_frames: Vec<(String, Rect)> = if !is_move {
        drag.frames.clone()
    } else if is_resize {
        let new_bbox = compute_drag_frame(&drag, curr_screen, shift_held, scale);
        resize_bbox = Some(new_bbox);
        scale_frames(drag.orig_frame, new_bbox, &drag.frames)
    } else {
        let preview = compute_drag_frame(&drag, curr_screen, shift_held, scale);
        let dx = preview.min.x - drag.orig_frame.min.x;
        let dy = preview.min.y - drag.orig_frame.min.y;
        drag.frames
            .iter()
            .map(|(id, frame)| {
                (
                    id.clone(),
                    Rect::from_min_size(pos2(frame.min.x + dx, frame.min.y + dy), frame.size()),
                )
            })
            .collect()
    };

    let command_held = ui.input(|i| i.modifiers.command);
    let lines = crate::rulers::snap_lines(&editor.ui.rulers);
    if crate::snap::snapping_active(editor.ui.snap_to_guides || !lines.is_empty(), command_held)
        && scale > 0.0
    {
        let threshold = 6.0 / scale;
        let targets = snap_targets(editor, &drag.frames);
        if is_move {
            if let Some(bbox) = frames_rotated_bbox(editor, &preview_frames) {
                let result = crate::snap::snap_rect_lines(
                    bbox,
                    &targets,
                    slide_space_rect(editor),
                    threshold,
                    editor.ui.snap_to_guides,
                    &lines,
                );
                if result.offset != vec2(0.0, 0.0) {
                    for (_, frame) in &mut preview_frames {
                        *frame = Rect::from_min_max(
                            frame.min + result.offset,
                            frame.max + result.offset,
                        );
                    }
                }
                editor.ui.snap_guides = result.guides;
            }
        } else if is_resize && drag.frames.len() == 1 && !shift_held {
            // Snapping moves edges on their own, which would break Shift's proportions.
            if let (Some(raw), DragOp::Resize { handle, rotation }) = (resize_bbox, drag.op) {
                if rotation != 0.0 && rotation.is_finite() {
                    // A rotated object's resize is anchored in its own axes;
                    // snapping its axis-aligned raw frame would move that anchor.
                    editor.ui.snap_guides.clear();
                } else {
                    let result = crate::snap::snap_rect_lines(
                        raw,
                        &targets,
                        slide_space_rect(editor),
                        threshold,
                        editor.ui.snap_to_guides,
                        &lines,
                    );
                    let moves_left = matches!(
                        handle,
                        ResizeHandle::Left | ResizeHandle::TopLeft | ResizeHandle::BottomLeft
                    );
                    let moves_right = matches!(
                        handle,
                        ResizeHandle::Right | ResizeHandle::TopRight | ResizeHandle::BottomRight
                    );
                    let moves_top = matches!(
                        handle,
                        ResizeHandle::Top | ResizeHandle::TopLeft | ResizeHandle::TopRight
                    );
                    let moves_bottom = matches!(
                        handle,
                        ResizeHandle::Bottom | ResizeHandle::BottomLeft | ResizeHandle::BottomRight
                    );
                    let affects_x = moves_left || moves_right;
                    let affects_y = moves_top || moves_bottom;
                    let mut min = raw.min;
                    let mut max = raw.max;
                    if moves_left {
                        min.x += result.offset.x;
                    }
                    if moves_right {
                        max.x += result.offset.x;
                    }
                    if moves_top {
                        min.y += result.offset.y;
                    }
                    if moves_bottom {
                        max.y += result.offset.y;
                    }
                    let snapped = Rect::from_min_max(min, max);
                    if snapped.width() >= 4.0
                        && snapped.height() >= 4.0
                        && snapped.min.is_finite()
                        && snapped.max.is_finite()
                    {
                        preview_frames = scale_frames(drag.orig_frame, snapped, &drag.frames);
                        editor.ui.snap_guides = result
                            .guides
                            .into_iter()
                            .filter(|guide| if guide.vertical { affects_x } else { affects_y })
                            .collect();
                    } else {
                        editor.ui.snap_guides.clear();
                    }
                }
            }
        } else {
            editor.ui.snap_guides.clear();
        }
    } else {
        editor.ui.snap_guides.clear();
    }

    if primary_released || !primary_down {
        editor.ui.canvas_drag = None;
        editor.ui.snap_guides.clear();

        if is_move {
            let slide = editor.selection.slide.clone();
            let mut steps: Vec<(String, Json)> = Vec::new();
            for ((id, orig), (_, preview)) in drag.frames.iter().zip(&preview_frames) {
                let fx = preview.min.x.round();
                let fy = preview.min.y.round();
                let fw = preview.width().round().max(4.0);
                let fh = preview.height().round().max(4.0);
                if !fx.is_finite() || !fy.is_finite() || !fw.is_finite() || !fh.is_finite() {
                    continue;
                }
                if fx != orig.min.x || fy != orig.min.y || fw != orig.width() || fh != orig.height()
                {
                    steps.push((
                        "object.set_frame".to_owned(),
                        serde_json::json!({
                            "slide": slide,
                            "object": id,
                            "frame": { "x": fx, "y": fy, "w": fw, "h": fh },
                        }),
                    ));
                }
            }
            if !steps.is_empty() {
                editor.run_batch(&steps);
            }
        }
    } else if is_move {
        for (id, preview) in &preview_frames {
            for it in items.iter_mut() {
                if it.hit == Hit::Object(id.clone()) {
                    it.rect = *preview;
                }
            }
        }
    }
}

fn handle_canvas_secondary_click(
    editor: &mut Editor,
    pos: Pos2,
    t: &SlideTransform,
    items: &[DrawItem],
) {
    editor.ui.last_active_area = crate::ActiveArea::Canvas;
    let inside = t.slide_rect().contains(pos);
    let slide_pos = t.to_slide_pos(pos);
    editor.ui.canvas_context_pos =
        (slide_pos.x.is_finite() && slide_pos.y.is_finite()).then_some(slide_pos);
    let picked = if inside && editor.ui.canvas_context_pos.is_some() {
        pick_object(
            &editor.session.course,
            &editor.selection,
            &editor.ui.locked_objects,
            slide_pos,
            items,
        )
        .map(|(id, _)| id)
    } else {
        None
    };
    if let Some(id) = picked {
        if !editor.selection.contains(&id) {
            editor.select_object_group(&id);
        }
    } else {
        editor.selection.clear_objects();
        editor.ui.group_focus = None;
    }
}

fn show_canvas_context_menu(editor: &mut Editor, ui: &mut Ui, special_mode: bool) {
    if special_mode {
        crate::menu::show_canvas_special_menu(editor, ui);
    } else if editor.selection.object.is_some() {
        crate::menu::show_object_row_menu(editor, ui);
    } else {
        crate::menu::show_canvas_background_menu(editor, ui);
    }
}

fn handle_canvas_click(
    editor: &mut Editor,
    pos: Pos2,
    t: &SlideTransform,
    items: &[DrawItem],
    modifiers: egui::Modifiers,
) {
    editor.ui.last_active_area = crate::ActiveArea::Canvas;
    let additive = modifiers.shift || modifiers.command;
    if t.slide_rect().contains(pos) {
        let slide_pos = t.to_slide_pos(pos);
        if let Some(slide) = editor.session.course.slide(&editor.selection.slide)
            && let Some(ref q) = slide.question
            && q.kind == QuestionKind::Hotspot
        {
            for region in q.regions.iter().rev() {
                let r_rect =
                    Rect::from_min_size(pos2(region.x, region.y), vec2(region.w, region.h));
                if r_rect.contains(slide_pos) {
                    editor.ui.question.selected_region = Some(region.id.clone());
                    editor.selection.clear_objects();
                    return;
                }
            }
        }
        let picked = pick_object(
            &editor.session.course,
            &editor.selection,
            &editor.ui.locked_objects,
            slide_pos,
            items,
        )
        .map(|(id, _)| id);
        if editor.ui.format_painter.is_some() {
            if let Some(id) = picked {
                let targets = editor.selection.group_members(&editor.session.course, &id);
                editor.apply_format_painter(&targets);
            }
            return;
        }
        editor.ui.table_cell = picked.as_deref().and_then(|id| {
            editor
                .session
                .course
                .slide(&editor.selection.slide)
                .and_then(|slide| slide.object(id))
                .and_then(|(object, _)| table_cell_at_point(object, slide_pos))
        });
        if let Some(edit) = editor.ui.path_edit.as_ref()
            && picked.as_deref() != Some(edit.object.as_str())
        {
            editor.ui.path_edit = None;
        }
        match picked {
            Some(id) if additive => editor.toggle_object_group(&id),
            Some(id) if editor.selection.contains(&id) => {
                if editor.ui.group_focus.as_deref() != Some(id.as_str()) {
                    editor.ui.group_focus = None;
                }
            }
            Some(id) => editor.select_object_group(&id),
            None if !additive => {
                editor.selection.clear_objects();
                editor.ui.group_focus = None;
            }
            None => {}
        }
    } else if !additive {
        editor.selection.clear_objects();
        editor.ui.group_focus = None;
        editor.ui.path_edit = None;
    }
}

fn handle_canvas_double_click(
    editor: &mut Editor,
    pos: Pos2,
    t: &SlideTransform,
    items: &[DrawItem],
    modifiers: egui::Modifiers,
) {
    if t.slide_rect().contains(pos) {
        let slide_pos = t.to_slide_pos(pos);
        let picked = pick_object(
            &editor.session.course,
            &editor.selection,
            &editor.ui.locked_objects,
            slide_pos,
            items,
        )
        .map(|(id, _)| id);
        if let Some(id) = picked {
            let group_size = editor
                .selection
                .group_members(&editor.session.course, &id)
                .len();
            let in_group = group_size > 1;
            let in_selection = editor.selection.contains(&id);
            let focused = editor.ui.group_focus.as_deref() == Some(id.as_str());
            if in_group && in_selection && !focused {
                editor.selection.set_many([id.clone()]);
                editor.ui.group_focus = Some(id);
                return;
            }
            let editing_path = editor
                .ui
                .path_edit
                .as_ref()
                .is_some_and(|edit| edit.object == id);
            if editing_path {
                if path_edit_double_click(editor, &id, slide_pos, t) {
                    return;
                }
                editor.ui.path_edit = None;
            }
            let kind = editor
                .session
                .course
                .slide(&editor.selection.slide)
                .and_then(|slide| slide.object(&id))
                .map(|(object, _)| object.kind.clone());
            if matches!(kind, Some(ObjectKind::Path { .. })) {
                start_path_edit(editor, &id);
                return;
            }
            match kind {
                Some(
                    coursecraft_course::ObjectKind::Text { .. }
                    | coursecraft_course::ObjectKind::Button { .. },
                ) => {
                    start_inline_edit(editor, &id);
                    return;
                }
                Some(coursecraft_course::ObjectKind::Shape { shape, .. })
                    if coursecraft_course::shape_allows_text(shape) =>
                {
                    start_inline_edit(editor, &id);
                    return;
                }
                Some(coursecraft_course::ObjectKind::Table { .. }) => {
                    let cell = editor
                        .session
                        .course
                        .slide(&editor.selection.slide)
                        .and_then(|slide| slide.object(&id))
                        .and_then(|(object, _)| table_cell_at_point(object, slide_pos));
                    if let Some((row, column)) = cell {
                        start_table_cell_edit(editor, &id, row, column);
                    }
                    return;
                }
                Some(coursecraft_course::ObjectKind::Image { .. }) if editor.start_crop(&id) => {
                    // Crop mode opened; a placeholder (missing file) falls
                    // through and stays a plain selection.
                    return;
                }
                _ => {}
            }
        }
    }
    handle_canvas_click(editor, pos, t, items, modifiers);
}

#[derive(Debug, Clone, PartialEq)]
enum CropDragOp {
    /// Dragging the whole source under the kept frame.
    Source,
    /// Dragging one of the eight handles of the kept window.
    Handle(crate::crop::CropHandle),
}

/// A pointer drag inside crop mode, in slide pixels from the press position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CropDrag {
    start_slide: Pos2,
    op: CropDragOp,
    start_edit: crate::crop::CropEdit,
}

/// Hit radius, in screen pixels, for the crop handles.
const CROP_HANDLE_HIT_PX: f32 = 7.0;
const CROP_HANDLE_SIZE_PX: f32 = 8.0;

/// The kept window's screen rect for the crop session, if it still applies.
fn crop_screen_rect(editor: &Editor, t: &SlideTransform) -> Option<(crate::CropSession, Rect)> {
    let session = editor.ui.crop_edit.clone()?;
    if session.slide != editor.selection.slide {
        return None;
    }
    let kept = t.to_screen_rect(session.edit.frame);
    Some((session, kept))
}

/// Apply a crop drag from the press position to the current pointer.
fn update_crop_drag(
    editor: &mut Editor,
    drag: &CropDrag,
    t: &SlideTransform,
    pointer: Pos2,
    shift: bool,
) {
    let Some(session) = editor.ui.crop_edit.as_mut() else {
        return;
    };
    let delta = t.to_slide_pos(pointer) - drag.start_slide;
    if !delta.x.is_finite() || !delta.y.is_finite() {
        return;
    }
    session.edit = match drag.op {
        CropDragOp::Source => drag.start_edit.drag_source(delta),
        CropDragOp::Handle(handle) => {
            drag.start_edit
                .drag_handle(handle, delta, shift && handle.is_corner())
        }
    };
    editor.ui.snap_guides.clear();
}

/// Press handling in crop mode: start a handle or source drag, or leave on a
/// click outside the picture. Returns true when the press was used.
fn crop_press(editor: &mut Editor, pos: Pos2, t: &SlideTransform) -> bool {
    let Some((session, kept)) = crop_screen_rect(editor, t) else {
        return false;
    };
    if let Some(handle) = crate::crop::CropHandle::at(kept, pos, CROP_HANDLE_HIT_PX) {
        editor.ui.canvas_drag = None;
        let drag = CropDrag {
            start_slide: t.to_slide_pos(pos),
            op: CropDragOp::Handle(handle),
            start_edit: session.edit,
        };
        editor.ui.crop_drag = Some(drag);
        return true;
    }
    if kept.contains(pos) {
        editor.ui.canvas_drag = None;
        editor.ui.crop_drag = Some(CropDrag {
            start_slide: t.to_slide_pos(pos),
            op: CropDragOp::Source,
            start_edit: session.edit,
        });
        return true;
    }
    // Clicking outside the kept area applies, like clicking away from the
    // in-place text editor.
    editor.ui.crop_drag = None;
    editor.apply_crop();
    true
}

/// Keyboard and pointer input while cropping.
///
/// Enter applies, Escape (handled by the caller) cancels, a press starts a
/// handle or source drag, and the drag follows the pointer until release.
fn crop_input(
    editor: &mut Editor,
    ui: &Ui,
    t: &SlideTransform,
    pointer: Option<Pos2>,
    canvas_rect: Rect,
    pointer_in_hud: bool,
) {
    let (enter, released, down, pressed, shift) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Enter) && !i.modifiers.command && !i.modifiers.ctrl,
            i.pointer.primary_released(),
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.modifiers.shift,
        )
    });
    if enter {
        editor.apply_crop();
        return;
    }
    if released {
        editor.ui.crop_drag = None;
        return;
    }
    if pressed
        && editor.ui.crop_drag.is_none()
        && let Some(pos) = pointer
        // A press in the inspector or another panel is not a canvas press; only
        // presses on the canvas, away from the zoom HUD, can start a drag or
        // click away to apply.
        && canvas_rect.contains(pos)
        && !pointer_in_hud
    {
        crop_press(editor, pos, t);
        return;
    }
    if down && let (Some(drag), Some(pos)) = (editor.ui.crop_drag.clone(), pointer) {
        update_crop_drag(editor, &drag, t, pos, shift);
    }
}

/// Paint the whole source dimmed under an accent outline, with the eight
/// handles of the kept window.
fn draw_crop_overlay(editor: &Editor, painter: &Painter, t: &SlideTransform, ui: &Ui) {
    let Some((session, kept)) = crop_screen_rect(editor, t) else {
        return;
    };
    let p = coursecraft_render::palette(ui.ctx());
    let full = session.edit.source_overlay();
    let whole = t.to_screen_rect(full);

    // The part outside the kept frame is what Apply will hide, so show it
    // dimmed behind the full-strength kept window.
    let src = editor
        .session
        .course
        .slide(&session.slide)
        .and_then(|slide| slide.object(&session.object))
        .and_then(|(object, _)| match &object.kind {
            coursecraft_course::ObjectKind::Image { src, .. } => Some(src.clone()),
            _ => None,
        });
    let texture = src
        .as_deref()
        .and_then(|src| editor.media_cache.texture_id(src));
    let full_uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let kept_uv = Rect::from_min_size(
        pos2(session.edit.crop.x, session.edit.crop.y),
        vec2(session.edit.crop.w, session.edit.crop.h),
    );
    if let Some(texture) = texture {
        painter.image(texture, whole, full_uv, Color32::WHITE);
        painter.rect_filled(
            whole,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(0, 0, 0, 150),
        );
        painter.image(texture, kept, kept_uv, Color32::WHITE);
    } else {
        painter.rect_filled(
            whole,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(0, 0, 0, 150),
        );
    }

    draw_dashed_rect(painter, whole, Stroke::new(1.0, p.text_tertiary));
    painter.rect_stroke(
        kept,
        CornerRadius::ZERO,
        Stroke::new(2.0, p.accent),
        StrokeKind::Outside,
    );

    let pointer = ui.input(|i| i.pointer.interact_pos());
    if editor.ui.crop_drag.is_none()
        && let Some(pos) = pointer
        && let Some(handle) = crate::crop::CropHandle::at(kept, pos, CROP_HANDLE_HIT_PX)
    {
        let cursor = match (
            handle.moves_left() || handle.moves_right(),
            handle.is_corner(),
        ) {
            (true, true) if handle.moves_left() == handle.moves_top() => CursorIcon::ResizeNwSe,
            (true, true) => CursorIcon::ResizeNeSw,
            (false, _) => CursorIcon::ResizeVertical,
            (true, false) => CursorIcon::ResizeHorizontal,
        };
        ui.ctx().set_cursor_icon(cursor);
    }

    for (_, center) in crate::crop::CropHandle::positions(kept) {
        let handle_rect =
            Rect::from_center_size(center, vec2(CROP_HANDLE_SIZE_PX, CROP_HANDLE_SIZE_PX));
        painter.rect_filled(handle_rect, CornerRadius::ZERO, Color32::WHITE);
        painter.rect_stroke(
            handle_rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, p.accent),
            StrokeKind::Middle,
        );
    }
}

fn align_for(align: coursecraft_course::Align) -> egui::Align {
    match align {
        coursecraft_course::Align::Left => egui::Align::LEFT,
        coursecraft_course::Align::Center => egui::Align::Center,
        coursecraft_course::Align::Right => egui::Align::RIGHT,
    }
}

fn valign_for(valign: coursecraft_course::VAlign) -> egui::Align {
    match valign {
        coursecraft_course::VAlign::Top => egui::Align::TOP,
        coursecraft_course::VAlign::Middle => egui::Align::Center,
        coursecraft_course::VAlign::Bottom => egui::Align::BOTTOM,
    }
}

/// Frame for the in-place editor: no fill, so the object shows through, with a
/// thin accent outline around the editable area.
pub(crate) fn inline_edit_frame(accent: Color32) -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, accent))
        .corner_radius(CornerRadius::same(coursecraft_render::RADIUS_SM as u8))
}

/// Text-editing visuals for the in-place editor. The caret and selected text use
/// the object's own colour so they stay visible on light and dark slides.
pub(crate) fn inline_edit_visuals(
    mut visuals: egui::Visuals,
    text_color: Color32,
    accent: Color32,
) -> egui::Visuals {
    visuals.text_cursor.stroke = Stroke::new(2.0, text_color);
    visuals.selection = egui::style::Selection {
        bg_fill: Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 140),
        stroke: Stroke::new(1.0, text_color),
    };
    visuals
}

/// Replace missing images with a neutral authoring placeholder, so a course that
/// has not chosen its image yet reads as "no image yet" rather than an error.
fn missing_images_as_placeholders(
    items: &mut [DrawItem],
    media: &[(String, std::sync::Arc<[u8]>)],
) {
    for item in items {
        if let Visual::Image { src, .. } = &item.visual
            && !media.iter().any(|(name, _)| name == src)
        {
            item.visual = Visual::ImagePlaceholder {
                label: "No image yet".to_owned(),
            };
        }
    }
}

/// Blank the object's own text while it is edited in place, so the old text
/// doesn't show through the transparent editor.
fn hide_edited_text(items: &mut [DrawItem], edit: &InlineEdit) {
    for item in items {
        if !matches!(&item.hit, Hit::Object(id) if id == &edit.object) {
            continue;
        }
        match (&mut item.visual, edit.cell) {
            (Visual::Text { runs, .. }, None) => runs.clear(),
            (Visual::Button { label, .. }, None) => label.clear(),
            (
                Visual::Shape {
                    text: Some(text), ..
                },
                None,
            ) => text.runs.clear(),
            (Visual::Table(table), Some((row, column))) => {
                if let Some(cell) = table
                    .rows
                    .get_mut(row)
                    .and_then(|line| line.cells.get_mut(column))
                {
                    cell.text.runs.clear();
                }
            }
            _ => {}
        }
    }
}

/// Draw the in-place editor over the object's frame and commit or cancel it.
fn draw_inline_edit(editor: &mut Editor, ui: &mut Ui, t: &SlideTransform) {
    let Some(edit) = editor.ui.inline_edit.clone() else {
        return;
    };
    if edit.slide != editor.selection.slide {
        cancel_inline_edit(editor);
        return;
    }
    let Some(slide) = editor.session.course.slide(&edit.slide) else {
        cancel_inline_edit(editor);
        return;
    };
    let Some((object, _)) = slide.object(&edit.object) else {
        cancel_inline_edit(editor);
        return;
    };

    let slide_rect = match (&object.kind, edit.cell) {
        (coursecraft_course::ObjectKind::Table { .. }, Some((row, column))) => {
            match table_cell_slide_rect(object, row, column) {
                Some(rect) => rect.shrink(6.0),
                None => {
                    cancel_inline_edit(editor);
                    return;
                }
            }
        }
        (coursecraft_course::ObjectKind::Shape { shape, adjust, .. }, None)
            if coursecraft_course::shape_allows_text(*shape) =>
        {
            let (x, y, w, h) =
                coursecraft_render::shape_text_rect(*shape, object.frame.w, object.frame.h, adjust);
            Rect::from_min_size(pos2(object.frame.x + x, object.frame.y + y), vec2(w, h))
        }
        _ => Rect::from_min_size(
            pos2(object.frame.x, object.frame.y),
            vec2(object.frame.w, object.frame.h),
        ),
    };
    let rect = t.to_screen_rect(slide_rect);
    if !rect.width().is_finite()
        || !rect.height().is_finite()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
    {
        cancel_inline_edit(editor);
        return;
    }

    let scale = t.scale();
    let (font_size, text_color, align, valign) = match &object.kind {
        coursecraft_course::ObjectKind::Text { style, .. } => (
            style.size * scale,
            coursecraft_render::parse_color(&style.color)
                .unwrap_or_else(|| coursecraft_render::palette(ui.ctx()).text),
            align_for(style.align),
            valign_for(style.valign),
        ),
        coursecraft_course::ObjectKind::Button { .. } => (
            20.0 * scale,
            Color32::WHITE,
            egui::Align::Center,
            egui::Align::Center,
        ),
        coursecraft_course::ObjectKind::Shape {
            shape, fill, text, ..
        } => {
            if !coursecraft_course::shape_allows_text(*shape) {
                cancel_inline_edit(editor);
                return;
            }
            let style = text
                .as_ref()
                .map(|text| text.style.clone())
                .unwrap_or_else(|| default_shape_text_style(fill));
            (
                style.size * scale,
                coursecraft_render::parse_color(&style.color)
                    .unwrap_or_else(|| coursecraft_render::palette(ui.ctx()).text),
                align_for(style.align),
                valign_for(style.valign),
            )
        }
        coursecraft_course::ObjectKind::Table { rows, .. } => {
            let Some((row, column)) = edit.cell else {
                cancel_inline_edit(editor);
                return;
            };
            let Some(cell) = rows.get(row).and_then(|line| line.cells.get(column)) else {
                cancel_inline_edit(editor);
                return;
            };
            (
                cell.style.size * scale,
                coursecraft_render::parse_color(&cell.style.color)
                    .unwrap_or_else(|| coursecraft_render::palette(ui.ctx()).text),
                align_for(cell.style.align),
                valign_for(cell.valign),
            )
        }
        _ => {
            cancel_inline_edit(editor);
            return;
        }
    };
    let font_size = font_size.clamp(4.0, 400.0);
    if edit.cell.is_some() {
        let forward =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        let backward =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if forward || backward {
            let next = edit
                .cell
                .and_then(|(row, column)| table_next_cell(object, row, column, forward));
            if let Some(state) = editor.ui.inline_edit.as_mut() {
                state.buffer = edit.buffer.clone();
            }
            commit_inline_edit(editor);
            if let Some((row, column)) = next {
                start_table_cell_edit(editor, &edit.object, row, column);
            }
            return;
        }
    }
    let edit_id = Id::unique(("canvas_inline_edit", &edit.slide, &edit.object));
    let accent = coursecraft_render::palette(ui.ctx()).accent;

    let mut buffer = edit.buffer.clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let visuals = inline_edit_visuals(child.visuals().clone(), text_color, accent);
    *child.visuals_mut() = visuals;
    let edit_builder = if edit.is_button {
        egui::TextEdit::singleline(&mut buffer)
    } else {
        egui::TextEdit::multiline(&mut buffer).desired_rows(1)
    };
    let mut output = edit_builder
        .id(edit_id)
        .font(FontId::proportional(font_size))
        .text_color(text_color)
        .align(egui::Align2::new(align, valign))
        .desired_width(rect.width())
        .frame(inline_edit_frame(accent))
        .show(&mut child);

    let cmd_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    let button_enter =
        edit.is_button && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let clicked_outside = ui.input(|i| {
        i.pointer.primary_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !rect.contains(pos))
    });

    // Keep keyboard focus on the in-place editor while it is open. The canvas
    // interaction and the click that placed the object can otherwise take it.
    let pointer_active = ui.input(|i| {
        i.pointer.primary_down() || i.pointer.primary_pressed() || i.pointer.primary_released()
    });
    if edit.just_entered || (!output.response.has_focus() && !clicked_outside && !escaped) {
        output.response.request_focus();
    }
    if edit.just_entered {
        // Re-apply select-all through the release click so the placeholder stays
        // selected; clear it once the pointer is idle.
        let chars = buffer.chars().count();
        output
            .state
            .cursor
            .set_char_range(Some(egui::text_selection::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(chars),
            )));
        output.state.store(ui.ctx(), output.response.id);
        if !pointer_active
            && output.response.has_focus()
            && let Some(state) = editor.ui.inline_edit.as_mut()
        {
            state.just_entered = false;
        }
    }

    if let Some(state) = editor.ui.inline_edit.as_mut() {
        state.buffer = buffer;
    }
    if escaped {
        cancel_inline_edit(editor);
    } else if cmd_enter || button_enter || clicked_outside {
        commit_inline_edit(editor);
    }
}

fn draw_marquee(editor: &Editor, painter: &Painter, ui: &Ui) {
    let Some(drag) = editor.ui.canvas_drag.as_ref() else {
        return;
    };
    if !matches!(drag.op, DragOp::Marquee) {
        return;
    }
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let rect = Rect::from_two_pos(drag.start_screen, pointer);
    if rect.width() < 2.0 && rect.height() < 2.0 {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let fill = Color32::from_rgba_unmultiplied(p.accent.r(), p.accent.g(), p.accent.b(), 28);
    painter.rect_filled(rect, CornerRadius::ZERO, fill);
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, p.accent),
        StrokeKind::Inside,
    );
}

fn draw_placement_preview(editor: &Editor, painter: &Painter, t: &SlideTransform, ui: &Ui) {
    let Some(drag) = editor.ui.placement_drag.as_ref() else {
        return;
    };
    if !finite_rect(drag.frame) {
        return;
    }
    if drag.frame.width() < 0.5 && drag.frame.height() < 0.5 {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let screen_rect = t.to_screen_rect(drag.frame);
    let fill = Color32::from_rgba_unmultiplied(p.accent.r(), p.accent.g(), p.accent.b(), 24);
    painter.rect_filled(screen_rect, CornerRadius::ZERO, fill);
    painter.rect_stroke(
        screen_rect,
        CornerRadius::ZERO,
        Stroke::new(1.5, p.accent),
        StrokeKind::Inside,
    );

    let label = format!(
        "{:.0} × {:.0}",
        drag.frame.width().max(0.0),
        drag.frame.height().max(0.0)
    );
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = painter.layout_no_wrap(label, font, p.accent_text);
    let size = galley.size() + vec2(8.0, 4.0);
    let bg = Rect::from_min_size(screen_rect.max - size - vec2(4.0, 4.0), size);
    if bg.min.x.is_finite() && bg.min.y.is_finite() {
        painter.rect_filled(bg, CornerRadius::same(3), p.raised);
        painter.galley(bg.min + vec2(4.0, 2.0), galley, p.accent_text);
    }
}

fn draw_draw_preview(editor: &Editor, painter: &Painter, t: &SlideTransform, ui: &Ui) {
    let Some(session) = editor.ui.draw.as_ref() else {
        return;
    };
    let p = coursecraft_render::palette(ui.ctx());
    let stroke = Stroke::new(1.5, p.accent);
    let mut points: Vec<Pos2> = session
        .points
        .iter()
        .map(|point| t.to_screen_pos(*point))
        .collect();
    if !session.tool.is_drag()
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
        && let Some(last) = points.last()
        && (pointer - *last).length() > 1.0
    {
        points.push(pointer);
    }
    if session.tool.is_drag() && session.points.len() < 2 {
        return;
    }
    if points.len() < 2 {
        return;
    }
    if session.tool == DrawTool::CurvedConnector || session.tool == DrawTool::Curve {
        let mut shaped = Vec::new();
        let style = if session.tool == DrawTool::Curve {
            PathStyle::Curved
        } else {
            PathStyle::Straight
        };
        let connector = if session.tool == DrawTool::CurvedConnector {
            Some(ConnectorStyle::Curved)
        } else {
            None
        };
        let screen_points: Vec<coursecraft_render::ShapePoint> =
            points.iter().map(|point| (point.x, point.y)).collect();
        for (x, y) in coursecraft_render::flatten_path(&screen_points, style, connector, false) {
            shaped.push(pos2(x, y));
        }
        painter.add(egui::Shape::line(shaped, stroke));
    } else {
        painter.add(egui::Shape::line(points.clone(), stroke));
    }
    for point in &points {
        painter.circle_filled(*point, 3.0, p.accent);
    }
}

/// Show the four connection sites on the object under or near the pointer while
/// a connector end is being dragged or a connector tool is armed.
fn draw_connector_sites(editor: &Editor, painter: &Painter, t: &SlideTransform, ui: &Ui) {
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let connector_tool = matches!(
        editor.ui.placement_tool,
        Some(PlacementTool::Draw(
            DrawTool::ElbowConnector | DrawTool::CurvedConnector
        ))
    );
    let dragging = editor.ui.canvas_drag.as_ref().and_then(|drag| {
        if !matches!(drag.op, DragOp::PathPoint { .. }) {
            return None;
        }
        let is_connector = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|slide| slide.object(&drag.object_id))
            .is_some_and(|(object, _)| {
                matches!(&object.kind, ObjectKind::Path { path } if path.connector.is_some())
            });
        is_connector.then(|| drag.object_id.clone())
    });
    if !connector_tool && dragging.is_none() {
        return;
    }
    let exclude = dragging.as_deref();
    let Some((object_id, frame)) = connector_site_target(editor, t, pointer, exclude) else {
        return;
    };
    let alt = ui.input(|i| i.modifiers.alt);
    let highlight = connector_snap(editor, t, pointer, exclude, alt)
        .filter(|snap| snap.glue.object == object_id)
        .map(|snap| snap.glue.site());
    let p = coursecraft_render::palette(ui.ctx());
    for (site, point) in connector_sites(frame).into_iter().enumerate() {
        let screen = t.to_screen_pos(point);
        if !screen.x.is_finite() || !screen.y.is_finite() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let site = site as u8;
        if highlight == Some(site) {
            painter.circle_filled(screen, 5.0, p.accent);
            painter.circle_stroke(screen, 5.0, Stroke::new(1.0, p.accent_text));
        } else {
            painter.circle_filled(screen, 4.0, p.raised);
            painter.circle_stroke(screen, 4.0, Stroke::new(1.5, p.accent));
        }
    }
}

#[allow(clippy::cognitive_complexity)]
fn draw_handles_and_outline(
    editor: &Editor,
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    current_pointer: Option<Pos2>,
    ui: &Ui,
) {
    let ids = editor.selection.ids();
    if ids.is_empty() {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let accent_color = p.accent;

    let mut bbox: Option<Rect> = None;
    for id in &ids {
        let Some(item) = items.iter().find(|it| it.hit == Hit::Object(id.clone())) else {
            continue;
        };
        let corners: Vec<Pos2> = coursecraft_render::item_corners(item)
            .into_iter()
            .map(|point| t.to_screen_pos(point))
            .collect();
        if corners.len() == 4 {
            painter.add(egui::Shape::closed_line(
                corners,
                Stroke::new(2.0, accent_color),
            ));
        }
        let bounds = coursecraft_render::item_bounds(item);
        bbox = Some(match bbox {
            Some(current) => current.union(bounds),
            None => bounds,
        });
    }
    let Some(bbox) = bbox else {
        return;
    };

    if ids.iter().any(|id| editor.ui.locked_objects.contains(id)) {
        // Locked: outline and a lock marker, no handles to grab.
        let corner = t.to_screen_rect(bbox).left_top();
        let marker = Rect::from_min_size(corner + vec2(2.0, 2.0), vec2(16.0, 16.0));
        painter.rect_filled(marker, CornerRadius::same(3), accent_color);
        painter.text(
            marker.center(),
            Align2::CENTER_CENTER,
            coursecraft_render::Icon::Lock.codepoint().to_string(),
            FontId::new(11.0, egui::FontFamily::Name("phosphor".into())),
            p.accent_text,
        );
        return;
    }

    let single_item = if editor.selection.is_multi() {
        None
    } else {
        editor.selection.object.as_ref().and_then(|id| {
            items
                .iter()
                .find(|item| item.hit == Hit::Object(id.clone()))
        })
    };
    let screen_rect = t.to_screen_rect(bbox);
    let handle_size = 8.0;
    let centers = match single_item {
        Some(item) => rotated_handle_centers(t.to_screen_rect(item.rect), item.rotation),
        None => get_handle_centers(screen_rect),
    };
    let rotate_handle =
        single_item.map(|item| rotate_handle_center(t.to_screen_rect(item.rect), item.rotation));

    let path_visual = if !editor.selection.is_multi() {
        editor
            .selection
            .object
            .as_ref()
            .and_then(|id| {
                editor
                    .session
                    .course
                    .slide(&editor.selection.slide)
                    .and_then(|slide| slide.object(id))
                    .map(|(object, _)| (id.clone(), object))
            })
            .and_then(|(id, object)| match &object.kind {
                ObjectKind::Path { path } => Some((id, object, path)),
                _ => None,
            })
    } else {
        None
    };
    let editing_path = path_visual.as_ref().is_some_and(|(id, _, _)| {
        editor
            .ui
            .path_edit
            .as_ref()
            .is_some_and(|edit| &edit.object == id)
    });
    let two_point_path = path_visual
        .as_ref()
        .is_some_and(|(_, _, path)| path.points.len() == 2);
    let connector_path = path_visual
        .as_ref()
        .is_some_and(|(_, _, path)| path.connector.is_some());
    let suppress_box = two_point_path || connector_path;

    if !suppress_box {
        if let (Some(item), Some(handle)) = (single_item, rotate_handle) {
            let frame_screen = t.to_screen_rect(item.rect);
            let top = coursecraft_render::rotate_point(
                pos2(frame_screen.center().x, frame_screen.top()),
                frame_screen.center(),
                item.rotation,
            );
            if editor.ui.canvas_drag.is_none()
                && editor.ui.placement_tool.is_none()
                && current_pointer.is_some_and(|pos| {
                    Rect::from_center_size(handle, vec2(14.0, 14.0)).contains(pos)
                })
            {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
            painter.line_segment([top, handle], Stroke::new(1.0, accent_color));
            painter.circle_filled(handle, 6.0, Color32::WHITE);
            painter.circle_stroke(handle, 6.0, Stroke::new(1.5, accent_color));
        }

        if editor.ui.canvas_drag.is_none()
            && editor.ui.placement_tool.is_none()
            && let Some(pos) = current_pointer
        {
            for (h, c) in centers {
                let handle_rect = Rect::from_center_size(c, vec2(handle_size, handle_size));
                if handle_rect.contains(pos) {
                    let cursor = match h {
                        ResizeHandle::TopLeft | ResizeHandle::BottomRight => CursorIcon::ResizeNwSe,
                        ResizeHandle::TopRight | ResizeHandle::BottomLeft => CursorIcon::ResizeNeSw,
                        ResizeHandle::Top | ResizeHandle::Bottom => CursorIcon::ResizeVertical,
                        ResizeHandle::Left | ResizeHandle::Right => CursorIcon::ResizeHorizontal,
                    };
                    ui.ctx().set_cursor_icon(cursor);
                    break;
                }
            }
        }

        for (_, c) in centers {
            let handle_rect = Rect::from_center_size(c, vec2(handle_size, handle_size));
            painter.rect_filled(handle_rect, CornerRadius::ZERO, Color32::WHITE);
            painter.rect_stroke(
                handle_rect,
                CornerRadius::ZERO,
                Stroke::new(1.0, accent_color),
                StrokeKind::Middle,
            );
        }
    }

    if let Some((_, object, path)) = &path_visual
        && (editing_path || two_point_path || connector_path)
    {
        let selected = if editing_path {
            editor
                .ui
                .path_edit
                .as_ref()
                .and_then(|edit| edit.selected_point)
        } else {
            None
        };
        let last_index = path.points.len().saturating_sub(1);
        for (index, point) in path.points.iter().enumerate() {
            if !editing_path && connector_path && index != 0 && index != last_index {
                continue;
            }
            let centre = t.to_screen_pos(pos2(
                object.frame.x + point.x * object.frame.w,
                object.frame.y + point.y * object.frame.h,
            ));
            let glued = path.connector.is_some()
                && ((index == 0 && path.start_glue.is_some())
                    || (index == last_index && path.end_glue.is_some()));
            painter.circle_filled(
                centre,
                4.5,
                if glued { accent_color } else { Color32::WHITE },
            );
            let is_selected = selected == Some(index);
            painter.circle_stroke(
                centre,
                4.5,
                Stroke::new(
                    if is_selected { 2.0 } else { 1.2 },
                    if is_selected {
                        p.accent_text
                    } else {
                        accent_color
                    },
                ),
            );
            if is_selected {
                painter.circle_filled(centre, 2.0, accent_color);
            }
        }
        if editor.ui.canvas_drag.is_none()
            && editor.ui.placement_tool.is_none()
            && let Some(pos) = current_pointer
        {
            let over_point = path.points.iter().any(|point| {
                let centre = t.to_screen_pos(pos2(
                    object.frame.x + point.x * object.frame.w,
                    object.frame.y + point.y * object.frame.h,
                ));
                Rect::from_center_size(centre, vec2(12.0, 12.0)).contains(pos)
            });
            if over_point {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
    }

    // Yellow diamonds for shape adjustment handles sit above the white resize
    // handles, so the smaller target wins where they overlap.
    if !editor.selection.is_multi()
        && editor.ui.preview_state.is_none()
        && let Some(primary) = editor.selection.object.as_ref()
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(primary)
        && let coursecraft_course::ObjectKind::Shape { shape, adjust, .. } = &obj.kind
        && coursecraft_course::shape_is_adjustable(*shape)
    {
        let hits = coursecraft_render::adjust_handles(*shape, obj.frame.w, obj.frame.h, adjust);
        for (index, (x, y)) in hits.iter().enumerate() {
            let center = t.to_screen_pos(pos2(obj.frame.x + x, obj.frame.y + y));
            let hit_rect = Rect::from_center_size(center, vec2(12.0, 12.0));
            if editor.ui.canvas_drag.is_none()
                && editor.ui.placement_tool.is_none()
                && current_pointer.is_some_and(|pos| hit_rect.contains(pos))
            {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    painter.layer_id(),
                    Id::unique(("shape_adjust_tip", index)),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.label(coursecraft_render::adjust_handle_name(*shape, index));
                });
            }
            let diamond = vec![
                pos2(center.x, center.y - 6.0),
                pos2(center.x + 6.0, center.y),
                pos2(center.x, center.y + 6.0),
                pos2(center.x - 6.0, center.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                diamond,
                p.adjust_handle,
                Stroke::new(1.0, p.app),
            ));
        }
    }

    if !editor.selection.is_multi()
        && let Some(primary) = editor.selection.object.as_ref()
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(primary)
        && !obj.motion_paths.is_empty()
    {
        draw_motion_paths(editor, painter, t, obj, ui, current_pointer);
    }
}

fn draw_dashed_line(
    painter: &Painter,
    p1: Pos2,
    p2: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let dir = p2 - p1;
    let total_len = dir.length();
    if total_len <= 0.0 {
        return;
    }
    let norm = dir / total_len;
    let mut dist = 0.0;
    while dist < total_len {
        let seg_end = (dist + dash_len).min(total_len);
        painter.line_segment([p1 + norm * dist, p1 + norm * seg_end], stroke);
        dist += dash_len + gap_len;
    }
}

fn draw_motion_paths(
    editor: &Editor,
    painter: &Painter,
    t: &SlideTransform,
    obj: &coursecraft_course::Object,
    ui: &Ui,
    current_pointer: Option<Pos2>,
) {
    let path_color = Color32::from_rgb(0x1B, 0x87, 0x3F);

    for path in &obj.motion_paths {
        let mut screen_pts = Vec::with_capacity(path.points.len());
        for (idx, pt) in path.points.iter().enumerate() {
            let mut pt_coords = *pt;
            if let Some(ref drag) = editor.ui.canvas_drag
                && let DragOp::MotionPathPoint {
                    ref path_id,
                    point_index,
                    relative,
                    obj_origin,
                    ..
                } = drag.op
                && path_id == &path.id
                && point_index == idx
                && let Some(cur_pos) = current_pointer
            {
                let slide_pos = t.to_slide_pos(cur_pos);
                if relative {
                    pt_coords.x = slide_pos.x - obj_origin.x;
                    pt_coords.y = slide_pos.y - obj_origin.y;
                } else {
                    pt_coords.x = slide_pos.x;
                    pt_coords.y = slide_pos.y;
                }
            }

            let slide_pos = if path.relative {
                pos2(obj.frame.x + pt_coords.x, obj.frame.y + pt_coords.y)
            } else {
                pos2(pt_coords.x, pt_coords.y)
            };
            screen_pts.push(t.to_screen_pos(slide_pos));
        }

        for window in screen_pts.windows(2) {
            draw_dashed_line(
                painter,
                window[0],
                window[1],
                Stroke::new(2.0, path_color),
                6.0,
                4.0,
            );
        }

        for (idx, &s_pt) in screen_pts.iter().enumerate() {
            let handle_rect = Rect::from_center_size(s_pt, vec2(8.0, 8.0));
            if let Some(pos) = current_pointer
                && handle_rect.contains(pos)
                && !(path.relative && idx == 0)
            {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            painter.circle_filled(s_pt, 4.0, Color32::WHITE);
            painter.circle_stroke(s_pt, 4.0, Stroke::new(1.5, path_color));
        }
    }
}

/// Show the layers strip, the central slide canvas, and the floating zoom HUD.
#[allow(clippy::cognitive_complexity)]
pub fn show_canvas_and_layers(editor: &mut Editor, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;

    let available_size = ui.available_size();
    if !available_size.x.is_finite()
        || !available_size.y.is_finite()
        || available_size.x <= 0.0
        || available_size.y <= 0.0
    {
        return;
    }

    let viewport_rect = ui.available_rect_before_wrap();
    let p = coursecraft_render::palette(ui.ctx());

    let slide_size = editor.session.course.slide_size;
    let (sw, sh) = (slide_size.w as f32, slide_size.h as f32);

    let pinch_scroll = pinch_canvas_zoom(editor, ui, viewport_rect, available_size, vec2(sw, sh));

    let (content_size, scale_override) = match editor.ui.zoom.fixed_scale() {
        None => {
            // Fit mode: slide fits available viewport with at least 32 px margin
            (available_size, None)
        }
        Some(scale) => (
            canvas_content_size(available_size, vec2(sw, sh), scale).0,
            Some(scale),
        ),
    };

    let zoom_hud_w = 132.0;
    let zoom_hud_h = 32.0;
    let zoom_hud_pos = pos2(
        viewport_rect.max.x - zoom_hud_w - 16.0,
        viewport_rect.max.y - zoom_hud_h - 16.0,
    );
    let zoom_hud_rect = Rect::from_min_size(zoom_hud_pos, vec2(zoom_hud_w, zoom_hud_h));

    let mut scroll_area = ScrollArea::both()
        .id_salt("canvas_scroll_area")
        .auto_shrink([false, false]);
    if let Some(offset) = pinch_scroll {
        scroll_area = scroll_area.scroll_offset(offset.max(egui::Vec2::ZERO));
    }
    scroll_area.show(ui, |ui| {
        let (canvas_rect, response) = ui.allocate_exact_size(content_size, Sense::click_and_drag());
        let painter = ui.painter_at(canvas_rect);

        painter.rect_filled(canvas_rect, CornerRadius::ZERO, p.canvas);

        let has_slide = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .is_some();
        if !has_slide {
            painter.text(
                canvas_rect.center(),
                Align2::CENTER_CENTER,
                "No slide selected",
                FontId::proportional(16.0),
                p.text_secondary,
            );
            return;
        }

        let t = if let Some(scale) = scale_override {
            let slide_w = sw * scale;
            let slide_h = sh * scale;
            let offset_x = canvas_rect.min.x + (canvas_rect.width() - slide_w) * 0.5;
            let offset_y = canvas_rect.min.y + (canvas_rect.height() - slide_h) * 0.5;
            let slide_rect = Rect::from_min_size(pos2(offset_x, offset_y), vec2(slide_w, slide_h));
            SlideTransform::fit(slide_rect, slide_size)
        } else {
            let fit_rect = canvas_fit_rect(canvas_rect);
            SlideTransform::fit(fit_rect, slide_size)
        };

        let Some(t) = t else {
            return;
        };

        let crop_cancelled =
            editor.ui.crop_edit.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if crop_cancelled {
            editor.cancel_crop();
        }

        editor.ui.last_slide_rect = Some(t.slide_rect());
        crate::content_region::mark_content_region(ui.ctx(), ui.layer_id(), t.slide_rect());
        editor.ui.last_zoom_hud_rect = Some(zoom_hud_rect);

        // Layer pills above the slide, left-aligned with slide left edge
        show_layer_pills(editor, ui, &t);

        // Enter edits the selected text-bearing object in place, like a
        // double click. Text fields and other modals keep their own Enter.
        enter_edits_selection(editor, ui);

        let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
            return;
        };

        let extra_layers = editor
            .ui
            .layers
            .extra_layer_ids(slide, editor.selection.layer.as_deref());
        let mut items = compose_authoring_layers(
            &editor.session.course,
            slide,
            editor.selection.layer.as_deref(),
            &extra_layers,
            editor.ui.layers.dim_other_layers,
        );
        if !editor.ui.hidden_objects.is_empty() {
            items.retain(|item| {
                !matches!(&item.hit, Hit::Object(id) if editor.ui.hidden_objects.contains(id))
            });
        }
        missing_images_as_placeholders(&mut items, &editor.media);
        if let Some(selected) = &editor.selection.object {
            for item in &mut items {
                if item.hit == Hit::Object(selected.clone())
                    && let Visual::Marker { open, .. } = &mut item.visual
                {
                    *open = true;
                }
            }
        }
        for item in &mut items {
            if matches!(item.visual, Visual::Table(_)) {
                item.show_table_overflow = true;
            }
        }
        if let Some(edit) = &editor.ui.inline_edit {
            hide_edited_text(&mut items, edit);
        }

        if let (Some(sel_obj_id), Some(preview_state)) =
            (&editor.selection.object, &editor.ui.preview_state)
            && let Some((obj, _)) = slide.object(sel_obj_id)
            && let Some(preview_item) = compose_object(obj, preview_state)
        {
            if let Some(pos) = items
                .iter()
                .position(|it| it.hit == Hit::Object(sel_obj_id.clone()))
            {
                items[pos] = preview_item;
            } else {
                items.push(preview_item);
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let had_tool = editor.ui.placement_tool.is_some();
            let had_draw = editor.ui.draw.is_some();
            let had_path_edit = editor.ui.path_edit.is_some();
            let had_placement_drag = editor.ui.placement_drag.is_some();
            let had_drag = editor.ui.canvas_drag.is_some();
            let had_edit = editor.ui.inline_edit.is_some();
            let had_crop = crop_cancelled || editor.ui.crop_edit.is_some();
            editor.ui.placement_tool = None;
            editor.ui.placement_drag = None;
            editor.ui.draw = None;
            editor.ui.path_edit = None;
            editor.ui.canvas_drag = None;
            editor.ui.snap_guides.clear();
            let other_modal = editor.ui.show_shortcuts_dialog
                || editor.ui.show_course_settings_window
                || editor.ui.show_theme_window
                || editor.ui.trigger_form.is_some()
                || editor.ui.layer_settings.is_open();
            if !had_tool
                && !had_draw
                && !had_path_edit
                && !had_placement_drag
                && !had_drag
                && !had_edit
                && !had_crop
                && !other_modal
                && !ui.ctx().egui_wants_keyboard_input()
                && !ui.memory(|m| m.focused().is_some())
            {
                editor.selection.clear_objects();
            }
        }

        let modifiers = ui.input(|i| i.modifiers);
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let current_pointer = ui
            .input(|i| i.pointer.interact_pos())
            .or_else(|| response.interact_pointer_pos());

        let pointer_in_hud = current_pointer.is_some_and(|p| zoom_hud_rect.contains(p))
            || current_pointer
                .is_some_and(|p| crate::rulers::captures(&editor.ui.rulers, canvas_rect, &t, p));
        let was_editing = editor.ui.inline_edit.is_some();

        let was_cropping = editor.ui.crop_edit.is_some();
        if was_cropping {
            crop_input(editor, ui, &t, current_pointer, canvas_rect, pointer_in_hud);
        } else if !pointer_in_hud && !was_editing {
            if matches!(editor.ui.placement_tool, Some(PlacementTool::Draw(_))) {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                process_draw_tool(editor, &t, ui, &response);
            } else if editor.ui.placement_tool.is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                let press_pos = ui
                    .input(|i| {
                        i.pointer
                            .primary_pressed()
                            .then(|| i.pointer.interact_pos())
                    })
                    .flatten()
                    .filter(|p| canvas_rect.contains(*p));
                if let Some(pos) = press_pos {
                    let start_slide = t.to_slide_pos(pos);
                    if finite_pos(start_slide) {
                        editor.ui.placement_drag = Some(PlacementDrag {
                            start_slide,
                            frame: Rect::from_min_size(start_slide, vec2(0.0, 0.0)),
                        });
                    }
                }
                if editor.ui.placement_drag.is_some() {
                    process_placement_drag(editor, &t, ui);
                }
            } else {
                if editor.ui.canvas_drag.is_none()
                    && (response.is_pointer_button_down_on() || response.drag_started())
                    && primary_pressed
                    && let Some(start_pos) = current_pointer
                {
                    start_drag_if_pressed(editor, start_pos, &t, &items, modifiers);
                }

                if editor.ui.canvas_drag.is_some() {
                    process_active_drag(editor, &mut items, &t, ui);
                }
                if response.clicked()
                    && let Some(pos) = current_pointer
                {
                    if response.double_clicked() {
                        handle_canvas_double_click(editor, pos, &t, &items, modifiers);
                    } else {
                        handle_canvas_click(editor, pos, &t, &items, modifiers);
                    }
                }
            }
        }

        // Right-click acts on what is under the pointer. An unselected object
        // becomes the selection first; a selected one keeps a multi-selection.
        // Crop, inline text editing and placement tools keep their old menu.
        let special_mode = editor.ui.placement_tool.is_some()
            || editor.ui.crop_edit.is_some()
            || editor.ui.inline_edit.is_some();
        if !special_mode
            && response.secondary_clicked()
            && let Some(pos) = current_pointer
        {
            handle_canvas_secondary_click(editor, pos, &t, &items);
        }
        response.context_menu(|ui| show_canvas_context_menu(editor, ui, special_mode));

        // Timeline video: flush the newest seek first, then let the
        // painter draw, for each video whose timing includes the
        // playhead, the backend's newest frame for
        // `playhead_ms - timing.start_ms`. The texture store keeps the
        // last frame while a seek is in flight (never a blank flash).
        // On wasm `TimelinePlayback` uses the null backend, so the
        // web editor keeps its placeholder (M12 step 2).
        let playhead_ms = editor.ui.timeline.playhead_ms;
        editor.timeline_playback.flush_seeks(playhead_ms);
        if editor.timeline_playback.needs_repaint(playhead_ms) {
            ui.ctx().request_repaint();
        }
        let memory = crate::MemoryMediaSource {
            media: &editor.media,
        };
        let source = crate::TimelineMediaSource {
            inner: &memory,
            playback: &editor.timeline_playback,
            playhead_ms,
        };
        let theme = Some(&editor.session.course.theme);
        let slide_bg = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .map(|s| editor.session.course.slide_background(s));

        // Slide drop shadow behind the slide canvas
        let shadow_mode = coursecraft_render::theme_mode(ui.ctx());
        painter.add(
            coursecraft_render::slide_shadow(shadow_mode)
                .as_shape(t.slide_rect(), CornerRadius::ZERO),
        );

        paint(
            &painter,
            &t,
            &items,
            Highlight::default(),
            Some((&mut editor.media_cache, &source)),
            theme,
            slide_bg,
        );

        // 1 px border outline around the slide
        painter.rect_stroke(
            t.slide_rect(),
            CornerRadius::ZERO,
            Stroke::new(1.0, p.border),
            StrokeKind::Outside,
        );

        crate::rulers::draw_grid(editor, &painter, &t);
        draw_marquee(editor, &painter, ui);
        draw_placement_preview(editor, &painter, &t, ui);
        draw_draw_preview(editor, &painter, &t, ui);
        if editor.ui.crop_edit.is_some() {
            draw_crop_overlay(editor, &painter, &t, ui);
        } else {
            draw_handles_and_outline(editor, &painter, &t, &items, current_pointer, ui);
            draw_connector_sites(editor, &painter, &t, ui);
            draw_snap_guides(editor, &painter, &t, ui);
        }
        crate::rulers::show(editor, ui, &painter, &t, canvas_rect);
        if let Some(slide) = editor.session.course.slide(&editor.selection.slide) {
            if let Some(ref q) = slide.question
                && q.kind == QuestionKind::Hotspot
            {
                draw_hotspot_regions(editor, &painter, &t, q, ui, current_pointer);
            }

            let layer_is_empty = match editor.selection.layer.as_deref() {
                Some(lid) => slide.layer(lid).is_some_and(|l| l.objects.is_empty()),
                None => slide.objects.is_empty(),
            };
            if layer_is_empty && editor.ui.placement_tool.is_none() {
                draw_empty_slide_hint(editor, ui, &t);
            }
        }

        draw_inline_edit(editor, ui, &t);
    });

    // Floating zoom HUD at bottom-right of viewport
    let shadow_mode = coursecraft_render::theme_mode(ui.ctx());
    Area::new(Id::unique("canvas_zoom_hud"))
        .fixed_pos(zoom_hud_pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(p.raised)
                .corner_radius(8)
                .stroke(Stroke::new(1.0, p.border))
                .shadow(coursecraft_render::popover_shadow(shadow_mode))
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;

                        let minus_enabled = editor.ui.zoom.zoom_out() != editor.ui.zoom;
                        if crate::widgets::icon_button_enabled(
                            ui,
                            coursecraft_render::Icon::MagnifyingGlassMinus,
                            minus_enabled,
                            "Zoom out",
                        )
                        .clicked()
                            && minus_enabled
                        {
                            editor.ui.zoom = editor.ui.zoom.zoom_out();
                        }

                        let zoom_resp = zoom_value_button(ui, &editor.ui.zoom.label());
                        egui::Popup::menu(&zoom_resp).show(|ui| {
                            for z in ZoomLevel::ALL {
                                if ui.button(z.label()).clicked() {
                                    editor.ui.zoom = z;
                                    ui.close();
                                }
                            }
                        });

                        let plus_enabled = editor.ui.zoom.zoom_in() != editor.ui.zoom;
                        if crate::widgets::icon_button_enabled(
                            ui,
                            coursecraft_render::Icon::MagnifyingGlassPlus,
                            plus_enabled,
                            "Zoom in",
                        )
                        .clicked()
                            && plus_enabled
                        {
                            editor.ui.zoom = editor.ui.zoom.zoom_in();
                        }
                    });
                });
        });
}

/// Quiet zoom value button: `text_secondary` at rest, `text` on hover, with a caret.
fn zoom_value_button(ui: &mut Ui, label: &str) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let font = FontId::proportional(coursecraft_render::SIZE_BODY);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 24.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.to_owned()));
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                p.hover,
            );
        }
        let color = if hovered { p.text } else { p.text_secondary };
        let text_pos = pos2(rect.left() + 6.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(text_pos, galley, color);

        let caret = ui.painter().layout_no_wrap(
            coursecraft_render::Icon::CaretDown.codepoint().to_string(),
            FontId::new(10.0, egui::FontFamily::Name("phosphor".into())),
            color,
        );
        let caret_pos = pos2(
            rect.right() - caret.size().x - 4.0,
            rect.center().y - caret.size().y * 0.5,
        );
        ui.painter().galley(caret_pos, caret, color);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn draw_dashed_rect(painter: &Painter, rect: Rect, stroke: Stroke) {
    draw_dashed_line(painter, rect.left_top(), rect.right_top(), stroke, 6.0, 4.0);
    draw_dashed_line(
        painter,
        rect.right_top(),
        rect.right_bottom(),
        stroke,
        6.0,
        4.0,
    );
    draw_dashed_line(
        painter,
        rect.right_bottom(),
        rect.left_bottom(),
        stroke,
        6.0,
        4.0,
    );
    draw_dashed_line(
        painter,
        rect.left_bottom(),
        rect.left_top(),
        stroke,
        6.0,
        4.0,
    );
}

fn draw_dashed_ellipse(painter: &Painter, center: Pos2, radius: egui::Vec2, stroke: Stroke) {
    let steps = 36;
    let mut prev = pos2(center.x + radius.x, center.y);
    for i in 1..=steps {
        let angle = (i as f32) * std::f32::consts::TAU / (steps as f32);
        let curr = pos2(
            center.x + radius.x * angle.cos(),
            center.y + radius.y * angle.sin(),
        );
        draw_dashed_line(painter, prev, curr, stroke, 6.0, 4.0);
        prev = curr;
    }
}

fn draw_hotspot_regions(
    editor: &Editor,
    painter: &Painter,
    t: &SlideTransform,
    q: &coursecraft_course::Question,
    ui: &Ui,
    current_pointer: Option<Pos2>,
) {
    let scale = t.scale();
    let shift_held = ui.input(|i| i.modifiers.shift);
    let p = coursecraft_render::palette(ui.ctx());

    for region in &q.regions {
        let is_selected = editor.ui.question.selected_region.as_deref() == Some(&region.id);
        let orig_rect = Rect::from_min_size(pos2(region.x, region.y), vec2(region.w, region.h));

        let current_rect = if let Some(ref drag) = editor.ui.canvas_drag
            && drag.object_id == region.id
            && matches!(drag.op, DragOp::RegionMove | DragOp::RegionResize(_))
        {
            let curr_screen = current_pointer.unwrap_or(drag.start_screen);
            let delta = curr_screen - drag.start_screen;
            if delta.length() >= 3.0 {
                compute_drag_frame(drag, curr_screen, shift_held, scale)
            } else {
                orig_rect
            }
        } else {
            orig_rect
        };

        let screen_rect = t.to_screen_rect(current_rect);
        let color = if region.correct {
            p.success
        } else {
            p.text_tertiary
        };

        match region.shape {
            HotspotShape::Rect => {
                draw_dashed_rect(painter, screen_rect, Stroke::new(2.0, color));
            }
            HotspotShape::Ellipse => {
                draw_dashed_ellipse(
                    painter,
                    screen_rect.center(),
                    vec2(screen_rect.width() * 0.5, screen_rect.height() * 0.5),
                    Stroke::new(2.0, color),
                );
            }
        }

        if is_selected {
            let handle_size = 8.0;
            let centers = get_handle_centers(screen_rect);

            if editor.ui.canvas_drag.is_none()
                && editor.ui.placement_tool.is_none()
                && let Some(pos) = current_pointer
            {
                for (h, c) in centers {
                    let handle_rect = Rect::from_center_size(c, vec2(handle_size, handle_size));
                    if handle_rect.contains(pos) {
                        let cursor = match h {
                            ResizeHandle::TopLeft | ResizeHandle::BottomRight => {
                                CursorIcon::ResizeNwSe
                            }
                            ResizeHandle::TopRight | ResizeHandle::BottomLeft => {
                                CursorIcon::ResizeNeSw
                            }
                            ResizeHandle::Top | ResizeHandle::Bottom => CursorIcon::ResizeVertical,
                            ResizeHandle::Left | ResizeHandle::Right => {
                                CursorIcon::ResizeHorizontal
                            }
                        };
                        ui.ctx().set_cursor_icon(cursor);
                        break;
                    }
                }
            }

            for (_, c) in centers {
                let handle_rect = Rect::from_center_size(c, vec2(handle_size, handle_size));
                painter.rect_filled(handle_rect, CornerRadius::ZERO, Color32::WHITE);
                painter.rect_stroke(
                    handle_rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, color),
                    StrokeKind::Middle,
                );
            }
        }
    }
}

fn draw_empty_slide_hint(editor: &mut Editor, ui: &mut Ui, t: &SlideTransform) {
    let p = coursecraft_render::palette(ui.ctx());
    let s_rect = t.slide_rect();
    let hint_w = 340.0f32.min(s_rect.width() - 32.0);
    let hint_h = 70.0f32;
    let hint_rect = Rect::from_center_size(s_rect.center(), vec2(hint_w, hint_h));

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(hint_rect));
    child.vertical_centered(|ui| {
        ui.label(
            RichText::new("Add something to this slide")
                .size(coursecraft_render::SIZE_BODY)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let btn_total_w = 3.0 * 68.0 + 2.0 * 12.0;
            let pad = ((ui.available_width() - btn_total_w) * 0.5).max(0.0);
            ui.add_space(pad);

            let text_btn = egui::Button::new(
                coursecraft_render::icon_text(coursecraft_render::Icon::TextT, "Text")
                    .size(coursecraft_render::SIZE_BODY)
                    .color(p.text_secondary),
            )
            .fill(Color32::TRANSPARENT);
            if ui
                .add(text_btn)
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                crate::toolbar::insert_text_box(editor);
            }

            let shape_btn = egui::Button::new(
                coursecraft_render::icon_text(coursecraft_render::Icon::Shapes, "Shape")
                    .size(coursecraft_render::SIZE_BODY)
                    .color(p.text_secondary),
            )
            .fill(Color32::TRANSPARENT);
            if ui
                .add(shape_btn)
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                crate::toolbar::insert_rectangle(editor);
            }

            let img_btn = egui::Button::new(
                coursecraft_render::icon_text(coursecraft_render::Icon::Image, "Image")
                    .size(coursecraft_render::SIZE_BODY)
                    .color(p.text_secondary),
            )
            .fill(Color32::TRANSPARENT);
            if ui
                .add(img_btn)
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                let mut reqs = Vec::new();
                crate::toolbar::insert_image(editor, &mut reqs);
                editor.pending_requests.extend(reqs);
            }
        });
    });
}
