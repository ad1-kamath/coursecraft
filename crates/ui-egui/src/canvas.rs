//! Canvas center view: slide display, selection, outline, handles, zoom, and layer pills.

use crate::{DrawTool, Editor, PlacementTool, tl};
use coursecraft_course::{
    ConnectorStyle, Course, Glue, HotspotShape, LineEnd, Object, ObjectKind, Path, PathStyle,
    Point, QuestionKind, ShapeKind, shape_info, shape_label,
};
use coursecraft_render::{DrawItem, Highlight, Hit, SlideTransform, Visual, paint};
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

    /// One step in. From `Fit` the step starts at the scale `Fit` shows now
    /// (`fit_scale`): the next preset above it, so zooming in never zooms out.
    #[must_use]
    pub fn zoom_in_at(&self, fit_scale: Option<f32>) -> Self {
        match (self, fit_scale) {
            (Self::Fit, Some(fit)) if fit.is_finite() && fit > 0.0 => Self::ALL
                .into_iter()
                .find(|level| {
                    level
                        .fixed_scale()
                        .is_some_and(|preset| preset > fit + 0.001)
                })
                .unwrap_or_else(|| {
                    // Past the largest preset: step by a half, up to the pinch limit.
                    let next = (fit * 1.5).min(PINCH_ZOOM_RANGE.1);
                    if next > fit + 0.001 {
                        Self::Custom(next)
                    } else {
                        Self::Fit
                    }
                }),
            _ => self.zoom_in(),
        }
    }

    /// One step out. From `Fit` the next preset below the scale `Fit` shows now.
    #[must_use]
    pub fn zoom_out_at(&self, fit_scale: Option<f32>) -> Self {
        match (self, fit_scale) {
            (Self::Fit, Some(fit)) if fit.is_finite() && fit > 0.0 => Self::ALL
                .into_iter()
                .rev()
                .find(|level| {
                    level
                        .fixed_scale()
                        .is_some_and(|preset| preset < fit - 0.001)
                })
                .unwrap_or_else(|| {
                    // Below the smallest preset: step by a third, down to the pinch limit.
                    let next = (fit / 1.5).max(PINCH_ZOOM_RANGE.0);
                    if next < fit - 0.001 {
                        Self::Custom(next)
                    } else {
                        Self::Fit
                    }
                }),
            _ => self.zoom_out(),
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
    StateExtraMove,
    StateExtraResize {
        handle: ResizeHandle,
        rotation: f32,
    },
    StateExtraRotate {
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
        press_offset: egui::Vec2,
    },
    Adjust {
        index: usize,
        original: Vec<f32>,
        press_offset: egui::Vec2,
    },
    /// Dragging one point of a path (an end handle or a point-edit handle).
    PathPoint {
        index: usize,
        original: Vec<coursecraft_course::PathPoint>,
        two_point: bool,
        press_offset: egui::Vec2,
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
    /// True when Alt was held at move-drag start (duplicates on drop).
    pub alt_duplicate: bool,
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
    /// Live text runs tracked across edits while the inline editor is open.
    pub live_runs: Vec<coursecraft_course::TextRun>,
    /// Live paragraph styles tracked across edits while the inline editor is open.
    pub live_paragraphs: Vec<coursecraft_course::Paragraph>,
    /// Course revision at which `live_runs` and `live_paragraphs` were synced.
    pub last_revision: u64,
    /// When editing inside Canvas Edit-State Mode, the state name to patch on commit.
    pub state_name: Option<String>,
    /// When editing a `StateExtra` inside Canvas Edit-State Mode, its extra ID.
    pub extra_id: Option<String>,
}

/// Enter in-place editing for a text object, button or shape on the current slide.
fn start_inline_edit(editor: &mut Editor, object_id: &str) {
    let slide_id = editor.selection.slide.clone();
    if let Some(session) = editor.ui.editing_state.clone()
        && session.slide_id == slide_id
        && session.object_id == object_id
        && let Some(extra_id) = session.selected_extra
    {
        start_extra_inline_edit(editor, &slide_id, object_id, &session.state_name, &extra_id);
        return;
    }
    let Some(slide) = editor.session.course.slide(&slide_id) else {
        return;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return;
    };
    let state_name = editor
        .ui
        .editing_state
        .as_ref()
        .filter(|s| s.slide_id == slide_id && s.object_id == object_id)
        .map(|s| s.state_name.clone());
    let state_def = state_name
        .as_deref()
        .and_then(|name| object.states.iter().find(|s| s.name == name));
    let (base_buffer, is_button, is_shape, base_runs, base_paragraphs) = match &object.kind {
        coursecraft_course::ObjectKind::Text {
            text,
            runs,
            paragraphs,
            ..
        } => {
            let plain = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|r| r.text.as_str()).collect()
            };
            (plain, false, false, runs.clone(), paragraphs.clone())
        }
        coursecraft_course::ObjectKind::Button { label, .. } => {
            (label.clone(), true, false, Vec::new(), Vec::new())
        }
        coursecraft_course::ObjectKind::Shape { shape, text, .. }
            if coursecraft_course::shape_allows_text(*shape) =>
        {
            let plain = text
                .as_ref()
                .map(|t| {
                    if t.runs.is_empty() {
                        t.text.clone()
                    } else {
                        t.runs.iter().map(|r| r.text.as_str()).collect()
                    }
                })
                .unwrap_or_default();
            (
                plain,
                false,
                true,
                text.as_ref().map(|t| t.runs.clone()).unwrap_or_default(),
                text.as_ref()
                    .map(|t| t.paragraphs.clone())
                    .unwrap_or_default(),
            )
        }
        _ => return,
    };
    let (buffer, live_runs, live_paragraphs) = if let Some(st) = state_def {
        let runs = st.runs.clone().unwrap_or_else(|| {
            if st.text.is_some() {
                Vec::new()
            } else {
                base_runs
            }
        });
        let paragraphs = st.paragraphs.clone().unwrap_or(base_paragraphs);
        let plain = if !runs.is_empty() {
            runs.iter().map(|r| r.text.as_str()).collect()
        } else if let Some(t) = &st.text {
            t.clone()
        } else {
            base_buffer
        };
        (plain, runs, paragraphs)
    } else {
        (base_buffer, base_runs, base_paragraphs)
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
        live_runs,
        live_paragraphs,
        last_revision: editor.session.revision,
        state_name,
        extra_id: None,
    });
    editor.ui.inline_edit_yielded_to_panel = false;
    editor.ui.inline_edit_measured_size = None;
    editor.ui.inline_edit_live_rect = None;
}

/// Enter in-place text editing for a `StateExtra` (Text or text-bearing Shape).
fn start_extra_inline_edit(
    editor: &mut Editor,
    slide_id: &str,
    object_id: &str,
    state_name: &str,
    extra_id: &str,
) {
    let Some(slide) = editor.session.course.slide(slide_id) else {
        return;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return;
    };
    let Some(extra) = object
        .states
        .iter()
        .find(|s| s.name == state_name)
        .and_then(|s| s.extras.iter().find(|e| e.id == extra_id))
    else {
        return;
    };
    let (buffer, is_shape, live_runs, live_paragraphs) = match &extra.kind {
        coursecraft_course::StateExtraKind::Text {
            text,
            runs,
            paragraphs,
            ..
        } => {
            let plain = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|r| r.text.as_str()).collect()
            };
            (plain, false, runs.clone(), paragraphs.clone())
        }
        coursecraft_course::StateExtraKind::Shape { shape, text, .. }
            if coursecraft_course::shape_allows_text(*shape) =>
        {
            let plain = text
                .as_ref()
                .map(|t| {
                    if t.runs.is_empty() {
                        t.text.clone()
                    } else {
                        t.runs.iter().map(|r| r.text.as_str()).collect()
                    }
                })
                .unwrap_or_default();
            (
                plain,
                true,
                text.as_ref().map(|t| t.runs.clone()).unwrap_or_default(),
                text.as_ref()
                    .map(|t| t.paragraphs.clone())
                    .unwrap_or_default(),
            )
        }
        _ => return,
    };
    editor.ui.inline_edit = Some(InlineEdit {
        slide: slide_id.to_owned(),
        object: object_id.to_owned(),
        original: buffer.clone(),
        buffer,
        is_button: false,
        is_shape,
        cell: None,
        just_entered: true,
        live_runs,
        live_paragraphs,
        last_revision: editor.session.revision,
        state_name: Some(state_name.to_owned()),
        extra_id: Some(extra_id.to_owned()),
    });
    editor.ui.inline_edit_yielded_to_panel = false;
    editor.ui.inline_edit_measured_size = None;
    editor.ui.inline_edit_live_rect = None;
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
    let plain = if cell.runs.is_empty() {
        cell.text.clone()
    } else {
        cell.runs.iter().map(|r| r.text.as_str()).collect()
    };
    editor.ui.inline_edit = Some(InlineEdit {
        slide: slide_id,
        object: object_id.to_owned(),
        original: plain.clone(),
        buffer: plain,
        is_button: false,
        is_shape: false,
        cell: Some((row, column)),
        just_entered: true,
        live_runs: cell.runs.clone(),
        live_paragraphs: cell.paragraphs.clone(),
        last_revision: editor.session.revision,
        state_name: None,
        extra_id: None,
    });
    editor.ui.inline_edit_yielded_to_panel = false;
    editor.ui.inline_edit_measured_size = None;
    editor.ui.inline_edit_live_rect = None;
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
        || editor.edit_keys_blocked()
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

fn commit_state_extra_inline_edit(
    editor: &mut Editor,
    edit: InlineEdit,
    object: &coursecraft_course::Object,
    state_name: String,
    extra_id: String,
    measured_size: Option<(f32, f32)>,
) {
    let Some(extra) = object
        .states
        .iter()
        .find(|s| s.name == state_name)
        .and_then(|s| s.extras.iter().find(|e| e.id == extra_id))
    else {
        return;
    };
    let live_plain: String = edit.live_runs.iter().map(|r| r.text.as_str()).collect();
    let mut updated_kind = extra.kind.clone();
    let mut pass_measured_size = false;
    match &mut updated_kind {
        coursecraft_course::StateExtraKind::Text {
            text,
            runs,
            paragraphs,
            autofit,
            point_text,
            ..
        } => {
            pass_measured_size = *point_text || *autofit == coursecraft_course::Autofit::Grow;
            if !edit.live_runs.is_empty() && live_plain == edit.buffer {
                *runs = coursecraft_course::normalize_runs(&edit.live_runs);
                *paragraphs = edit.live_paragraphs;
            } else {
                let prev_plain: String = if runs.is_empty() {
                    text.clone()
                } else {
                    runs.iter().map(|r| r.text.as_str()).collect()
                };
                if !runs.is_empty() {
                    *runs = coursecraft_course::apply_text_edit(runs, &prev_plain, &edit.buffer);
                }
                if !paragraphs.is_empty() {
                    *paragraphs =
                        coursecraft_course::align_paragraphs(&prev_plain, paragraphs, &edit.buffer);
                }
            }
            *text = edit.buffer;
        }
        coursecraft_course::StateExtraKind::Shape {
            shape, fill, text, ..
        } if coursecraft_course::shape_allows_text(*shape) => {
            let st = text.get_or_insert_with(|| coursecraft_course::ShapeText {
                text: String::new(),
                style: default_shape_text_style(fill),
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: coursecraft_course::Autofit::None,
            });
            if !edit.live_runs.is_empty() && live_plain == edit.buffer {
                st.runs = coursecraft_course::normalize_runs(&edit.live_runs);
                st.paragraphs = edit.live_paragraphs;
            } else {
                let prev_plain: String = if st.runs.is_empty() {
                    st.text.clone()
                } else {
                    st.runs.iter().map(|r| r.text.as_str()).collect()
                };
                if !st.runs.is_empty() {
                    st.runs =
                        coursecraft_course::apply_text_edit(&st.runs, &prev_plain, &edit.buffer);
                }
                if !st.paragraphs.is_empty() {
                    st.paragraphs = coursecraft_course::align_paragraphs(
                        &prev_plain,
                        &st.paragraphs,
                        &edit.buffer,
                    );
                }
            }
            st.text = edit.buffer;
        }
        _ => return,
    }
    let mut params = serde_json::json!({
        "slide": edit.slide,
        "object": edit.object,
        "state": state_name,
        "extra": extra_id,
        "patch": { "kind": updated_kind },
    });
    if pass_measured_size && let Some((w, h)) = measured_size {
        params["measured_size"] = serde_json::json!({ "w": w, "h": h });
    }
    editor.run("object.state_update_extra", params);
}

/// Writes the text of an inline edit into the state `state_name` of `object`.
fn commit_state_inline_edit(
    editor: &mut Editor,
    edit: &InlineEdit,
    object: &coursecraft_course::Object,
    state_name: &str,
) {
    let mut patch = serde_json::json!({ "text": edit.buffer });
    let live_plain: String = edit.live_runs.iter().map(|r| r.text.as_str()).collect();
    if !edit.live_runs.is_empty() && live_plain == edit.buffer {
        patch["runs"] = serde_json::to_value(coursecraft_course::normalize_runs(&edit.live_runs))
            .unwrap_or_default();
        patch["paragraphs"] = serde_json::to_value(&edit.live_paragraphs).unwrap_or_default();
    } else if !matches!(object.kind, coursecraft_course::ObjectKind::Button { .. }) {
        let state_def = object.states.iter().find(|s| s.name == state_name);
        let (base_runs, base_paras, base_text) = match &object.kind {
            coursecraft_course::ObjectKind::Text {
                text,
                runs,
                paragraphs,
                ..
            } => (runs.as_slice(), paragraphs.as_slice(), text.as_str()),
            coursecraft_course::ObjectKind::Shape { text: Some(t), .. } => {
                (t.runs.as_slice(), t.paragraphs.as_slice(), t.text.as_str())
            }
            _ => (&[][..], &[][..], ""),
        };
        let prev_runs = state_def
            .and_then(|s| s.runs.as_deref())
            .unwrap_or(base_runs);
        let prev_paras = state_def
            .and_then(|s| s.paragraphs.as_deref())
            .unwrap_or(base_paras);
        let prev_plain: String = if prev_runs.is_empty() {
            state_def
                .and_then(|s| s.text.as_deref())
                .unwrap_or(base_text)
                .to_owned()
        } else {
            prev_runs.iter().map(|r| r.text.as_str()).collect()
        };
        if !prev_runs.is_empty() {
            patch["runs"] = serde_json::to_value(coursecraft_course::apply_text_edit(
                prev_runs,
                &prev_plain,
                &edit.buffer,
            ))
            .unwrap_or_default();
        }
        if !prev_paras.is_empty() {
            patch["paragraphs"] = serde_json::to_value(coursecraft_course::align_paragraphs(
                &prev_plain,
                prev_paras,
                &edit.buffer,
            ))
            .unwrap_or_default();
        }
    }
    editor.run(
        "object.patch_state",
        serde_json::json!({
            "slide": edit.slide,
            "object": edit.object,
            "name": state_name,
            "patch": patch,
        }),
    );
}

pub(crate) fn commit_inline_edit(editor: &mut Editor) {
    let Some(edit) = editor.ui.inline_edit.take() else {
        return;
    };
    let measured_size = editor
        .ui
        .inline_edit_measured_size
        .take()
        .and_then(|(buf, (w, h))| {
            (buf == edit.buffer && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
                .then_some((w, h))
        });
    editor.ui.inline_edit_live_rect = None;
    if edit.buffer == edit.original {
        return;
    }
    let Some(slide) = editor.session.course.slide(&edit.slide) else {
        return;
    };
    let Some((object, _)) = slide.object(&edit.object) else {
        return;
    };
    let object = object.clone();
    if let Some(state_name) = edit.state_name.clone() {
        if let Some(extra_id) = edit.extra_id.clone() {
            commit_state_extra_inline_edit(
                editor,
                edit,
                &object,
                state_name,
                extra_id,
                measured_size,
            );
            return;
        }
        commit_state_inline_edit(editor, &edit, &object, &state_name);
        return;
    }
    match &object.kind {
        coursecraft_course::ObjectKind::Text {
            point_text,
            autofit,
            ..
        } => {
            let mut params = serde_json::json!({
                "slide": edit.slide,
                "object": edit.object,
                "text": edit.buffer,
            });
            let live_plain: String = edit.live_runs.iter().map(|r| r.text.as_str()).collect();
            if !edit.live_runs.is_empty() && live_plain == edit.buffer {
                params["runs"] = serde_json::to_value(&edit.live_runs).unwrap_or_default();
                params["paragraphs"] =
                    serde_json::to_value(&edit.live_paragraphs).unwrap_or_default();
            }
            if (*point_text || *autofit == coursecraft_course::Autofit::Grow)
                && let Some((w, h)) = measured_size
            {
                params["measured_size"] = serde_json::json!({ "w": w, "h": h });
            }
            editor.run("text.set", params);
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
                    let live_plain: String =
                        edit.live_runs.iter().map(|r| r.text.as_str()).collect();
                    let (runs, paragraphs) =
                        if !edit.live_runs.is_empty() && live_plain == edit.buffer {
                            (
                                coursecraft_course::normalize_runs(&edit.live_runs),
                                edit.live_paragraphs.clone(),
                            )
                        } else {
                            let old_plain: String = if text.runs.is_empty() {
                                text.text.clone()
                            } else {
                                text.runs.iter().map(|run| run.text.as_str()).collect()
                            };
                            (
                                coursecraft_course::apply_text_edit(
                                    &text.runs,
                                    &old_plain,
                                    &edit.buffer,
                                ),
                                coursecraft_course::align_paragraphs(
                                    &old_plain,
                                    &text.paragraphs,
                                    &edit.buffer,
                                ),
                            )
                        };
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
            let live_plain: String = edit.live_runs.iter().map(|r| r.text.as_str()).collect();
            let (runs, paragraphs) = if !edit.live_runs.is_empty() && live_plain == edit.buffer {
                (
                    coursecraft_course::normalize_runs(&edit.live_runs),
                    edit.live_paragraphs.clone(),
                )
            } else {
                let old_plain = if cell.runs.is_empty() {
                    cell.text.clone()
                } else {
                    cell.runs.iter().map(|run| run.text.as_str()).collect()
                };
                (
                    coursecraft_course::apply_text_edit(&cell.runs, &old_plain, &edit.buffer),
                    coursecraft_course::align_paragraphs(
                        &old_plain,
                        &cell.paragraphs,
                        &edit.buffer,
                    ),
                )
            };
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
    editor.ui.inline_edit_measured_size = None;
    editor.ui.inline_edit_live_rect = None;
}

/// Minimum height of the dedicated layer bar above the canvas scroll area
/// (26 px row + 4 px top and bottom padding).
pub const LAYER_BAR_MIN_H: f32 = 34.0;

/// Return the canvas rect within the central panel area, below the single-row layer bar.
#[must_use]
pub fn canvas_rect(central_rect: Rect) -> Rect {
    Rect::from_min_max(
        pos2(
            central_rect.min.x,
            (central_rect.min.y + LAYER_BAR_MIN_H).min(central_rect.max.y),
        ),
        central_rect.max,
    )
}

/// Compute the canvas fit rect reserving zoom HUD space.
#[must_use]
pub fn canvas_fit_rect(canvas_rect: Rect) -> Rect {
    let zoom_hud_h = 32.0;
    let margin_x = 32.0;
    let margin_top = 20.0;
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
        DragOp::Move | DragOp::RegionMove | DragOp::StateExtraMove => {
            let (dx, dy) = if shift {
                if delta_x.abs() >= delta_y.abs() {
                    (delta_x, 0.0)
                } else {
                    (0.0, delta_y)
                }
            } else {
                (delta_x, delta_y)
            };
            Rect::from_min_size(
                pos2(drag.orig_frame.min.x + dx, drag.orig_frame.min.y + dy),
                drag.orig_frame.size(),
            )
        }
        DragOp::Resize { handle, rotation } | DragOp::StateExtraResize { handle, rotation } => {
            resize_frame_rotated(
                drag.orig_frame,
                handle,
                vec2(delta_x, delta_y),
                shift,
                rotation,
            )
        }
        DragOp::RegionResize(handle) => {
            resize_frame(drag.orig_frame, handle, vec2(delta_x, delta_y), shift)
        }
        DragOp::Rotate { .. }
        | DragOp::StateExtraRotate { .. }
        | DragOp::MotionPathPoint { .. }
        | DragOp::Adjust { .. }
        | DragOp::PathPoint { .. }
        | DragOp::TableBorder { .. }
        | DragOp::Marquee => drag.orig_frame,
    }
}

/// Hit area of a resize handle in screen pixels. The drawn square stays 8 px.
pub(crate) const HANDLE_HIT_PX: f32 = 12.0;

/// Whether `handle` sits on the line of a straight line frame (under 2 slide px high or
/// wide; the line tool makes 1 px frames). Such handles are hidden and cannot be grabbed, so a drag of the
/// middle of the line moves it.
fn handle_on_line(handle: ResizeHandle, frame: Rect) -> bool {
    let flat_h = frame.height() < 2.0;
    let flat_w = frame.width() < 2.0;
    (flat_h && matches!(handle, ResizeHandle::Top | ResizeHandle::Bottom))
        || (flat_w && matches!(handle, ResizeHandle::Left | ResizeHandle::Right))
}

/// The resize cursor for `handle` on a frame turned by `rotation` degrees: the
/// handle angle plus the rotation, snapped to the nearest of the four cursors.
pub(crate) fn handle_cursor(handle: ResizeHandle, rotation: f32) -> CursorIcon {
    let base = match handle {
        ResizeHandle::Right => 0.0_f32,
        ResizeHandle::BottomRight => 45.0,
        ResizeHandle::Bottom => 90.0,
        ResizeHandle::BottomLeft => 135.0,
        ResizeHandle::Left => 180.0,
        ResizeHandle::TopLeft => 225.0,
        ResizeHandle::Top => 270.0,
        ResizeHandle::TopRight => 315.0,
    };
    let rotation = if rotation.is_finite() { rotation } else { 0.0 };
    cursor_for_angle(base + rotation)
}

/// The resize cursor along a direction of `angle` degrees (0 is rightwards,
/// clockwise on screen), snapped to the nearest of the four resize cursors.
fn cursor_for_angle(angle: f32) -> CursorIcon {
    let angle = angle.rem_euclid(180.0);
    match ((angle / 45.0).round() as i32).rem_euclid(4) {
        0 => CursorIcon::ResizeHorizontal,
        1 => CursorIcon::ResizeNwSe,
        2 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeNeSw,
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

fn draw_snap_guides(
    editor: &Editor,
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    ui: &Ui,
) {
    if editor.ui.snap_guides.is_empty() && editor.ui.spacing_guides.is_empty() {
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
    draw_spacing_guides(editor, painter, t, items, p.accent);
}

fn selection_handle_avoid_zones(
    editor: &Editor,
    t: &SlideTransform,
    items: &[DrawItem],
) -> (Option<Rect>, Vec<Rect>) {
    let ids = editor.selection.ids();
    if ids.is_empty() {
        return (None, Vec::new());
    }
    let mut bbox: Option<Rect> = None;
    for id in &ids {
        let Some(item) = items.iter().find(|it| it.hit == Hit::Object(id.clone())) else {
            continue;
        };
        let bounds = coursecraft_render::item_bounds(item);
        bbox = Some(match bbox {
            Some(current) => current.union(bounds),
            None => bounds,
        });
    }
    let Some(bbox) = bbox else {
        return (None, Vec::new());
    };
    let screen_rect = t.to_screen_rect(bbox);
    let single_item_live: Option<DrawItem> = if editor.selection.is_multi() {
        None
    } else if let Some(id) = editor.selection.object.as_ref()
        && let Some(item) = items
            .iter()
            .find(|item| item.hit == Hit::Object(id.clone()))
    {
        let mut live = item.clone();
        if let Some(live_rect) = editor.ui.inline_edit_live_rect
            && editor.ui.inline_edit.as_ref().is_some_and(|edit| {
                edit.object == *id && edit.extra_id.is_none() && edit.cell.is_none()
            })
        {
            live.rect = live_rect;
        }
        Some(live)
    } else {
        None
    };
    let single_item = single_item_live.as_ref();
    let mut centers: Vec<Pos2> = match single_item {
        Some(item) => rotated_handle_centers(t.to_screen_rect(item.rect), item.rotation)
            .into_iter()
            .map(|(_, pos)| pos)
            .collect(),
        None => get_handle_centers(screen_rect)
            .into_iter()
            .map(|(_, pos)| pos)
            .collect(),
    };
    if let Some(item) = single_item {
        centers.push(rotate_handle_center(
            t.to_screen_rect(item.rect),
            item.rotation,
        ));
    }
    let zones = centers
        .into_iter()
        .map(|center| Rect::from_center_size(center, vec2(12.0, 12.0)))
        .collect();
    (Some(screen_rect), zones)
}

fn draw_spacing_guides(
    editor: &Editor,
    painter: &Painter,
    t: &SlideTransform,
    items: &[DrawItem],
    accent: Color32,
) {
    if editor.ui.spacing_guides.is_empty() {
        return;
    }
    let stroke = Stroke::new(1.0, accent);
    let tick = 4.0;
    let (moving_screen, handle_zones) = selection_handle_avoid_zones(editor, t, items);

    for guide in &editor.ui.spacing_guides {
        let (start, end) = if guide.vertical {
            let s = t.to_screen_pos(pos2(guide.across, guide.from));
            let e = t.to_screen_pos(pos2(guide.across, guide.to));
            painter.line_segment([s, e], stroke);
            painter.line_segment([pos2(s.x - tick, s.y), pos2(s.x + tick, s.y)], stroke);
            painter.line_segment([pos2(e.x - tick, e.y), pos2(e.x + tick, e.y)], stroke);
            (s, e)
        } else {
            let s = t.to_screen_pos(pos2(guide.from, guide.across));
            let e = t.to_screen_pos(pos2(guide.to, guide.across));
            painter.line_segment([s, e], stroke);
            painter.line_segment([pos2(s.x, s.y - tick), pos2(s.x, s.y + tick)], stroke);
            painter.line_segment([pos2(e.x, e.y - tick), pos2(e.x, e.y + tick)], stroke);
            (s, e)
        };

        let gap_px = (guide.to - guide.from).abs().round() as i32;
        let galley = painter.layout_no_wrap(gap_px.to_string(), FontId::proportional(10.0), accent);
        let size = galley.size();
        let mid = pos2((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);

        let mut candidates = if guide.vertical {
            vec![
                pos2(mid.x + 5.0, mid.y - size.y * 0.5),
                pos2(mid.x - 5.0 - size.x, mid.y - size.y * 0.5),
            ]
        } else {
            vec![
                pos2(mid.x - size.x * 0.5, mid.y - 4.0 - size.y),
                pos2(mid.x - size.x * 0.5, mid.y + 4.0),
            ]
        };
        if let Some(rect) = moving_screen {
            if guide.vertical {
                candidates.push(pos2(rect.right() + 10.0, mid.y - size.y * 0.5));
                candidates.push(pos2(rect.left() - 10.0 - size.x, mid.y - size.y * 0.5));
            } else {
                candidates.push(pos2(mid.x - size.x * 0.5, rect.bottom() + 10.0));
                candidates.push(pos2(mid.x - size.x * 0.5, rect.top() - 28.0 - size.y));
            }
        }

        let label_pos = candidates
            .iter()
            .copied()
            .find(|&pos| {
                let label_rect = Rect::from_min_size(pos, size);
                !handle_zones.iter().any(|zone| zone.intersects(label_rect))
            })
            .or_else(|| candidates.last().copied())
            .unwrap_or(mid);

        painter.galley(label_pos, galley, accent);
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
/// Text colour used while editing a button label in place (tests only).
#[cfg(test)]
pub(crate) fn inline_button_label_color_for_test(
    object: &coursecraft_course::Object,
) -> Option<String> {
    inline_edit_text_props(object, None, None, None).map(|props| props.style.color.0)
}

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

/// The scroll offset that keeps the slide point under the viewport centre in place
/// after the zoom level changed (a zoom button or key). Fit has no scroll, so
/// zooming back to Fit resets it.
fn keep_centre_on_zoom(
    editor: &Editor,
    viewport: Rect,
    available: egui::Vec2,
    slide: egui::Vec2,
) -> Option<egui::Vec2> {
    let Some(scale) = editor.ui.zoom.fixed_scale() else {
        return Some(egui::Vec2::ZERO);
    };
    let slide_rect = editor.ui.last_slide_rect?;
    let before = slide_rect.width() / slide.x;
    if !before.is_finite() || before <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let in_slide = (viewport.center() - slide_rect.min) / before;
    let (content, _) = canvas_content_size(available, slide, scale);
    let origin = (content - slide * scale) * 0.5;
    Some(origin + in_slide * scale - viewport.size() * 0.5)
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

fn show_layer_bar(editor: &mut Editor, ui: &mut Ui) {
    // A master has no layers, so there is nothing to switch between or add.
    if editor.ui.master_view {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let frame_resp = egui::Frame::NONE
        .fill(p.canvas)
        .inner_margin(egui::Margin::symmetric(
            coursecraft_render::PANEL_MARGIN as i8,
            4,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|pills_ui| {
                pills_ui.spacing_mut().item_spacing = vec2(4.0, 4.0);

                let is_base = editor.selection.layer.is_none();
                let base_text = coursecraft_render::small(tl!("Base")).color(if is_base {
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
                    coursecraft_render::icon(
                        coursecraft_render::Icon::Plus,
                        coursecraft_render::ICON_SM,
                    )
                    .color(p.accent_text),
                )
                .fill(Color32::TRANSPARENT)
                .corner_radius(12)
                .min_size(vec2(24.0, 24.0));

                let plus_resp = pills_ui.add(plus_btn).on_hover_text(tl!("Add layer"));
                plus_resp.widget_info(|| {
                    let mut info = egui::WidgetInfo::new(egui::Role::Button);
                    info.label = Some(tl!("Add layer").to_owned());
                    info
                });
                if plus_resp.clicked() {
                    editor.run(
                        "layer.add",
                        serde_json::json!({ "slide": editor.selection.slide }),
                    );
                }

                if !is_base {
                    pills_ui.add_space(8.0);
                    pills_ui.add(
                        egui::Label::new(
                            coursecraft_render::small(tl!("Base slide:")).color(p.text_secondary),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    let current = editor.session.course.editor.base_slide_view;
                    for (mode, label, api_val) in [
                        (coursecraft_course::BaseSlideView::Show, tl!("Show"), "show"),
                        (coursecraft_course::BaseSlideView::Dim, tl!("Dim"), "dim"),
                        (coursecraft_course::BaseSlideView::Hide, tl!("Hide"), "hide"),
                    ] {
                        let active = current == mode;
                        let text = coursecraft_render::small(label).color(if active {
                            p.accent_text
                        } else {
                            p.text_secondary
                        });
                        let btn = egui::Button::new(text)
                            .fill(if active {
                                p.accent_subtle
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(12)
                            .min_size(vec2(0.0, 24.0));
                        if pills_ui.add(btn).clicked() && !active {
                            editor.run(
                                "view.set",
                                serde_json::json!({ "base_slide_view": api_val }),
                            );
                        }
                    }
                }
            });
        });

    let rect = frame_resp.response.rect;
    ui.painter()
        .hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, p.border));
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
                "autofit": "grow",
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
                "autoplay": true,
            }),
            (48.0, 48.0),
        ),
        PlacementTool::Video { src } => (
            serde_json::json!({
                "type": "video",
                "src": src,
                "autoplay": true,
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

fn resolved_parent_transform(parent: &Object, state_name: &str) -> (f32, bool, bool) {
    let state_def = parent.states.iter().find(|s| s.name == state_name);
    let rot = state_def
        .and_then(|s| s.rotation)
        .unwrap_or(parent.rotation);
    let flip_h = state_def.and_then(|s| s.flip_h).unwrap_or(parent.flip_h);
    let flip_v = state_def.and_then(|s| s.flip_v).unwrap_or(parent.flip_v);
    (coursecraft_course::normalize_rotation(rot), flip_h, flip_v)
}

fn rotate_point_2d(pt: Pos2, center: Pos2, degrees: f32) -> Pos2 {
    if degrees.abs() <= 0.0001 {
        return pt;
    }
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = pt.x - center.x;
    let dy = pt.y - center.y;
    pos2(
        center.x + dx * cos - dy * sin,
        center.y + dx * sin + dy * cos,
    )
}

fn flip_point_2d(pt: Pos2, center: Pos2, flip_h: bool, flip_v: bool) -> Pos2 {
    pos2(
        if flip_h { 2.0 * center.x - pt.x } else { pt.x },
        if flip_v { 2.0 * center.y - pt.y } else { pt.y },
    )
}

pub(crate) fn state_extra_world_geometry(
    parent: &Object,
    state_name: &str,
    extra: &coursecraft_course::StateExtra,
) -> (Rect, f32) {
    let parent_rect = Rect::from_min_size(
        pos2(parent.frame.x, parent.frame.y),
        vec2(parent.frame.w, parent.frame.h),
    );
    let (parent_rot, parent_flip_h, parent_flip_v) = resolved_parent_transform(parent, state_name);
    let unrotated = Rect::from_min_size(
        pos2(
            parent.frame.x + extra.frame.x,
            parent.frame.y + extra.frame.y,
        ),
        vec2(extra.frame.w.max(1.0), extra.frame.h.max(1.0)),
    );
    let parent_center = parent_rect.center();
    let extra_center = flip_point_2d(
        rotate_point_2d(unrotated.center(), parent_center, parent_rot),
        parent_center,
        parent_flip_h,
        parent_flip_v,
    );
    let world_rect = Rect::from_center_size(extra_center, unrotated.size());
    let flip_sign = if parent_flip_h ^ parent_flip_v {
        -1.0
    } else {
        1.0
    };
    let extra_rot = coursecraft_course::normalize_rotation(extra.rotation);
    let world_rot = coursecraft_course::normalize_rotation(parent_rot + flip_sign * extra_rot);
    (world_rect, world_rot)
}

fn world_to_extra_frame(
    parent: &Object,
    state_name: &str,
    world_rect: Rect,
) -> coursecraft_course::Rect {
    let parent_center = pos2(
        parent.frame.x + parent.frame.w * 0.5,
        parent.frame.y + parent.frame.h * 0.5,
    );
    let (parent_rot, parent_flip_h, parent_flip_v) = resolved_parent_transform(parent, state_name);
    let unflipped = flip_point_2d(
        world_rect.center(),
        parent_center,
        parent_flip_h,
        parent_flip_v,
    );
    let unrotated_center = rotate_point_2d(unflipped, parent_center, -parent_rot);
    let w = world_rect.width().round().max(1.0);
    let h = world_rect.height().round().max(1.0);
    coursecraft_course::Rect {
        x: (unrotated_center.x - w * 0.5 - parent.frame.x).round(),
        y: (unrotated_center.y - h * 0.5 - parent.frame.y).round(),
        w,
        h,
    }
}

fn world_to_extra_rotation(parent: &Object, state_name: &str, world_rot: f32) -> f32 {
    let (parent_rot, parent_flip_h, parent_flip_v) = resolved_parent_transform(parent, state_name);
    let flip_sign = if parent_flip_h ^ parent_flip_v {
        -1.0
    } else {
        1.0
    };
    coursecraft_course::normalize_rotation(flip_sign * (world_rot - parent_rot))
}

pub(crate) fn pick_state_extra(editor: &Editor, slide_pos: Pos2) -> Option<(String, Rect, f32)> {
    let session = editor.ui.editing_state.as_ref()?;
    let slide = editor.session.course.slide(&session.slide_id)?;
    let (parent, _) = slide.object(&session.object_id)?;
    let state = parent
        .states
        .iter()
        .find(|s| s.name == session.state_name)?;
    for extra in state.extras.iter().rev() {
        let (world_rect, world_rot) = state_extra_world_geometry(parent, &state.name, extra);
        let local_pos = rotate_point_2d(slide_pos, world_rect.center(), -world_rot);
        if world_rect.contains(local_pos) {
            return Some((extra.id.clone(), world_rect, world_rot));
        }
    }
    None
}

fn selected_state_extra_geometry(editor: &Editor) -> Option<(String, Rect, f32)> {
    let session = editor.ui.editing_state.as_ref()?;
    let extra_id = session.selected_extra.as_ref()?;
    let slide = editor.session.course.slide(&session.slide_id)?;
    let (parent, _) = slide.object(&session.object_id)?;
    let state = parent
        .states
        .iter()
        .find(|s| s.name == session.state_name)?;
    let extra = state.extras.iter().find(|ex| &ex.id == extra_id)?;
    let (world_rect, world_rot) = state_extra_world_geometry(parent, &state.name, extra);
    Some((extra.id.clone(), world_rect, world_rot))
}

/// Measure the fitted slide-space `(w, h)` of unwrapped point text using egui's
/// active font layout plus point-text padding and insets.
fn measure_point_text_box_size(
    ctx: &egui::Context,
    text: &str,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
) -> (f32, f32) {
    let text_color = coursecraft_render::parse_color(&style.color)
        .unwrap_or_else(|| coursecraft_render::palette(ctx).text);
    let probe_text = if text.is_empty() { " " } else { text };
    let probe_runs: &[coursecraft_course::TextRun] = if text.is_empty() { &[] } else { runs };
    let resolved = coursecraft_render::resolve_text_runs(
        probe_text,
        style,
        probe_runs,
        None,
        text_color,
        text_color,
        Some(theme_font),
    );
    let probe = Rect::from_min_size(
        pos2(0.0, 0.0),
        vec2(
            coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM,
            coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM,
        ),
    );
    let laid = coursecraft_render::layout_text_with_wrap(
        ctx,
        probe,
        1.0,
        &resolved,
        paragraphs,
        coursecraft_course::Align::Left,
        coursecraft_course::VAlign::Top,
        coursecraft_course::Autofit::None,
        false,
    );
    let content_w = if text.is_empty() { 0.0 } else { laid.width };
    let content_h = laid.height;
    let insets = style.insets.map_or(
        coursecraft_media::text_metrics::TextBoxInsets::ZERO,
        |ins| coursecraft_media::text_metrics::TextBoxInsets {
            left: ins.left,
            top: ins.top,
            right: ins.right,
            bottom: ins.bottom,
        },
    );
    let vertical = !matches!(
        style.direction,
        coursecraft_course::TextDirection::Horizontal
    );
    coursecraft_media::text_metrics::point_text_box_size(content_w, content_h, insets, vertical)
}

/// Anchor a refitted point-text box relative to `base` according to `align` and
/// `valign`, matching `coursecraft_engine`'s `fit_point_text_frame`.
fn anchor_point_text_rect(
    base: Rect,
    new_w: f32,
    new_h: f32,
    align: coursecraft_course::Align,
    valign: coursecraft_course::VAlign,
) -> Rect {
    let old_x = if base.min.x.is_finite() {
        base.min.x
    } else {
        0.0
    };
    let old_y = if base.min.y.is_finite() {
        base.min.y
    } else {
        0.0
    };
    let old_w = if base.width().is_finite() && base.width() > 0.0 {
        base.width()
    } else {
        new_w
    };
    let old_h = if base.height().is_finite() && base.height() > 0.0 {
        base.height()
    } else {
        new_h
    };
    let x = match align {
        coursecraft_course::Align::Left | coursecraft_course::Align::Justify => old_x,
        coursecraft_course::Align::Center => old_x + (old_w - new_w) * 0.5,
        coursecraft_course::Align::Right => old_x + (old_w - new_w),
    };
    let y = match valign {
        coursecraft_course::VAlign::Top => old_y,
        coursecraft_course::VAlign::Middle => old_y + (old_h - new_h) * 0.5,
        coursecraft_course::VAlign::Bottom => old_y + (old_h - new_h),
    };
    let fx = if x.is_finite() { x.round() } else { 0.0 };
    let fy = if y.is_finite() { y.round() } else { 0.0 };
    let fw = new_w
        .round()
        .clamp(1.0, coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM);
    let fh = new_h
        .round()
        .clamp(1.0, coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM);
    Rect::from_min_size(pos2(fx, fy), vec2(fw, fh))
}

/// Measure the fitted slide-space height of wrapped `Autofit::Grow` text using
/// egui's active font layout at the fixed box width (minus horizontal insets),
/// plus vertical padding and insets. Never smaller than one line.
fn measure_grow_text_box_height(
    ctx: &egui::Context,
    text: &str,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
    box_width: f32,
) -> f32 {
    let text_color = coursecraft_render::parse_color(&style.color)
        .unwrap_or_else(|| coursecraft_render::palette(ctx).text);
    let probe_text = if text.is_empty() { " " } else { text };
    let probe_runs: &[coursecraft_course::TextRun] = if text.is_empty() { &[] } else { runs };
    let resolved = coursecraft_render::resolve_text_runs(
        probe_text,
        style,
        probe_runs,
        None,
        text_color,
        text_color,
        Some(theme_font),
    );
    let box_w = if box_width.is_finite() && box_width > 0.0 {
        box_width.clamp(1.0, coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM)
    } else {
        coursecraft_media::text_metrics::POINT_TEXT_MIN_W
    };
    let insets = style.insets.map_or(
        coursecraft_media::text_metrics::TextBoxInsets::ZERO,
        |ins| coursecraft_media::text_metrics::TextBoxInsets {
            left: ins.left,
            top: ins.top,
            right: ins.right,
            bottom: ins.bottom,
        },
    );
    let left_ins = if insets.left.is_finite() {
        insets.left.max(0.0).min(box_w * 0.45)
    } else {
        0.0
    };
    let right_ins = if insets.right.is_finite() {
        insets.right.max(0.0).min(box_w * 0.45)
    } else {
        0.0
    };
    let inner_w = (box_w - left_ins - right_ins).max(1.0);
    let probe = Rect::from_min_size(
        pos2(0.0, 0.0),
        vec2(inner_w, coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM),
    );
    let laid = coursecraft_render::layout_text_with_wrap(
        ctx,
        probe,
        1.0,
        &resolved,
        paragraphs,
        style.align,
        coursecraft_course::VAlign::Top,
        coursecraft_course::Autofit::None,
        style.wrap,
    );
    coursecraft_media::text_metrics::grow_text_box_height(laid.height, insets)
}

/// Anchor a refitted `Autofit::Grow` box vertically relative to `base`
/// according to `valign`, keeping `x` and `width` fixed. Matches
/// `coursecraft_engine`'s `fit_grow_text_frame`.
fn anchor_grow_text_rect(base: Rect, new_h: f32, valign: coursecraft_course::VAlign) -> Rect {
    let old_x = if base.min.x.is_finite() {
        base.min.x
    } else {
        0.0
    };
    let old_y = if base.min.y.is_finite() {
        base.min.y
    } else {
        0.0
    };
    let old_w = if base.width().is_finite() && base.width() > 0.0 {
        base.width()
    } else {
        coursecraft_media::text_metrics::POINT_TEXT_MIN_W
    };
    let old_h = if base.height().is_finite() && base.height() > 0.0 {
        base.height()
    } else {
        new_h
    };
    let y = match valign {
        coursecraft_course::VAlign::Top => old_y,
        coursecraft_course::VAlign::Middle => old_y + (old_h - new_h) * 0.5,
        coursecraft_course::VAlign::Bottom => old_y + (old_h - new_h),
    };
    let fy = if y.is_finite() { y.round() } else { 0.0 };
    let fh = new_h
        .round()
        .clamp(1.0, coursecraft_media::text_metrics::POINT_TEXT_MAX_DIM);
    Rect::from_min_size(pos2(old_x, fy), vec2(old_w, fh))
}

/// Compute live `(slide_rect, measured_size)` for an actively edited point-text
/// or `Autofit::Grow` text box. Returns `None` for fixed-size (`None`/`Shrink`)
/// boxes, shapes, buttons, and table cells.
#[allow(clippy::too_many_arguments)]
fn inline_edit_live_bounds_for(
    ctx: &egui::Context,
    base_slide_rect: Rect,
    text: &str,
    original: &str,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
    point_text: bool,
    grow_text: bool,
) -> Option<(Rect, (f32, f32))> {
    if !point_text && !grow_text {
        return None;
    }
    // Measuring lays the text out; the same inputs come in on every frame, so
    // measure only when the text, its style or the box changes.
    let key = inline_bounds_key(
        ctx.pixels_per_point(),
        base_slide_rect,
        text,
        text == original,
        style,
        runs,
        paragraphs,
        theme_font,
        point_text,
        grow_text,
    );
    let cache_id = egui::Id::unique("coursecraft_inline_bounds_cache");
    let hit = ctx.data(|d| {
        d.get_temp::<InlineBoundsCache>(cache_id)
            .and_then(|cache| cache.0.iter().find(|(k, _)| *k == key).map(|(_, v)| *v))
    });
    if let Some(hit) = hit {
        return Some(hit);
    }
    INLINE_MEASURES.with(|count| count.set(count.get() + 1));
    let value = measure_inline_edit_live_bounds(
        ctx,
        base_slide_rect,
        text,
        original,
        style,
        runs,
        paragraphs,
        theme_font,
        point_text,
        grow_text,
    )?;
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<InlineBoundsCache>(cache_id);
        cache.0.retain(|(k, _)| *k != key);
        cache.0.push((key, value));
        // The frame measures the box at most a few ways (stored text, typed
        // text), so a short list is enough.
        if cache.0.len() > 6 {
            cache.0.remove(0);
        }
    });
    Some(value)
}

/// Recent in-place text box measurements, keyed by [`inline_bounds_key`].
#[derive(Clone, Default)]
struct InlineBoundsCache(Vec<(u64, InlineBounds)>);

/// The live frame of an in-place text box and its measured size.
type InlineBounds = (Rect, (f32, f32));

thread_local! {
    /// How many times the in-place text box was really measured (not served
    /// from the cache) on this thread. Tests read it.
    static INLINE_MEASURES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Number of real in-place text box measurements on this thread so far.
#[cfg(test)]
pub(crate) fn inline_measure_count() -> usize {
    INLINE_MEASURES.with(std::cell::Cell::get)
}

/// A `fmt::Write` that feeds a hasher, to hash a value through `Debug`
/// without building the string.
struct HashWriter(std::collections::hash_map::DefaultHasher);

impl std::fmt::Write for HashWriter {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        std::hash::Hasher::write(&mut self.0, text.as_bytes());
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_bounds_key(
    pixels_per_point: f32,
    base_slide_rect: Rect,
    text: &str,
    unchanged: bool,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
    point_text: bool,
    grow_text: bool,
) -> u64 {
    use std::fmt::Write as _;
    use std::hash::{Hash, Hasher};
    let mut writer = HashWriter(std::collections::hash_map::DefaultHasher::new());
    pixels_per_point.to_bits().hash(&mut writer.0);
    text.hash(&mut writer.0);
    unchanged.hash(&mut writer.0);
    theme_font.hash(&mut writer.0);
    point_text.hash(&mut writer.0);
    grow_text.hash(&mut writer.0);
    for value in [
        base_slide_rect.min.x,
        base_slide_rect.min.y,
        base_slide_rect.max.x,
        base_slide_rect.max.y,
    ] {
        value.to_bits().hash(&mut writer.0);
    }
    // Writing into a hasher cannot fail.
    let _ = write!(writer, "{style:?}|{runs:?}|{paragraphs:?}");
    writer.0.finish()
}

#[allow(clippy::too_many_arguments)]
fn measure_inline_edit_live_bounds(
    ctx: &egui::Context,
    base_slide_rect: Rect,
    text: &str,
    original: &str,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
    point_text: bool,
    grow_text: bool,
) -> Option<(Rect, (f32, f32))> {
    if point_text {
        let (fitted_w, fitted_h) =
            measure_point_text_box_size(ctx, text, style, runs, paragraphs, theme_font);
        let live_rect = anchor_point_text_rect(
            base_slide_rect,
            fitted_w,
            fitted_h,
            style.align,
            style.valign,
        );
        Some((live_rect, (fitted_w, fitted_h)))
    } else if grow_text {
        let fitted_h = measure_grow_text_box_height(
            ctx,
            text,
            style,
            runs,
            paragraphs,
            theme_font,
            base_slide_rect.width(),
        );
        // Before the author types or deletes anything (e.g. immediately after
        // dragging out a taller area text box), preserve the dragged height
        // while still expanding if the existing text overflows.
        let display_h = if text == original {
            fitted_h.max(base_slide_rect.height())
        } else {
            fitted_h
        };
        let live_rect = anchor_grow_text_rect(base_slide_rect, display_h, style.valign);
        Some((live_rect, (live_rect.width(), display_h)))
    } else {
        None
    }
}

/// Keep `inline_edit_live_rect` current BEFORE painting the slide and selection
/// handles, so entry frames and external style/text changes never lag by a frame.
fn sync_inline_edit_live_rect_before_paint(editor: &mut Editor, ctx: &egui::Context) {
    let Some(edit) = editor.ui.inline_edit.as_ref() else {
        editor.ui.inline_edit_live_rect = None;
        return;
    };
    if edit.slide != editor.selection.slide {
        return;
    }
    let Some(slide) = editor.session.course.slide(&edit.slide) else {
        return;
    };
    let Some((object, _)) = slide.object(&edit.object) else {
        return;
    };
    let Some(base_slide_rect) = inline_edit_slide_rect(object, edit) else {
        return;
    };
    let Some(props) = inline_edit_text_props(
        object,
        edit.cell,
        edit.state_name.as_deref(),
        edit.extra_id.as_deref(),
    ) else {
        return;
    };
    if !props.point_text && !props.grow_text {
        editor.ui.inline_edit_live_rect = None;
        return;
    }
    let (synced_runs, synced_paragraphs, _) =
        sync_inline_edit_runs(object, edit, editor.session.revision);
    let theme_font = editor.session.course.theme.font.as_str();
    if let Some((live_rect, measured)) = inline_edit_live_bounds_for(
        ctx,
        base_slide_rect,
        &edit.buffer,
        &edit.original,
        &props.style,
        &synced_runs,
        &synced_paragraphs,
        theme_font,
        props.point_text,
        props.grow_text,
    ) {
        editor.ui.inline_edit_live_rect = Some(live_rect);
        editor.ui.inline_edit_measured_size = Some((edit.buffer.clone(), measured));
    }
}

/// Substitute `inline_edit_live_rect` into the edited object's `DrawItem` so
/// painted text-box borders/outlines, selection outlines, and all 8 resize
/// handles stay locked to the live bounds every frame.
fn apply_inline_edit_live_rect(editor: &Editor, items: &[DrawItem]) -> Option<Vec<DrawItem>> {
    let live_rect = editor.ui.inline_edit_live_rect?;
    let edit = editor.ui.inline_edit.as_ref()?;
    if edit.cell.is_some() {
        return None;
    }
    let editing_extra = edit.extra_id.is_some();
    let expected_base = if editing_extra {
        editor
            .session
            .course
            .slide(&edit.slide)
            .and_then(|s| s.object(&edit.object))
            .and_then(|(obj, _)| inline_edit_slide_rect(obj, edit))
    } else {
        None
    };
    let mut updated = items.to_vec();
    let mut changed = false;
    for item in &mut updated {
        if item.is_state_extra != editing_extra {
            continue;
        }
        if !matches!(&item.hit, Hit::Object(id) if id == &edit.object) {
            continue;
        }
        if let Some(base) = expected_base {
            let same_extra = (item.rect.min - base.min).length() <= 1.0
                && (item.rect.max - base.max).length() <= 1.0;
            if !same_extra {
                continue;
            }
        }
        item.rect = live_rect;
        changed = true;
    }
    changed.then_some(updated)
}

/// Fitted initial frame for clicking with the Text box tool, anchored at the
/// click point vertically centred on the single line.
fn point_text_click_frame(editor: &Editor, ctx: &egui::Context, slide_pos: Pos2) -> Rect {
    let style = coursecraft_course::TextStyle {
        color: editor.session.course.theme.text.clone(),
        wrap: false,
        ..coursecraft_course::TextStyle::default()
    };
    let (w, h) = measure_point_text_box_size(
        ctx,
        "Text",
        &style,
        &[],
        &[],
        &editor.session.course.theme.font,
    );
    let slide_size = editor.session.course.slide_size;
    let slide_w = slide_size.w.max(1) as f32;
    let slide_h = slide_size.h.max(1) as f32;
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
        pos_x.round().clamp(0.0, slide_w - w)
    } else {
        0.0
    };
    let y = if h <= slide_h {
        (pos_y - h * 0.5).round().clamp(0.0, slide_h - h)
    } else {
        0.0
    };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// Create the object with `frame` through the same command a click uses.
fn place_object(editor: &mut Editor, tool: &PlacementTool, frame: Rect, is_click: bool) {
    let is_point_text = is_click && matches!(tool, PlacementTool::TextBox);
    if let Some(session) = editor.ui.editing_state.clone() {
        let Some((extra_frame, extra_rot)) = editor
            .session
            .course
            .slide(&session.slide_id)
            .and_then(|s| s.object(&session.object_id))
            .map(|(parent, _)| {
                (
                    world_to_extra_frame(parent, &session.state_name, frame),
                    world_to_extra_rotation(parent, &session.state_name, 0.0),
                )
            })
        else {
            return;
        };
        let (kind, extra_name) = match tool {
            PlacementTool::TextBox => (
                if is_point_text {
                    serde_json::json!({
                        "type": "text",
                        "text": "Text",
                        "point_text": true,
                        "style": {
                            "color": editor.session.course.theme.text.0,
                            "wrap": false,
                        }
                    })
                } else {
                    serde_json::json!({
                        "type": "text",
                        "text": "Text",
                        "autofit": "grow",
                        "style": {
                            "color": editor.session.course.theme.text.0,
                        }
                    })
                },
                "Text".to_owned(),
            ),
            PlacementTool::Shape(kind) => (
                serde_json::json!({
                    "type": "shape",
                    "shape": kind,
                    "fill": "#D0D7DE",
                }),
                shape_label(*kind).to_owned(),
            ),
            PlacementTool::Image { src, .. } => (
                serde_json::json!({
                    "type": "image",
                    "src": src,
                    "alt": "",
                }),
                "Image".to_owned(),
            ),
            _ => return,
        };
        let mut params = serde_json::json!({
            "slide": session.slide_id,
            "object": session.object_id,
            "state": session.state_name,
            "extra": {
                "name": extra_name,
                "kind": kind,
                "frame": extra_frame,
                "rotation": extra_rot,
            }
        });
        if matches!(tool, PlacementTool::TextBox)
            && frame.width().is_finite()
            && frame.height().is_finite()
            && frame.width() > 0.0
            && frame.height() > 0.0
        {
            params["measured_size"] = serde_json::json!({
                "w": frame.width(),
                "h": frame.height(),
            });
        }
        if let Some(res) = editor.run("object.state_add_extra", params)
            && let Some(id) = res.get("id").and_then(serde_json::Value::as_str)
            && let Some(active) = editor.ui.editing_state.as_mut()
        {
            active.selected_extra = Some(id.to_owned());
        }
        return;
    }
    let is_text = matches!(tool, PlacementTool::TextBox);
    let kind = if is_point_text {
        serde_json::json!({
            "type": "text",
            "text": "Text",
            "point_text": true,
            "style": {
                "color": editor.session.course.theme.text.0,
                "wrap": false,
            }
        })
    } else {
        placement_kind_and_size(editor, tool).0
    };
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
    if is_text
        && frame.width().is_finite()
        && frame.height().is_finite()
        && frame.width() > 0.0
        && frame.height() > 0.0
    {
        params["measured_size"] = serde_json::json!({
            "w": frame.width(),
            "h": frame.height(),
        });
    }
    if let PlacementTool::Shape(kind) = tool {
        params["name"] = serde_json::json!(next_shape_name(editor, *kind));
    }
    if let Some(ref lid) = editor.selection.layer {
        params["layer"] = serde_json::json!(lid);
    }
    // Audio and video play when the slide reaches them, and a base slide
    // shorter than the media grows to fit it, in the same undo step.
    let media = match tool {
        PlacementTool::Audio { src } => Some((coursecraft_playback::MediaKind::Audio, src)),
        PlacementTool::Video { src } => Some((coursecraft_playback::MediaKind::Video, src)),
        _ => None,
    };
    if let Some((kind, src)) = media {
        let fit = if editor.selection.layer.is_none() {
            let slide = editor.selection.slide.clone();
            editor.media_fit_step(&slide, kind, src)
        } else {
            None
        };
        let mut steps = vec![("object.add".to_owned(), params)];
        if let Some((step, _)) = &fit {
            steps.push(step.clone());
        }
        if editor.run_batch(&steps).is_some()
            && let Some((_, duration_ms)) = fit
        {
            editor.note_slide_fitted(kind, duration_ms);
        }
        return;
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
        subpaths: Vec::new(),
        fill: closed.then(|| theme.accent.clone()),
        fill_style: None,
        outline: coursecraft_course::Outline {
            color: theme.text.clone(),
            width: 2.0,
            dash: coursecraft_course::DashStyle::Solid,
            dash_pattern: None,
            join: coursecraft_course::LineJoin::Miter,
            miter_limit: None,
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
        text: None,
    };
    let name = next_draw_name(editor, tool.base_name());
    if let Some(session) = editor.ui.editing_state.clone() {
        let world_rect = Rect::from_min_size(pos2(frame.x, frame.y), vec2(frame.w, frame.h));
        let Some((extra_frame, extra_rot)) = editor
            .session
            .course
            .slide(&session.slide_id)
            .and_then(|s| s.object(&session.object_id))
            .map(|(parent, _)| {
                (
                    world_to_extra_frame(parent, &session.state_name, world_rect),
                    world_to_extra_rotation(parent, &session.state_name, 0.0),
                )
            })
        else {
            editor.ui.placement_tool = None;
            editor.ui.draw = None;
            return;
        };
        let mut extra_path = path;
        extra_path.connector = None;
        extra_path.start_glue = None;
        extra_path.end_glue = None;
        let Ok(path_json) = serde_json::to_value(&extra_path) else {
            editor.ui.placement_tool = None;
            editor.ui.draw = None;
            return;
        };
        let mut kind_json = path_json;
        if let Some(obj) = kind_json.as_object_mut() {
            obj.insert("type".to_owned(), serde_json::json!("path"));
        }
        let result = editor.run(
            "object.state_add_extra",
            serde_json::json!({
                "slide": session.slide_id,
                "object": session.object_id,
                "state": session.state_name,
                "extra": {
                    "name": name,
                    "kind": kind_json,
                    "frame": extra_frame,
                    "rotation": extra_rot,
                }
            }),
        );
        editor.ui.placement_tool = None;
        editor.ui.draw = None;
        if let Some(id) = result
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(serde_json::Value::as_str)
            && let Some(active) = editor.ui.editing_state.as_mut()
        {
            active.selected_extra = Some(id.to_owned());
        }
        return;
    }
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
            let frame = if matches!(tool, PlacementTool::TextBox) {
                point_text_click_frame(editor, ui.ctx(), drag.start_slide)
            } else {
                let (_, (w, h)) = placement_kind_and_size(editor, &tool);
                default_placement_frame(editor, drag.start_slide, w, h)
            };
            place_object(editor, &tool, frame, true);
        } else if let Some((frame, _)) = placed_frame {
            place_object(editor, &tool, frame, false);
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

fn is_locked(editor: &Editor, id: &str) -> bool {
    editor
        .session
        .course
        .slide(&editor.selection.slide)
        .and_then(|s| s.object(id))
        .is_some_and(|(o, _)| o.locked)
}

fn pick_object(
    course: &Course,
    selection: &crate::Selection,
    slide_pos: Pos2,
    items: &[DrawItem],
    scale: f32,
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
            && !slide.object(id).is_some_and(|(o, _)| o.locked)
            && coursecraft_render::item_hit_test_scaled(item, slide_pos, scale)
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
    if let Some(session) = editor.ui.editing_state.clone() {
        if is_locked(editor, &session.object_id) {
            return;
        }
        if let Some((extra_id, world_rect, world_rot)) = selected_state_extra_geometry(editor) {
            let screen_rect = t.to_screen_rect(world_rect);
            let rot_handle = rotate_handle_center(screen_rect, world_rot);
            if Rect::from_center_size(rot_handle, vec2(14.0, 14.0)).contains(start_pos) {
                let centre = screen_rect.center();
                let start_angle = (start_pos.y - centre.y).atan2(start_pos.x - centre.x);
                editor.ui.canvas_drag = Some(CanvasDrag {
                    op: DragOp::StateExtraRotate {
                        centre,
                        start_angle,
                        orig_rotation: world_rot,
                    },
                    object_id: extra_id,
                    orig_frame: world_rect,
                    start_screen: start_pos,
                    frames: Vec::new(),
                    alt_duplicate: false,
                });
                return;
            }
            for (h, c) in rotated_handle_centers(screen_rect, world_rot) {
                if Rect::from_center_size(c, vec2(HANDLE_HIT_PX, HANDLE_HIT_PX)).contains(start_pos)
                {
                    editor.ui.canvas_drag = Some(CanvasDrag {
                        op: DragOp::StateExtraResize {
                            handle: h,
                            rotation: world_rot,
                        },
                        object_id: extra_id,
                        orig_frame: world_rect,
                        start_screen: start_pos,
                        frames: Vec::new(),
                        alt_duplicate: false,
                    });
                    return;
                }
            }
        }
        if t.slide_rect().contains(start_pos) {
            let slide_pos = t.to_slide_pos(start_pos);
            if let Some((extra_id, world_rect, _)) = pick_state_extra(editor, slide_pos) {
                if let Some(active) = editor.ui.editing_state.as_mut() {
                    active.selected_extra = Some(extra_id.clone());
                }
                editor.ui.canvas_drag = Some(CanvasDrag {
                    op: DragOp::StateExtraMove,
                    object_id: extra_id.clone(),
                    orig_frame: world_rect,
                    start_screen: start_pos,
                    frames: vec![(extra_id, world_rect)],
                    alt_duplicate: false,
                });
                return;
            }
        }
        if session.selected_extra.is_none()
            && let Some(item) = items.iter().find(|item| {
                !item.is_state_extra && item.hit == Hit::Object(session.object_id.clone())
            })
        {
            let screen_rect = t.to_screen_rect(item.rect);
            let handle = rotate_handle_center(screen_rect, item.rotation);
            if Rect::from_center_size(handle, vec2(14.0, 14.0)).contains(start_pos) {
                let centre = screen_rect.center();
                let start_angle = (start_pos.y - centre.y).atan2(start_pos.x - centre.x);
                editor.ui.canvas_drag = Some(CanvasDrag {
                    op: DragOp::Rotate {
                        centre,
                        start_angle,
                        orig_rotation: item.rotation,
                    },
                    object_id: session.object_id.clone(),
                    orig_frame: item.rect,
                    start_screen: start_pos,
                    frames: Vec::new(),
                    alt_duplicate: false,
                });
            }
        }
        return;
    }
    // A locked object has no canvas handles: pressing the canvas leaves it.
    if editor
        .selection
        .ids()
        .iter()
        .any(|id| is_locked(editor, id))
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
                let point_slide = pos2(
                    obj.frame.x + point.x * obj.frame.w,
                    obj.frame.y + point.y * obj.frame.h,
                );
                let centre = t.to_screen_pos(point_slide);
                let hit_rect = Rect::from_center_size(centre, vec2(12.0, 12.0));
                if hit_rect.contains(start_pos) {
                    if editing && let Some(edit) = editor.ui.path_edit.as_mut() {
                        edit.selected_point = Some(index);
                    }
                    let start_slide = t.to_slide_pos(start_pos);
                    let press_offset = if finite_pos(start_slide) {
                        point_slide - start_slide
                    } else {
                        egui::Vec2::ZERO
                    };
                    path_point_hit = Some((
                        sel_id.clone(),
                        index,
                        path.points.clone(),
                        path.points.len() == 2,
                        Rect::from_min_size(
                            pos2(obj.frame.x, obj.frame.y),
                            vec2(obj.frame.w, obj.frame.h),
                        ),
                        press_offset,
                    ));
                    break;
                }
            }
        }
    }
    if let Some((id, index, original, two_point, orig_frame, press_offset)) = path_point_hit {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::PathPoint {
                index,
                original,
                two_point,
                press_offset,
            },
            object_id: id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
            alt_duplicate: false,
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
                    let start_slide = t.to_slide_pos(start_pos);
                    let press_offset = if finite_pos(start_slide) {
                        slide_pt - start_slide
                    } else {
                        egui::Vec2::ZERO
                    };
                    path_handle_hit = Some((
                        sel_id.clone(),
                        path.id.clone(),
                        idx,
                        path.points.clone(),
                        path.relative,
                        pos2(obj.frame.x, obj.frame.y),
                        press_offset,
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
            alt_duplicate: false,
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
            let handle_slide = pos2(obj.frame.x + x, obj.frame.y + y);
            let screen = t.to_screen_pos(handle_slide);
            let hit_rect = Rect::from_center_size(screen, vec2(12.0, 12.0));
            if hit_rect.contains(start_pos) {
                let start_slide = t.to_slide_pos(start_pos);
                let press_offset = if finite_pos(start_slide) {
                    handle_slide - start_slide
                } else {
                    egui::Vec2::ZERO
                };
                adjust_handle_hit = Some((sel_id.clone(), index, adjust.clone(), press_offset));
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
            if handle_on_line(h, orig) {
                continue;
            }
            let h_rect = Rect::from_center_size(c, vec2(HANDLE_HIT_PX, HANDLE_HIT_PX));
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
                let h_rect = Rect::from_center_size(c, vec2(HANDLE_HIT_PX, HANDLE_HIT_PX));
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

    if let Some((sel_id, path_id, point_index, orig_points, relative, obj_origin, press_offset)) =
        path_handle_hit
    {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::MotionPathPoint {
                path_id,
                point_index,
                orig_points,
                relative,
                obj_origin,
                press_offset,
            },
            object_id: sel_id,
            orig_frame: Rect::ZERO,
            start_screen: start_pos,
            frames: Vec::new(),
            alt_duplicate: false,
        });
    } else if let Some((sel_id, index, original, press_offset)) = adjust_handle_hit {
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
            op: DragOp::Adjust {
                index,
                original,
                press_offset,
            },
            object_id: sel_id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
            alt_duplicate: false,
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
            alt_duplicate: false,
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
            alt_duplicate: false,
        });
    } else if let Some((h, region_id, orig_frame)) = region_handle_hit {
        editor.ui.canvas_drag = Some(CanvasDrag {
            op: DragOp::RegionResize(h),
            object_id: region_id,
            orig_frame,
            start_screen: start_pos,
            frames: Vec::new(),
            alt_duplicate: false,
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
            alt_duplicate: false,
        });
    } else if t.slide_rect().contains(start_pos) {
        let slide_pos = t.to_slide_pos(start_pos);
        if let Some((id, orig_frame)) = pick_object(
            &editor.session.course,
            &editor.selection,
            slide_pos,
            items,
            t.scale(),
        ) {
            let additive = modifiers.shift || modifiers.command;
            if additive && !editor.selection.contains(&id) {
                // Add on release (a click), so the press does not replace the selection.
                return;
            }
            // A modifier press on a selected object starts a move of the selection.
            // A click without a drag toggles it on release instead.
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
                alt_duplicate: modifiers.alt,
            });
        } else {
            editor.ui.canvas_drag = Some(CanvasDrag {
                op: DragOp::Marquee,
                object_id: String::new(),
                orig_frame: Rect::ZERO,
                start_screen: start_pos,
                frames: Vec::new(),
                alt_duplicate: false,
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
            alt_duplicate: false,
        });
    }
}

pub(crate) fn apply_rotate_preview(items: &mut [DrawItem], object_id: &str, rotation: f32) {
    for item in items.iter_mut() {
        if item.hit == Hit::Object(object_id.to_owned()) && !item.is_state_extra {
            item.rotation = rotation;
        }
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
    let is_move = (curr_screen - drag.start_screen).length() >= 3.0;

    if let DragOp::PathPoint {
        index,
        ref original,
        two_point,
        press_offset,
    } = drag.op
    {
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if !is_move {
                return;
            }
        } else if !is_move {
            return;
        }
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
        if let Some(slide_pos) = current_pointer.map(|pos| t.to_slide_pos(pos) + press_offset)
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
        press_offset,
    } = drag.op
    {
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if !is_move {
                return;
            }
        } else if !is_move {
            return;
        }
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
        let slide_point = t.to_slide_pos(curr_screen) + press_offset;
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
        press_offset,
    } = drag.op
    {
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if is_move {
                let current_slide_pos = t.to_slide_pos(curr_screen) + press_offset;
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
        }
        return;
    }

    if let DragOp::StateExtraRotate {
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
            if (rotation - orig).abs() > 0.001
                && let Some(session) = editor.ui.editing_state.clone()
                && let Some(parent) = editor
                    .session
                    .course
                    .slide(&session.slide_id)
                    .and_then(|s| s.object(&session.object_id))
                    .map(|(o, _)| o.clone())
            {
                let extra_rot = world_to_extra_rotation(&parent, &session.state_name, rotation);
                editor.run(
                    "object.state_update_extra",
                    serde_json::json!({
                        "slide": session.slide_id,
                        "object": session.object_id,
                        "state": session.state_name,
                        "extra": drag.object_id,
                        "patch": {
                            "rotation": extra_rot,
                        }
                    }),
                );
            }
        }
        return;
    }

    if matches!(
        drag.op,
        DragOp::StateExtraMove | DragOp::StateExtraResize { .. }
    ) {
        let scale = t.scale();
        let preview_rect = if is_move {
            compute_drag_frame(&drag, curr_screen, shift_held, scale)
        } else {
            drag.orig_frame
        };
        if primary_released || !primary_down {
            editor.ui.canvas_drag = None;
            if is_move
                && let Some(session) = editor.ui.editing_state.clone()
                && let Some(parent) = editor
                    .session
                    .course
                    .slide(&session.slide_id)
                    .and_then(|s| s.object(&session.object_id))
                    .map(|(o, _)| o.clone())
            {
                let new_frame = world_to_extra_frame(&parent, &session.state_name, preview_rect);
                let orig_frame =
                    world_to_extra_frame(&parent, &session.state_name, drag.orig_frame);
                if new_frame != orig_frame {
                    editor.run(
                        "object.state_update_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": drag.object_id,
                            "patch": {
                                "frame": {
                                    "x": new_frame.x,
                                    "y": new_frame.y,
                                    "w": new_frame.w,
                                    "h": new_frame.h,
                                }
                            }
                        }),
                    );
                }
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
                if let Some(session) = editor.ui.editing_state.clone()
                    && session.object_id == drag.object_id
                {
                    editor.run(
                        "object.patch_state",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "name": session.state_name,
                            "patch": {
                                "rotation": rotation,
                            },
                        }),
                    );
                } else {
                    editor.run(
                        "object.set_rotation",
                        serde_json::json!({
                            "slide": editor.selection.slide,
                            "object": drag.object_id,
                            "degrees": rotation,
                        }),
                    );
                }
            }
        } else {
            apply_rotate_preview(items, &drag.object_id, rotation);
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
                            && !slide.object(id).is_some_and(|(o, _)| o.locked)
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
    let lines = crate::rulers::snap_lines(&editor.session.course.editor);
    if crate::snap::snapping_active(editor.ui.snap_to_guides || !lines.is_empty(), command_held)
        && scale > 0.0
    {
        let threshold = 6.0 / scale;
        let targets = snap_targets(editor, &drag.frames);
        if is_move {
            if let Some(bbox) = frames_rotated_bbox(editor, &preview_frames) {
                let mut result = crate::snap::snap_rect_lines(
                    bbox,
                    &targets,
                    slide_space_rect(editor),
                    threshold,
                    editor.ui.snap_to_guides,
                    &lines,
                );
                let spacing_guides = if editor.ui.snap_to_guides
                    && !(shift_held && matches!(drag.op, DragOp::Move))
                {
                    let spacing = crate::snap::snap_spacing(bbox, &targets, threshold);
                    crate::snap::apply_spacing_fallback(&mut result, spacing)
                } else {
                    Vec::new()
                };
                if shift_held && matches!(drag.op, DragOp::Move) {
                    let delta_x = (curr_screen.x - drag.start_screen.x).abs();
                    let delta_y = (curr_screen.y - drag.start_screen.y).abs();
                    if delta_x >= delta_y {
                        result.offset.y = 0.0;
                        result.guides.retain(|guide| guide.vertical);
                    } else {
                        result.offset.x = 0.0;
                        result.guides.retain(|guide| !guide.vertical);
                    }
                }
                if result.offset != vec2(0.0, 0.0) {
                    for (_, frame) in &mut preview_frames {
                        *frame = Rect::from_min_max(
                            frame.min + result.offset,
                            frame.max + result.offset,
                        );
                    }
                }
                editor.ui.snap_guides = result.guides;
                editor.ui.spacing_guides = spacing_guides;
            }
        } else if is_resize && drag.frames.len() == 1 && !shift_held {
            editor.ui.spacing_guides.clear();
            // Snapping moves edges on their own, which would break Shift's proportions.
            if let (Some(raw), &DragOp::Resize { handle, rotation }) = (resize_bbox, &drag.op) {
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
            editor.ui.spacing_guides.clear();
        }
    } else {
        editor.ui.snap_guides.clear();
        editor.ui.spacing_guides.clear();
    }

    if primary_released || !primary_down {
        editor.ui.canvas_drag = None;
        editor.ui.snap_guides.clear();
        editor.ui.spacing_guides.clear();

        if is_move {
            let slide = editor.selection.slide.clone();
            if drag.alt_duplicate && matches!(drag.op, DragOp::Move) {
                if let Some(((_, orig), (_, preview))) =
                    drag.frames.iter().zip(&preview_frames).next()
                {
                    let dx = (preview.min.x - orig.min.x).round();
                    let dy = (preview.min.y - orig.min.y).round();
                    if dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0) {
                        let ids: Vec<String> =
                            drag.frames.iter().map(|(id, _)| id.clone()).collect();
                        if let Some(res) = editor.run(
                            "object.duplicate",
                            serde_json::json!({
                                "slide": slide,
                                "objects": ids,
                                "dx": dx,
                                "dy": dy,
                            }),
                        ) {
                            let new_ids: Vec<String> = res
                                .get("ids")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(str::to_owned))
                                        .collect()
                                })
                                .unwrap_or_default();
                            if !new_ids.is_empty() {
                                editor.selection.set_many(new_ids);
                            }
                        }
                    }
                }
            } else {
                let mut steps: Vec<(String, Json)> = Vec::new();
                for ((id, orig), (_, preview)) in drag.frames.iter().zip(&preview_frames) {
                    let fx = preview.min.x.round();
                    let fy = preview.min.y.round();
                    let fw = preview.width().round().max(4.0);
                    let fh = preview.height().round().max(4.0);
                    if !fx.is_finite() || !fy.is_finite() || !fw.is_finite() || !fh.is_finite() {
                        continue;
                    }
                    if fx != orig.min.x
                        || fy != orig.min.y
                        || fw != orig.width()
                        || fh != orig.height()
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
        }
    } else if is_move && !drag.alt_duplicate {
        for (id, preview) in &preview_frames {
            for it in items.iter_mut() {
                if it.hit == Hit::Object(id.clone()) {
                    it.rect = *preview;
                }
            }
        }
    } else if is_move && matches!(drag.op, DragOp::Move) {
        // Alt+drag: the originals stay; outline where the copies will land.
        let accent = coursecraft_render::palette(ui.ctx()).accent;
        let mut painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("canvas_duplicate_ghost"),
        ));
        painter.set_clip_rect(ui.clip_rect());
        for shape in duplicate_ghost_shapes(t, &preview_frames, accent) {
            painter.add(shape);
        }
    }
}

/// Dashed outlines, in screen space, of the frames the Alt+drag copies will get.
fn duplicate_ghost_shapes(
    t: &SlideTransform,
    frames: &[(String, Rect)],
    color: Color32,
) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    for (_, frame) in frames {
        let rect = t.to_screen_rect(*frame);
        if !rect.min.x.is_finite() || !rect.max.x.is_finite() {
            continue;
        }
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ];
        shapes.extend(egui::Shape::dashed_line(
            &corners,
            Stroke::new(1.5, color),
            6.0,
            4.0,
        ));
    }
    shapes
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
            slide_pos,
            items,
            t.scale(),
        )
        .map(|(id, _)| id)
    } else {
        None
    };
    if let Some(session) = editor.ui.editing_state.clone() {
        if inside && editor.ui.canvas_context_pos.is_some() {
            if let Some((extra_id, _, _)) = pick_state_extra(editor, slide_pos) {
                if let Some(active) = editor.ui.editing_state.as_mut() {
                    active.selected_extra = Some(extra_id);
                }
            } else if picked.as_deref() == Some(session.object_id.as_str())
                && let Some(active) = editor.ui.editing_state.as_mut()
            {
                active.selected_extra = None;
            }
        }
        return;
    }
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
    } else if editor
        .ui
        .editing_state
        .as_ref()
        .is_some_and(|s| s.selected_extra.is_some())
    {
        crate::menu::show_state_extra_menu(editor, ui);
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
    if let Some(session) = editor.ui.editing_state.clone() {
        if t.slide_rect().contains(pos) {
            let slide_pos = t.to_slide_pos(pos);
            if let Some((extra_id, _, _)) = pick_state_extra(editor, slide_pos) {
                if let Some(active) = editor.ui.editing_state.as_mut() {
                    active.selected_extra = Some(extra_id);
                }
                return;
            }
            let picked = pick_object(
                &editor.session.course,
                &editor.selection,
                slide_pos,
                items,
                t.scale(),
            )
            .map(|(id, _)| id);
            if picked.as_deref() == Some(session.object_id.as_str())
                && let Some(active) = editor.ui.editing_state.as_mut()
            {
                active.selected_extra = None;
            }
        }
        return;
    }
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
            slide_pos,
            items,
            t.scale(),
        )
        .map(|(id, _)| id);
        if let Some(ref session) = editor.ui.editing_state
            && picked.as_deref() != Some(session.object_id.as_str())
        {
            return;
        }
        if editor.ui.format_painter.is_some() {
            if let Some(id) = picked {
                let targets = if editor.animation_painter_armed()
                    && editor.ui.group_focus.is_none()
                    && editor
                        .selection
                        .group_members(&editor.session.course, &id)
                        .len()
                        >= 2
                    && let Some(gid) = editor
                        .session
                        .course
                        .slide(&editor.selection.slide)
                        .and_then(|s| s.object(&id))
                        .and_then(|(o, _)| o.group.clone())
                {
                    vec![gid]
                } else {
                    editor.selection.group_members(&editor.session.course, &id)
                };
                editor.apply_format_painter(&targets);
            } else if editor
                .ui
                .format_painter
                .as_ref()
                .is_some_and(|fp| !fp.sticky)
            {
                editor.ui.format_painter = None;
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
                if editor.selection.is_multi() {
                    // A click without a drag on one object of a multi-selection
                    // narrows the selection to that object (or its group).
                    let members = editor.selection.group_members(&editor.session.course, &id);
                    let ids = editor.selection.ids();
                    let same = members.len() == ids.len()
                        && ids.iter().all(|other| members.contains(other));
                    if !same {
                        editor.select_object_group(&id);
                        return;
                    }
                }
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
    } else if editor.ui.format_painter.is_some() {
        if editor
            .ui
            .format_painter
            .as_ref()
            .is_some_and(|fp| !fp.sticky)
        {
            editor.ui.format_painter = None;
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
        if let Some(session) = editor.ui.editing_state.clone()
            && let Some((extra_id, _, _)) = pick_state_extra(editor, slide_pos)
        {
            if let Some(active) = editor.ui.editing_state.as_mut() {
                active.selected_extra = Some(extra_id.clone());
            }
            start_extra_inline_edit(
                editor,
                &session.slide_id,
                &session.object_id,
                &session.state_name,
                &extra_id,
            );
            return;
        }
        let picked = pick_object(
            &editor.session.course,
            &editor.selection,
            slide_pos,
            items,
            t.scale(),
        )
        .map(|(id, _)| id);
        if let Some(ref session) = editor.ui.editing_state
            && picked.as_deref() != Some(session.object_id.as_str())
        {
            return;
        }
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

/// How the picture being cropped is turned on the slide: rotation, then flips,
/// both about the centre of its frame. Crop mode edits the picture in its own
/// upright frame, so pointer positions go through [`Self::local_point`] and the
/// overlay is drawn through [`Self::world_point`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct CropXform {
    pub(crate) pivot: Pos2,
    pub(crate) rotation: f32,
    pub(crate) flip_h: bool,
    pub(crate) flip_v: bool,
}

impl CropXform {
    fn is_identity(&self) -> bool {
        self.rotation == 0.0 && !self.flip_h && !self.flip_v
    }

    /// Upright picture frame position to its place on the slide.
    pub(crate) fn world_point(&self, point: Pos2) -> Pos2 {
        if self.is_identity() {
            return point;
        }
        let turned = coursecraft_render::rotate_point(point, self.pivot, self.rotation);
        coursecraft_render::flip_point(turned, self.pivot, self.flip_h, self.flip_v)
    }

    /// Slide position to the upright picture frame.
    pub(crate) fn local_point(&self, point: Pos2) -> Pos2 {
        if self.is_identity() {
            return point;
        }
        let flipped = coursecraft_render::flip_point(point, self.pivot, self.flip_h, self.flip_v);
        coursecraft_render::rotate_point(flipped, self.pivot, -self.rotation)
    }

    /// The slide frame of an upright `frame`: same size, centre moved to its
    /// place on the slide.
    pub(crate) fn world_frame(&self, frame: Rect) -> Rect {
        Rect::from_center_size(self.world_point(frame.center()), frame.size())
    }
}

/// The transform of the object in the crop session. Identity when the object is gone.
fn crop_xform(editor: &Editor) -> CropXform {
    let identity = CropXform {
        pivot: Pos2::ZERO,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
    };
    let Some(session) = editor.ui.crop_edit.as_ref() else {
        return identity;
    };
    editor
        .session
        .course
        .slide(&session.slide)
        .and_then(|slide| slide.object(&session.object))
        .map_or(identity, |(object, _)| CropXform {
            pivot: pos2(
                object.frame.x + object.frame.w * 0.5,
                object.frame.y + object.frame.h * 0.5,
            ),
            rotation: if object.rotation.is_finite() {
                object.rotation
            } else {
                0.0
            },
            flip_h: object.flip_h,
            flip_v: object.flip_v,
        })
}

/// A screen position on the slide, as the matching screen position in the
/// upright picture frame that crop mode edits in.
fn crop_local_screen(editor: &Editor, t: &SlideTransform, screen: Pos2) -> Pos2 {
    let xform = crop_xform(editor);
    if xform.is_identity() {
        return screen;
    }
    t.to_screen_pos(xform.local_point(t.to_slide_pos(screen)))
}

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
    let pointer = crop_local_screen(editor, t, pointer);
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
    editor.ui.spacing_guides.clear();
}

/// Press handling in crop mode: start a handle or source drag, or leave on a
/// click outside the picture. Returns true when the press was used.
fn crop_press(editor: &mut Editor, pos: Pos2, t: &SlideTransform) -> bool {
    let Some((session, kept)) = crop_screen_rect(editor, t) else {
        return false;
    };
    let pos = crop_local_screen(editor, t, pos);
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
    let xform = crop_xform(editor);
    let full = session.edit.source_overlay();
    let whole = t.to_screen_rect(full);
    // Corners of an upright screen rect, turned to where the picture is drawn.
    let world_corners = |rect: Rect| -> [Pos2; 4] {
        [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ]
        .map(|corner| t.to_screen_pos(xform.world_point(t.to_slide_pos(corner))))
    };

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
    let dim = Color32::from_rgba_unmultiplied(0, 0, 0, 150);
    let whole_quad = world_corners(whole);
    let kept_quad = world_corners(kept);
    if let Some(texture) = texture {
        paint_quad_image(painter, texture, whole_quad, full_uv);
        painter.add(egui::Shape::convex_polygon(
            whole_quad.to_vec(),
            dim,
            Stroke::NONE,
        ));
        paint_quad_image(painter, texture, kept_quad, kept_uv);
    } else {
        painter.add(egui::Shape::convex_polygon(
            whole_quad.to_vec(),
            dim,
            Stroke::NONE,
        ));
    }

    let dashed_corners = [
        whole_quad[0],
        whole_quad[1],
        whole_quad[2],
        whole_quad[3],
        whole_quad[0],
    ];
    painter.extend(egui::Shape::dashed_line(
        &dashed_corners,
        Stroke::new(1.0, p.text_tertiary),
        6.0,
        4.0,
    ));
    painter.add(egui::Shape::closed_line(
        kept_quad.to_vec(),
        Stroke::new(2.0, p.accent),
    ));

    let pointer = ui
        .input(|i| i.pointer.interact_pos())
        .map(|pos| crop_local_screen(editor, t, pos));
    if editor.ui.crop_drag.is_none()
        && let Some(pos) = pointer
        && let Some(handle) = crate::crop::CropHandle::at(kept, pos, CROP_HANDLE_HIT_PX)
    {
        let horizontal = handle.moves_left() || handle.moves_right();
        let vertical = handle.moves_top() || handle.moves_bottom();
        let base = if horizontal && !vertical {
            0.0_f32
        } else if vertical && !horizontal {
            90.0
        } else if handle.moves_left() == handle.moves_top() {
            45.0
        } else {
            135.0
        };
        let mut angle = base + xform.rotation;
        if xform.flip_h != xform.flip_v {
            angle = -angle;
        }
        ui.ctx().set_cursor_icon(cursor_for_angle(angle));
    }

    for (_, center) in crate::crop::CropHandle::positions(kept) {
        let center = t.to_screen_pos(xform.world_point(t.to_slide_pos(center)));
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

/// Paint `uv` of a texture onto a (possibly turned or mirrored) screen quad,
/// corners in the order top-left, top-right, bottom-right, bottom-left.
fn paint_quad_image(painter: &Painter, texture: egui::TextureId, quad: [Pos2; 4], uv: Rect) {
    let uvs = [
        uv.left_top(),
        uv.right_top(),
        uv.right_bottom(),
        uv.left_bottom(),
    ];
    let mut mesh = egui::Mesh::with_texture(texture);
    for (pos, uv) in quad.into_iter().zip(uvs) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

fn align_for(align: coursecraft_course::Align) -> egui::Align {
    match align {
        coursecraft_course::Align::Left | coursecraft_course::Align::Justify => egui::Align::LEFT,
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
pub(crate) fn apply_canvas_state_preview(
    items: &mut Vec<DrawItem>,
    obj: &coursecraft_course::Object,
    preview_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&coursecraft_course::Value>,
) {
    let preview_items = coursecraft_render::compose_object_items(obj, preview_state, theme, value);
    if preview_items.is_empty() {
        return;
    }
    let matches_obj =
        |it: &DrawItem| matches!(&it.hit, Hit::Object(id) | Hit::DragItem(id) if id == &obj.id);
    if let Some(pos) = items.iter().position(matches_obj) {
        let mut insert_at = pos;
        let mut idx = 0usize;
        items.retain(|it| {
            let keep = !matches_obj(it);
            if !keep && idx < pos {
                insert_at = insert_at.saturating_sub(1);
            }
            idx += 1;
            keep
        });
        items.splice(insert_at..insert_at, preview_items);
    } else {
        items.extend(preview_items);
    }
}

fn canvas_media_fingerprint(media: &[(String, std::sync::Arc<[u8]>)]) -> (usize, u64) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, bytes) in media {
        for &b in name.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        let ptr = bytes.as_ptr() as usize as u64;
        hash ^= ptr ^ (bytes.len() as u64);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (media.len(), hash)
}

pub(crate) fn missing_images_as_placeholders(
    items: &mut [DrawItem],
    media: &[(String, std::sync::Arc<[u8]>)],
    media_index: Option<&std::collections::HashMap<String, usize>>,
) {
    for item in items {
        if let Visual::Image { src, .. } = &item.visual {
            let exists = match media_index {
                Some(index) => match index.get(src.as_str()) {
                    Some(&idx) => {
                        media.get(idx).is_some_and(|(name, _)| name == src)
                            || media.iter().any(|(name, _)| name == src)
                    }
                    None => false,
                },
                None => media.iter().any(|(name, _)| name == src),
            };
            if !exists {
                item.visual = Visual::ImagePlaceholder {
                    label: "No image yet".to_owned(),
                };
            }
        }
    }
}

/// Blank the object's own text while it is edited in place, so the old text
/// doesn't show through the transparent editor.
fn hide_edited_text(items: &mut [DrawItem], edit: &InlineEdit) {
    let editing_extra = edit.extra_id.is_some();
    for item in items {
        if item.is_state_extra != editing_extra
            || !matches!(&item.hit, Hit::Object(id) if id == &edit.object)
        {
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

/// Styles the selected characters of an inline edit inside a state and writes the state.
fn apply_state_inline_style(
    editor: &mut Editor,
    edit: &InlineEdit,
    buffer: &str,
    char_range: (usize, usize),
    patch: &coursecraft_course::TextRunPatch,
    style: &coursecraft_course::TextStyle,
) {
    let typed = if buffer == edit.buffer {
        edit.live_runs.clone()
    } else {
        coursecraft_course::apply_text_edit(&edit.live_runs, &edit.buffer, buffer)
    };
    let mut next = edit.clone();
    next.live_runs = coursecraft_course::apply_style_to_range_with_base(
        &typed,
        buffer,
        char_range,
        patch,
        Some(style),
    );
    next.live_paragraphs =
        coursecraft_course::align_paragraphs(&edit.buffer, &edit.live_paragraphs, buffer);
    next.buffer = buffer.to_owned();
    let Some(object) = editor
        .session
        .course
        .slide(&edit.slide)
        .and_then(|slide| slide.object(&edit.object))
        .map(|(obj, _)| obj.clone())
    else {
        return;
    };
    let Some(state_name) = edit.state_name.clone() else {
        return;
    };
    match edit.extra_id.clone() {
        Some(extra_id) => commit_state_extra_inline_edit(
            editor,
            next.clone(),
            &object,
            state_name,
            extra_id,
            None,
        ),
        None => commit_state_inline_edit(editor, &next, &object, &state_name),
    }
    if let Some(live) = editor.ui.inline_edit.as_mut() {
        live.original = next.buffer.clone();
        live.buffer = next.buffer;
        live.live_runs = next.live_runs;
        live.live_paragraphs = next.live_paragraphs;
    }
}

/// Draw the in-place editor over the object's frame and commit or cancel it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_inline_edit_style_shortcut(
    editor: &mut Editor,
    edit: &InlineEdit,
    buffer: &str,
    cursor_range: Option<egui::text::CCursorRange>,
    toggle_bold: bool,
    toggle_italic: bool,
    toggle_underline: bool,
) {
    let Some(cr) = cursor_range else {
        return;
    };
    let sorted = cr.as_sorted_char_range();
    let is_selection = !cr.is_empty() && sorted.start.0 != sorted.end.0;
    let char_range = (sorted.start.0, sorted.end.0);
    // Inside a state the text, runs and style are the state's own, and nothing may reach the
    // base object (`N85`).
    let state_parts = editor
        .session
        .course
        .slide(&edit.slide)
        .and_then(|slide| slide.object(&edit.object))
        .and_then(|(obj, _)| state_inline_parts(obj, edit));
    if edit.state_name.is_some() && state_parts.is_none() {
        return;
    }
    let eff = editor
        .session
        .course
        .slide(&edit.slide)
        .and_then(|slide| slide.object(&edit.object))
        .and_then(|(obj, _)| match (&obj.kind, edit.cell) {
            _ if state_parts.is_some() => state_parts.as_ref().map(|(_, _, _, style)| {
                if is_selection {
                    crate::properties::style_for_range(&edit.live_runs, style, char_range)
                } else {
                    crate::properties::style_at_cursor(&edit.live_runs, style, char_range.0)
                }
            }),
            (coursecraft_course::ObjectKind::Text { style, .. }, None) => Some(if is_selection {
                crate::properties::style_for_range(&edit.live_runs, style, char_range)
            } else {
                crate::properties::style_at_cursor(&edit.live_runs, style, char_range.0)
            }),
            (coursecraft_course::ObjectKind::Shape { fill, text, .. }, None) => {
                let fallback = default_shape_text_style(fill);
                let style = text.as_ref().map_or(&fallback, |t| &t.style);
                Some(if is_selection {
                    crate::properties::style_for_range(&edit.live_runs, style, char_range)
                } else {
                    crate::properties::style_at_cursor(&edit.live_runs, style, char_range.0)
                })
            }
            (coursecraft_course::ObjectKind::Table { rows, .. }, Some((row, col))) => {
                rows.get(row).and_then(|r| r.cells.get(col)).map(|cell| {
                    if is_selection {
                        crate::properties::style_for_range(&edit.live_runs, &cell.style, char_range)
                    } else {
                        crate::properties::style_at_cursor(
                            &edit.live_runs,
                            &cell.style,
                            char_range.0,
                        )
                    }
                })
            }
            _ => None,
        });
    let Some((mut eff_bold, mut eff_italic, mut eff_underline, _, _)) = eff else {
        return;
    };
    if !is_selection {
        let caret = char_range.0;
        let matches_pending = editor
            .ui
            .properties
            .pending_caret_style
            .as_ref()
            .is_some_and(|p| {
                p.slide == edit.slide
                    && p.object == edit.object
                    && p.cell == edit.cell
                    && p.cursor == caret
            });
        if matches_pending && let Some(pending) = &editor.ui.properties.pending_caret_style {
            if let Some(b) = pending.patch.bold {
                eff_bold = b;
            }
            if let Some(it) = pending.patch.italic {
                eff_italic = it;
            }
            if let Some(u) = pending.patch.underline {
                eff_underline = u;
            }
        }
        if !matches_pending {
            // A pending style of another caret or object must not carry over.
            editor.ui.properties.pending_caret_style = None;
        }
        let entry = editor
            .ui
            .properties
            .pending_caret_style
            .get_or_insert_with(|| crate::properties::PendingCaretStyle {
                slide: edit.slide.clone(),
                object: edit.object.clone(),
                cell: edit.cell,
                cursor: caret,
                patch: coursecraft_course::TextRunPatch::default(),
            });
        entry.slide = edit.slide.clone();
        entry.object = edit.object.clone();
        entry.cell = edit.cell;
        entry.cursor = caret;
        if toggle_bold {
            entry.patch.bold = Some(!eff_bold);
        }
        if toggle_italic {
            entry.patch.italic = Some(!eff_italic);
        }
        if toggle_underline {
            entry.patch.underline = Some(!eff_underline);
        }
        return;
    }
    if let Some((_, _, _, style)) = &state_parts {
        let patch = coursecraft_course::TextRunPatch {
            bold: toggle_bold.then_some(!eff_bold),
            italic: toggle_italic.then_some(!eff_italic),
            underline: toggle_underline.then_some(!eff_underline),
            ..coursecraft_course::TextRunPatch::default()
        };
        apply_state_inline_style(editor, edit, buffer, char_range, &patch, style);
        return;
    }
    let mut params = serde_json::json!({
        "slide": edit.slide,
        "object": edit.object,
        "start": char_range.0,
        "end": char_range.1,
    });
    if let Some((row, col)) = edit.cell {
        params["row"] = serde_json::json!(row);
        params["column"] = serde_json::json!(col);
    }
    if toggle_bold {
        params["bold"] = serde_json::json!(!eff_bold);
    }
    if toggle_italic {
        params["italic"] = serde_json::json!(!eff_italic);
    }
    if toggle_underline {
        params["underline"] = serde_json::json!(!eff_underline);
    }
    if buffer != edit.original {
        let mut text_params = serde_json::json!({
            "slide": edit.slide,
            "object": edit.object,
            "text": buffer,
        });
        if let Some((row, col)) = edit.cell {
            text_params["row"] = serde_json::json!(row);
            text_params["column"] = serde_json::json!(col);
        }
        if editor
            .run_batch(&[
                ("text.set".to_owned(), text_params),
                ("text.style_range".to_owned(), params),
            ])
            .is_some()
            && let Some(state) = editor.ui.inline_edit.as_mut()
        {
            state.original = buffer.to_owned();
        }
    } else {
        editor.run("text.style_range", params);
    }
}

fn update_inline_edit_panel_yield(
    editor: &mut Editor,
    ui: &Ui,
    rect: Rect,
    panel_right: f32,
) -> bool {
    let (clicked_in_panel, clicked_in_edit) = ui.input(|i| {
        let pressed = i.pointer.primary_pressed();
        let pos = i.pointer.interact_pos();
        (
            pressed
                && pos.is_some_and(|p| {
                    p.x >= panel_right || editor.ui.insert_text.window_contains(p)
                }),
            pressed && pos.is_some_and(|p| rect.contains(p)),
        )
    });
    if clicked_in_panel {
        editor.ui.inline_edit_yielded_to_panel = true;
    } else if clicked_in_edit {
        editor.ui.inline_edit_yielded_to_panel = false;
    }
    editor.ui.inline_edit_yielded_to_panel
}

struct InlineEditTextProps {
    style: coursecraft_course::TextStyle,
    align: egui::Align,
    valign: egui::Align,
    autofit: coursecraft_course::Autofit,
    insets: Option<coursecraft_course::TextInsets>,
    point_text: bool,
    grow_text: bool,
}

fn inline_edit_text_props(
    object: &coursecraft_course::Object,
    cell: Option<(usize, usize)>,
    state_name: Option<&str>,
    extra_id: Option<&str>,
) -> Option<InlineEditTextProps> {
    let state_def = state_name.and_then(|name| object.states.iter().find(|s| s.name == name));
    if let (Some(sd), Some(eid)) = (state_def, extra_id) {
        let extra = sd.extras.iter().find(|e| e.id == eid)?;
        return match &extra.kind {
            coursecraft_course::StateExtraKind::Text {
                style,
                autofit,
                point_text,
                ..
            } => {
                let is_pt = *point_text && !style.wrap;
                Some(InlineEditTextProps {
                    style: style.clone(),
                    align: align_for(style.align),
                    valign: valign_for(style.valign),
                    autofit: *autofit,
                    insets: style.insets,
                    point_text: is_pt,
                    grow_text: !is_pt && *autofit == coursecraft_course::Autofit::Grow,
                })
            }
            coursecraft_course::StateExtraKind::Shape {
                shape, fill, text, ..
            } if coursecraft_course::shape_allows_text(*shape) => {
                let st = text
                    .as_ref()
                    .map(|t| t.style.clone())
                    .unwrap_or_else(|| default_shape_text_style(fill));
                let af = text
                    .as_ref()
                    .map_or(coursecraft_course::Autofit::None, |t| t.autofit);
                Some(InlineEditTextProps {
                    align: align_for(st.align),
                    valign: valign_for(st.valign),
                    autofit: af,
                    insets: st.insets,
                    style: st,
                    point_text: false,
                    grow_text: false,
                })
            }
            _ => None,
        };
    }
    let mut props = match (&object.kind, cell) {
        (
            coursecraft_course::ObjectKind::Text {
                style,
                autofit,
                point_text,
                ..
            },
            None,
        ) => {
            let mut st = style.clone();
            if let Some(sd) = state_def
                && sd.text_style.is_none()
                && let Some(c) = &sd.fill
            {
                st.color = c.clone();
            }
            let is_pt = *point_text && state_name.is_none() && !st.wrap;
            Some(InlineEditTextProps {
                style: st.clone(),
                align: align_for(st.align),
                valign: valign_for(st.valign),
                autofit: *autofit,
                insets: st.insets,
                point_text: is_pt,
                grow_text: state_name.is_none()
                    && !is_pt
                    && *autofit == coursecraft_course::Autofit::Grow,
            })
        }
        (coursecraft_course::ObjectKind::Button { fill, .. }, None) => Some(InlineEditTextProps {
            style: coursecraft_course::TextStyle {
                size: 20.0,
                // Same label colour the player paints, so typing stays visible on light fills.
                color: coursecraft_course::parse_color(fill).map_or_else(
                    || coursecraft_course::Color("#ffffff".into()),
                    |fill| {
                        let c = coursecraft_course::button_label_color(fill, None);
                        coursecraft_course::Color(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
                    },
                ),
                align: coursecraft_course::Align::Center,
                valign: coursecraft_course::VAlign::Middle,
                ..Default::default()
            },
            align: egui::Align::Center,
            valign: egui::Align::Center,
            autofit: coursecraft_course::Autofit::None,
            insets: None,
            point_text: false,
            grow_text: false,
        }),
        (
            coursecraft_course::ObjectKind::Shape {
                shape, fill, text, ..
            },
            None,
        ) if coursecraft_course::shape_allows_text(*shape) => {
            let st = text
                .as_ref()
                .map(|t| t.style.clone())
                .unwrap_or_else(|| default_shape_text_style(fill));
            let af = text
                .as_ref()
                .map_or(coursecraft_course::Autofit::None, |t| t.autofit);
            Some(InlineEditTextProps {
                align: align_for(st.align),
                valign: valign_for(st.valign),
                autofit: af,
                insets: st.insets,
                style: st,
                point_text: false,
                grow_text: false,
            })
        }
        (coursecraft_course::ObjectKind::Table { rows, .. }, Some((row, column))) => {
            let cell = rows.get(row).and_then(|line| line.cells.get(column))?;
            Some(InlineEditTextProps {
                style: cell.style.clone(),
                align: align_for(cell.style.align),
                valign: valign_for(cell.valign),
                autofit: coursecraft_course::Autofit::None,
                insets: None,
                point_text: false,
                grow_text: false,
            })
        }
        _ => None,
    }?;
    if let Some(sd) = state_def {
        if let Some(ts) = &sd.text_style {
            props.align = align_for(ts.align);
            props.valign = valign_for(ts.valign);
            props.insets = ts.insets;
            props.style = ts.clone();
        }
        if let Some(af) = sd.autofit {
            props.autofit = af;
        }
    }
    Some(props)
}

/// The text an inline edit inside a state shows: plain text, runs, paragraphs and the base style.
/// `None` when the edit is not inside a state or its target is gone (`N85`).
fn state_inline_parts(
    object: &coursecraft_course::Object,
    edit: &InlineEdit,
) -> Option<(
    String,
    Vec<coursecraft_course::TextRun>,
    Vec<coursecraft_course::Paragraph>,
    coursecraft_course::TextStyle,
)> {
    let state_name = edit.state_name.as_deref()?;
    let state = object.states.iter().find(|s| s.name == state_name)?;
    let plain_of = |text: &str, runs: &[coursecraft_course::TextRun]| -> String {
        if runs.is_empty() {
            text.to_owned()
        } else {
            runs.iter().map(|r| r.text.as_str()).collect()
        }
    };
    if let Some(extra_id) = edit.extra_id.as_deref() {
        let extra = state.extras.iter().find(|e| e.id == extra_id)?;
        return match &extra.kind {
            coursecraft_course::StateExtraKind::Text {
                text,
                style,
                runs,
                paragraphs,
                ..
            } => Some((
                plain_of(text, runs),
                runs.clone(),
                paragraphs.clone(),
                style.clone(),
            )),
            coursecraft_course::StateExtraKind::Shape { text: Some(t), .. } => Some((
                plain_of(&t.text, &t.runs),
                t.runs.clone(),
                t.paragraphs.clone(),
                t.style.clone(),
            )),
            _ => None,
        };
    }
    let (base_text, base_runs, base_paras, base_style) = match &object.kind {
        coursecraft_course::ObjectKind::Text {
            text,
            runs,
            paragraphs,
            style,
            ..
        } => (text.as_str(), runs.as_slice(), paragraphs.as_slice(), style),
        coursecraft_course::ObjectKind::Shape { text: Some(t), .. } => (
            t.text.as_str(),
            t.runs.as_slice(),
            t.paragraphs.as_slice(),
            &t.style,
        ),
        _ => return None,
    };
    let runs = state.runs.as_deref().unwrap_or(base_runs);
    let text = state.text.as_deref().unwrap_or(base_text);
    let paragraphs = state.paragraphs.as_deref().unwrap_or(base_paras);
    let style = state.text_style.as_ref().unwrap_or(base_style);
    Some((
        plain_of(text, runs),
        runs.to_vec(),
        paragraphs.to_vec(),
        style.clone(),
    ))
}

fn sync_inline_edit_runs(
    object: &coursecraft_course::Object,
    edit: &InlineEdit,
    revision: u64,
) -> (
    Vec<coursecraft_course::TextRun>,
    Vec<coursecraft_course::Paragraph>,
    bool,
) {
    if edit.last_revision == revision {
        return (edit.live_runs.clone(), edit.live_paragraphs.clone(), false);
    }
    let from_state = state_inline_parts(object, edit).map(|(p, r, a, _)| (p, r, a));
    let (obj_plain, obj_runs, obj_paras) = if let Some(parts) = from_state {
        parts
    } else {
        sync_object_parts(object, edit)
    };
    let (runs, paras) = if edit.buffer == obj_plain {
        (obj_runs, obj_paras)
    } else {
        (
            coursecraft_course::apply_text_edit(&obj_runs, &obj_plain, &edit.buffer),
            coursecraft_course::align_paragraphs(&obj_plain, &obj_paras, &edit.buffer),
        )
    };
    (runs, paras, true)
}

/// The base object's text parts for an inline edit outside any state.
fn sync_object_parts(
    object: &coursecraft_course::Object,
    edit: &InlineEdit,
) -> (
    String,
    Vec<coursecraft_course::TextRun>,
    Vec<coursecraft_course::Paragraph>,
) {
    match (&object.kind, edit.cell) {
        (
            coursecraft_course::ObjectKind::Text {
                text,
                runs,
                paragraphs,
                ..
            },
            None,
        ) => {
            let p = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|r| r.text.as_str()).collect()
            };
            (p, runs.clone(), paragraphs.clone())
        }
        (coursecraft_course::ObjectKind::Shape { text: Some(t), .. }, None) => {
            let p = if t.runs.is_empty() {
                t.text.clone()
            } else {
                t.runs.iter().map(|r| r.text.as_str()).collect()
            };
            (p, t.runs.clone(), t.paragraphs.clone())
        }
        (coursecraft_course::ObjectKind::Table { rows, .. }, Some((r, c))) => {
            if let Some(cell) = rows.get(r).and_then(|row| row.cells.get(c)) {
                let p = if cell.runs.is_empty() {
                    cell.text.clone()
                } else {
                    cell.runs.iter().map(|run| run.text.as_str()).collect()
                };
                (p, cell.runs.clone(), cell.paragraphs.clone())
            } else {
                (String::new(), Vec::new(), Vec::new())
            }
        }
        _ => (edit.buffer.clone(), Vec::new(), Vec::new()),
    }
}

fn inset_text_rect(rect: Rect, insets: Option<coursecraft_course::TextInsets>, scale: f32) -> Rect {
    let Some(ins) = insets else {
        return rect;
    };
    let l = (ins.left.max(0.0) * scale).min(rect.width() * 0.45);
    let r = (ins.right.max(0.0) * scale).min(rect.width() * 0.45);
    let top = (ins.top.max(0.0) * scale).min(rect.height() * 0.45);
    let b = (ins.bottom.max(0.0) * scale).min(rect.height() * 0.45);
    let inner = Rect::from_min_max(
        pos2(rect.min.x + l, rect.min.y + top),
        pos2(rect.max.x - r, rect.max.y - b),
    );
    if inner.is_positive() { inner } else { rect }
}

#[allow(clippy::too_many_arguments)]
fn inline_edit_shrink(
    ctx: &egui::Context,
    autofit: coursecraft_course::Autofit,
    text_rect: Rect,
    scale: f32,
    buffer: &str,
    style: &coursecraft_course::TextStyle,
    runs: &[coursecraft_course::TextRun],
    paragraphs: &[coursecraft_course::Paragraph],
    text_color: Color32,
    link_color: Color32,
    theme_font: &str,
) -> f32 {
    if autofit != coursecraft_course::Autofit::Shrink {
        return 1.0;
    }
    let resolved = coursecraft_render::resolve_text_runs(
        buffer,
        style,
        runs,
        None,
        text_color,
        link_color,
        Some(theme_font),
    );
    coursecraft_render::layout_text(
        ctx,
        text_rect,
        scale,
        &resolved,
        paragraphs,
        style.align,
        style.valign,
        autofit,
    )
    .shrink
}

fn update_inline_edit_buffer(
    editor: &mut Editor,
    edit: &InlineEdit,
    style: &coursecraft_course::TextStyle,
    buffer: &str,
) {
    let Some(state) = editor.ui.inline_edit.as_mut() else {
        return;
    };
    if buffer == state.buffer {
        return;
    }
    let old_chars: Vec<char> = state.buffer.chars().collect();
    let new_chars: Vec<char> = buffer.chars().collect();
    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars.get(prefix) == new_chars.get(prefix)
    {
        prefix = prefix.saturating_add(1);
    }
    let max_suffix = old_chars
        .len()
        .saturating_sub(prefix)
        .min(new_chars.len().saturating_sub(prefix));
    let mut suffix = 0usize;
    while suffix < max_suffix {
        let old_idx = old_chars.len().saturating_sub(1).saturating_sub(suffix);
        let new_idx = new_chars.len().saturating_sub(1).saturating_sub(suffix);
        if old_chars.get(old_idx) == new_chars.get(new_idx) {
            suffix = suffix.saturating_add(1);
        } else {
            break;
        }
    }
    let inserted_end = new_chars.len().saturating_sub(suffix);
    let mut next_runs =
        coursecraft_course::apply_text_edit(&state.live_runs, &state.buffer, buffer);
    if inserted_end > prefix
        && let Some(pending) = editor.ui.properties.pending_caret_style.take()
    {
        if pending.slide == edit.slide
            && pending.object == edit.object
            && pending.cell == edit.cell
            && pending.cursor == prefix
        {
            next_runs = coursecraft_course::apply_style_to_range_with_base(
                &next_runs,
                buffer,
                (prefix, inserted_end),
                &pending.patch,
                Some(style),
            );
        }
    } else if editor
        .ui
        .properties
        .pending_caret_style
        .as_ref()
        .is_some_and(|p| p.slide == edit.slide && p.object == edit.object && p.cell == edit.cell)
    {
        editor.ui.properties.pending_caret_style = None;
    }
    state.live_runs = next_runs;
    state.live_paragraphs =
        coursecraft_course::align_paragraphs(&state.buffer, &state.live_paragraphs, buffer);
    state.buffer = buffer.to_owned();
}

pub(crate) fn inline_edit_slide_rect(
    object: &coursecraft_course::Object,
    edit: &InlineEdit,
) -> Option<Rect> {
    if let (Some(state_name), Some(extra_id)) =
        (edit.state_name.as_deref(), edit.extra_id.as_deref())
    {
        let extra = object
            .states
            .iter()
            .find(|s| s.name == state_name)
            .and_then(|s| s.extras.iter().find(|e| e.id == extra_id))?;
        let (world_rect, _world_rot) = state_extra_world_geometry(object, state_name, extra);
        return Some(match &extra.kind {
            coursecraft_course::StateExtraKind::Shape { shape, adjust, .. }
                if coursecraft_course::shape_allows_text(*shape) =>
            {
                let (x, y, w, h) = coursecraft_render::shape_text_rect(
                    *shape,
                    world_rect.width(),
                    world_rect.height(),
                    adjust,
                );
                Rect::from_min_size(pos2(world_rect.min.x + x, world_rect.min.y + y), vec2(w, h))
            }
            _ => world_rect,
        });
    }
    Some(match (&object.kind, edit.cell) {
        (coursecraft_course::ObjectKind::Table { .. }, Some((row, column))) => {
            table_cell_slide_rect(object, row, column)?.shrink(6.0)
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
    })
}

fn inline_edit_id(edit: &InlineEdit) -> Id {
    if edit.cell.is_none() && edit.extra_id.is_none() {
        Id::unique(("canvas_inline_edit", &edit.slide, &edit.object))
    } else {
        Id::unique((
            "canvas_inline_edit",
            &edit.slide,
            &edit.object,
            edit.cell,
            &edit.extra_id,
        ))
    }
}

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

    let Some(base_slide_rect) = inline_edit_slide_rect(object, &edit) else {
        cancel_inline_edit(editor);
        return;
    };

    let scale = t.scale();
    let Some(InlineEditTextProps {
        style,
        align,
        valign,
        autofit,
        insets,
        point_text,
        grow_text,
    }) = inline_edit_text_props(
        object,
        edit.cell,
        edit.state_name.as_deref(),
        edit.extra_id.as_deref(),
    )
    else {
        cancel_inline_edit(editor);
        return;
    };
    let live_resize = point_text || grow_text;
    let text_color = coursecraft_render::parse_color(&style.color)
        .unwrap_or_else(|| coursecraft_render::palette(ui.ctx()).text);
    let link_color = coursecraft_render::parse_color(&editor.session.course.theme.accent)
        .unwrap_or_else(|| coursecraft_render::palette(ui.ctx()).accent);
    let theme_font = editor.session.course.theme.font.clone();
    let font_size = (style.size * scale).clamp(4.0, 400.0);

    let (synced_runs, synced_paragraphs, synced_changed) =
        sync_inline_edit_runs(object, &edit, editor.session.revision);
    let slide_rect = if let Some((live_rect, measured)) = inline_edit_live_bounds_for(
        ui.ctx(),
        base_slide_rect,
        &edit.buffer,
        &edit.original,
        &style,
        &synced_runs,
        &synced_paragraphs,
        &theme_font,
        point_text,
        grow_text,
    ) {
        editor.ui.inline_edit_measured_size = Some((edit.buffer.clone(), measured));
        editor.ui.inline_edit_live_rect = Some(live_rect);
        live_rect
    } else {
        editor.ui.inline_edit_measured_size = None;
        editor.ui.inline_edit_live_rect = None;
        base_slide_rect
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
    // A table cell or a state extra is its own text field with its own undo history (`N109`).
    let edit_id = inline_edit_id(&edit);
    let accent = coursecraft_render::palette(ui.ctx()).accent;

    if synced_changed && let Some(state) = editor.ui.inline_edit.as_mut() {
        state.live_runs = synced_runs.clone();
        state.live_paragraphs = synced_paragraphs.clone();
        state.last_revision = editor.session.revision;
    }
    let mut buffer = edit.buffer.clone();
    crate::insert_text::apply_pending_inline(editor, ui.ctx(), edit_id, &mut buffer);
    let text_rect = inset_text_rect(rect, insets, scale);
    let frame = inline_edit_frame(accent);
    if live_resize {
        draw_dashed_rect(ui.painter(), rect, Stroke::new(1.0, accent));
    } else if insets.is_some() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::ZERO,
            frame.stroke,
            egui::StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    if live_resize {
        child.spacing_mut().interact_size = egui::Vec2::ZERO;
    }
    let visuals = inline_edit_visuals(child.visuals().clone(), text_color, accent);
    *child.visuals_mut() = visuals;
    let shrink = inline_edit_shrink(
        ui.ctx(),
        autofit,
        text_rect,
        scale,
        &buffer,
        &style,
        &synced_runs,
        &synced_paragraphs,
        text_color,
        link_color,
        &theme_font,
    );
    let base_buffer = edit.buffer.clone();
    let course_align = style.align;
    let is_multiline = !edit.is_button;
    let wrap = style.wrap;
    let mut layouter = |egui_ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let s = buf.as_str();
        let (runs_for_s, paras_for_s) = if s == base_buffer {
            (synced_runs.clone(), synced_paragraphs.clone())
        } else {
            (
                coursecraft_course::apply_text_edit(&synced_runs, &base_buffer, s),
                coursecraft_course::align_paragraphs(&base_buffer, &synced_paragraphs, s),
            )
        };
        let resolved = coursecraft_render::resolve_text_runs(
            s,
            &style,
            &runs_for_s,
            None,
            text_color,
            link_color,
            Some(&theme_font),
        );
        let job = coursecraft_render::inline_edit_layout_job_with_wrap(
            egui_ui.ctx(),
            scale,
            shrink,
            &resolved,
            &paras_for_s,
            course_align,
            wrap_width,
            is_multiline,
            wrap,
        );
        let mut galley = egui_ui.fonts_mut(|fonts| fonts.layout_job(job));
        if course_align == coursecraft_course::Align::Justify && is_multiline && wrap {
            let ppp = egui_ui.ctx().pixels_per_point();
            let origin_x = text_rect.left()
                + if insets.is_some() {
                    0.0
                } else {
                    f32::from(frame.inner_margin.left)
                };
            coursecraft_render::justify_galley_snapped(
                &mut galley,
                wrap_width.max(1.0),
                ppp,
                origin_x,
            );
        }
        galley
    };
    let edit_builder = if edit.is_button {
        egui::TextEdit::singleline(&mut buffer)
    } else {
        egui::TextEdit::multiline(&mut buffer).desired_rows(1)
    };
    let mut edit_builder = edit_builder
        .id(edit_id)
        .font(FontId::proportional(font_size))
        .text_color(text_color)
        .align(egui::Align2::new(align, valign))
        .desired_width(text_rect.width())
        .frame(if live_resize || insets.is_some() {
            egui::Frame::NONE
        } else {
            frame
        })
        .layouter(&mut layouter);
    if live_resize {
        edit_builder = edit_builder.min_size(text_rect.size());
    }
    let mut output = edit_builder.show(&mut child);

    // As-you-type spelling: underline each misspelling from the galley and
    // offer the word menu on a right click over a squiggle.
    let spell_words = editor.spell_words(&buffer);
    let misspellings: Vec<coursecraft_spell::Misspelling> = spell_words
        .iter()
        .map(|word| word.misspelling.clone())
        .collect();
    editor.ui.spell_inline_debug = misspellings.iter().map(|miss| miss.word.clone()).collect();
    editor.ui.spell_inline_rects.clear();
    let mut menu_open_at_start = false;
    if !misspellings.is_empty() {
        let words = crate::spell_check::underline_rects(
            &output.galley,
            output.galley_pos,
            &buffer,
            &misspellings,
        );
        editor.ui.spell_inline_rects = words.clone();
        crate::spell_check::spell_underlines(
            child.painter(),
            &output.galley,
            output.galley_pos,
            &buffer,
            &misspellings,
            crate::spell_check::squiggle_color(child.ctx()),
        );
        let menu_open = output.response.context_menu_opened();
        let secondary = ui.input(|i| {
            i.pointer.secondary_pressed()
                || i.pointer.secondary_clicked()
                || i.pointer.secondary_released()
        });
        if secondary {
            editor.ui.spell_menu_pos =
                ui.input(|i| i.pointer.press_origin().or_else(|| i.pointer.latest_pos()));
        } else if !menu_open {
            editor.ui.spell_menu_pos = None;
        }
        menu_open_at_start = menu_open;
        if let Some(click_pos) = editor.ui.spell_menu_pos {
            // Suggestions only while the menu is up: they are the slow part.
            let suggestions = editor.spell_suggestion_map(&spell_words);
            let mut choice = crate::spell_check::WordMenuOutcome::default();
            let _ = output.response.context_menu(|menu_ui| {
                choice =
                    crate::spell_check::show_word_menu(menu_ui, click_pos, &words, &suggestions);
            });
            // A chosen suggestion edits the local buffer; the assignment below
            // puts it back into the inline edit, so committing stays one step.
            let (_changed, added, ignored) =
                crate::spell_check::apply_word_menu(choice, &mut buffer);
            for word in added {
                editor.add_spell_word(&word);
            }
            for word in ignored {
                editor.ignore_spell_word(&word);
            }
        }
    }

    let cmd_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    let button_enter =
        edit.is_button && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    // Escape closes the Symbol or Reference window first, not the edit.
    let escaped =
        ui.input(|i| i.key_pressed(egui::Key::Escape)) && !editor.ui.insert_text.any_window_open();
    let toggle_bold = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::B));
    let toggle_italic = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I));
    let toggle_underline = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::U));
    let panel_right = ui.max_rect().max.x;
    // Read the press first: `layer_id_at` takes the context lock `input` holds.
    let press = ui.input(|i| {
        (i.pointer.primary_pressed() && !menu_open_at_start)
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    let clicked_outside = press.is_some_and(|pos| {
        !rect.contains(pos)
            && pos.x < panel_right
            // The ribbon and its menus format the text being edited.
            && !editor.ui.ribbon.rect.contains(pos)
            && !editor.ui.insert_text.window_contains(pos)
            && !ui.ctx().layer_id_at(pos).is_some_and(|layer| {
                matches!(layer.order, egui::Order::Foreground | egui::Order::Tooltip)
            })
    });
    let yielded_to_panel = update_inline_edit_panel_yield(editor, ui, rect, panel_right);

    // Keep keyboard focus on the in-place editor while it is open. The canvas
    // interaction and the click that placed the object can otherwise take it.
    let pointer_active = ui.input(|i| {
        i.pointer.primary_down() || i.pointer.primary_pressed() || i.pointer.primary_released()
    });
    if edit.just_entered
        || (!output.response.has_focus() && !clicked_outside && !escaped && !yielded_to_panel)
    {
        output.response.request_focus();
    }
    let cursor_range = output.state.cursor.char_range();
    let buffer_changed = buffer != edit.buffer;
    if edit.just_entered {
        handle_inline_edit_just_entered(
            editor,
            ui.ctx(),
            &mut output,
            &buffer,
            buffer_changed,
            pointer_active,
        );
    }

    update_inline_edit_buffer(editor, &edit, &style, &buffer);
    if live_resize && buffer_changed {
        update_inline_edit_live_bounds(
            editor,
            ui.ctx(),
            &buffer,
            &edit.original,
            &style,
            &synced_runs,
            &synced_paragraphs,
            &theme_font,
            base_slide_rect,
            slide_rect,
            point_text,
            grow_text,
        );
    }
    if output.response.has_focus()
        && let Some(cr) = cursor_range
    {
        let (sel_range, cpos) = if !cr.is_empty() {
            let sorted = cr.as_sorted_char_range();
            (Some((sorted.start.0, sorted.end.0)), sorted.start.0)
        } else {
            (None, cr.primary.index.0)
        };
        editor.ui.inline_edit_selection =
            Some((edit.slide.clone(), edit.object.clone(), sel_range, cpos));
    }
    if (toggle_bold || toggle_italic || toggle_underline) && !escaped {
        dispatch_inline_edit_format_shortcuts(
            editor,
            &edit,
            &buffer,
            cursor_range,
            toggle_bold,
            toggle_italic,
            toggle_underline,
        );
    }
    if escaped {
        cancel_inline_edit(editor);
    } else if cmd_enter || button_enter || clicked_outside {
        commit_inline_edit(editor);
    }
}

fn handle_inline_edit_just_entered(
    editor: &mut Editor,
    ctx: &egui::Context,
    output: &mut egui::text_edit::TextEditOutput,
    buffer: &str,
    buffer_changed: bool,
    pointer_active: bool,
) {
    if buffer_changed {
        // The author already replaced the placeholder text on this pass;
        // do not re-select what they just typed.
        if let Some(state) = editor.ui.inline_edit.as_mut() {
            state.just_entered = false;
        }
        return;
    }
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
    output.state.clone().store(ctx, output.response.id);
    if !pointer_active
        && output.response.has_focus()
        && let Some(state) = editor.ui.inline_edit.as_mut()
    {
        state.just_entered = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_inline_edit_live_bounds(
    editor: &mut Editor,
    ctx: &egui::Context,
    buffer: &str,
    original: &str,
    style: &coursecraft_course::TextStyle,
    synced_runs: &[coursecraft_course::TextRun],
    synced_paragraphs: &[coursecraft_course::Paragraph],
    theme_font: &str,
    base_slide_rect: Rect,
    prev_slide_rect: Rect,
    point_text: bool,
    grow_text: bool,
) {
    let (next_runs, next_paragraphs) = editor
        .ui
        .inline_edit
        .as_ref()
        .map_or((synced_runs, synced_paragraphs), |state| {
            (state.live_runs.as_slice(), state.live_paragraphs.as_slice())
        });
    if let Some((next_rect, measured)) = inline_edit_live_bounds_for(
        ctx,
        base_slide_rect,
        buffer,
        original,
        style,
        next_runs,
        next_paragraphs,
        theme_font,
        point_text,
        grow_text,
    ) {
        editor.ui.inline_edit_measured_size = Some((buffer.to_owned(), measured));
        editor.ui.inline_edit_live_rect = Some(next_rect);
        let rect_changed = (next_rect.min - prev_slide_rect.min).length() > 0.25
            || (next_rect.max - prev_slide_rect.max).length() > 0.25;
        if rect_changed {
            // Re-run this frame's layout/paint immediately so the painted box
            // border, selection outline, resize handles, and TextEdit rect all
            // match the new size on the same frame without 1-frame lag.
            ctx.request_discard("inline text box live resize");
        }
        ctx.request_repaint();
    }
}

fn dispatch_inline_edit_format_shortcuts(
    editor: &mut Editor,
    edit: &InlineEdit,
    buffer: &str,
    cursor_range: Option<egui::text::CCursorRange>,
    toggle_bold: bool,
    toggle_italic: bool,
    toggle_underline: bool,
) {
    if crate::text_format::targets(editor).is_ok() {
        // The same path as the ribbon's Bold, Italic and Underline.
        for (on, change) in [
            (toggle_bold, crate::text_format::Change::Bold),
            (toggle_italic, crate::text_format::Change::Italic),
            (toggle_underline, crate::text_format::Change::Underline),
        ] {
            if on {
                crate::text_format::apply(editor, &change);
            }
        }
    } else {
        // Text inside a state: its own runs, not yet in `text_format`.
        apply_inline_edit_style_shortcut(
            editor,
            edit,
            buffer,
            cursor_range,
            toggle_bold,
            toggle_italic,
            toggle_underline,
        );
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
    if let Some(PlacementTool::Shape(kind)) = editor.ui.placement_tool {
        coursecraft_render::paint_shape(painter, screen_rect, kind, &[], fill);
        let outline = coursecraft_render::ResolvedOutline {
            color: p.accent,
            width: 1.5,
            dash: coursecraft_course::DashStyle::Solid,
            dash_pattern: None,
            join: coursecraft_course::LineJoin::Miter,
            miter_limit: None,
        };
        coursecraft_render::paint_outline(painter, screen_rect, kind, &[], &outline, 1.0);
    } else {
        painter.rect_filled(screen_rect, CornerRadius::ZERO, fill);
        painter.rect_stroke(
            screen_rect,
            CornerRadius::ZERO,
            Stroke::new(1.5, p.accent),
            StrokeKind::Inside,
        );
    }

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
        let mut live_item;
        let active_item = if let Some(live_rect) = editor.ui.inline_edit_live_rect
            && editor.ui.inline_edit.as_ref().is_some_and(|edit| {
                edit.object == *id && edit.extra_id.is_none() && edit.cell.is_none()
            }) {
            live_item = item.clone();
            live_item.rect = live_rect;
            &live_item
        } else {
            item
        };
        let corners: Vec<Pos2> = coursecraft_render::item_corners(active_item)
            .into_iter()
            .map(|point| t.to_screen_pos(point))
            .collect();
        if corners.len() == 4 {
            if editor.ui.editing_state.is_some() {
                for i in 0..4 {
                    painter.add(egui::Shape::dashed_line(
                        &[corners[i], corners[(i + 1) % 4]],
                        Stroke::new(1.5, accent_color),
                        6.0,
                        4.0,
                    ));
                }
            } else {
                painter.add(egui::Shape::closed_line(
                    corners,
                    Stroke::new(2.0, accent_color),
                ));
            }
        }
        let bounds = coursecraft_render::item_bounds(active_item);
        bbox = Some(match bbox {
            Some(current) => current.union(bounds),
            None => bounds,
        });
    }
    let Some(bbox) = bbox else {
        return;
    };

    if ids.iter().any(|id| is_locked(editor, id)) {
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

    if editor.ui.editing_state.is_some()
        && let Some((extra_id, mut world_rect, mut world_rot)) =
            selected_state_extra_geometry(editor)
    {
        if let Some(live_rect) = editor.ui.inline_edit_live_rect
            && editor
                .ui
                .inline_edit
                .as_ref()
                .and_then(|edit| edit.extra_id.as_deref())
                == Some(extra_id.as_str())
        {
            world_rect = live_rect;
        }
        if let Some(drag) = editor.ui.canvas_drag.as_ref()
            && drag.object_id == extra_id
        {
            match &drag.op {
                DragOp::StateExtraMove | DragOp::StateExtraResize { .. } => {
                    if let Some(curr) = current_pointer {
                        let moved = (curr - drag.start_screen).length() >= 2.0;
                        if moved {
                            let shift = ui.input(|i| i.modifiers.shift);
                            world_rect = compute_drag_frame(drag, curr, shift, t.scale());
                        }
                    }
                }
                DragOp::StateExtraRotate {
                    centre,
                    start_angle,
                    orig_rotation,
                } => {
                    if let Some(curr) = current_pointer {
                        let current_angle = (curr.y - centre.y).atan2(curr.x - centre.x);
                        let mut degrees =
                            orig_rotation + (current_angle - start_angle).to_degrees();
                        if ui.input(|i| i.modifiers.shift) {
                            degrees = (degrees / 15.0).round() * 15.0;
                        }
                        world_rot = coursecraft_course::normalize_rotation(degrees);
                    }
                }
                _ => {}
            }
        }
        let extra_screen = t.to_screen_rect(world_rect);
        let extra_corners: Vec<Pos2> = [
            extra_screen.left_top(),
            extra_screen.right_top(),
            extra_screen.right_bottom(),
            extra_screen.left_bottom(),
        ]
        .into_iter()
        .map(|pt| rotate_point_2d(pt, extra_screen.center(), world_rot))
        .collect();
        painter.add(egui::Shape::closed_line(
            extra_corners,
            Stroke::new(2.0, accent_color),
        ));
        let centers = rotated_handle_centers(extra_screen, world_rot);
        let rot_handle = rotate_handle_center(extra_screen, world_rot);
        let top = rotated_handle_centers(extra_screen, world_rot)[1].1;
        painter.line_segment([top, rot_handle], Stroke::new(1.0, accent_color));
        painter.circle_filled(rot_handle, 6.0, Color32::WHITE);
        painter.circle_stroke(rot_handle, 6.0, Stroke::new(1.5, accent_color));
        for (_, c) in centers {
            let handle_rect = Rect::from_center_size(c, vec2(8.0, 8.0));
            painter.rect_filled(handle_rect, CornerRadius::ZERO, Color32::WHITE);
            painter.rect_stroke(
                handle_rect,
                CornerRadius::ZERO,
                Stroke::new(1.0, accent_color),
                StrokeKind::Middle,
            );
        }
        if editor.ui.canvas_drag.is_none()
            && editor.ui.placement_tool.is_none()
            && let Some(pos) = current_pointer
        {
            if Rect::from_center_size(rot_handle, vec2(14.0, 14.0)).contains(pos) {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            } else {
                for (h, c) in centers {
                    if Rect::from_center_size(c, vec2(HANDLE_HIT_PX, HANDLE_HIT_PX)).contains(pos) {
                        ui.ctx().set_cursor_icon(handle_cursor(h, world_rot));
                        break;
                    }
                }
            }
        }
        return;
    }

    let single_item_live: Option<DrawItem> = if editor.selection.is_multi() {
        None
    } else if let Some(id) = editor.selection.object.as_ref()
        && let Some(item) = items
            .iter()
            .find(|item| item.hit == Hit::Object(id.clone()))
    {
        let mut live = item.clone();
        if let Some(live_rect) = editor.ui.inline_edit_live_rect
            && editor.ui.inline_edit.as_ref().is_some_and(|edit| {
                edit.object == *id && edit.extra_id.is_none() && edit.cell.is_none()
            })
        {
            live.rect = live_rect;
        }
        Some(live)
    } else {
        None
    };
    let single_item = single_item_live.as_ref();
    let screen_rect = t.to_screen_rect(bbox);
    let handle_size = 8.0;
    let centers = match single_item {
        Some(item) => rotated_handle_centers(t.to_screen_rect(item.rect), item.rotation),
        None => get_handle_centers(screen_rect),
    };
    let rotate_handle =
        single_item.map(|item| rotate_handle_center(t.to_screen_rect(item.rect), item.rotation));
    let handle_frame = single_item.map_or(bbox, |item| item.rect);
    let handle_rotation = single_item.map_or(0.0, |item| item.rotation);

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
                && editor.ui.format_painter.is_none()
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

        if editor.ui.editing_state.is_none()
            && editor.ui.canvas_drag.is_none()
            && editor.ui.placement_tool.is_none()
            && editor.ui.format_painter.is_none()
            && let Some(pos) = current_pointer
        {
            for (h, c) in centers {
                if handle_on_line(h, handle_frame) {
                    continue;
                }
                let handle_rect = Rect::from_center_size(c, vec2(HANDLE_HIT_PX, HANDLE_HIT_PX));
                if handle_rect.contains(pos) {
                    ui.ctx().set_cursor_icon(handle_cursor(h, handle_rotation));
                    break;
                }
            }
        }

        if editor.ui.editing_state.is_none() {
            for (h, c) in centers {
                if handle_on_line(h, handle_frame) {
                    continue;
                }
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
                    press_offset,
                    ..
                } = drag.op
                && path_id == &path.id
                && point_index == idx
                && let Some(cur_pos) = current_pointer
                && (cur_pos - drag.start_screen).length() >= 3.0
            {
                let slide_pos = t.to_slide_pos(cur_pos) + press_offset;
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

fn show_edit_state_banner(editor: &mut Editor, ui: &mut Ui) {
    let Some(session) = editor.ui.editing_state.clone() else {
        return;
    };
    let p = coursecraft_render::palette(ui.ctx());
    let (object_label, has_overrides) = editor
        .session
        .course
        .slide(&session.slide_id)
        .and_then(|s| s.object(&session.object_id))
        .map_or_else(
            || (session.object_id.clone(), false),
            |(obj, _)| {
                let label = if obj.name.trim().is_empty() {
                    obj.id.clone()
                } else {
                    obj.name.clone()
                };
                let has = obj
                    .states
                    .iter()
                    .find(|st| st.name == session.state_name)
                    .is_some_and(crate::states::state_has_overrides);
                (label, has)
            },
        );

    let mut do_reset = false;
    let mut do_done = false;
    egui::Frame::NONE
        .fill(p.accent.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, p.accent))
        .inner_margin(egui::Margin::symmetric(
            coursecraft_render::PANEL_MARGIN as i8,
            2,
        ))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 32.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let done_btn = egui::Button::new(
                        RichText::new("Done editing").color(egui::Color32::WHITE),
                    )
                    .fill(p.accent)
                    .min_size(vec2(0.0, 32.0));
                    if ui.add(done_btn).clicked() {
                        do_done = true;
                    }
                    let reset_btn = ui
                        .add_enabled(
                            has_overrides,
                            egui::Button::new("Reset overrides").min_size(vec2(0.0, 24.0)),
                        )
                        .on_disabled_hover_text("No overrides to reset");
                    if reset_btn.clicked() {
                        do_reset = true;
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(
                                coursecraft_render::Icon::SlidersHorizontal
                                    .codepoint()
                                    .to_string(),
                            )
                            .font(FontId::new(15.0, egui::FontFamily::Name("phosphor".into())))
                            .color(p.accent),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Editing state \u{201c}{}\u{201d} of {object_label}",
                                    session.state_name
                                ))
                                .strong()
                                .color(p.text),
                            )
                            .truncate(),
                        );
                    });
                },
            );
        });

    if do_reset {
        editor.run(
            "object.patch_state",
            serde_json::json!({
                "slide": session.slide_id,
                "object": session.object_id,
                "name": session.state_name,
                "reset_all": true,
            }),
        );
    }
    if do_done {
        editor.exit_edit_state();
    }
}

/// Show the layers strip, the central slide canvas, and the floating zoom HUD.
#[allow(clippy::cognitive_complexity)]
pub fn show_canvas_and_layers(editor: &mut Editor, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;

    if editor.ui.editing_state.is_some() {
        show_edit_state_banner(editor, ui);
    }
    if !editor.ui.master_view
        && editor
            .session
            .course
            .slide(&editor.selection.slide)
            .is_some()
    {
        show_layer_bar(editor, ui);
    }

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
    // While the eyedropper is picking, the canvas draws as usual but ignores
    // input: the picking click must not select or deselect anything.
    let eyedropper_owns_input = crate::eyedropper::swallows_canvas_input(ui.ctx());

    let slide_size = editor.session.course.slide_size;
    let (sw, sh) = (slide_size.w as f32, slide_size.h as f32);

    let pinch_scroll = pinch_canvas_zoom(editor, ui, viewport_rect, available_size, vec2(sw, sh));
    // A zoom step from a button or key keeps the slide point at the view centre in place.
    let pinch_scroll = pinch_scroll.or_else(|| {
        let changed = editor
            .ui
            .zoom_drawn
            .is_some_and(|drawn| drawn != editor.ui.zoom);
        editor.ui.zoom_drawn = Some(editor.ui.zoom);
        if !changed {
            return None;
        }
        keep_centre_on_zoom(editor, viewport_rect, available_size, vec2(sw, sh))
    });
    editor.ui.zoom_drawn = Some(editor.ui.zoom);

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

    let screen = ui
        .ctx()
        .data(|d| d.get_temp::<Rect>(egui::Id::unique("coursecraft_screen_rect")))
        .unwrap_or_else(|| ui.ctx().content_rect());
    let hud_bounds = viewport_rect.intersect(screen);
    let zoom_hud_w = 132.0;
    let zoom_hud_h = 32.0;
    let zoom_hud_pos = pos2(
        (hud_bounds.max.x - zoom_hud_w - 16.0).max(hud_bounds.min.x + 4.0),
        (hud_bounds.max.y - zoom_hud_h - 16.0).max(hud_bounds.min.y + 4.0),
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
        let base_slide_view = editor.session.course.editor.base_slide_view;
        let hide_base_view = editor.selection.layer.is_some()
            && base_slide_view == coursecraft_course::BaseSlideView::Hide;
        let selected_marker_id = editor.selection.object.as_deref().filter(|sel| {
            slide.object(sel).is_some_and(|(obj, _)| {
                matches!(obj.kind, coursecraft_course::ObjectKind::Marker { .. })
            })
        });
        let preview_state = editor
            .selection
            .object
            .as_deref()
            .zip(editor.ui.preview_state.as_deref());
        let editing_state = editor
            .ui
            .editing_state
            .as_ref()
            .map(|session| session.object_id.as_str());
        let inline_edit_target =
            editor
                .ui
                .inline_edit
                .as_ref()
                .map(|edit| crate::InlineEditTargetRef {
                    object: edit.object.as_str(),
                    cell: edit.cell,
                    is_extra: edit.extra_id.is_some(),
                });
        let media_fp = canvas_media_fingerprint(&editor.media);

        let cached_items = if let Some(hit) = editor.ui.canvas_compose_cache.get_if_matching(
            editor.session.revision,
            &editor.selection.slide,
            editor.selection.layer.as_deref(),
            &extra_layers,
            base_slide_view,
            selected_marker_id,
            preview_state,
            editing_state,
            inline_edit_target,
            media_fp,
        ) {
            hit
        } else {
            let dim_base = editor.selection.layer.is_some()
                && base_slide_view == coursecraft_course::BaseSlideView::Dim;
            #[cfg(test)]
            crate::guard_counters::note_canvas_compose();
            let mut composed = coursecraft_render::compose_authoring_layers_opts(
                &editor.session.course,
                slide,
                editor.selection.layer.as_deref(),
                &extra_layers,
                dim_base,
                !hide_base_view,
            );
            // One pass over the slide's objects, not a lookup per composed item.
            // The first object with an id wins, like `Slide::object`.
            let mut hidden_by_id: std::collections::HashMap<&str, bool> =
                std::collections::HashMap::new();
            for object in slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            {
                hidden_by_id
                    .entry(object.id.as_str())
                    .or_insert(object.editor_hidden);
            }
            if hidden_by_id.values().any(|hidden| *hidden) {
                composed.retain(|item| {
                    !matches!(&item.hit, Hit::Object(id)
                        if hidden_by_id.get(id.as_str()).copied().unwrap_or(false))
                });
            }
            if let Some(selected) = selected_marker_id {
                for item in &mut composed {
                    if matches!(&item.hit, Hit::Object(id) if id == selected)
                        && let Visual::Marker { open, .. } = &mut item.visual
                    {
                        *open = true;
                    }
                }
            }
            for item in &mut composed {
                if matches!(item.visual, Visual::Table(_)) {
                    item.show_table_overflow = true;
                }
            }
            if let Some((sel_obj_id, preview_state_name)) = preview_state
                && let Some((obj, _)) = slide.object(sel_obj_id)
            {
                apply_canvas_state_preview(
                    &mut composed,
                    obj,
                    preview_state_name,
                    Some(&editor.session.course.theme),
                    crate::states::course_input_value(&editor.session.course, obj),
                );
            }
            if let Some(edit) = &editor.ui.inline_edit {
                hide_edited_text(&mut composed, edit);
            }
            missing_images_as_placeholders(
                &mut composed,
                &editor.media,
                Some(editor.media.index()),
            );
            if let Some(edit_obj_id) = editing_state {
                for item in &mut composed {
                    item.dimmed = !matches!(&item.hit, Hit::Object(id) if id == edit_obj_id);
                }
            }
            let arc_items: std::sync::Arc<[DrawItem]> = std::sync::Arc::from(composed);
            let selected_marker_owned = selected_marker_id.map(str::to_owned);
            let preview_state_owned = preview_state.map(|(o, s)| (o.to_owned(), s.to_owned()));
            let editing_state_owned = editing_state.map(str::to_owned);
            let inline_edit_owned = inline_edit_target
                .map(|target| (target.object.to_owned(), target.cell, target.is_extra));
            editor.ui.canvas_compose_cache.store(
                editor.session.revision,
                &editor.selection.slide,
                editor.selection.layer.as_deref(),
                extra_layers,
                base_slide_view,
                selected_marker_owned.as_deref(),
                preview_state_owned
                    .as_ref()
                    .map(|(o, s)| (o.as_str(), s.as_str())),
                editing_state_owned.as_deref(),
                inline_edit_owned
                    .as_ref()
                    .map(|(o, cell, extra)| crate::InlineEditTargetRef {
                        object: o.as_str(),
                        cell: *cell,
                        is_extra: *extra,
                    }),
                media_fp,
                std::sync::Arc::clone(&arc_items),
            );
            arc_items
        };
        let mut drag_items: Option<Vec<DrawItem>> = None;

        let popup_was_open = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(egui::Id::unique("coursecraft_popup_open_prev_frame")))
            .unwrap_or(false);
        if !eyedropper_owns_input
            && !popup_was_open
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
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
            editor.ui.spacing_guides.clear();
            let other_modal = editor.ui.show_shortcuts_dialog
                || editor.ui.show_course_settings_window
                || editor.ui.show_theme_window
                || editor.edit_keys_blocked();
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
                if editor
                    .ui
                    .editing_state
                    .as_mut()
                    .is_some_and(|s| s.selected_extra.take().is_some())
                {
                    // Cleared selected_extra; keep editing_state active.
                } else if editor.ui.editing_state.is_some() {
                    editor.exit_edit_state();
                } else {
                    editor.selection.clear_objects();
                }
            }
        }

        let modifiers = ui.input(|i| i.modifiers);
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let current_pointer = ui
            .input(|i| i.pointer.interact_pos())
            .or_else(|| response.interact_pointer_pos());

        // A pointer over a menu, popup or window drawn over the canvas is treated
        // like one over the zoom HUD: it cannot place, draw, crop or click away.
        let pointer_in_hud = current_pointer.is_some_and(|p| zoom_hud_rect.contains(p))
            || current_pointer.is_some_and(|p| !crate::widgets::pointer_reaches(ui, p))
            || current_pointer.is_some_and(|p| {
                crate::rulers::captures(
                    &editor.session.course.editor,
                    &editor.ui.rulers,
                    canvas_rect,
                    &t,
                    p,
                )
            });
        let was_editing = editor.ui.inline_edit.is_some();
        let space_held = ui.input(|i| i.key_down(egui::Key::Space))
            && !was_editing
            && !ui.ctx().egui_wants_keyboard_input();
        let middle_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        let pan_active = !eyedropper_owns_input
            && !pointer_in_hud
            && !was_editing
            && editor.ui.crop_edit.is_none()
            && ((space_held
                && editor.ui.canvas_drag.is_none()
                && editor.ui.placement_drag.is_none())
                || (middle_down && current_pointer.is_some_and(|p| canvas_rect.contains(p))));
        if pan_active {
            if middle_down || ui.input(|i| i.pointer.primary_down()) {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                let delta = ui.input(|i| i.pointer.delta());
                if delta != egui::Vec2::ZERO && delta.x.is_finite() && delta.y.is_finite() {
                    ui.scroll_with_delta_animation(delta, egui::style::ScrollAnimation::none());
                }
            } else if space_held {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
        }

        let playhead_ms = editor.ui.timeline.playhead_ms;
        let time_view = editor.timeline_playback.is_playing() || playhead_ms > 0;
        let input_animated = if time_view {
            editor
                .session
                .course
                .slide(&editor.selection.slide)
                .map(|slide| {
                    let master = (!hide_base_view)
                        .then_some(slide.master.as_deref())
                        .flatten()
                        .and_then(|id| editor.session.course.master(id));
                    timeline_animated_items(
                        &cached_items,
                        slide,
                        master,
                        editor.session.course.slide_size,
                        playhead_ms,
                    )
                })
        } else {
            None
        };

        let gesture_active = editor.ui.canvas_drag.is_some()
            || editor.ui.placement_drag.is_some()
            || editor.ui.draw.is_some();
        let was_cropping = editor.ui.crop_edit.is_some();
        if was_cropping {
            crop_input(editor, ui, &t, current_pointer, canvas_rect, pointer_in_hud);
        } else if !eyedropper_owns_input
            && (!pointer_in_hud || gesture_active)
            && !was_editing
            && !pan_active
        {
            if matches!(editor.ui.placement_tool, Some(PlacementTool::Draw(_))) {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                process_draw_tool(editor, &t, ui, &response);
            } else if editor.ui.placement_tool.is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                let press_pos = if pointer_in_hud {
                    None
                } else {
                    ui.input(|i| {
                        i.pointer
                            .primary_pressed()
                            .then(|| i.pointer.interact_pos())
                    })
                    .flatten()
                    .filter(|p| canvas_rect.contains(*p))
                };
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
            } else if editor.ui.format_painter.is_some() {
                if !pointer_in_hud && current_pointer.is_some_and(|p| canvas_rect.contains(p)) {
                    ui.ctx().set_cursor_icon(CursorIcon::Copy);
                }
                if !pointer_in_hud
                    && (response.clicked() || response.drag_stopped())
                    && let Some(pos) = current_pointer
                {
                    let click_items = input_animated.as_deref().unwrap_or(&cached_items);
                    handle_canvas_click(editor, pos, &t, click_items, modifiers);
                }
            } else {
                if !pointer_in_hud
                    && editor.ui.canvas_drag.is_none()
                    && (response.is_pointer_button_down_on() || response.drag_started())
                    && primary_pressed
                    && let Some(start_pos) = current_pointer
                {
                    start_drag_if_pressed(
                        editor,
                        start_pos,
                        &t,
                        input_animated.as_deref().unwrap_or(&cached_items),
                        modifiers,
                    );
                }

                if editor.ui.canvas_drag.is_some() {
                    let mut owned = cached_items.to_vec();
                    process_active_drag(editor, &mut owned, &t, ui);
                    drag_items = Some(owned);
                }
                if !pointer_in_hud
                    && response.clicked()
                    && let Some(pos) = current_pointer
                {
                    let click_items = input_animated
                        .as_deref()
                        .unwrap_or(drag_items.as_deref().unwrap_or(&cached_items));
                    if response.double_clicked() {
                        handle_canvas_double_click(editor, pos, &t, click_items, modifiers);
                    } else {
                        handle_canvas_click(editor, pos, &t, click_items, modifiers);
                    }
                }
            }
        }

        let items: &[DrawItem] = drag_items.as_deref().unwrap_or(&cached_items);

        // Right-click acts on what is under the pointer. An unselected object
        // becomes the selection first; a selected one keeps a multi-selection.
        // Crop, inline text editing and placement tools keep their old menu.
        let special_mode = editor.ui.placement_tool.is_some()
            || editor.ui.crop_edit.is_some()
            || editor.ui.inline_edit.is_some();
        if !eyedropper_owns_input
            && !special_mode
            && response.secondary_clicked()
            && let Some(pos) = current_pointer
        {
            handle_canvas_secondary_click(
                editor,
                pos,
                &t,
                input_animated.as_deref().unwrap_or(items),
            );
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
        sync_inline_edit_live_rect_before_paint(editor, ui.ctx());
        let memory = crate::MemoryMediaSource::from_store(&editor.media);
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

        // While playing or scrubbed past 0, show objects at `playhead_ms`
        // (hidden outside their start/end, with entrance/exit progress). At
        // playhead 0 when paused, every object stays at rest for editing.
        let animated = if time_view {
            editor
                .session
                .course
                .slide(&editor.selection.slide)
                .map(|slide| {
                    let master = (!hide_base_view)
                        .then_some(slide.master.as_deref())
                        .flatten()
                        .and_then(|id| editor.session.course.master(id));
                    timeline_animated_items(
                        items,
                        slide,
                        master,
                        editor.session.course.slide_size,
                        playhead_ms,
                    )
                })
        } else {
            // A live preview (M46 phase 5) plays the selected objects' animation in place;
            // pressing on the canvas stops it.
            if editor.ui.anim_preview.is_playing()
                && ui.input(|i| {
                    i.pointer.any_pressed()
                        && i.pointer
                            .interact_pos()
                            .is_some_and(|p| t.slide_rect().contains(p))
                })
            {
                editor.ui.anim_preview.stop();
            }
            let ids = editor.selection.ids();
            let (now, pass) = (ui.input(|i| i.time), ui.ctx().cumulative_pass_nr());
            let previewed = editor.ui.anim_preview.items(
                &editor.session.course,
                (&editor.selection.slide, &ids),
                items,
                now,
                pass,
            );
            if previewed.is_some() {
                ui.ctx().request_repaint();
            }
            previewed
        };
        let base_view_items = animated.as_deref().unwrap_or(items);
        let live_view_items_owned = apply_inline_edit_live_rect(editor, base_view_items);
        let view_items = live_view_items_owned.as_deref().unwrap_or(base_view_items);
        paint(
            &painter,
            &t,
            view_items,
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
            draw_handles_and_outline(editor, &painter, &t, view_items, current_pointer, ui);
            draw_connector_sites(editor, &painter, &t, ui);
            draw_snap_guides(editor, &painter, &t, view_items, ui);
        }
        crate::rulers::show(editor, ui, &painter, &t, canvas_rect);
        if let Some(slide) = editor.session.course.slide(&editor.selection.slide) {
            let base_hidden = editor.selection.layer.is_some()
                && editor.session.course.editor.base_slide_view
                    == coursecraft_course::BaseSlideView::Hide;
            if !base_hidden
                && let Some(ref q) = slide.question
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

                        let minus_enabled = editor.zoom_out_level() != editor.ui.zoom;
                        if crate::widgets::icon_button_enabled_reason(
                            ui,
                            coursecraft_render::Icon::MagnifyingGlassMinus,
                            minus_enabled,
                            "Zoom out",
                            "Already at the smallest zoom",
                        )
                        .clicked()
                            && minus_enabled
                        {
                            editor.zoom_out();
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

                        let plus_enabled = editor.zoom_in_level() != editor.ui.zoom;
                        if crate::widgets::icon_button_enabled_reason(
                            ui,
                            coursecraft_render::Icon::MagnifyingGlassPlus,
                            plus_enabled,
                            "Zoom in",
                            "Already at the largest zoom",
                        )
                        .clicked()
                            && plus_enabled
                        {
                            editor.zoom_in();
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
    let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 26.0), Sense::click());
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
    resp.on_hover_text("Zoom level")
        .on_hover_cursor(CursorIcon::PointingHand)
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

/// The canvas items as they look at `playhead_ms` during timeline Play:
/// each object (and master object when `master` is `Some`) moved, scaled, faded
/// and clipped by its entrance or exit animation, and objects outside their
/// timing left out. Other non-object chrome (question rows, etc.) is kept as-is.
#[allow(clippy::too_many_arguments)]
fn animate_item_with_object(
    item: &DrawItem,
    obj: &coursecraft_course::Object,
    owner_slide: &coursecraft_course::Slide,
    owner_layer_id: Option<&str>,
    fallback_duration_ms: u32,
    slide_size: coursecraft_course::Size,
    playhead_ms: u32,
    group_id_prefix: &str,
    group_cache: &mut std::collections::HashMap<
        String,
        Option<Option<std::sync::Arc<coursecraft_render::GroupAnim>>>,
    >,
) -> Vec<DrawItem> {
    let group_anim = if let Some((group, owner)) = owner_slide.group_of_object(&obj.id)
        && group.has_animation()
    {
        let cache_key = format!("{group_id_prefix}{}", group.id);
        if let Some(cached) = group_cache.get(&cache_key) {
            match cached {
                None => return Vec::new(),
                Some(ga) => ga.clone(),
            }
        } else {
            let group_dur = owner
                .and_then(|l| owner_slide.layer(l))
                .map_or(fallback_duration_ms, |l| l.duration_ms);
            let res = owner_slide
                .group_timing(&group.id, owner)
                .zip(owner_slide.group_frame(&group.id, owner))
                .and_then(|(timing, frame)| {
                    let sample = coursecraft_logic::sample_group_animation(
                        group,
                        timing,
                        frame,
                        playhead_ms,
                        group_dur,
                        slide_size,
                    )?;
                    if sample.opacity <= 0.0 {
                        return None;
                    }
                    let ga = (sample != coursecraft_logic::ObjectSample::IDENTITY).then(|| {
                        std::sync::Arc::new(coursecraft_render::GroupAnim {
                            id: cache_key.clone(),
                            frame: egui::Rect::from_min_size(
                                egui::pos2(frame.x, frame.y),
                                egui::vec2(frame.w, frame.h),
                            ),
                            sample,
                        })
                    });
                    Some(ga)
                });
            group_cache.insert(cache_key, res.clone());
            match res {
                None => return Vec::new(),
                Some(ga) => ga,
            }
        }
    } else {
        None
    };
    let duration_ms = owner_layer_id
        .and_then(|layer| owner_slide.layer(layer))
        .map_or(fallback_duration_ms, |layer| layer.duration_ms);
    let Some(sample) =
        coursecraft_logic::sample_timed_animation(obj, playhead_ms, duration_ms, slide_size)
    else {
        return Vec::new();
    };
    // By paragraph or by letter: each part gets its own sample (M46 phase 7).
    if !item.is_state_extra
        && let Some(seq) = coursecraft_logic::plays_in_parts(obj)
        && let Some(mut parts) = coursecraft_render::sequence::split_parts(item, seq)
        && let Some(samples) = coursecraft_logic::sample_timed_parts(
            obj,
            playhead_ms,
            duration_ms,
            slide_size,
            parts.len(),
        )
        && samples.len() == parts.len()
    {
        for (part, s) in parts.iter_mut().zip(&samples) {
            coursecraft_render::apply_sample_to_items(std::slice::from_mut(part), s);
            if group_anim.is_some() {
                part.group_anim = group_anim.clone();
            }
        }
        parts.retain(|p| p.opacity > 0.0);
        return parts;
    }
    if sample.opacity <= 0.0 {
        return Vec::new();
    }
    let mut item = item.clone();
    coursecraft_render::apply_sample_to_items(std::slice::from_mut(&mut item), &sample);
    if group_anim.is_some() {
        item.group_anim = group_anim;
    }
    vec![item]
}

fn is_emitted_master_object(obj: &coursecraft_course::Object) -> bool {
    let f = &obj.frame;
    !matches!(obj.kind, coursecraft_course::ObjectKind::Audio { .. })
        && f.x.is_finite()
        && f.y.is_finite()
        && f.w.is_finite()
        && f.h.is_finite()
        && f.w >= 0.0
        && f.h >= 0.0
        && (f.x + f.w).is_finite()
        && (f.y + f.h).is_finite()
}

fn item_matches_master_object(item: &DrawItem, obj: &coursecraft_course::Object) -> bool {
    (item.rect.min.x - obj.frame.x).abs() < 0.5
        && (item.rect.min.y - obj.frame.y).abs() < 0.5
        && (item.rect.width() - obj.frame.w).abs() < 0.5
        && (item.rect.height() - obj.frame.h).abs() < 0.5
}

pub(crate) fn timeline_animated_items(
    items: &[DrawItem],
    slide: &coursecraft_course::Slide,
    master: Option<&coursecraft_course::Slide>,
    slide_size: coursecraft_course::Size,
    playhead_ms: u32,
) -> Vec<DrawItem> {
    let mut group_cache: std::collections::HashMap<
        String,
        Option<Option<std::sync::Arc<coursecraft_render::GroupAnim>>>,
    > = std::collections::HashMap::new();
    let mut master_cursor = 0usize;
    let mut in_master_prefix = master.is_some();
    items
        .iter()
        .flat_map(|item| {
            if let Hit::Object(id) = &item.hit {
                in_master_prefix = false;
                let Some((obj, layer_id)) = slide.object(id) else {
                    return vec![item.clone()];
                };
                return animate_item_with_object(
                    item,
                    obj,
                    slide,
                    layer_id,
                    slide.duration_ms,
                    slide_size,
                    playhead_ms,
                    "",
                    &mut group_cache,
                );
            }
            if in_master_prefix && let Some(master_slide) = master {
                if item.hit == Hit::None {
                    while let Some(obj) = master_slide.objects.get(master_cursor) {
                        if is_emitted_master_object(obj) {
                            break;
                        }
                        master_cursor += 1;
                    }
                    if let Some(obj) = master_slide.objects.get(master_cursor)
                        && item_matches_master_object(item, obj)
                    {
                        master_cursor += 1;
                        if obj.initial_state == "hidden" {
                            return Vec::new();
                        }
                        return animate_item_with_object(
                            item,
                            obj,
                            master_slide,
                            None,
                            slide.duration_ms,
                            slide_size,
                            playhead_ms,
                            "master:",
                            &mut group_cache,
                        );
                    }
                }
                in_master_prefix = false;
            }
            vec![item.clone()]
        })
        .collect()
}
