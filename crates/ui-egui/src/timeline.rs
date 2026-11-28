//! Timeline bottom panel: time ruler, object timing bars, move/resize dragging, and owner duration.

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::{Course, Cue, ObjectKind, Timing};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, DragValue, FontId, Id, Key, Rect, RichText, Sense,
    Stroke, StrokeKind, TextEdit, Ui, pos2, vec2,
};
use serde_json::Value as Json;
use std::collections::{BTreeSet, HashMap};

/// Default height of the bottom panel (user-resizable by dragging its top edge).
pub const TIMELINE_PANEL_HEIGHT: f32 = 260.0;
/// Minimum height of the bottom panel.
pub const TIMELINE_PANEL_MIN_HEIGHT: f32 = 140.0;
/// Maximum height of the bottom panel as a fraction of the window height.
pub const TIMELINE_PANEL_MAX_FRACTION: f32 = 0.65;
/// Height (px) of the grab zone just inside the bottom panel's top edge.
pub const PANEL_RESIZE_GRAB_PX: f32 = 8.0;
pub const TIMELINE_HEADER_HEIGHT: f32 = 28.0;
pub const TIME_RULER_HEIGHT: f32 = 22.0;
pub const TRACK_HEADER_WIDTH: f32 = 180.0;
pub const OBJECT_ROW_HEIGHT: f32 = 28.0;

/// Active tab in the bottom panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomTab {
    #[default]
    Timeline,
    States,
    Triggers,
    Variables,
    Question,
}

impl BottomTab {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Timeline => "Timeline",
            Self::States => "States",
            Self::Triggers => "Triggers",
            Self::Variables => "Variables",
            Self::Question => "Question",
        }
    }
}

/// Drag operation on an object's timeline bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineDragOp {
    Move,
    ResizeStart,
    ResizeEnd,
}

/// Active drag state on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineDrag {
    pub op: TimelineDragOp,
    pub object_id: String,
    pub orig_start_ms: u32,
    pub orig_end_ms: Option<u32>,
    pub orig_duration_ms: u32,
    pub start_pointer_x: f32,
    /// Pointer y at the press: a mostly vertical pull on a bar becomes a row drag.
    pub start_pointer_y: f32,
    pub current_start_ms: u32,
    pub current_end_ms: Option<u32>,
}

/// Active drag of one or more timeline rows in the name column.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRowDrag {
    /// Pointer y at the press, used for the plain-click threshold.
    pub start_pointer_y: f32,
    /// Ids moving together: the selection, with whole groups expanded.
    pub unit_ids: Vec<String>,
    /// True once the pointer moved past the click threshold.
    pub moved: bool,
    /// Insertion slot in the displayed (front-to-back) row list.
    pub target_slot: usize,
}

/// Active drag of one video cue marker.
#[derive(Debug, Clone, PartialEq)]
pub struct CueDrag {
    pub object_id: String,
    pub cue_id: String,
    pub orig_at_ms: f64,
    pub current_at_ms: f64,
    pub start_pointer_x: f32,
    pub duration_ms: u32,
}

/// Inline rename of one video cue opened by double-clicking its marker.
#[derive(Debug, Clone, PartialEq)]
pub struct CueRename {
    pub slide: String,
    pub object: String,
    pub cue: String,
    pub text: String,
    pub error: Option<String>,
}

/// Active drag of one slide/layer timeline cue marker.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideCueDrag {
    pub layer: Option<String>,
    pub cue_id: String,
    pub orig_at_ms: u32,
    pub current_at_ms: u32,
    pub start_pointer_x: f32,
    pub duration_ms: u32,
}

/// Inline rename of one slide/layer timeline cue opened by double-clicking its marker.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideCueRename {
    pub slide: String,
    pub layer: Option<String>,
    pub cue: String,
    pub text: String,
    pub error: Option<String>,
}

/// Inline rename of one object opened by double-clicking its name in the timeline's left column.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectRename {
    pub slide: String,
    pub object: String,
    pub buffer: String,
    pub original: String,
    pub just_entered: bool,
}

/// Cached object summary `(id, name, timing, group)` for timeline rendering.
pub type CachedTimelineObject = (String, String, Timing, Option<String>);

/// Active drag of a group's summary bar: every member's timing moves by the same amount.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupBarDrag {
    pub group: String,
    /// Member ids with their timing at the press.
    pub members: Vec<(String, Timing)>,
    pub start_pointer_x: f32,
    /// Current shift in milliseconds (snapped and clamped).
    pub delta_ms: i64,
    /// Smallest and largest shift that keep every member inside the timeline.
    pub min_delta_ms: i64,
    pub max_delta_ms: i64,
}

/// One row of the timeline's name column, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TimelineRow {
    /// A group's folder row; `members` are indices into the object list, top row first.
    Group { group: String, members: Vec<usize> },
    /// An object row; `indented` when it sits under an expanded group row.
    Object { model_idx: usize, indented: bool },
}

/// Rows in display order. A group with two or more members on this timeline gets a
/// folder row at the place of its frontmost member, followed by its members unless
/// the group is collapsed. The course model is unchanged; this is a view only.
pub(crate) fn timeline_rows(
    objects: &[CachedTimelineObject],
    collapsed: &BTreeSet<String>,
) -> Vec<TimelineRow> {
    let display: Vec<usize> = (0..objects.len()).rev().collect();
    let mut members_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for &idx in &display {
        if let Some((_, _, _, Some(group))) = objects.get(idx) {
            members_of.entry(group.as_str()).or_default().push(idx);
        }
    }
    let mut emitted: BTreeSet<&str> = BTreeSet::new();
    let mut rows = Vec::with_capacity(objects.len());
    for &idx in &display {
        let group = objects
            .get(idx)
            .and_then(|(_, _, _, group)| group.as_deref())
            .filter(|g| members_of.get(g).is_some_and(|m| m.len() >= 2));
        let Some(group) = group else {
            rows.push(TimelineRow::Object {
                model_idx: idx,
                indented: false,
            });
            continue;
        };
        if !emitted.insert(group) {
            continue;
        }
        let members = members_of.get(group).cloned().unwrap_or_default();
        let expanded = !collapsed.contains(group);
        rows.push(TimelineRow::Group {
            group: group.to_owned(),
            members: members.clone(),
        });
        if expanded {
            rows.extend(members.into_iter().map(|model_idx| TimelineRow::Object {
                model_idx,
                indented: true,
            }));
        }
    }
    rows
}

/// Interaction and edit state for the Timeline panel.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineState {
    pub last_synced_revision: crate::SyncRevision,
    pub last_synced_selection: crate::Selection,

    pub duration_ms: u32,
    pub active_drag: Option<TimelineDrag>,
    pub active_row_drag: Option<TimelineRowDrag>,
    /// Current timeline playhead in milliseconds.
    pub playhead_ms: u32,
    /// Active video cue marker drag.
    pub cue_drag: Option<CueDrag>,
    /// The selected cue marker, for Delete.
    pub cue_selected: Option<(String, String)>,
    /// Open inline cue rename.
    pub cue_rename: Option<CueRename>,
    /// Cue marker rects from the last frame, keyed by (object id, cue id).
    pub cue_marker_rects: HashMap<(String, String), Rect>,
    /// True when the timeline panel was the last-clicked panel (for `C` shortcut).
    pub focused: bool,
    /// Active slide/layer cue marker drag.
    pub slide_cue_drag: Option<SlideCueDrag>,
    /// Selected slide/layer cue marker `(layer, cue_id)`, for Delete.
    pub slide_cue_selected: Option<(Option<String>, String)>,
    /// Open inline slide/layer cue rename.
    pub slide_cue_rename: Option<SlideCueRename>,
    /// Open inline object rename in the left column.
    pub object_rename: Option<ObjectRename>,
    /// Slide/layer cue marker rects from the last frame, keyed by cue id.
    pub slide_cue_rects: HashMap<String, Rect>,

    /// Height the user last chose for the bottom panel (kept for the session).
    pub panel_height: f32,

    // Tracked rects for test interaction
    /// Outer rect of the whole bottom panel (as laid out last frame).
    pub panel_rect: Option<Rect>,
    pub duration_edit_rect: Option<Rect>,
    pub tab_rects: HashMap<String, Rect>,
    pub object_row_rects: HashMap<String, Rect>,
    pub object_bar_rects: HashMap<String, Rect>,
    pub object_left_handle_rects: HashMap<String, Rect>,
    pub object_right_handle_rects: HashMap<String, Rect>,
    /// Rects of the time-ruler labels painted in the last frame (for layout tests).
    pub ruler_label_rects: Vec<Rect>,
    /// Play/Pause button rect in the timeline header (for tests).
    pub play_button_rect: Option<Rect>,
    /// How far the track is zoomed in; 1.0 fits the whole duration.
    pub zoom: f32,
    /// How far the zoomed track is scrolled, in screen points.
    pub scroll_x: f32,

    /// Cached object list for the current revision + slide/layer.
    pub cached_objects: std::sync::Arc<[CachedTimelineObject]>,
    /// Cached slide/layer cue points for the current revision + slide/layer.
    pub cached_slide_cues: std::sync::Arc<[coursecraft_course::SlideCue]>,
    /// Cached audio sources for the current revision + slide/layer.
    pub cached_audio_srcs: std::sync::Arc<[String]>,

    /// Groups shown collapsed (one folder row). View state only: not saved in the course.
    pub collapsed_groups: BTreeSet<String>,
    /// Active drag of a group's summary bar.
    pub group_drag: Option<GroupBarDrag>,
    /// Group folder rows laid out last frame, keyed by group id (for tests).
    pub group_row_rects: HashMap<String, Rect>,
    /// Group summary bars laid out last frame, keyed by group id.
    pub group_bar_rects: HashMap<String, Rect>,
    /// Expand/collapse arrows laid out last frame, keyed by group id.
    pub group_toggle_rects: HashMap<String, Rect>,
    /// Group eye and lock buttons laid out last frame, keyed by group id.
    pub group_eye_rects: HashMap<String, Rect>,
    pub group_lock_rects: HashMap<String, Rect>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            last_synced_revision: crate::SyncRevision::default(),
            last_synced_selection: crate::Selection::default(),
            duration_ms: 0,
            active_drag: None,
            active_row_drag: None,
            playhead_ms: 0,
            cue_drag: None,
            cue_selected: None,
            cue_rename: None,
            cue_marker_rects: HashMap::new(),
            focused: false,
            slide_cue_drag: None,
            slide_cue_selected: None,
            slide_cue_rename: None,
            object_rename: None,
            slide_cue_rects: HashMap::new(),
            panel_height: TIMELINE_PANEL_HEIGHT,
            panel_rect: None,
            duration_edit_rect: None,
            tab_rects: HashMap::new(),
            object_row_rects: HashMap::new(),
            object_bar_rects: HashMap::new(),
            object_left_handle_rects: HashMap::new(),
            object_right_handle_rects: HashMap::new(),
            ruler_label_rects: Vec::new(),
            play_button_rect: None,
            zoom: 1.0,
            scroll_x: 0.0,
            cached_objects: std::sync::Arc::from([]),
            cached_slide_cues: std::sync::Arc::from([]),
            cached_audio_srcs: std::sync::Arc::from([]),
            collapsed_groups: BTreeSet::new(),
            group_drag: None,
            group_row_rects: HashMap::new(),
            group_bar_rects: HashMap::new(),
            group_toggle_rects: HashMap::new(),
            group_eye_rects: HashMap::new(),
            group_lock_rects: HashMap::new(),
        }
    }
}

/// Snap radius for playhead dragging, in screen points.
pub const PLAYHEAD_SNAP_PX: f32 = 6.0;

/// Format `ms` in seconds with one decimal and an `"s"` suffix (for example `5.0 s`).
fn format_seconds(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / 1000.0)
}

/// Format `ms` as `m:ss.d` (for example `0:03.2`).
#[must_use]
pub fn format_timeline_ms(ms: u32) -> String {
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let tenths = (ms % 1000) / 100;
    format!("{minutes}:{seconds:02}.{tenths}")
}

/// Playhead readout, for example `0:03.2 / 0:10.0`.
#[must_use]
pub fn format_timeline_readout(playhead_ms: u32, duration_ms: u32) -> String {
    format!(
        "{} / {}",
        format_timeline_ms(playhead_ms.min(duration_ms)),
        format_timeline_ms(duration_ms)
    )
}

/// Nearest cue time in `cue_ms` within [`PLAYHEAD_SNAP_PX`] points of `pointer_x`.
#[must_use]
pub fn nearest_cue_ms(
    pointer_x: f32,
    track_x: f32,
    track_width: f32,
    duration_ms: u32,
    cue_ms: &[u32],
) -> Option<u32> {
    let duration_ms = duration_ms.max(100);
    if !pointer_x.is_finite()
        || !track_x.is_finite()
        || !track_width.is_finite()
        || track_width <= 0.0
    {
        return None;
    }
    let mut nearest: Option<u32> = None;
    let mut nearest_dist = f32::INFINITY;
    for &cue in cue_ms {
        let cue = cue.min(duration_ms);
        let cue_x = track_x + ((cue as f32) / duration_ms as f32) * track_width;
        if !cue_x.is_finite() {
            continue;
        }
        let dist = (pointer_x - cue_x).abs();
        if dist <= PLAYHEAD_SNAP_PX && dist < nearest_dist {
            nearest = Some(cue);
            nearest_dist = dist;
        }
    }
    nearest.map(|cue| cue.min(duration_ms))
}

/// The playhead `pointer_x` asks for, snapping to the nearest cue within
/// [`PLAYHEAD_SNAP_PX`] points unless Shift is held.
#[must_use]
pub fn snapped_playhead_ms(
    pointer_x: f32,
    track_x: f32,
    track_width: f32,
    duration_ms: u32,
    cue_ms: &[u32],
    shift: bool,
) -> u32 {
    let duration_ms = duration_ms.max(100);
    if !pointer_x.is_finite()
        || !track_x.is_finite()
        || !track_width.is_finite()
        || track_width <= 0.0
    {
        return 0;
    }
    if !shift
        && let Some(cue) = nearest_cue_ms(pointer_x, track_x, track_width, duration_ms, cue_ms)
    {
        return cue;
    }
    let fraction = ((pointer_x - track_x) / track_width).clamp(0.0, 1.0);
    (fraction * duration_ms as f32)
        .round()
        .clamp(0.0, duration_ms as f32) as u32
}

impl TimelineState {
    /// Re-sync duration edit buffer if selection or revision changed.
    pub fn sync(&mut self, course: &Course, selection: &crate::Selection, revision: u64) {
        if self.last_synced_revision.is(revision) && &self.last_synced_selection == selection {
            return;
        }
        let owner_changed = !self.last_synced_revision.is(revision)
            || self.last_synced_selection.slide != selection.slide
            || self.last_synced_selection.layer != selection.layer;
        if self.last_synced_selection.slide != selection.slide {
            self.object_rename = None;
        }
        if self.last_synced_selection.slide != selection.slide
            || self.last_synced_selection.layer != selection.layer
        {
            self.group_drag = None;
        }
        self.last_synced_revision.set(revision);
        self.last_synced_selection = selection.clone();

        if let Some(slide) = course.slide(&selection.slide) {
            if owner_changed {
                #[cfg(test)]
                crate::guard_counters::note_timeline_list_rebuild();
                let layer = selection.layer.as_deref().and_then(|lid| slide.layer(lid));
                let objs = match (&selection.layer, layer) {
                    (Some(_), Some(l)) => l.objects.as_slice(),
                    (Some(_), None) => &[],
                    (None, _) => slide.objects.as_slice(),
                };
                self.cached_objects = objs
                    .iter()
                    .map(|o| (o.id.clone(), o.name.clone(), o.timing, o.group.clone()))
                    .collect();
                self.cached_slide_cues = match (&selection.layer, layer) {
                    (Some(_), Some(l)) => l.cue_points.clone().into(),
                    (Some(_), None) => std::sync::Arc::from([]),
                    (None, _) => slide.cue_points.clone().into(),
                };
                self.cached_audio_srcs = objs
                    .iter()
                    .filter_map(|o| match &o.kind {
                        coursecraft_course::ObjectKind::Audio { src, .. } => Some(src.clone()),
                        _ => None,
                    })
                    .collect();
            }
            if let Some(ref lid) = selection.layer
                && let Some(layer) = slide.layer(lid)
            {
                self.duration_ms = layer.duration_ms;
                return;
            }
            self.duration_ms = slide.duration_ms;
        } else if owner_changed {
            self.cached_objects = std::sync::Arc::from([]);
            self.cached_slide_cues = std::sync::Arc::from([]);
            self.cached_audio_srcs = std::sync::Arc::from([]);
        }
    }
}

/// A 1 px hairline across the whole bottom panel, bleeding through its horizontal inner margin.
fn bleed_separator(ui: &mut Ui) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.add_space(ui.spacing().item_spacing.y);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    let bleed = coursecraft_render::PANEL_MARGIN;
    ui.painter().line_segment(
        [
            pos2(rect.left() - bleed, rect.center().y),
            pos2(rect.right() + bleed, rect.center().y),
        ],
        Stroke::new(1.0, p.border),
    );
    ui.add_space(ui.spacing().item_spacing.y);
}

/// Hidden and locked object IDs on the active slide: its own objects and those
/// of every layer, so "Show all" and "Unlock all" mean the whole slide.
pub(crate) fn hidden_and_locked_ids(
    course: &Course,
    selection: &crate::Selection,
) -> (Vec<String>, Vec<String>) {
    let Some(slide) = course.slide(&selection.slide) else {
        return (Vec::new(), Vec::new());
    };
    let objs = || {
        slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
    };
    let hidden_ids = objs()
        .filter(|o| o.editor_hidden)
        .map(|o| o.id.clone())
        .collect();
    let locked_ids = objs().filter(|o| o.locked).map(|o| o.id.clone()).collect();
    (hidden_ids, locked_ids)
}

fn timeline_header_quick_button(
    ui: &mut Ui,
    icon: coursecraft_render::Icon,
    label: &str,
    tooltip: &str,
) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let btn = egui::Button::new(
        coursecraft_render::icon_text(icon, label)
            .color(p.accent_text)
            .size(coursecraft_render::SIZE_BODY),
    )
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::NONE)
    .corner_radius(coursecraft_render::RADIUS_SM as u8)
    .min_size(vec2(0.0, coursecraft_render::CONTROL_COMPACT));
    ui.add(btn)
        .on_hover_text(tooltip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[allow(clippy::too_many_arguments)]
fn render_timeline_header(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    _owner_title: &str,
    owner_duration: u32,
    is_layer: bool,
    selected_layer_id: Option<&str>,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let is_dark = coursecraft_render::theme_mode(ui.ctx()) == coursecraft_render::ThemeMode::Dark;
    let seg_selected_fill = if is_dark { p.pressed } else { p.raised };

    ui.horizontal(|ui| {
        egui::Frame::NONE
            // In light, the white thumb needs a tinted track to show against.
            .fill(if is_dark { p.input } else { p.hover })
            .corner_radius(coursecraft_render::RADIUS_MD as u8)
            .inner_margin(egui::Margin::same(2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                editor.ui.timeline.tab_rects.clear();
                // The selected tab's fill glides to it; painted behind the tabs once they are laid out.
                let thumb_slot = ui.painter().add(egui::Shape::Noop);
                let mut selected_rect = None;
                let mut tabs = vec![BottomTab::Timeline];
                // Docked, each bottom tab is a dock panel with its own dock tab.
                if editor.ui.dock.enabled {
                    tabs.clear();
                } else if !editor.ui.master_view {
                    tabs.extend([BottomTab::States, BottomTab::Triggers, BottomTab::Variables]);
                    // A draw slide has no question panel.
                    let slide_is_draw = editor
                        .session
                        .course
                        .slide(&editor.selection.slide)
                        .is_some_and(|slide| slide.draw.is_some());
                    if !slide_is_draw {
                        tabs.push(BottomTab::Question);
                    }
                } else if editor.ui.bottom_tab == BottomTab::States {
                    editor.ui.bottom_tab = BottomTab::Timeline;
                }
                for tab in tabs {
                    let is_sel = editor.ui.bottom_tab == tab;
                    let text = if is_sel {
                        RichText::new(tab.name()).color(p.text)
                    } else {
                        RichText::new(tab.name()).color(p.text_secondary)
                    };
                    let btn = egui::Button::new(text)
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(coursecraft_render::RADIUS_SM as u8)
                        .min_size(vec2(0.0, 22.0));
                    let tab_btn = ui.add(btn);
                    if is_sel {
                        selected_rect = Some(tab_btn.rect);
                    }
                    editor
                        .ui
                        .timeline
                        .tab_rects
                        .insert(tab.name().to_string(), tab_btn.rect);
                    if tab_btn.clicked() {
                        editor.ui.bottom_tab = tab;
                    }
                }
                if let Some(target) = selected_rect {
                    let ctx = ui.ctx().clone();
                    let id = ui.scope_id().with("bottom_tab_thumb");
                    let left = ctx.animate_value_with_time(id.with("l"), target.left(), 0.14);
                    let right = ctx.animate_value_with_time(id.with("r"), target.right(), 0.14);
                    let thumb = Rect::from_x_y_ranges(left..=right, target.y_range());
                    ui.painter().set(
                        thumb_slot,
                        egui::Shape::rect_filled(
                            thumb,
                            CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                            seg_selected_fill,
                        ),
                    );
                }
            });

        // Timeline Play/Pause plus the playhead readout. Play is
        // authoring-only (M12 step 2): it never runs triggers, cues,
        // layers or states, and never changes the document, so nothing
        // here goes on the undo stack. Space does the same when no
        // text field has focus (handled in `Editor::show`).
        let playing = editor.timeline_playback.is_playing();
        let (icon, tooltip) = if playing {
            (coursecraft_render::Icon::Pause, "Pause timeline (Space)")
        } else {
            (coursecraft_render::Icon::Play, "Play timeline (Space)")
        };
        let play_resp = crate::widgets::icon_button(ui, icon, tooltip);
        editor.ui.timeline.play_button_rect = Some(play_resp.rect);
        if play_resp.clicked() {
            editor.toggle_timeline_playback(owner_duration);
        }
        let readout = format_timeline_readout(editor.ui.timeline.playhead_ms, owner_duration);
        // In a narrow panel (a docked timeline under the canvas) the readout
        // gives way to the duration field rather than overlap it.
        let readout_w = ui
            .painter()
            .layout_no_wrap(
                readout.clone(),
                egui::TextStyle::Body.resolve(ui.style()),
                p.text_secondary,
            )
            .size()
            .x;
        if ui.available_width() >= readout_w + 140.0 {
            ui.label(RichText::new(readout).color(p.text_secondary));
        }

        let (hidden_ids, locked_ids) =
            hidden_and_locked_ids(&editor.session.course, &editor.selection);
        if !hidden_ids.is_empty() {
            let label = format!("Show all ({})", hidden_ids.len());
            if timeline_header_quick_button(
                ui,
                coursecraft_render::Icon::Eye,
                &label,
                "Show all hidden objects on this slide",
            )
            .clicked()
            {
                actions.push((
                    "object.hide_in_editor",
                    serde_json::json!({
                        "slide": slide_id,
                        "objects": hidden_ids,
                        "hidden": false,
                    }),
                ));
            }
        }
        if !locked_ids.is_empty() {
            let label = format!("Unlock all ({})", locked_ids.len());
            if timeline_header_quick_button(
                ui,
                coursecraft_render::Icon::LockOpen,
                &label,
                "Unlock all locked objects on this slide",
            )
            .clicked()
            {
                actions.push((
                    "object.lock",
                    serde_json::json!({
                        "slide": slide_id,
                        "objects": locked_ids,
                        "locked": false,
                    }),
                ));
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut duration_s = editor.ui.timeline.duration_ms as f64 / 1000.0;
            let dur_val = ui.add(
                DragValue::new(&mut duration_s)
                    .speed(0.1)
                    .range(0.1..=86_400.0)
                    .custom_formatter(|v, _| format!("{v:.1} s"))
                    .custom_parser(|s| s.trim().trim_end_matches('s').trim().parse::<f64>().ok()),
            );
            if dur_val.changed() {
                editor.ui.timeline.duration_ms =
                    (duration_s * 1000.0).round().clamp(100.0, 86_400_000.0) as u32;
            }
            editor.ui.timeline.duration_edit_rect = Some(dur_val.rect);
            let enter = dur_val.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (dur_val.drag_stopped() || dur_val.lost_focus() || enter)
                && editor.ui.timeline.duration_ms != owner_duration
            {
                if is_layer {
                    if let Some(lid) = selected_layer_id {
                        actions.push((
                            "layer.update",
                            serde_json::json!({
                                "slide": slide_id,
                                "layer": lid,
                                "duration_ms": editor.ui.timeline.duration_ms,
                            }),
                        ));
                    }
                } else {
                    actions.push((
                        "slide.update",
                        serde_json::json!({
                            "slide": slide_id,
                            "duration_ms": editor.ui.timeline.duration_ms,
                        }),
                    ));
                }
            }

            if ui.available_width() >= 60.0 {
                ui.label(coursecraft_render::caption("Duration").color(p.text_secondary));
            }
        });
    });
}

fn visible_ruler_ticks(
    duration_ms: u32,
    track_min_x: f32,
    track_width: f32,
    vis_min_x: f32,
    vis_max_x: f32,
) -> Vec<u32> {
    if duration_ms == 0
        || !track_width.is_finite()
        || track_width <= 0.0
        || !vis_min_x.is_finite()
        || !vis_max_x.is_finite()
        || vis_max_x < vis_min_x
    {
        return Vec::new();
    }
    let half_second_px = 500.0 / duration_ms as f32 * track_width;
    let tick_step: u32 = if half_second_px >= 120.0 {
        100
    } else if half_second_px >= 4.0 {
        500
    } else {
        const STEPS: &[u32] = &[
            1_000, 2_000, 5_000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000,
            1_800_000, 3_600_000, 7_200_000, 14_400_000, 28_800_000, 86_400_000,
        ];
        STEPS
            .iter()
            .copied()
            .find(|&step| (step as f64 / duration_ms as f64) * track_width as f64 >= 4.0)
            .unwrap_or_else(|| {
                ((4.0 / track_width as f64) * duration_ms as f64)
                    .ceil()
                    .clamp(1.0, u32::MAX as f64) as u32
            })
    };
    let start_frac = ((vis_min_x - track_min_x) as f64 / track_width as f64).clamp(0.0, 1.0);
    let end_frac = ((vis_max_x - track_min_x) as f64 / track_width as f64).clamp(0.0, 1.0);
    let start_ms = (start_frac * duration_ms as f64).floor() as u32;
    let end_ms = (end_frac * duration_ms as f64)
        .ceil()
        .min(duration_ms as f64) as u32;
    let mut ms_tick = (start_ms / tick_step) * tick_step;
    let mut ticks = Vec::new();
    while ms_tick <= end_ms {
        ticks.push(ms_tick);
        let next = ms_tick.saturating_add(tick_step);
        if next <= ms_tick {
            break;
        }
        ms_tick = next;
    }
    ticks
}

fn render_time_ruler(
    painter: &egui::Painter,
    track_rect: Rect,
    ruler_rect: Rect,
    duration_ms: u32,
    track_width: f32,
    total_area: Rect,
    label_rects: &mut Vec<Rect>,
) {
    let p = coursecraft_render::palette(painter.ctx());
    painter.rect_filled(ruler_rect, CornerRadius::ZERO, p.panel);
    painter.line_segment(
        [
            pos2(total_area.min.x, ruler_rect.max.y),
            pos2(total_area.max.x, ruler_rect.max.y),
        ],
        Stroke::new(1.0, p.border),
    );
    if duration_ms == 0 || !track_width.is_finite() || track_width <= 0.0 {
        return;
    }

    let ms_to_x =
        |ms: u32| -> f32 { track_rect.min.x + (ms as f32 / duration_ms as f32) * track_width };

    // Ticks and grid lines stay in the track when it is zoomed and scrolled.
    let label_painter = painter;
    let clipped_rect = painter.clip_rect().intersect(Rect::from_x_y_ranges(
        ruler_rect.x_range(),
        painter.clip_rect().y_range(),
    ));
    let painter = &painter.with_clip_rect(clipped_rect);
    let vis_min_x = clipped_rect.min.x;
    let vis_max_x = clipped_rect.max.x;
    // Label every 1, 2, 5, 10, ... seconds, whatever keeps labels about 40 px apart.
    let second_px = 1000.0 / duration_ms.max(1) as f32 * track_width;
    let label_every_s: u32 = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600]
        .into_iter()
        .find(|step| *step as f32 * second_px >= 40.0)
        .unwrap_or(600);
    let mut last_label_right = f32::NEG_INFINITY;
    for ms_tick in visible_ruler_ticks(
        duration_ms,
        track_rect.min.x,
        track_width,
        vis_min_x,
        vis_max_x,
    ) {
        let x = ms_to_x(ms_tick);
        let is_sec = ms_tick.is_multiple_of(1000);
        let labelled = is_sec && (ms_tick / 1000).is_multiple_of(label_every_s);
        let tick_h = if is_sec { 12.0 } else { 6.0 };
        painter.line_segment(
            [
                pos2(x, ruler_rect.max.y - tick_h),
                pos2(x, ruler_rect.max.y),
            ],
            Stroke::new(1.0, p.text_tertiary),
        );
        if labelled && (ruler_rect.min.x - 0.5..=ruler_rect.max.x + 0.5).contains(&x) {
            let galley = label_painter.layout_no_wrap(
                format!("{}s", ms_tick / 1000),
                FontId::proportional(11.0),
                p.text_tertiary,
            );
            // Keep every label inside the ruler: right-align the last label to the
            // edge instead of letting it run past the panel.
            let max_x = (ruler_rect.max.x - galley.size().x).max(ruler_rect.min.x);
            let label_x = (x + 2.0).clamp(ruler_rect.min.x, max_x);
            // The last label is pulled in to the edge; drop it rather than draw
            // it on top of the one before.
            if label_x >= last_label_right + 4.0 {
                let label_pos = pos2(label_x, ruler_rect.min.y + 2.0);
                last_label_right = label_x + galley.size().x;
                label_rects.push(Rect::from_min_size(label_pos, galley.size()));
                label_painter.galley(label_pos, galley, p.text_tertiary);
            }
        }
        painter.line_segment(
            [pos2(x, ruler_rect.max.y), pos2(x, total_area.max.y)],
            Stroke::new(1.0, p.border),
        );
    }
}

struct ActiveDragContext<'a> {
    duration_ms: u32,
    track_x: f32,
    track_width: f32,
    cue_ms: &'a [u32],
    slide_id: &'a str,
}

fn handle_active_drag(
    editor: &mut Editor,
    ui: &mut Ui,
    ctx: &ActiveDragContext<'_>,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let ActiveDragContext {
        duration_ms,
        track_x,
        track_width,
        cue_ms,
        slide_id,
    } = *ctx;
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        editor.ui.timeline.active_drag = None;
        return;
    }

    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_down = ui.input(|i| i.pointer.primary_down());
    let is_shift = ui.input(|i| i.modifiers.shift);

    let Some(ref mut drag) = editor.ui.timeline.active_drag else {
        return;
    };

    if pointer_down && let Some(pos) = pointer_pos {
        let delta_x = pos.x - drag.start_pointer_x;
        let snap_step: i64 = if is_shift { 10 } else { 100 };
        let delta_ms_raw = (delta_x / track_width * duration_ms as f32).round() as i64;
        let delta_ms = (delta_ms_raw / snap_step) * snap_step;
        let ms_to_x = |ms: i64| -> f32 { track_x + (ms as f32 / duration_ms as f32) * track_width };

        match drag.op {
            TimelineDragOp::ResizeStart => {
                let eff_end = drag.orig_end_ms.unwrap_or(drag.orig_duration_ms);
                let max_start = eff_end.saturating_sub(snap_step as u32);
                let raw_unrounded = drag.orig_start_ms as i64 + delta_ms_raw;
                let snapped_cue = if is_shift {
                    None
                } else {
                    nearest_cue_ms(
                        ms_to_x(raw_unrounded),
                        track_x,
                        track_width,
                        duration_ms,
                        cue_ms,
                    )
                };
                let new_start = snapped_cue
                    .map(i64::from)
                    .unwrap_or(drag.orig_start_ms as i64 + delta_ms);
                drag.current_start_ms = new_start.clamp(0, max_start as i64) as u32;
                drag.current_end_ms = drag.orig_end_ms;
            }
            TimelineDragOp::ResizeEnd => {
                let eff_end = drag.orig_end_ms.unwrap_or(drag.orig_duration_ms);
                let min_end = drag.orig_start_ms.saturating_add(snap_step as u32);
                let max_end = drag.orig_duration_ms.max(min_end);
                let raw_unrounded = eff_end as i64 + delta_ms_raw;
                let snapped_cue = if is_shift {
                    None
                } else {
                    nearest_cue_ms(
                        ms_to_x(raw_unrounded),
                        track_x,
                        track_width,
                        duration_ms,
                        cue_ms,
                    )
                };
                let raw_end = snapped_cue
                    .map(i64::from)
                    .unwrap_or(eff_end as i64 + delta_ms);
                let new_end =
                    coursecraft_course::safe_clamp_ord(raw_end, min_end as i64, max_end as i64)
                        as u32;
                drag.current_start_ms = drag.orig_start_ms;
                drag.current_end_ms = Some(new_end);
            }
            TimelineDragOp::Move => {
                if let Some(orig_end) = drag.orig_end_ms {
                    let len = orig_end
                        .saturating_sub(drag.orig_start_ms)
                        .max(snap_step as u32);
                    let max_start = drag.orig_duration_ms.saturating_sub(len);
                    let raw_unrounded = drag.orig_start_ms as i64 + delta_ms_raw;
                    let snapped_start = if is_shift {
                        None
                    } else {
                        nearest_cue_ms(
                            ms_to_x(raw_unrounded),
                            track_x,
                            track_width,
                            duration_ms,
                            cue_ms,
                        )
                        .or_else(|| {
                            nearest_cue_ms(
                                ms_to_x(raw_unrounded + i64::from(len)),
                                track_x,
                                track_width,
                                duration_ms,
                                cue_ms,
                            )
                            .map(|cue| cue.saturating_sub(len))
                        })
                    };
                    let raw_start = snapped_start
                        .map(i64::from)
                        .unwrap_or(drag.orig_start_ms as i64 + delta_ms);
                    let new_start = raw_start.clamp(0, max_start as i64) as u32;
                    drag.current_start_ms = new_start;
                    drag.current_end_ms = Some(new_start + len);
                } else {
                    let max_start = drag.orig_duration_ms.saturating_sub(snap_step as u32);
                    let raw_unrounded = drag.orig_start_ms as i64 + delta_ms_raw;
                    let snapped_start = if is_shift {
                        None
                    } else {
                        nearest_cue_ms(
                            ms_to_x(raw_unrounded),
                            track_x,
                            track_width,
                            duration_ms,
                            cue_ms,
                        )
                    };
                    let raw_start = snapped_start
                        .map(i64::from)
                        .unwrap_or(drag.orig_start_ms as i64 + delta_ms);
                    let new_start = raw_start.clamp(0, max_start as i64) as u32;
                    drag.current_start_ms = new_start;
                    drag.current_end_ms = None;
                }
            }
        }
    } else if !pointer_down {
        let finished_drag = editor.ui.timeline.active_drag.take();
        if let Some(drag) = finished_drag {
            let new_timing = Timing {
                start_ms: drag.current_start_ms,
                end_ms: drag.current_end_ms,
            };
            let orig_timing = Timing {
                start_ms: drag.orig_start_ms,
                end_ms: drag.orig_end_ms,
            };
            if new_timing != orig_timing {
                actions.push((
                    "object.update",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": drag.object_id,
                        "timing": new_timing,
                    }),
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bar_or_preview(
    editor: &Editor,
    painter: &egui::Painter,
    oid: &str,
    bar_rect: Rect,
    row_y: f32,
    is_selected: bool,
    eff_start: u32,
    timing: &Timing,
    duration_ms: u32,
    track_width: f32,
    track_rect: Rect,
) {
    let p = coursecraft_render::palette(painter.ctx());
    let is_dragging_this = editor
        .ui
        .timeline
        .active_drag
        .as_ref()
        .map(|d| d.object_id == oid)
        .unwrap_or(false);

    let ms_to_x =
        |ms: u32| -> f32 { track_rect.min.x + (ms as f32 / duration_ms as f32) * track_width };

    if is_dragging_this && let Some(ref drag) = editor.ui.timeline.active_drag {
        painter.rect_filled(bar_rect, CornerRadius::same(4), p.accent_subtle);

        let p_start_x = ms_to_x(drag.current_start_ms);
        let p_end_x = ms_to_x(drag.current_end_ms.unwrap_or(duration_ms)).max(p_start_x + 6.0);
        let preview_rect =
            Rect::from_min_max(pos2(p_start_x, row_y + 4.0), pos2(p_end_x, row_y + 24.0));

        painter.rect_filled(preview_rect, CornerRadius::same(4), p.accent);
        painter.rect_stroke(
            preview_rect,
            CornerRadius::same(4),
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Outside,
        );

        let preview_label = format!(
            "{} - {}",
            format_seconds(u64::from(drag.current_start_ms)),
            drag.current_end_ms
                .map(|e| format_seconds(u64::from(e)))
                .unwrap_or_else(|| "end".to_owned())
        );
        painter.text(
            preview_rect.center(),
            Align2::CENTER_CENTER,
            preview_label,
            FontId::proportional(11.0),
            Color32::WHITE,
        );
    } else {
        let radius = CornerRadius::same(coursecraft_render::RADIUS_MD as u8);
        let bar_color = if is_selected { p.accent } else { p.pressed };
        painter.rect_filled(bar_rect, radius, bar_color);
        paint_animation_marks(
            editor,
            painter,
            oid,
            bar_rect,
            timing,
            duration_ms,
            &ms_to_x,
        );

        if let Some((peaks, file_dur_ms, trim)) = audio_waveform_for_object(editor, oid)
            && !peaks.is_empty()
            && bar_rect.width() >= 4.0
        {
            let shift = group_drag_shift(editor, oid);
            let eff_end = timing
                .end_ms
                .map_or(duration_ms, |end| shifted_ms(end, shift));
            let bar_len_ms = u64::from(eff_end.saturating_sub(eff_start));
            let window = match file_dur_ms {
                Some(dur) if dur > 0 => audio_waveform_window(dur, bar_len_ms, trim),
                _ => Some((0.0, 1.0, 1.0)),
            };
            if let Some((peak_start_frac, peak_end_frac, bar_fill_frac)) = window {
                let wf_painter = painter.with_clip_rect(painter.clip_rect().intersect(bar_rect));
                let center_y = bar_rect.center().y;
                let max_half_h = (bar_rect.height() * 0.42).max(1.0);
                let wf_color = Color32::from_white_alpha(if is_selected { 175 } else { 135 });
                let stroke = Stroke::new(1.0, wf_color);
                let step = 2.0_f32;
                let wf_width = (bar_rect.width() * bar_fill_frac).max(1.0);
                let wf_max_x = (bar_rect.min.x + wf_width).min(bar_rect.max.x - 1.0);
                let peak_span = (peak_end_frac - peak_start_frac).max(0.0);
                let n = peaks.len();
                // Draw only what is visible: a zoomed-in bar can be far wider
                // than the panel, and the clip would discard the rest.
                let visible = wf_painter.clip_rect();
                let first_x = bar_rect.min.x + 1.0;
                let skipped = ((visible.min.x - first_x) / step).floor().max(0.0);
                let mut x = first_x + skipped * step;
                let wf_max_x = wf_max_x.min(visible.max.x + step);
                while x < wf_max_x {
                    let u0 = ((x - bar_rect.min.x) / wf_width).clamp(0.0, 1.0);
                    let u1 = ((x + step - bar_rect.min.x) / wf_width).clamp(0.0, 1.0);
                    let t0 = (peak_start_frac + u0 * peak_span).clamp(0.0, 1.0);
                    let t1 = (peak_start_frac + u1 * peak_span).clamp(0.0, 1.0);
                    let i0 = ((t0 * n as f32).floor() as usize).min(n - 1);
                    let i1 = ((t1 * n as f32).ceil() as usize).max(i0 + 1).min(n);
                    let peak = peaks[i0..i1].iter().copied().fold(0.0_f32, f32::max);
                    let half_h = (peak * max_half_h).max(1.0);
                    wf_painter.line_segment(
                        [pos2(x, center_y - half_h), pos2(x, center_y + half_h)],
                        stroke,
                    );
                    x += step;
                }
            }
        }

        // Trim grips only on the selected bar; the resize cursor teaches the rest.
        if is_selected {
            let grip = Stroke::new(1.0, Color32::from_white_alpha(120));
            for x in [bar_rect.min.x + 4.0, bar_rect.max.x - 4.0] {
                painter.line_segment(
                    [pos2(x, bar_rect.min.y + 5.0), pos2(x, bar_rect.max.y - 5.0)],
                    grip,
                );
            }
        }
    }
}

/// The object's animations as strips along the bottom of its bar (M46 phase 6): entrance
/// green at the start, exit red at the end, each emphasis amber where it plays.
fn paint_animation_marks(
    editor: &Editor,
    painter: &egui::Painter,
    oid: &str,
    bar_rect: Rect,
    timing: &Timing,
    duration_ms: u32,
    ms_to_x: &dyn Fn(u32) -> f32,
) {
    let Some((obj, _)) = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .and_then(|s| s.object(oid))
    else {
        return;
    };
    let start = timing.start_ms;
    let end = timing.end_ms.unwrap_or(duration_ms);
    paint_anim_strips(
        painter,
        obj.entrance.as_ref(),
        obj.exit.as_ref(),
        &obj.emphasis,
        start,
        end,
        bar_rect,
        ms_to_x,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_anim_strips(
    painter: &egui::Painter,
    entrance: Option<&coursecraft_course::Animation>,
    exit: Option<&coursecraft_course::Animation>,
    emphasis: &[coursecraft_course::Emphasis],
    start: u32,
    end: u32,
    bar_rect: Rect,
    ms_to_x: &dyn Fn(u32) -> f32,
) {
    let p = coursecraft_render::palette(painter.ctx());
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(bar_rect));
    let strip = |from: u32, to: u32, color: Color32| {
        let x0 = ms_to_x(from).max(bar_rect.min.x);
        let x1 = ms_to_x(to).min(bar_rect.max.x).max(x0 + 2.0);
        painter.rect_filled(
            Rect::from_min_max(
                pos2(x0, bar_rect.max.y - 4.0),
                pos2(x1, bar_rect.max.y - 1.0),
            ),
            CornerRadius::same(1),
            color,
        );
    };
    if let Some(a) = entrance {
        strip(
            start,
            start.saturating_add(a.duration_ms).min(end),
            p.success,
        );
    }
    if let Some(a) = exit {
        strip(end.saturating_sub(a.duration_ms).max(start), end, p.danger);
    }
    for e in emphasis {
        strip(e.start_ms.max(start), e.end_ms().min(end), p.warning);
    }
}

fn select_timeline_row(
    editor: &mut Editor,
    objects: &[(String, String, Timing, Option<String>)],
    oid: &str,
    modifiers: egui::Modifiers,
) {
    editor.ui.last_active_area = crate::ActiveArea::Canvas;
    if modifiers.shift {
        let ids: Vec<String> = objects.iter().map(|(id, _, _, _)| id.clone()).collect();
        let primary = editor
            .selection
            .object
            .as_ref()
            .and_then(|primary| ids.iter().position(|id| id == primary));
        let clicked = ids.iter().position(|id| id == oid);
        if let (Some(primary), Some(clicked)) = (primary, clicked) {
            let (start, end) = if primary <= clicked {
                (primary, clicked)
            } else {
                (clicked, primary)
            };
            editor.selection.set_many(
                ids.get(start..=end)
                    .map(<[String]>::to_vec)
                    .unwrap_or_default(),
            );
        } else {
            editor.selection.set_many([oid.to_owned()]);
        }
        editor.expand_selection_groups();
        editor.ui.group_focus = None;
    } else if modifiers.command {
        editor.toggle_object_group(oid);
    } else if !editor.selection.contains(oid) {
        editor.select_object_group(oid);
    }
}

/// Objects in displayed row order: the frontmost object is the top row.
fn display_ids(objects: &[(String, String, Timing, Option<String>)]) -> Vec<String> {
    objects.iter().rev().map(|(id, ..)| id.clone()).collect()
}

/// Ids the press should move: the selection plus every member of any selected group,
/// restricted to the rows this panel shows, in stacking (model) order.
fn row_unit_ids(
    editor: &Editor,
    objects: &[(String, String, Timing, Option<String>)],
    object_id: &str,
) -> Vec<String> {
    let on_row: BTreeSet<&str> = objects.iter().map(|(id, ..)| id.as_str()).collect();
    let mut wanted: BTreeSet<String> = editor
        .selection
        .ids()
        .into_iter()
        .filter(|id| on_row.contains(id.as_str()))
        .collect();
    if !wanted.contains(object_id) {
        wanted.clear();
        wanted.insert(object_id.to_owned());
    }
    let groups: BTreeSet<&str> = objects
        .iter()
        .filter(|(id, ..)| wanted.contains(id.as_str()))
        .filter_map(|(_, _, _, group)| group.as_deref())
        .collect();
    for (id, _, _, group) in objects {
        if let Some(group) = group
            && groups.contains(group.as_str())
        {
            wanted.insert(id.clone());
        }
    }
    objects
        .iter()
        .filter(|(id, ..)| wanted.contains(id.as_str()))
        .map(|(id, ..)| id.clone())
        .collect()
}

/// Insertion slot in the displayed list for a pointer y, using row centres.
fn drop_slot(display: &[String], row_rects: &HashMap<String, Rect>, pointer_y: f32) -> usize {
    for (index, id) in display.iter().enumerate() {
        if let Some(rect) = row_rects.get(id)
            && pointer_y < rect.center().y
        {
            return index;
        }
    }
    display.len()
}

/// Move `unit` to `slot` in the displayed list, gathering it into one block.
fn reorder_display_order(display: &[String], unit: &BTreeSet<&str>, slot: usize) -> Vec<String> {
    let slot = slot.min(display.len());
    let before = display
        .iter()
        .take(slot)
        .filter(|id| unit.contains(id.as_str()))
        .count();
    let mut remaining: Vec<String> = display
        .iter()
        .filter(|id| !unit.contains(id.as_str()))
        .cloned()
        .collect();
    let block: Vec<String> = display
        .iter()
        .filter(|id| unit.contains(id.as_str()))
        .cloned()
        .collect();
    let at = slot.saturating_sub(before).min(remaining.len());
    remaining.splice(at..at, block);
    remaining
}

/// y of the drop line for `slot`, or `None` when no row is laid out.
fn drop_line_y(display: &[String], row_rects: &HashMap<String, Rect>, slot: usize) -> Option<f32> {
    if let Some(id) = display.get(slot)
        && let Some(rect) = row_rects.get(id)
    {
        return Some(rect.min.y);
    }
    display
        .last()
        .and_then(|id| row_rects.get(id))
        .map(|rect| rect.max.y)
}

/// Pointer travel that decides a bar drag is a reorder rather than a timing move.
const BAR_REORDER_PULL: f32 = 8.0;

/// A move of a bar pulled mostly up or down is a reorder, so hand it to the row drag.
/// Bars usually fill their row, so this is where people grab first.
fn promote_bar_drag_to_row_drag(
    editor: &mut Editor,
    ui: &Ui,
    objects: &[(String, String, Timing, Option<String>)],
) {
    let Some(drag) = editor.ui.timeline.active_drag.as_ref() else {
        return;
    };
    if !matches!(drag.op, TimelineDragOp::Move) {
        return;
    }
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let pull_y = (pos.y - drag.start_pointer_y).abs();
    let pull_x = (pos.x - drag.start_pointer_x).abs();
    if pull_y < BAR_REORDER_PULL || pull_y <= pull_x {
        return;
    }
    let object_id = drag.object_id.clone();
    let start_pointer_y = drag.start_pointer_y;
    let unit_ids = row_unit_ids(editor, objects, &object_id);
    let target_slot = display_ids(objects)
        .iter()
        .position(|id| *id == object_id)
        .unwrap_or(0);
    editor.ui.timeline.active_drag = None;
    editor.ui.timeline.active_row_drag = Some(TimelineRowDrag {
        start_pointer_y,
        unit_ids,
        moved: true,
        target_slot,
    });
}

/// The name of what is being dragged, carried beside the pointer.
fn paint_row_drag_label(
    ui: &Ui,
    objects: &[(String, String, Timing, Option<String>)],
    drag: &TimelineRowDrag,
    pos: egui::Pos2,
) {
    let label = match drag.unit_ids.as_slice() {
        [only] => objects
            .iter()
            .find(|(id, ..)| id == only)
            .map(|(id, name, ..)| {
                if name.is_empty() {
                    id.clone()
                } else {
                    name.clone()
                }
            })
            .unwrap_or_default(),
        many => format!("{} objects", many.len()),
    };
    let p = coursecraft_render::palette(ui.ctx());
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        ui.scope_id().with("timeline_row_drag_label"),
    ));
    let galley = painter.layout_no_wrap(label, FontId::proportional(13.0), p.text);
    let rect = Rect::from_min_size(pos + vec2(12.0, -10.0), galley.size() + vec2(16.0, 8.0));
    painter.rect_filled(rect, CornerRadius::same(4), p.hover);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, p.accent),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(8.0, 4.0), galley, p.text);
}

#[allow(clippy::too_many_arguments)]
fn handle_row_drag(
    editor: &mut Editor,
    ui: &mut Ui,
    painter: &egui::Painter,
    objects: &[(String, String, Timing, Option<String>)],
    total_area: Rect,
    row_rects: &HashMap<String, Rect>,
) {
    let Some(mut drag) = editor.ui.timeline.active_row_drag.take() else {
        return;
    };
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_down = ui.input(|i| i.pointer.primary_down());

    if pointer_down {
        if let Some(pos) = pointer_pos {
            if (pos.y - drag.start_pointer_y).abs() >= 4.0 {
                drag.moved = true;
            }
            if drag.moved {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                paint_row_drag_label(ui, objects, &drag, pos);
                // Scroll the rows when the pointer nears the visible top or bottom edge.
                let edge = OBJECT_ROW_HEIGHT;
                if pos.y < total_area.min.y + edge {
                    ui.scroll_with_delta(vec2(0.0, OBJECT_ROW_HEIGHT * 0.5));
                    ui.ctx().request_repaint();
                } else if pos.y > total_area.max.y - edge {
                    ui.scroll_with_delta(vec2(0.0, -OBJECT_ROW_HEIGHT * 0.5));
                    ui.ctx().request_repaint();
                }
                let display = display_ids(objects);
                drag.target_slot = drop_slot(&display, row_rects, pos.y);
                if let Some(y) = drop_line_y(&display, row_rects, drag.target_slot) {
                    let p = coursecraft_render::palette(ui.ctx());
                    painter.line_segment(
                        [pos2(total_area.min.x, y), pos2(total_area.max.x, y)],
                        Stroke::new(2.0, p.accent),
                    );
                }
            }
        }
        editor.ui.timeline.active_row_drag = Some(drag);
    } else if drag.moved {
        let display = display_ids(objects);
        let unit: BTreeSet<&str> = drag.unit_ids.iter().map(String::as_str).collect();
        let target_display = reorder_display_order(&display, &unit, drag.target_slot);
        let target_model: Vec<String> = target_display.into_iter().rev().collect();
        let current_model: Vec<String> = objects.iter().map(|(id, ..)| id.clone()).collect();
        if target_model != current_model {
            editor.reorder_selection_to_order(target_model, drag.unit_ids);
        }
    }
}

/// Portion of a full-file peak envelope to draw in a timeline bar at real time scale.
///
/// Returns `Some((peak_start_frac, peak_end_frac, bar_fill_frac))`, all in `0.0..=1.0`:
/// - `peak_start_frac..peak_end_frac` is the kept slice of the file's peak array.
/// - `bar_fill_frac` is the fraction of the bar width occupied by audio (`< 1.0` when
///   the bar is longer than the remaining audio).
///
/// Returns `None` when nothing is audible in the bar (zero-length bar or trim past end).
pub(crate) fn audio_waveform_window(
    file_duration_ms: u64,
    bar_len_ms: u64,
    trim: Option<coursecraft_course::Trim>,
) -> Option<(f32, f32, f32)> {
    if file_duration_ms == 0 || bar_len_ms == 0 {
        return None;
    }
    let clip_start = trim.map_or(0, |t| t.start_ms).min(file_duration_ms);
    let clip_end = trim
        .and_then(|t| t.end_ms)
        .map_or(file_duration_ms, |e| e.min(file_duration_ms))
        .max(clip_start);
    let shown_end = clip_start.saturating_add(bar_len_ms).min(clip_end);
    let drawn_ms = shown_end.saturating_sub(clip_start);
    if drawn_ms == 0 {
        return None;
    }
    let peak_start = (clip_start as f64 / file_duration_ms as f64).clamp(0.0, 1.0) as f32;
    let peak_end =
        (shown_end as f64 / file_duration_ms as f64).clamp(peak_start as f64, 1.0) as f32;
    let bar_fill = (drawn_ms as f64 / bar_len_ms as f64).clamp(0.0, 1.0) as f32;
    Some((peak_start, peak_end, bar_fill))
}

fn audio_waveform_for_object<'a>(
    editor: &'a Editor,
    oid: &str,
) -> Option<(&'a [f32], Option<u64>, Option<coursecraft_course::Trim>)> {
    let slide = editor.session.course.slide(&editor.selection.slide)?;
    let (obj, _) = slide.object(oid)?;
    let coursecraft_course::ObjectKind::Audio { src, trim, .. } = &obj.kind else {
        return None;
    };
    let peaks = editor.audio_waveforms.get(src)?.as_deref()?;
    let dur = editor.audio_durations.get(src).copied().flatten();
    Some((peaks, dur, *trim))
}

fn object_cues(editor: &Editor, oid: &str) -> Vec<Cue> {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return Vec::new();
    };
    let Some((object, _)) = slide.object(oid) else {
        return Vec::new();
    };
    match &object.kind {
        ObjectKind::Video { cues, .. } => cues.clone(),
        _ => Vec::new(),
    }
}

/// Whether the media object `oid` on the current timeline has a trim window.
fn object_is_trimmed(editor: &Editor, oid: &str) -> bool {
    editor
        .session
        .course
        .slide(&editor.selection.slide)
        .and_then(|slide| slide.object(oid))
        .is_some_and(|(object, _)| match &object.kind {
            ObjectKind::Audio { trim, .. } | ObjectKind::Video { trim, .. } => trim.is_some(),
            _ => false,
        })
}

/// Paint one cue diamond. Returns its rect.
fn draw_cue_marker(painter: &egui::Painter, center: egui::Pos2, selected: bool) -> Rect {
    let p = coursecraft_render::palette(painter.ctx());
    let r = 5.5;
    let points = vec![
        pos2(center.x, center.y - r),
        pos2(center.x + r, center.y),
        pos2(center.x, center.y + r),
        pos2(center.x - r, center.y),
    ];
    let fill = if selected { p.text } else { p.accent };
    painter.add(egui::Shape::convex_polygon(
        points,
        fill,
        Stroke::new(1.0, Color32::WHITE),
    ));
    Rect::from_center_size(center, vec2(r * 2.4, r * 2.4))
}

#[allow(clippy::too_many_arguments, clippy::cognitive_complexity)]
fn render_object_rows(
    editor: &mut Editor,
    ui: &mut Ui,
    painter: &egui::Painter,
    objects: &[(String, String, Timing, Option<String>)],
    content_start_y: f32,
    total_area: Rect,
    track_rect: Rect,
    visible_track: Rect,
    duration_ms: u32,
    track_width: f32,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    editor.ui.timeline.cue_marker_rects.clear();
    // Rows of a collapsed group are not laid out, so drop last frame's rects.
    let tl = &mut editor.ui.timeline;
    tl.object_row_rects.clear();
    tl.object_bar_rects.clear();
    tl.object_left_handle_rects.clear();
    tl.object_right_handle_rects.clear();
    tl.group_row_rects.clear();
    tl.group_bar_rects.clear();
    tl.group_toggle_rects.clear();
    tl.group_eye_rects.clear();
    tl.group_lock_rects.clear();
    let slide_id = editor.selection.slide.clone();
    // Bars scroll under the row names when zoomed, so neither paint nor hit there.
    let bar_painter = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_x_y_ranges(
        visible_track.x_range(),
        painter.clip_rect().y_range(),
    )));

    if objects.is_empty() {
        let center_y = content_start_y + (total_area.max.y - content_start_y) * 0.5;
        painter.text(
            pos2(total_area.center().x, center_y),
            Align2::CENTER_CENTER,
            "Objects you add appear here on the timeline.",
            FontId::proportional(13.0),
            p.text_secondary,
        );
        return;
    }

    // A pointer over a menu or window drawn on top of the timeline is not over its rows.
    let pointer_pos = ui
        .input(|i| i.pointer.interact_pos())
        .filter(|pos| crate::widgets::pointer_reaches(ui, *pos));
    let primary_pressed = pointer_pos.is_some() && ui.input(|i| i.pointer.primary_pressed());
    let modifiers = ui.input(|i| i.modifiers);

    handle_group_drag(editor, ui, duration_ms, track_width);

    let rows = timeline_rows(objects, &editor.ui.timeline.collapsed_groups);
    // The top row is the frontmost object, so display order is stacking order reversed.
    let display = display_ids(objects);
    // Rects the row drag uses for drop slots: members of a collapsed group use its row.
    let mut drop_rects: HashMap<String, Rect> = HashMap::new();
    let geometry = RowGeometry {
        total_area,
        track_rect,
        visible_track,
        duration_ms,
        track_width,
        pointer_pos,
        primary_pressed,
        modifiers,
    };
    for (row_idx, row) in rows.iter().enumerate() {
        let row_y = content_start_y + row_idx as f32 * OBJECT_ROW_HEIGHT;
        let (model_idx, indented) = match row {
            TimelineRow::Group { group, members } => {
                render_group_row(
                    editor,
                    ui,
                    painter,
                    &bar_painter,
                    objects,
                    &display,
                    group,
                    members,
                    row_y,
                    &geometry,
                    &mut drop_rects,
                );
                continue;
            }
            TimelineRow::Object {
                model_idx,
                indented,
            } => (*model_idx, *indented),
        };
        let Some((oid, oname, timing, group)) = objects.get(model_idx) else {
            continue;
        };
        let display_idx = display.iter().position(|id| id == oid).unwrap_or(0);

        let is_selected = editor.selection.contains(oid);
        let row_rect = Rect::from_min_max(
            pos2(total_area.min.x, row_y),
            pos2(total_area.max.x, row_y + OBJECT_ROW_HEIGHT),
        );
        let header_row_rect = Rect::from_min_max(
            pos2(total_area.min.x, row_y),
            pos2(visible_track.min.x, row_y + OBJECT_ROW_HEIGHT),
        );
        let bleed = coursecraft_render::PANEL_MARGIN;
        let header_bg_rect = Rect::from_min_max(
            pos2(header_row_rect.min.x - bleed, header_row_rect.min.y),
            header_row_rect.max,
        );

        editor
            .ui
            .timeline
            .object_row_rects
            .insert(oid.clone(), row_rect);
        drop_rects.insert(oid.clone(), row_rect);

        let row_response = ui.interact(
            row_rect,
            ui.scope_id().with("timeline_row").with(oid),
            Sense::click(),
        );
        // Eye and lock buttons at the right end of the name column (>= 24x24 hit target).
        let lock_rect = Rect::from_center_size(
            pos2(header_row_rect.max.x - 12.0, header_row_rect.center().y),
            vec2(24.0, 24.0),
        );
        let eye_rect = lock_rect.translate(vec2(-26.0, 0.0));
        let (is_hidden, is_locked) = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .and_then(|s| s.object(oid))
            .map(|(o, _)| (o.editor_hidden, o.locked))
            .unwrap_or((false, false));
        let eye_label = if is_hidden {
            "Show on canvas"
        } else {
            "Hide on canvas"
        };
        let lock_label = if is_locked { "Unlock" } else { "Lock" };
        let eye_resp = ui
            .interact(
                eye_rect,
                ui.scope_id().with("timeline_eye").with(oid),
                Sense::click(),
            )
            .on_hover_text(eye_label);
        let lock_resp = ui
            .interact(
                lock_rect,
                ui.scope_id().with("timeline_lock").with(oid),
                Sense::click(),
            )
            .on_hover_text(lock_label);
        eye_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, eye_label));
        lock_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, lock_label));
        if eye_resp.clicked() {
            let next = !is_hidden;
            let slide_id = editor.selection.slide.clone();
            editor.run(
                "object.hide_in_editor",
                serde_json::json!({
                    "slide": slide_id,
                    "object": oid,
                    "hidden": next,
                }),
            );
            if next && editor.selection.contains(oid) {
                editor.selection.toggle(oid);
            }
        }
        if lock_resp.clicked() {
            let next = !is_locked;
            let slide_id = editor.selection.slide.clone();
            editor.run(
                "object.lock",
                serde_json::json!({
                    "slide": slide_id,
                    "object": oid,
                    "locked": next,
                }),
            );
        }
        let on_icons =
            pointer_pos.is_some_and(|pos| eye_rect.contains(pos) || lock_rect.contains(pos));
        if row_response.container_secondary_clicked() {
            editor.ui.last_active_area = crate::ActiveArea::Canvas;
            if !editor.selection.contains(oid) {
                select_timeline_row(editor, objects, oid, egui::Modifiers::NONE);
            }
        }
        row_response.container_context_menu(|ui| {
            crate::menu::show_object_row_menu(editor, ui);
        });

        let is_dragged = editor
            .ui
            .timeline
            .active_row_drag
            .as_ref()
            .is_some_and(|drag| drag.unit_ids.iter().any(|id| id == oid));
        let row_bg = if is_dragged {
            p.hover
        } else if is_selected {
            p.accent_subtle
        } else {
            p.panel
        };
        painter.rect_filled(header_bg_rect, CornerRadius::ZERO, row_bg);

        let display_name = if oname.is_empty() {
            oid.clone()
        } else {
            oname.clone()
        };
        let display_name = if object_is_trimmed(editor, oid) {
            format!("{display_name} (trimmed)")
        } else {
            display_name
        };
        let text_color = if is_selected {
            p.text
        } else {
            p.text_secondary
        };
        let text_x = if indented {
            // Members sit under their group's folder row.
            header_row_rect.min.x + GROUP_INDENT
        } else if group.is_some() {
            painter.text(
                pos2(header_row_rect.min.x, header_row_rect.center().y),
                Align2::LEFT_CENTER,
                coursecraft_render::Icon::Stack.codepoint().to_string(),
                FontId::new(12.0, egui::FontFamily::Name("phosphor".into())),
                p.text_tertiary,
            );
            header_row_rect.min.x + 16.0
        } else {
            header_row_rect.min.x
        };
        let name_slot_rect = Rect::from_min_max(
            pos2(text_x, header_row_rect.min.y + 2.0),
            pos2(
                (eye_rect.min.x - 4.0).max(text_x + 20.0),
                header_row_rect.max.y - 2.0,
            ),
        );
        let renaming_this = editor
            .ui
            .timeline
            .object_rename
            .as_ref()
            .is_some_and(|r| r.object == *oid);
        if renaming_this {
            show_object_rename_editor(editor, ui, name_slot_rect, actions);
        } else {
            // The name stops short of the eye icon: elided with "...", full name on hover.
            let name_max_w = (eye_rect.min.x - 6.0 - text_x).max(0.0);
            let mut name_job = egui::text::LayoutJob::simple_singleline(
                display_name.clone(),
                FontId::proportional(13.0),
                text_color,
            );
            name_job.wrap = egui::text::TextWrapping::truncate_at_width(name_max_w);
            let name_galley = painter.layout_job(name_job);
            let name_elided = name_galley.elided;
            let name_rect = Rect::from_min_size(
                pos2(
                    text_x,
                    header_row_rect.center().y - name_galley.size().y * 0.5,
                ),
                name_galley.size(),
            );
            painter.galley(name_rect.min, name_galley, text_color);
            if name_elided {
                ui.interact(
                    name_rect,
                    ui.scope_id().with("timeline_name").with(oid),
                    Sense::hover(),
                )
                .on_hover_text(display_name);
            }
            if row_response.double_clicked()
                && pointer_pos.is_some_and(|pos| header_row_rect.contains(pos) && !on_icons)
            {
                editor.ui.timeline.object_rename = Some(ObjectRename {
                    slide: slide_id.clone(),
                    object: oid.clone(),
                    buffer: oname.clone(),
                    original: oname.clone(),
                    just_entered: true,
                });
            }
        }

        for (rect, icon, on) in [
            (
                eye_rect,
                if is_hidden {
                    coursecraft_render::Icon::EyeSlash
                } else {
                    coursecraft_render::Icon::Eye
                },
                is_hidden,
            ),
            (
                lock_rect,
                if is_locked {
                    coursecraft_render::Icon::Lock
                } else {
                    coursecraft_render::Icon::LockOpen
                },
                is_locked,
            ),
        ] {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                icon.codepoint().to_string(),
                FontId::new(12.0, egui::FontFamily::Name("phosphor".into())),
                if on { p.accent } else { p.text_tertiary },
            );
        }

        let track_row_rect = Rect::from_min_max(
            pos2(visible_track.min.x, row_y),
            pos2(visible_track.max.x, row_y + OBJECT_ROW_HEIGHT),
        );
        let track_bg_rect = Rect::from_min_max(
            track_row_rect.min,
            // Bleed to the panel edge, but never past the clip (the layers divider).
            pos2(
                (track_row_rect.max.x + bleed).min(painter.clip_rect().max.x),
                track_row_rect.max.y,
            ),
        );
        painter.rect_filled(track_bg_rect, CornerRadius::ZERO, row_bg);
        painter.line_segment(
            [track_bg_rect.left_bottom(), track_bg_rect.right_bottom()],
            Stroke::new(1.0, p.border),
        );

        let ms_to_x =
            |ms: u32| -> f32 { track_rect.min.x + (ms as f32 / duration_ms as f32) * track_width };
        // While the group's bar is dragged, its members' bars show the shift too.
        let shift = group_drag_shift(editor, oid);
        let eff_start = shifted_ms(timing.start_ms, shift);
        let eff_end = timing
            .end_ms
            .map_or(duration_ms, |end| shifted_ms(end, shift));
        let bar_x1 = ms_to_x(eff_start);
        let bar_x2 = ms_to_x(eff_end).max(bar_x1 + 6.0);
        let bar_rect = Rect::from_min_max(pos2(bar_x1, row_y + 4.0), pos2(bar_x2, row_y + 24.0));

        let row_cues = object_cues(editor, oid);
        let mut markers: Vec<(Cue, Rect)> = Vec::new();
        for cue in &row_cues {
            let dragging_this = editor
                .ui
                .timeline
                .cue_drag
                .as_ref()
                .is_some_and(|drag| drag.object_id == *oid && drag.cue_id == cue.id);
            let at_ms = if dragging_this {
                editor
                    .ui
                    .timeline
                    .cue_drag
                    .as_ref()
                    .map_or(cue.at_ms, |drag| drag.current_at_ms)
            } else {
                cue.at_ms
            };
            let at = at_ms.round().clamp(0.0, f64::from(duration_ms)) as u32;
            let rect = Rect::from_center_size(
                pos2(ms_to_x(at), row_y + OBJECT_ROW_HEIGHT * 0.5),
                vec2(12.0, 12.0),
            );
            markers.push((cue.clone(), rect));
        }
        let on_cue =
            pointer_pos.is_some_and(|pos| markers.iter().any(|(_, rect)| rect.contains(pos)));

        let timing_label = format!(
            "{} – {}",
            format_seconds(u64::from(eff_start)),
            timing
                .end_ms
                .map(|e| format_seconds(u64::from(shifted_ms(e, shift))))
                .unwrap_or_else(|| "end".to_owned())
        );
        ui.interact(
            bar_rect.intersect(visible_track),
            ui.scope_id().with(oid),
            Sense::hover(),
        )
        .on_hover_text(timing_label);

        let left_h_rect = Rect::from_min_max(
            pos2(bar_rect.min.x - 3.0, bar_rect.min.y),
            pos2(bar_rect.min.x + 5.0, bar_rect.max.y),
        );
        let right_h_rect = Rect::from_min_max(
            pos2(bar_rect.max.x - 5.0, bar_rect.min.y),
            pos2(bar_rect.max.x + 3.0, bar_rect.max.y),
        );

        editor
            .ui
            .timeline
            .object_bar_rects
            .insert(oid.clone(), bar_rect);
        editor
            .ui
            .timeline
            .object_left_handle_rects
            .insert(oid.clone(), left_h_rect);
        editor
            .ui
            .timeline
            .object_right_handle_rects
            .insert(oid.clone(), right_h_rect);

        if editor.ui.timeline.active_drag.is_none()
            && editor.ui.timeline.active_row_drag.is_none()
            && editor.ui.timeline.cue_drag.is_none()
            && editor.ui.timeline.group_drag.is_none()
            && let Some(pos) = pointer_pos
            && !on_cue
            && !on_icons
        {
            // The start handle reaches 3 px left of time zero, unless scrolled away.
            let left_reach = if track_rect.min.x < visible_track.min.x {
                0.0
            } else {
                3.0
            };
            let in_track =
                (visible_track.min.x - left_reach..=visible_track.max.x + 3.0).contains(&pos.x);
            if in_track && left_h_rect.contains(pos) {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                if primary_pressed {
                    select_timeline_row(editor, objects, oid, modifiers);
                    editor.ui.timeline.active_drag = Some(TimelineDrag {
                        op: TimelineDragOp::ResizeStart,
                        object_id: oid.clone(),
                        orig_start_ms: timing.start_ms,
                        orig_end_ms: timing.end_ms,
                        orig_duration_ms: duration_ms,
                        start_pointer_x: pos.x,
                        start_pointer_y: pos.y,
                        current_start_ms: timing.start_ms,
                        current_end_ms: timing.end_ms,
                    });
                }
            } else if in_track && right_h_rect.contains(pos) {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                if primary_pressed {
                    select_timeline_row(editor, objects, oid, modifiers);
                    editor.ui.timeline.active_drag = Some(TimelineDrag {
                        op: TimelineDragOp::ResizeEnd,
                        object_id: oid.clone(),
                        orig_start_ms: timing.start_ms,
                        orig_end_ms: timing.end_ms,
                        orig_duration_ms: duration_ms,
                        start_pointer_x: pos.x,
                        start_pointer_y: pos.y,
                        current_start_ms: timing.start_ms,
                        current_end_ms: timing.end_ms,
                    });
                }
            } else if in_track && bar_rect.contains(pos) {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
                if primary_pressed {
                    select_timeline_row(editor, objects, oid, modifiers);
                    editor.ui.timeline.active_drag = Some(TimelineDrag {
                        op: TimelineDragOp::Move,
                        object_id: oid.clone(),
                        orig_start_ms: timing.start_ms,
                        orig_end_ms: timing.end_ms,
                        orig_duration_ms: duration_ms,
                        start_pointer_x: pos.x,
                        start_pointer_y: pos.y,
                        current_start_ms: timing.start_ms,
                        current_end_ms: timing.end_ms,
                    });
                }
            } else if !renaming_this
                && header_row_rect.contains(pos)
                && row_response.contains_pointer()
            {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
                if primary_pressed {
                    select_timeline_row(editor, objects, oid, modifiers);
                    if editor.selection.contains(oid) {
                        let unit_ids = row_unit_ids(editor, objects, oid);
                        editor.ui.timeline.active_row_drag = Some(TimelineRowDrag {
                            start_pointer_y: pos.y,
                            unit_ids,
                            moved: false,
                            target_slot: display_idx,
                        });
                    }
                }
            } else if row_rect.contains(pos) && primary_pressed {
                select_timeline_row(editor, objects, oid, modifiers);
            }
        }

        draw_bar_or_preview(
            editor,
            &bar_painter,
            oid,
            bar_rect,
            row_y,
            is_selected,
            eff_start,
            timing,
            duration_ms,
            track_width,
            track_rect,
        );

        for (cue, rect) in &markers {
            let selected = editor
                .ui
                .timeline
                .cue_selected
                .as_ref()
                .is_some_and(|(o, c)| o == oid && c == &cue.id);
            let response = ui
                .interact(
                    *rect,
                    ui.scope_id()
                        .with("timeline_cue")
                        .with(oid)
                        .with(cue.id.as_str()),
                    Sense::click_and_drag(),
                )
                .on_hover_text(format!(
                    "{} ({})",
                    cue.name,
                    format_seconds(cue.at_ms_u64())
                ));
            draw_cue_marker(&bar_painter, rect.center(), selected);
            editor
                .ui
                .timeline
                .cue_marker_rects
                .insert((oid.clone(), cue.id.clone()), *rect);
            if response.clicked() {
                editor.ui.timeline.cue_selected = Some((oid.clone(), cue.id.clone()));
            }
            if response.double_clicked() {
                editor.ui.timeline.cue_selected = Some((oid.clone(), cue.id.clone()));
                editor.ui.timeline.cue_rename = Some(CueRename {
                    slide: slide_id.to_owned(),
                    object: oid.clone(),
                    cue: cue.id.clone(),
                    text: cue.name.clone(),
                    error: None,
                });
            }
            if response.secondary_clicked() {
                actions.push((
                    "video.remove_cue",
                    serde_json::json!({
                        "slide": &slide_id,
                        "object": oid,
                        "cue": cue.id,
                    }),
                ));
                editor.ui.timeline.cue_selected = None;
            }
            if response.drag_started() {
                editor.ui.timeline.cue_drag = Some(CueDrag {
                    object_id: oid.clone(),
                    cue_id: cue.id.clone(),
                    orig_at_ms: cue.at_ms,
                    current_at_ms: cue.at_ms,
                    start_pointer_x: rect.center().x,
                    duration_ms,
                });
            }
            if response.dragged()
                && let Some(pos) = pointer_pos
                && let Some(drag) = &mut editor.ui.timeline.cue_drag
                && drag.object_id == *oid
                && drag.cue_id == cue.id
            {
                let delta_x = pos.x - drag.start_pointer_x;
                let raw = drag.orig_at_ms
                    + f64::from(delta_x / track_width.max(1.0) * duration_ms as f32);
                let snapped = if ui.input(|i| i.modifiers.shift) {
                    raw
                } else {
                    (raw / 100.0).round() * 100.0
                };
                drag.current_at_ms = snapped.clamp(0.0, f64::from(duration_ms));
            }
            if response.drag_stopped()
                && let Some(drag) = editor.ui.timeline.cue_drag.take()
                && drag.object_id == *oid
                && drag.cue_id == cue.id
                && (drag.current_at_ms - drag.orig_at_ms).abs() >= 0.5
            {
                actions.push((
                    "video.set_cue",
                    serde_json::json!({
                        "slide": &slide_id,
                        "object": drag.object_id,
                        "cue": drag.cue_id,
                        "patch": { "at_ms": drag.current_at_ms },
                    }),
                ));
            }
        }
    }

    promote_bar_drag_to_row_drag(editor, ui, objects);
    handle_row_drag(editor, ui, painter, objects, total_area, &drop_rects);
}

/// How far a group's members' names are indented under its folder row.
const GROUP_INDENT: f32 = 18.0;

/// Shared layout and pointer facts for one frame of timeline rows.
struct RowGeometry {
    total_area: Rect,
    track_rect: Rect,
    visible_track: Rect,
    duration_ms: u32,
    track_width: f32,
    pointer_pos: Option<egui::Pos2>,
    primary_pressed: bool,
    modifiers: egui::Modifiers,
}

/// The shift of the group bar drag that includes `oid`, or 0.
fn group_drag_shift(editor: &Editor, oid: &str) -> i64 {
    editor
        .ui
        .timeline
        .group_drag
        .as_ref()
        .filter(|drag| drag.members.iter().any(|(id, _)| id == oid))
        .map_or(0, |drag| drag.delta_ms)
}

/// `ms` moved by `shift`, kept inside `u32`.
fn shifted_ms(ms: u32, shift: i64) -> u32 {
    i64::from(ms)
        .saturating_add(shift)
        .clamp(0, i64::from(u32::MAX)) as u32
}

/// The shifts that keep every member inside `0..=duration_ms`.
/// A member that runs to the end of the slide keeps at least `min_len_ms` visible.
fn group_shift_limits(
    members: &[(String, Timing)],
    duration_ms: u32,
    min_len_ms: u32,
) -> (i64, i64) {
    let duration = i64::from(duration_ms);
    let mut min_delta = i64::MIN;
    let mut max_delta = i64::MAX;
    for (_, timing) in members {
        let start = i64::from(timing.start_ms);
        let (lo, room) = match timing.end_ms {
            Some(end) => {
                let end = i64::from(end);
                (-start.min(end), duration - end.max(start))
            }
            None => (-start, duration - i64::from(min_len_ms) - start),
        };
        min_delta = min_delta.max(lo);
        max_delta = max_delta.min(room);
    }
    if members.is_empty() || max_delta < min_delta {
        return (0, 0);
    }
    (min_delta.min(0), max_delta.max(0))
}

/// The earliest start and latest end of the members, as drawn on the track.
/// Open-ended members (`end_ms: None`) remain pinned to `duration_ms` during a shift.
fn group_span(
    objects: &[CachedTimelineObject],
    members: &[usize],
    duration_ms: u32,
    shift: i64,
) -> (u32, u32) {
    let mut start = u32::MAX;
    let mut end = 0;
    for (_, _, timing, _) in members.iter().filter_map(|&i| objects.get(i)) {
        let s = shifted_ms(timing.start_ms, shift);
        let e = timing
            .end_ms
            .map_or(duration_ms, |end_ms| shifted_ms(end_ms, shift));
        start = start.min(s);
        end = end.max(e);
    }
    if start == u32::MAX {
        return (0, 0);
    }
    (start, end.max(start))
}

/// Update or finish the group bar drag. On release, all members move in one undo step.
fn handle_group_drag(editor: &mut Editor, ui: &Ui, duration_ms: u32, track_width: f32) {
    if editor.ui.timeline.group_drag.is_none() {
        return;
    }
    // Consume Escape so cancelling a group bar drag does not also clear the canvas selection.
    let escaped = ui.input_mut(|i| {
        i.consume_key(egui::Modifiers::NONE, Key::Escape)
            || i.consume_key(egui::Modifiers::SHIFT, Key::Escape)
            || i.key_pressed(Key::Escape)
    });
    if escaped {
        editor.ui.timeline.group_drag = None;
        return;
    }
    let pointer_down = ui.input(|i| i.pointer.primary_down());
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let shift = ui.input(|i| i.modifiers.shift);
    if pointer_down {
        if let (Some(pos), Some(drag)) = (pointer_pos, editor.ui.timeline.group_drag.as_mut())
            && track_width > 0.0
        {
            let step: i64 = if shift { 10 } else { 100 };
            let raw = ((pos.x - drag.start_pointer_x) / track_width * duration_ms as f32).round();
            let raw = if raw.is_finite() { raw as i64 } else { 0 };
            let snapped = (raw / step) * step;
            drag.delta_ms = snapped.clamp(drag.min_delta_ms, drag.max_delta_ms);
        }
        return;
    }
    let Some(drag) = editor.ui.timeline.group_drag.take() else {
        return;
    };
    if drag.delta_ms == 0 {
        return;
    }
    let slide_id = editor.selection.slide.clone();
    let steps: Vec<(String, Json)> = drag
        .members
        .iter()
        .map(|(id, timing)| {
            let moved = Timing {
                start_ms: shifted_ms(timing.start_ms, drag.delta_ms),
                end_ms: timing.end_ms.map(|end| shifted_ms(end, drag.delta_ms)),
            };
            (
                "object.update".to_owned(),
                serde_json::json!({ "slide": slide_id, "object": id, "timing": moved }),
            )
        })
        .collect();
    editor.run_batch(&steps);
}

/// Paint a group folder row's label and optional quiet `"Button"` pill for interactive groups.
#[allow(clippy::too_many_arguments)]
fn paint_group_row_label(
    painter: &egui::Painter,
    p: &coursecraft_render::Palette,
    header_row_rect: Rect,
    text_x: f32,
    max_right: f32,
    is_selected: bool,
    member_count: usize,
    interactive_info: Option<(String, bool)>,
) {
    let is_interactive_group = interactive_info
        .as_ref()
        .is_some_and(|(_, interactive)| *interactive);
    let name = match interactive_info
        .as_ref()
        .map(|(group_name, _)| group_name.trim())
    {
        Some(group_name) if !group_name.is_empty() => group_name.to_owned(),
        _ => format!("Group ({member_count})"),
    };
    let text_color = if is_selected {
        p.text
    } else {
        p.text_secondary
    };
    let badge_galley = is_interactive_group.then(|| {
        painter.layout_no_wrap(
            tl!("Button").to_owned(),
            FontId::proportional(coursecraft_render::SIZE_CAPTION),
            p.text_secondary,
        )
    });
    let badge_pad_x = 5.0;
    let badge_pad_y = 2.0;
    let badge_gap = 6.0;
    let badge_w = badge_galley
        .as_ref()
        .map_or(0.0, |galley| galley.size().x + badge_pad_x * 2.0);
    let show_badge = badge_galley.is_some() && (max_right - text_x) >= badge_w + badge_gap + 12.0;
    let name_max_w = if show_badge {
        (max_right - badge_w - badge_gap - text_x).max(0.0)
    } else {
        (max_right - text_x).max(0.0)
    };
    let mut name_job =
        egui::text::LayoutJob::simple_singleline(name, FontId::proportional(13.0), text_color);
    name_job.wrap = egui::text::TextWrapping::truncate_at_width(name_max_w);
    let galley = painter.layout_job(name_job);
    let name_w = galley.size().x;
    painter.galley(
        pos2(text_x, header_row_rect.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );
    if show_badge && let Some(badge) = badge_galley {
        let pill_h = (badge.size().y + badge_pad_y * 2.0).min(OBJECT_ROW_HEIGHT - 6.0);
        let pill_x = (text_x + name_w + badge_gap).min(max_right - badge_w);
        let pill_rect = Rect::from_min_size(
            pos2(pill_x, header_row_rect.center().y - pill_h * 0.5),
            vec2(badge_w, pill_h),
        );
        let pill_bg = if is_selected {
            p.raised
        } else {
            p.accent_subtle
        };
        painter.rect_filled(
            pill_rect,
            CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
            pill_bg,
        );
        painter.galley(
            pos2(
                pill_rect.min.x + badge_pad_x,
                pill_rect.center().y - badge.size().y * 0.5,
            ),
            badge,
            p.text_secondary,
        );
    }
}

/// One group's folder row: expand/collapse arrow, name, eye and lock for all members,
/// and a summary bar that moves every member's timing together.
#[allow(clippy::too_many_arguments)]
fn render_group_row(
    editor: &mut Editor,
    ui: &mut Ui,
    painter: &egui::Painter,
    bar_painter: &egui::Painter,
    objects: &[CachedTimelineObject],
    display: &[String],
    group: &str,
    members: &[usize],
    row_y: f32,
    g: &RowGeometry,
    drop_rects: &mut HashMap<String, Rect>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let member_ids: Vec<String> = members
        .iter()
        .filter_map(|&i| objects.get(i).map(|(id, ..)| id.clone()))
        .collect();
    let Some(first) = member_ids.first().cloned() else {
        return;
    };
    let collapsed = editor.ui.timeline.collapsed_groups.contains(group);

    let row_rect = Rect::from_min_max(
        pos2(g.total_area.min.x, row_y),
        pos2(g.total_area.max.x, row_y + OBJECT_ROW_HEIGHT),
    );
    let header_row_rect = Rect::from_min_max(
        pos2(g.total_area.min.x, row_y),
        pos2(g.visible_track.min.x, row_y + OBJECT_ROW_HEIGHT),
    );
    let bleed = coursecraft_render::PANEL_MARGIN;
    let header_bg_rect = Rect::from_min_max(
        pos2(header_row_rect.min.x - bleed, header_row_rect.min.y),
        header_row_rect.max,
    );
    if collapsed {
        for id in &member_ids {
            drop_rects.insert(id.clone(), row_rect);
        }
    }

    let toggle_rect = Rect::from_center_size(
        pos2(header_row_rect.min.x + 10.0, header_row_rect.center().y),
        vec2(24.0, 24.0),
    );
    let lock_rect = Rect::from_center_size(
        pos2(header_row_rect.max.x - 12.0, header_row_rect.center().y),
        vec2(24.0, 24.0),
    );
    let eye_rect = lock_rect.translate(vec2(-26.0, 0.0));
    {
        let tl = &mut editor.ui.timeline;
        tl.group_row_rects.insert(group.to_owned(), row_rect);
        tl.group_toggle_rects.insert(group.to_owned(), toggle_rect);
        tl.group_eye_rects.insert(group.to_owned(), eye_rect);
        tl.group_lock_rects.insert(group.to_owned(), lock_rect);
    }

    let row_response = ui.interact(
        row_rect,
        ui.scope_id().with("timeline_group_row").with(group),
        Sense::click(),
    );

    let (all_hidden, all_locked) = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .map(|slide| {
            let flags: Vec<(bool, bool)> = member_ids
                .iter()
                .filter_map(|id| slide.object(id))
                .map(|(o, _)| (o.editor_hidden, o.locked))
                .collect();
            (
                !flags.is_empty() && flags.iter().all(|(h, _)| *h),
                !flags.is_empty() && flags.iter().all(|(_, l)| *l),
            )
        })
        .unwrap_or((false, false));

    let toggle_label = if collapsed {
        "Expand group"
    } else {
        "Collapse group"
    };
    let eye_label = if all_hidden {
        "Show group on canvas"
    } else {
        "Hide group on canvas"
    };
    let lock_label = if all_locked {
        "Unlock group"
    } else {
        "Lock group"
    };
    let toggle_resp = ui
        .interact(
            toggle_rect,
            ui.scope_id().with("timeline_group_toggle").with(group),
            Sense::click(),
        )
        .on_hover_text(toggle_label);
    let eye_resp = ui
        .interact(
            eye_rect,
            ui.scope_id().with("timeline_group_eye").with(group),
            Sense::click(),
        )
        .on_hover_text(eye_label);
    let lock_resp = ui
        .interact(
            lock_rect,
            ui.scope_id().with("timeline_group_lock").with(group),
            Sense::click(),
        )
        .on_hover_text(lock_label);
    toggle_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, toggle_label));
    eye_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, eye_label));
    lock_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, lock_label));

    if toggle_resp.clicked() {
        toggle_group_collapsed(editor, group);
    }
    if eye_resp.clicked() {
        let hidden = !all_hidden;
        if set_group_flag(
            editor,
            &member_ids,
            "object.hide_in_editor",
            "hidden",
            hidden,
        ) && hidden
        {
            for id in &member_ids {
                if editor.selection.contains(id) {
                    editor.selection.toggle(id);
                }
            }
        }
    }
    if lock_resp.clicked() {
        set_group_flag(editor, &member_ids, "object.lock", "locked", !all_locked);
    }
    let on_icons = g.pointer_pos.is_some_and(|pos| {
        toggle_rect.contains(pos) || eye_rect.contains(pos) || lock_rect.contains(pos)
    });

    if row_response.container_secondary_clicked() {
        editor.ui.last_active_area = crate::ActiveArea::Canvas;
        if !editor.selection.contains(&first) {
            select_timeline_row(editor, objects, &first, egui::Modifiers::NONE);
        }
    }
    row_response.container_context_menu(|ui| {
        crate::menu::show_object_row_menu(editor, ui);
    });
    if row_response.double_clicked()
        && g.pointer_pos
            .is_some_and(|pos| header_row_rect.contains(pos) && !on_icons)
    {
        toggle_group_collapsed(editor, group);
    }

    let is_selected = member_ids.iter().all(|id| editor.selection.contains(id));
    let is_dragged = editor
        .ui
        .timeline
        .active_row_drag
        .as_ref()
        .is_some_and(|drag| drag.unit_ids.contains(&first));
    let row_bg = if is_dragged {
        p.hover
    } else if is_selected {
        p.accent_subtle
    } else {
        p.panel
    };
    painter.rect_filled(header_bg_rect, CornerRadius::ZERO, row_bg);

    let phosphor = FontId::new(12.0, egui::FontFamily::Name("phosphor".into()));
    let caret = if collapsed {
        coursecraft_render::Icon::CaretRight
    } else {
        coursecraft_render::Icon::CaretDown
    };
    painter.text(
        toggle_rect.center(),
        Align2::CENTER_CENTER,
        caret.codepoint().to_string(),
        phosphor.clone(),
        p.text_secondary,
    );
    let icon_x = toggle_rect.max.x;
    painter.text(
        pos2(icon_x, header_row_rect.center().y),
        Align2::LEFT_CENTER,
        coursecraft_render::Icon::Stack.codepoint().to_string(),
        phosphor.clone(),
        p.text_tertiary,
    );
    let interactive_group_info: Option<(String, bool)> = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .and_then(|slide| slide.interactive_group(group))
        .map(|(g, _)| (g.name.clone(), g.interactive));
    paint_group_row_label(
        painter,
        &p,
        header_row_rect,
        icon_x + 16.0,
        eye_rect.min.x - 6.0,
        is_selected,
        member_ids.len(),
        interactive_group_info,
    );
    for (rect, icon, on) in [
        (
            eye_rect,
            if all_hidden {
                coursecraft_render::Icon::EyeSlash
            } else {
                coursecraft_render::Icon::Eye
            },
            all_hidden,
        ),
        (
            lock_rect,
            if all_locked {
                coursecraft_render::Icon::Lock
            } else {
                coursecraft_render::Icon::LockOpen
            },
            all_locked,
        ),
    ] {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon.codepoint().to_string(),
            phosphor.clone(),
            if on { p.accent } else { p.text_tertiary },
        );
    }

    // Track side: row background and the summary bar.
    let track_row_rect = Rect::from_min_max(
        pos2(g.visible_track.min.x, row_y),
        pos2(g.visible_track.max.x, row_y + OBJECT_ROW_HEIGHT),
    );
    let track_bg_rect = Rect::from_min_max(
        track_row_rect.min,
        pos2(
            (track_row_rect.max.x + bleed).min(painter.clip_rect().max.x),
            track_row_rect.max.y,
        ),
    );
    painter.rect_filled(track_bg_rect, CornerRadius::ZERO, row_bg);
    painter.line_segment(
        [track_bg_rect.left_bottom(), track_bg_rect.right_bottom()],
        Stroke::new(1.0, p.border),
    );

    let duration_ms = g.duration_ms.max(1);
    let dragging_this = editor
        .ui
        .timeline
        .group_drag
        .as_ref()
        .is_some_and(|drag| drag.group == group);
    let shift = editor
        .ui
        .timeline
        .group_drag
        .as_ref()
        .filter(|drag| drag.group == group)
        .map_or(0, |drag| drag.delta_ms);
    let (span_start, span_end) = group_span(objects, members, g.duration_ms, shift);
    let ms_to_x =
        |ms: u32| -> f32 { g.track_rect.min.x + (ms as f32 / duration_ms as f32) * g.track_width };
    let x1 = ms_to_x(span_start);
    let x2 = ms_to_x(span_end).max(x1 + 6.0);
    let bar_rect = Rect::from_min_max(pos2(x1, row_y + 7.0), pos2(x2, row_y + 21.0));
    editor
        .ui
        .timeline
        .group_bar_rects
        .insert(group.to_owned(), bar_rect);
    let radius = CornerRadius::same(coursecraft_render::RADIUS_MD as u8);
    let bar_color = if is_selected || dragging_this {
        p.accent
    } else {
        p.pressed
    };
    bar_painter.rect_filled(bar_rect, radius, bar_color.gamma_multiply(0.7));
    bar_painter.rect_stroke(
        bar_rect,
        radius,
        Stroke::new(1.0, bar_color),
        StrokeKind::Inside,
    );
    if let Some((grp, _)) = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .and_then(|s| s.group(group))
        && grp.has_animation()
    {
        paint_anim_strips(
            bar_painter,
            grp.entrance.as_ref(),
            grp.exit.as_ref(),
            &grp.emphasis,
            span_start,
            span_end,
            bar_rect,
            &ms_to_x,
        );
    }
    ui.interact(
        bar_rect.intersect(g.visible_track),
        ui.scope_id().with("timeline_group_bar").with(group),
        Sense::hover(),
    )
    .on_hover_text(format!(
        "{} – {} (drag to move the whole group)",
        format_seconds(u64::from(span_start)),
        format_seconds(u64::from(span_end))
    ));

    let hit = GroupRowHit {
        row_rect,
        header_row_rect,
        bar_rect,
        pointer_in_row: row_response.contains_pointer(),
        on_icons,
    };
    handle_group_row_press(
        editor, ui, objects, display, group, members, &first, &hit, g,
    );
}

/// Where a group row's parts are this frame.
struct GroupRowHit {
    row_rect: Rect,
    header_row_rect: Rect,
    bar_rect: Rect,
    pointer_in_row: bool,
    on_icons: bool,
}

/// A press on a group row: the bar starts a group timing drag, the header a reorder.
#[allow(clippy::too_many_arguments)]
fn handle_group_row_press(
    editor: &mut Editor,
    ui: &Ui,
    objects: &[CachedTimelineObject],
    display: &[String],
    group: &str,
    members: &[usize],
    first: &str,
    hit: &GroupRowHit,
    g: &RowGeometry,
) {
    let GroupRowHit {
        row_rect,
        header_row_rect,
        bar_rect,
        pointer_in_row,
        on_icons,
    } = *hit;
    let tl = &editor.ui.timeline;
    let idle = tl.active_drag.is_none()
        && tl.active_row_drag.is_none()
        && tl.cue_drag.is_none()
        && tl.group_drag.is_none();
    let Some(pos) = g.pointer_pos.filter(|_| idle && !on_icons) else {
        return;
    };
    let in_track = (g.visible_track.min.x..=g.visible_track.max.x).contains(&pos.x);
    if in_track && bar_rect.contains(pos) {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
        if g.primary_pressed {
            select_timeline_row(editor, objects, first, g.modifiers);
            let timings: Vec<(String, Timing)> = members
                .iter()
                .filter_map(|&i| objects.get(i))
                .map(|(id, _, timing, _)| (id.clone(), *timing))
                .collect();
            let (min_delta_ms, max_delta_ms) = group_shift_limits(&timings, g.duration_ms, 100);
            editor.ui.timeline.group_drag = Some(GroupBarDrag {
                group: group.to_owned(),
                members: timings,
                start_pointer_x: pos.x,
                delta_ms: 0,
                min_delta_ms,
                max_delta_ms,
            });
        }
    } else if header_row_rect.contains(pos) && pointer_in_row {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
        if g.primary_pressed {
            select_timeline_row(editor, objects, first, g.modifiers);
            if editor.selection.contains(first) {
                let unit_ids = row_unit_ids(editor, objects, first);
                let target_slot = display.iter().position(|id| *id == first).unwrap_or(0);
                editor.ui.timeline.active_row_drag = Some(TimelineRowDrag {
                    start_pointer_y: pos.y,
                    unit_ids,
                    moved: false,
                    target_slot,
                });
            }
        }
    } else if row_rect.contains(pos) && g.primary_pressed {
        select_timeline_row(editor, objects, first, g.modifiers);
    }
}

/// Run `command` with `key: value` on every member, as one undo step.
fn set_group_flag(
    editor: &mut Editor,
    member_ids: &[String],
    command: &str,
    key: &str,
    value: bool,
) -> bool {
    let slide_id = editor.selection.slide.clone();
    let steps: Vec<(String, Json)> = member_ids
        .iter()
        .map(|id| {
            let mut params = serde_json::json!({ "slide": slide_id, "object": id });
            if let Some(map) = params.as_object_mut() {
                map.insert(key.to_owned(), Json::Bool(value));
            }
            (command.to_owned(), params)
        })
        .collect();
    editor.run_batch(&steps).is_some()
}

/// Collapse an expanded group row, or expand a collapsed one.
fn toggle_group_collapsed(editor: &mut Editor, group: &str) {
    let collapsed = &mut editor.ui.timeline.collapsed_groups;
    if !collapsed.remove(group) {
        collapsed.insert(group.to_owned());
    }
}

/// The most a pinch can zoom the timeline in.
const TIMELINE_MAX_ZOOM: f32 = 20.0;

/// Pinch (or Ctrl+scroll) over the timeline zooms around the pointer, and a
/// sideways two-finger swipe (or Shift+scroll) pans once it is zoomed in.
fn zoom_and_scroll_track(editor: &mut Editor, ui: &Ui, area: Rect, track: Rect) {
    let state = &mut editor.ui.timeline;
    let width = track.width();
    if ui.rect_contains_pointer(area) && state.active_drag.is_none() && state.group_drag.is_none() {
        let (delta, scroll, pointer) = ui.input(|i| {
            (
                i.zoom_delta(),
                i.smooth_scroll_delta().x,
                i.pointer.hover_pos(),
            )
        });
        if delta.is_finite() && delta > 0.0 && (delta - 1.0).abs() >= 0.0001 {
            let zoom = (state.zoom * delta).clamp(1.0, TIMELINE_MAX_ZOOM);
            let anchor = pointer
                .map_or(0.0, |pos| pos.x - track.min.x)
                .clamp(0.0, width);
            let fraction = (anchor + state.scroll_x) / (width * state.zoom);
            state.zoom = zoom;
            state.scroll_x = fraction * width * zoom - anchor;
        }
        state.scroll_x -= scroll;
    }
    if !state.zoom.is_finite() {
        state.zoom = 1.0;
    }
    let max_scroll = width * (state.zoom - 1.0);
    state.scroll_x = if state.scroll_x.is_finite() {
        state.scroll_x.clamp(0.0, max_scroll.max(0.0))
    } else {
        0.0
    };
}

/// Render the Timeline bottom panel, with the Slide Layers panel on its right.
pub fn show_timeline_panel(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    // A master has no layers, so the layers panel is hidden there.
    if editor.ui.master_view {
        show_timeline_content(editor, ui, actions);
        return;
    }

    let total = ui.available_rect_before_wrap();
    // A window this narrow cannot give both panels usable width; keep the
    // timeline only rather than lay out a negative-width layers panel.
    if !total.is_finite() || total.width() < 240.0 {
        show_timeline_content(editor, ui, actions);
        return;
    }
    let desired = if total.width() < crate::LAYERS_PANEL_NARROW_BELOW {
        crate::LAYERS_PANEL_NARROW_WIDTH
    } else {
        crate::LAYERS_PANEL_WIDTH
    };
    // Never let the layers panel take the whole bottom panel on a tiny window.
    let layers_width = desired.min((total.width() - 120.0).max(120.0)).max(120.0);
    let right_x = total.max.x - layers_width;
    let divider_rect =
        Rect::from_min_max(pos2(right_x - 1.0, total.min.y), pos2(right_x, total.max.y));
    let left_rect = Rect::from_min_max(total.min, pos2(right_x - 1.0, total.max.y));
    let right_rect = Rect::from_min_max(pos2(right_x, total.min.y), total.max);

    let p = coursecraft_render::palette(ui.ctx());
    ui.painter()
        .rect_filled(divider_rect, CornerRadius::ZERO, p.border);

    let parent_clip = ui.clip_rect();
    let mut left_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(left_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    left_ui.set_clip_rect(left_rect.intersect(parent_clip));
    show_timeline_content(editor, &mut left_ui, actions);

    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(right_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    right_ui.set_clip_rect(right_rect.intersect(parent_clip));
    crate::layers::show_layers_panel(editor, &mut right_ui, actions);
}

/// Inline text field for renaming an object in the timeline's left column.
fn show_object_rename_editor(
    editor: &mut Editor,
    ui: &mut Ui,
    rect: Rect,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(rename) = editor.ui.timeline.object_rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("timeline_object_rename", &rename.object));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let mut output = TextEdit::singleline(&mut buffer)
        .id(edit_id)
        .font(FontId::proportional(13.0))
        .desired_width(rect.width())
        .margin(egui::Margin::same(1))
        .show(&mut child);

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
        if let Some(state) = editor.ui.timeline.object_rename.as_mut() {
            state.just_entered = false;
        }
    }
    if let Some(state) = editor.ui.timeline.object_rename.as_mut() {
        state.buffer = buffer.clone();
    }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    if escaped {
        editor.ui.timeline.object_rename = None;
    } else if enter || lost {
        let name = buffer.trim().to_owned();
        if !name.is_empty() && name != rename.original {
            actions.push((
                "object.update",
                serde_json::json!({
                    "slide": rename.slide,
                    "object": rename.object,
                    "name": name,
                }),
            ));
        }
        editor.ui.timeline.object_rename = None;
    }
}

/// Rename window opened by double-clicking a cue marker.
fn show_cue_rename_window(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(mut rename) = editor.ui.timeline.cue_rename.take() else {
        return;
    };
    let mut open = true;
    let mut commit = false;
    let mut cancel = false;
    Dialog::new("Rename cue")
        .width(320.0)
        .open(&mut open)
        .show_with_footer(ui.ctx(), |dialog| {
            dialog.body(|ui| {
                ui.label("Cue name");
                let response = ui.add(
                    TextEdit::singleline(&mut rename.text).id_salt("timeline_cue_rename_text"),
                );
                if (response.has_focus() || response.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    commit = true;
                }
                if let Some(error) = &rename.error {
                    ui.colored_label(coursecraft_render::palette(ui.ctx()).danger, error.clone());
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("Rename").clicked() {
                        commit = true;
                    }
                });
            });
        });
    if !open {
        cancel = true;
    }
    if commit {
        let name = rename.text.trim();
        if name.is_empty() {
            rename.error = Some("Cue name cannot be empty".to_owned());
            editor.ui.timeline.cue_rename = Some(rename);
        } else {
            actions.push((
                "video.set_cue",
                serde_json::json!({
                    "slide": rename.slide,
                    "object": rename.object,
                    "cue": rename.cue,
                    "patch": { "name": name },
                }),
            ));
        }
    } else if !cancel {
        editor.ui.timeline.cue_rename = Some(rename);
    }
}

fn show_slide_cue_rename_window(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(mut rename) = editor.ui.timeline.slide_cue_rename.take() else {
        return;
    };
    let mut open = true;
    let mut commit = false;
    let mut cancel = false;
    Dialog::new("Rename timeline cue")
        .width(320.0)
        .open(&mut open)
        .show_with_footer(ui.ctx(), |dialog| {
            dialog.body(|ui| {
                ui.label("Cue name");
                let response = ui.add(
                    TextEdit::singleline(&mut rename.text)
                        .id_salt("timeline_slide_cue_rename_text"),
                );
                if (response.has_focus() || response.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    commit = true;
                }
                if let Some(error) = &rename.error {
                    ui.colored_label(coursecraft_render::palette(ui.ctx()).danger, error.clone());
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("Rename").clicked() {
                        commit = true;
                    }
                });
            });
        });
    if !open {
        cancel = true;
    }
    if commit {
        let name = rename.text.trim();
        if name.is_empty() {
            rename.error = Some("Cue name cannot be empty".to_owned());
            editor.ui.timeline.slide_cue_rename = Some(rename);
        } else {
            let mut params = serde_json::json!({
                "slide": rename.slide,
                "cue": rename.cue,
                "patch": { "name": name },
            });
            if let Some(lid) = rename.layer {
                params["layer"] = serde_json::json!(lid);
            }
            actions.push(("slide.set_cue", params));
        }
    } else if !cancel {
        editor.ui.timeline.slide_cue_rename = Some(rename);
    }
}

struct SlideCueRulerContext<'a> {
    slide_id: &'a str,
    selected_layer_id: &'a Option<String>,
    slide_cues: &'a [coursecraft_course::SlideCue],
    track_rect: Rect,
    ruler_rect: Rect,
    visible_track: Rect,
    duration_ms: u32,
    track_width: f32,
    pointer_pos: Option<egui::Pos2>,
}

fn render_slide_cue_markers(
    editor: &mut Editor,
    ui: &mut Ui,
    painter: &egui::Painter,
    ctx: &SlideCueRulerContext<'_>,
    actions: &mut Vec<(&'static str, Json)>,
) -> bool {
    let SlideCueRulerContext {
        slide_id,
        selected_layer_id,
        slide_cues,
        track_rect,
        ruler_rect,
        visible_track,
        duration_ms,
        track_width,
        pointer_pos,
    } = ctx;
    let duration_ms = *duration_ms;
    let track_width = *track_width;

    editor.ui.timeline.slide_cue_rects.clear();
    let mut slide_cue_markers: Vec<(coursecraft_course::SlideCue, Rect)> = Vec::new();
    for cue in *slide_cues {
        let dragging_this = editor
            .ui
            .timeline
            .slide_cue_drag
            .as_ref()
            .is_some_and(|d| &d.layer == *selected_layer_id && d.cue_id == cue.id);
        let at_ms = if dragging_this {
            editor
                .ui
                .timeline
                .slide_cue_drag
                .as_ref()
                .map_or(cue.at_ms, |d| d.current_at_ms)
        } else {
            cue.at_ms
        };
        let cx =
            track_rect.min.x + (at_ms.min(duration_ms) as f32 / duration_ms as f32) * track_width;
        let rect = Rect::from_center_size(pos2(cx, ruler_rect.max.y - 6.0), vec2(12.0, 12.0));
        if visible_track.x_range().contains(cx) {
            slide_cue_markers.push((cue.clone(), rect));
        }
    }
    let on_slide_cue =
        pointer_pos.is_some_and(|pos| slide_cue_markers.iter().any(|(_, r)| r.contains(pos)));

    let ruler_cue_painter = painter.with_clip_rect(painter.clip_rect().intersect(
        Rect::from_x_y_ranges(visible_track.x_range(), ruler_rect.y_range()),
    ));
    for (cue, rect) in &slide_cue_markers {
        let selected = editor
            .ui
            .timeline
            .slide_cue_selected
            .as_ref()
            .is_some_and(|(l, id)| l == *selected_layer_id && id == &cue.id);
        let response = ui
            .interact(
                *rect,
                ui.scope_id()
                    .with("timeline_slide_cue")
                    .with(cue.id.as_str()),
                Sense::click_and_drag(),
            )
            .on_hover_text(format!(
                "{} ({})",
                cue.name,
                format_seconds(u64::from(cue.at_ms))
            ));
        draw_cue_marker(&ruler_cue_painter, rect.center(), selected);
        editor
            .ui
            .timeline
            .slide_cue_rects
            .insert(cue.id.clone(), *rect);
        if response.clicked() {
            editor.ui.timeline.cue_selected = None;
            editor.ui.timeline.slide_cue_selected =
                Some(((*selected_layer_id).clone(), cue.id.clone()));
        }
        if response.double_clicked() {
            editor.ui.timeline.cue_selected = None;
            editor.ui.timeline.slide_cue_selected =
                Some(((*selected_layer_id).clone(), cue.id.clone()));
            editor.ui.timeline.slide_cue_rename = Some(SlideCueRename {
                slide: (*slide_id).to_owned(),
                layer: (*selected_layer_id).clone(),
                cue: cue.id.clone(),
                text: cue.name.clone(),
                error: None,
            });
        }
        if response.secondary_clicked() {
            let mut params = serde_json::json!({
                "slide": *slide_id,
                "cue": cue.id,
            });
            if let Some(lid) = *selected_layer_id {
                params["layer"] = serde_json::json!(lid);
            }
            actions.push(("slide.remove_cue", params));
            editor.ui.timeline.slide_cue_selected = None;
        }
        if response.drag_started() {
            editor.ui.timeline.slide_cue_drag = Some(SlideCueDrag {
                layer: (*selected_layer_id).clone(),
                cue_id: cue.id.clone(),
                orig_at_ms: cue.at_ms,
                current_at_ms: cue.at_ms,
                start_pointer_x: rect.center().x,
                duration_ms,
            });
        }
        if response.dragged()
            && let Some(pos) = *pointer_pos
            && let Some(drag) = &mut editor.ui.timeline.slide_cue_drag
            && &drag.layer == *selected_layer_id
            && drag.cue_id == cue.id
        {
            let delta_x = pos.x - drag.start_pointer_x;
            let raw = f64::from(drag.orig_at_ms)
                + f64::from(delta_x / track_width.max(1.0) * duration_ms as f32);
            let snapped = if ui.input(|i| i.modifiers.shift) {
                raw
            } else {
                (raw / 100.0).round() * 100.0
            };
            drag.current_at_ms = snapped.clamp(0.0, f64::from(duration_ms)).round() as u32;
        }
        if response.drag_stopped()
            && let Some(drag) = editor.ui.timeline.slide_cue_drag.take()
            && &drag.layer == *selected_layer_id
            && drag.cue_id == cue.id
            && drag.current_at_ms != drag.orig_at_ms
        {
            let mut params = serde_json::json!({
                "slide": *slide_id,
                "cue": drag.cue_id,
                "patch": { "at_ms": drag.current_at_ms },
            });
            if let Some(ref lid) = drag.layer {
                params["layer"] = serde_json::json!(lid);
            }
            actions.push(("slide.set_cue", params));
        }
    }
    on_slide_cue
}

/// Draw one of the bottom tabs other than the timeline.
pub fn show_bottom_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    tab: BottomTab,
    actions: &mut Vec<(&'static str, Json)>,
) {
    match tab {
        BottomTab::Timeline => {}
        BottomTab::States => crate::states::show_states_tab(editor, ui, actions),
        BottomTab::Triggers => crate::triggers::show_triggers_tab(editor, ui, actions),
        BottomTab::Variables => crate::variables::show_variables_tab(editor, ui, actions),
        BottomTab::Question => crate::question::show_question_tab(editor, ui, actions),
    }
}

/// Why a bottom tab has nothing to show for the slide in hand, if it has not:
/// the fixed panel hides these tabs, a dock panel says why.
#[must_use]
pub fn bottom_tab_unavailable(editor: &Editor, tab: BottomTab) -> Option<&'static str> {
    if editor.ui.master_view {
        return match tab {
            BottomTab::Timeline => None,
            BottomTab::States => Some(tl!("Slide masters have no states.")),
            BottomTab::Triggers => Some(tl!("Slide masters have no triggers.")),
            BottomTab::Variables => Some(tl!(
                "Variables are edited on course slides, not on masters."
            )),
            BottomTab::Question => Some(tl!("Slide masters have no question.")),
        };
    }
    let slide_is_draw = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some_and(|slide| slide.draw.is_some());
    (tab == BottomTab::Question && slide_is_draw).then(|| tl!("A draw slide has no question."))
}

pub(crate) fn show_timeline_content(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    if editor.ui.master_view && editor.ui.bottom_tab == BottomTab::States {
        editor.ui.bottom_tab = BottomTab::Timeline;
    }
    editor.ui.timeline.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
    );

    // Escape cancels a row drag without also clearing the selection on the canvas.
    if editor.ui.timeline.active_row_drag.is_some()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        editor.ui.timeline.active_row_drag = None;
    }

    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        ui.label("No slide selected");
        return;
    };

    let slide_id = slide.id.clone();
    let selected_layer_id = editor.selection.layer.clone();
    let (owner_title, owner_duration, is_layer) = match &selected_layer_id {
        Some(lid) => match slide.layer(lid) {
            Some(l) => (format!("Layer: {}", l.name), l.duration_ms, true),
            None => ("Slide Base".to_owned(), slide.duration_ms, false),
        },
        None => (format!("Slide: {}", slide.title), slide.duration_ms, false),
    };

    let objects = std::sync::Arc::clone(&editor.ui.timeline.cached_objects);
    let slide_cues = std::sync::Arc::clone(&editor.ui.timeline.cached_slide_cues);
    let audio_srcs = std::sync::Arc::clone(&editor.ui.timeline.cached_audio_srcs);
    for src in audio_srcs.iter() {
        let _ = editor.ensure_audio_waveform(src);
    }

    render_timeline_header(
        editor,
        ui,
        &slide_id,
        &owner_title,
        owner_duration,
        is_layer,
        selected_layer_id.as_deref(),
        actions,
    );

    bleed_separator(ui);

    // Docked, the other bottom tabs are dock panels of their own.
    if editor.ui.bottom_tab != BottomTab::Timeline && !editor.ui.dock.enabled {
        show_bottom_tab(editor, ui, editor.ui.bottom_tab, actions);
        return;
    }

    let duration_ms = owner_duration.max(100);
    let total_area = ui.available_rect_before_wrap();
    if total_area.width() <= TRACK_HEADER_WIDTH + 50.0
        || total_area.height() <= TIME_RULER_HEIGHT
        || !total_area.is_finite()
    {
        return;
    }

    let scroll_gutter = timeline_scrollbar_gutter(ui);
    let track_rect = Rect::from_min_max(
        pos2(total_area.min.x + TRACK_HEADER_WIDTH, total_area.min.y),
        pos2(
            (total_area.max.x - scroll_gutter).max(total_area.min.x + TRACK_HEADER_WIDTH + 1.0),
            total_area.max.y,
        ),
    );
    let visible_track = track_rect;
    zoom_and_scroll_track(editor, ui, total_area, visible_track);
    let zoom = editor.ui.timeline.zoom;
    let scroll_x = editor.ui.timeline.scroll_x;
    let track_width = visible_track.width() * zoom;
    // Everything below maps time onto this full-length track; only part of it shows.
    let track_rect = Rect::from_min_size(
        pos2(visible_track.min.x - scroll_x, visible_track.min.y),
        vec2(track_width, visible_track.height()),
    );
    let ruler_rect = Rect::from_min_size(
        visible_track.min,
        vec2(visible_track.width(), TIME_RULER_HEIGHT),
    );
    // Row backgrounds bleed into the panel's horizontal inner margin; text and bars stay inside.
    let painter = ui.painter_at(total_area.expand2(vec2(coursecraft_render::PANEL_MARGIN, 0.0)));

    editor.ui.timeline.ruler_label_rects.clear();
    render_time_ruler(
        &painter,
        track_rect,
        ruler_rect,
        duration_ms,
        track_width,
        total_area,
        &mut editor.ui.timeline.ruler_label_rects,
    );

    editor.ui.timeline.playhead_ms = editor.ui.timeline.playhead_ms.min(duration_ms);
    let accent = coursecraft_render::palette(ui.ctx()).accent;

    let mut all_cue_ms: Vec<u32> = objects
        .iter()
        .flat_map(|(oid, _, _, _)| object_cues(editor, oid))
        .map(|cue| cue.at_ms_u64().min(u64::from(u32::MAX)) as u32)
        .collect();
    for cue in slide_cues.iter() {
        all_cue_ms.push(cue.at_ms);
    }

    // Slide/layer cue markers on the ruler.
    let pointer_pos = ui
        .input(|i| i.pointer.interact_pos())
        .filter(|pos| crate::widgets::pointer_reaches(ui, *pos));
    let cue_ctx = SlideCueRulerContext {
        slide_id: &slide_id,
        selected_layer_id: &selected_layer_id,
        slide_cues: &slide_cues,
        track_rect,
        ruler_rect,
        visible_track,
        duration_ms,
        track_width,
        pointer_pos,
    };
    let playhead_response = ui.interact(
        ruler_rect,
        ui.scope_id().with("timeline_playhead"),
        Sense::click_and_drag(),
    );
    let on_slide_cue = render_slide_cue_markers(editor, ui, &painter, &cue_ctx, actions);
    if !on_slide_cue
        && editor.ui.timeline.slide_cue_drag.is_none()
        && (playhead_response.clicked() || playhead_response.dragged())
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        // Dragging within PLAYHEAD_SNAP_PX of a cue marker snaps to it;
        // Shift disables snapping. Clicking or dragging while playing
        // seeks and keeps playing.
        let shift = ui.input(|i| i.modifiers.shift);
        let snapped = snapped_playhead_ms(
            pos.x,
            track_rect.min.x,
            track_width,
            duration_ms,
            &all_cue_ms,
            shift,
        );
        editor.ui.timeline.playhead_ms = snapped;
        let now_mono = editor.timeline_mono_ms;
        editor
            .timeline_playback
            .seek_to(snapped, duration_ms, now_mono);
        ui.ctx().request_repaint();
    }
    let playhead_x = track_rect.min.x
        + (editor.ui.timeline.playhead_ms as f32 / duration_ms as f32) * track_width;
    painter.line_segment(
        [
            pos2(playhead_x, ruler_rect.min.y),
            pos2(playhead_x, total_area.max.y),
        ],
        Stroke::new(1.5, accent),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(playhead_x - 5.0, ruler_rect.min.y),
            pos2(playhead_x + 5.0, ruler_rect.min.y),
            pos2(playhead_x, ruler_rect.min.y + 6.0),
        ],
        accent,
        Stroke::NONE,
    ));

    if zoom > 1.0 {
        // A thin thumb shows which part of the timeline is on screen.
        let p = coursecraft_render::palette(ui.ctx());
        let width = visible_track.width();
        let thumb = Rect::from_min_size(
            pos2(
                visible_track.min.x + scroll_x / zoom,
                ruler_rect.max.y - 3.0,
            ),
            vec2(width / zoom, 3.0),
        );
        painter.rect_filled(thumb, CornerRadius::same(1), p.text_tertiary);
    }

    let drag_ctx = ActiveDragContext {
        duration_ms,
        track_x: track_rect.min.x,
        track_width,
        cue_ms: &all_cue_ms,
        slide_id: &slide_id,
    };
    handle_active_drag(editor, ui, &drag_ctx, actions);

    show_cue_rename_window(editor, ui, actions);
    show_slide_cue_rename_window(editor, ui, actions);

    let row_area = Rect::from_min_max(pos2(total_area.min.x, ruler_rect.max.y), total_area.max);
    if row_area.height() > 0.0 {
        // The ruler is only painted, so step past it or the top row hides beneath it.
        ui.allocate_space(vec2(row_area.width(), row_area.min.y - total_area.min.y));
        ui.scope(|ui| {
            configure_timeline_scroll_style(&mut ui.spacing_mut().scroll);
            egui::ScrollArea::vertical()
                .id_salt("timeline_object_rows_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let origin = ui.available_rect_before_wrap();
                    let row_painter = ui
                        .painter_at(row_area.expand2(vec2(coursecraft_render::PANEL_MARGIN, 0.0)));
                    render_object_rows(
                        editor,
                        ui,
                        &row_painter,
                        &objects,
                        origin.min.y,
                        row_area,
                        track_rect,
                        visible_track,
                        duration_ms,
                        track_width,
                        actions,
                    );
                    let row_count =
                        timeline_rows(&objects, &editor.ui.timeline.collapsed_groups).len();
                    let content_height =
                        (row_count as f32 * OBJECT_ROW_HEIGHT).max(row_area.height());
                    ui.allocate_space(vec2(ui.available_width(), content_height));
                });
        });
    }
}

/// Constant width of the timeline vertical scrollbar, in screen points.
const TIMELINE_SCROLLBAR_WIDTH: f32 = 8.0;

/// Use a solid, constant-width scrollbar that reserves space and never grows on hover.
fn configure_timeline_scroll_style(scroll: &mut egui::style::ScrollStyle) {
    scroll.floating = false;
    scroll.bar_width = TIMELINE_SCROLLBAR_WIDTH;
    scroll.floating_width = TIMELINE_SCROLLBAR_WIDTH;
    scroll.floating_allocated_width = TIMELINE_SCROLLBAR_WIDTH;
}

/// Horizontal space reserved on the right of the timeline track so the vertical
/// scrollbar and the 3 px overhang of an object's end handle never overlap.
fn timeline_scrollbar_gutter(ui: &Ui) -> f32 {
    let scroll = &ui.spacing().scroll;
    TIMELINE_SCROLLBAR_WIDTH + scroll.bar_inner_margin + scroll.bar_outer_margin + 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_end_near_timeline_end_does_not_panic() {
        let course = coursecraft_engine::Session::blank("Timeline").course;
        let mut editor = Editor::new(course);
        editor.ui.timeline.active_drag = Some(TimelineDrag {
            op: TimelineDragOp::ResizeEnd,
            object_id: "obj1".to_owned(),
            orig_start_ms: 4950,
            orig_end_ms: None,
            orig_duration_ms: 5000,
            start_pointer_x: 100.0,
            start_pointer_y: 10.0,
            current_start_ms: 4950,
            current_end_ms: None,
        });
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .events
            .push(egui::Event::PointerMoved(pos2(100.0, 10.0)));
        input.events.push(egui::Event::PointerButton {
            pos: pos2(100.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        let out = ctx.run_ui(input, |ui| {
            let drag_ctx = ActiveDragContext {
                duration_ms: 5000,
                track_x: 0.0,
                track_width: 500.0,
                cue_ms: &[],
                slide_id: "slide1",
            };
            let mut actions = Vec::new();
            handle_active_drag(&mut editor, ui, &drag_ctx, &mut actions);
        });
        out.drop_without_applying_deltas();
        assert!(editor.ui.timeline.active_drag.is_some());
    }

    #[test]
    fn visible_ruler_ticks_with_u32_max_duration_returns_quickly() {
        let ticks = visible_ruler_ticks(u32::MAX, 0.0, 800.0, 0.0, 800.0);
        assert!(!ticks.is_empty());
        assert!(ticks.len() <= 500, "too many ticks: {}", ticks.len());
    }

    #[test]
    fn audio_waveform_window_keeps_real_time_scale_for_shorter_trimmed_and_longer_bars() {
        assert_eq!(
            audio_waveform_window(10_000, 5_000, None),
            Some((0.0, 0.5, 1.0))
        );

        let trim_start = Some(coursecraft_course::Trim {
            start_ms: 2_000,
            end_ms: None,
        });
        assert_eq!(
            audio_waveform_window(10_000, 5_000, trim_start),
            Some((0.2, 0.7, 1.0))
        );

        let (ps, pe, fill) = audio_waveform_window(10_000, 15_000, None).expect("window");
        assert_eq!((ps, pe), (0.0, 1.0));
        assert!((fill - (10.0_f32 / 15.0_f32)).abs() < 1e-5);

        let trim_both = Some(coursecraft_course::Trim {
            start_ms: 2_000,
            end_ms: Some(6_000),
        });
        assert_eq!(
            audio_waveform_window(10_000, 8_000, trim_both),
            Some((0.2, 0.6, 0.5))
        );

        assert_eq!(audio_waveform_window(10_000, 0, None), None);
        assert_eq!(
            audio_waveform_window(
                10_000,
                5_000,
                Some(coursecraft_course::Trim {
                    start_ms: 10_000,
                    end_ms: None,
                })
            ),
            None
        );
    }
}
