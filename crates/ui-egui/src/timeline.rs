//! Timeline bottom panel: time ruler, object timing bars, move/resize dragging, and owner duration.

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::{Course, Cue, ObjectKind, Timing};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, DragValue, FontId, Rect, RichText, Sense, Stroke,
    StrokeKind, TextEdit, Ui, pos2, vec2,
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
    Triggers,
    Variables,
    Question,
}

impl BottomTab {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Timeline => "Timeline",
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
        }
    }
}

/// Snap radius for playhead dragging, in screen points.
pub const PLAYHEAD_SNAP_PX: f32 = 6.0;

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
    if !shift {
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
        if let Some(cue) = nearest {
            return cue.min(duration_ms);
        }
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
        self.last_synced_revision.set(revision);
        self.last_synced_selection = selection.clone();

        if let Some(slide) = course.slide(&selection.slide) {
            if let Some(ref lid) = selection.layer
                && let Some(layer) = slide.layer(lid)
            {
                self.duration_ms = layer.duration_ms;
                return;
            }
            self.duration_ms = slide.duration_ms;
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
                if !editor.ui.master_view {
                    tabs.extend([BottomTab::Triggers, BottomTab::Variables]);
                    // A draw slide has no question panel.
                    let slide_is_draw = editor
                        .session
                        .course
                        .slide(&editor.selection.slide)
                        .is_some_and(|slide| slide.draw.is_some());
                    if !slide_is_draw {
                        tabs.push(BottomTab::Question);
                    }
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
        ui.label(RichText::new(readout).color(p.text_secondary));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dur_val = ui.add(
                DragValue::new(&mut editor.ui.timeline.duration_ms)
                    .speed(50.0)
                    .range(100..=86_400_000)
                    .suffix(" ms"),
            );
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

            ui.label(coursecraft_render::caption("Duration").color(p.text_secondary));
        });
    });
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

    let ms_to_x =
        |ms: u32| -> f32 { track_rect.min.x + (ms as f32 / duration_ms as f32) * track_width };

    // Ticks and grid lines stay in the track when it is zoomed and scrolled.
    let label_painter = painter;
    let painter = &painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_x_y_ranges(
        ruler_rect.x_range(),
        painter.clip_rect().y_range(),
    )));
    let half_second_px = 500.0 / duration_ms as f32 * track_width;
    let tick_step: u32 = if half_second_px >= 120.0 { 100 } else { 500 };
    let mut ms_tick: u32 = 0;
    while ms_tick <= duration_ms {
        let x = ms_to_x(ms_tick);
        let is_sec = ms_tick.is_multiple_of(1000);
        let tick_h = if is_sec { 12.0 } else { 6.0 };
        painter.line_segment(
            [
                pos2(x, ruler_rect.max.y - tick_h),
                pos2(x, ruler_rect.max.y),
            ],
            Stroke::new(1.0, p.text_tertiary),
        );
        if is_sec && (ruler_rect.min.x - 0.5..=ruler_rect.max.x + 0.5).contains(&x) {
            let galley = label_painter.layout_no_wrap(
                format!("{}s", ms_tick / 1000),
                FontId::proportional(11.0),
                p.text_tertiary,
            );
            // Keep every label inside the ruler: right-align the last label to the
            // edge instead of letting it run past the panel.
            let max_x = (ruler_rect.max.x - galley.size().x).max(ruler_rect.min.x);
            let label_x = (x + 2.0).clamp(ruler_rect.min.x, max_x);
            let label_pos = pos2(label_x, ruler_rect.min.y + 2.0);
            label_rects.push(Rect::from_min_size(label_pos, galley.size()));
            label_painter.galley(label_pos, galley, p.text_tertiary);
        }
        painter.line_segment(
            [pos2(x, ruler_rect.max.y), pos2(x, total_area.max.y)],
            Stroke::new(1.0, p.border),
        );
        ms_tick = ms_tick.saturating_add(tick_step);
        if ms_tick == 0 {
            break;
        }
    }
}

fn handle_active_drag(
    editor: &mut Editor,
    ui: &mut Ui,
    duration_ms: u32,
    track_width: f32,
    slide_id: &str,
    actions: &mut Vec<(&'static str, Json)>,
) {
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

        match drag.op {
            TimelineDragOp::ResizeStart => {
                let eff_end = drag.orig_end_ms.unwrap_or(drag.orig_duration_ms);
                let max_start = eff_end.saturating_sub(snap_step as u32);
                let raw_start = drag.orig_start_ms as i64 + delta_ms;
                drag.current_start_ms = raw_start.clamp(0, max_start as i64) as u32;
                drag.current_end_ms = drag.orig_end_ms;
            }
            TimelineDragOp::ResizeEnd => {
                let eff_end = drag.orig_end_ms.unwrap_or(drag.orig_duration_ms);
                let min_end = drag.orig_start_ms + snap_step as u32;
                let raw_end = eff_end as i64 + delta_ms;
                let new_end = raw_end.clamp(min_end as i64, drag.orig_duration_ms as i64) as u32;
                drag.current_start_ms = drag.orig_start_ms;
                drag.current_end_ms = Some(new_end);
            }
            TimelineDragOp::Move => {
                if let Some(orig_end) = drag.orig_end_ms {
                    let len = orig_end
                        .saturating_sub(drag.orig_start_ms)
                        .max(snap_step as u32);
                    let max_start = drag.orig_duration_ms.saturating_sub(len);
                    let raw_start = drag.orig_start_ms as i64 + delta_ms;
                    let new_start = raw_start.clamp(0, max_start as i64) as u32;
                    drag.current_start_ms = new_start;
                    drag.current_end_ms = Some(new_start + len);
                } else {
                    let max_start = drag.orig_duration_ms.saturating_sub(snap_step as u32);
                    let raw_start = drag.orig_start_ms as i64 + delta_ms;
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
    _eff_start: u32,
    _timing: &Timing,
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
            "{}ms - {}",
            drag.current_start_ms,
            drag.current_end_ms
                .map(|e| format!("{e}ms"))
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
                let row_rects = editor.ui.timeline.object_row_rects.clone();
                drag.target_slot = drop_slot(&display, &row_rects, pos.y);
                if let Some(y) = drop_line_y(&display, &row_rects, drag.target_slot) {
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

/// Cues on the object `oid` on the current timeline, when it is a video.
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

    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let modifiers = ui.input(|i| i.modifiers);

    let row_count = objects.len();
    for display_idx in 0..row_count {
        // The top row is the frontmost object, so display order is stacking order reversed.
        let model_idx = row_count.saturating_sub(1).saturating_sub(display_idx);
        let Some((oid, oname, timing, group)) = objects.get(model_idx) else {
            continue;
        };
        let row_y = content_start_y + display_idx as f32 * OBJECT_ROW_HEIGHT;

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

        let row_response = ui.interact(
            row_rect,
            ui.scope_id().with("timeline_row").with(oid),
            Sense::click(),
        );
        // Eye and lock buttons at the right end of the name column.
        let lock_rect = Rect::from_center_size(
            pos2(header_row_rect.max.x - 12.0, header_row_rect.center().y),
            vec2(16.0, 16.0),
        );
        let eye_rect = lock_rect.translate(vec2(-18.0, 0.0));
        let is_hidden = editor.ui.hidden_objects.contains(oid);
        let is_locked = editor.ui.locked_objects.contains(oid);
        let eye_resp = ui
            .interact(
                eye_rect,
                ui.scope_id().with("timeline_eye").with(oid),
                Sense::click(),
            )
            .on_hover_text(if is_hidden {
                "Show on canvas"
            } else {
                "Hide on canvas"
            });
        let lock_resp = ui
            .interact(
                lock_rect,
                ui.scope_id().with("timeline_lock").with(oid),
                Sense::click(),
            )
            .on_hover_text(if is_locked { "Unlock" } else { "Lock" });
        eye_resp
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Hide on canvas"));
        lock_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Lock"));
        if eye_resp.clicked() {
            if is_hidden {
                editor.ui.hidden_objects.remove(oid);
            } else {
                editor.ui.hidden_objects.insert(oid.clone());
                if editor.selection.contains(oid) {
                    editor.selection.toggle(oid);
                }
            }
        }
        if lock_resp.clicked() {
            if is_locked {
                editor.ui.locked_objects.remove(oid);
            } else {
                editor.ui.locked_objects.insert(oid.clone());
            }
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
        let text_x = if group.is_some() {
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
        let eff_start = timing.start_ms;
        let eff_end = timing.end_ms.unwrap_or(duration_ms);
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
            "{} ms – {}",
            eff_start,
            timing
                .end_ms
                .map(|e| format!("{e} ms"))
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
            } else if header_row_rect.contains(pos) && row_response.contains_pointer() {
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
                .on_hover_text(format!("{} ({} ms)", cue.name, cue.at_ms_u64()));
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
    handle_row_drag(editor, ui, painter, objects, total_area);
}

/// The most a pinch can zoom the timeline in.
const TIMELINE_MAX_ZOOM: f32 = 20.0;

/// Pinch (or Ctrl+scroll) over the timeline zooms around the pointer, and a
/// sideways two-finger swipe (or Shift+scroll) pans once it is zoomed in.
fn zoom_and_scroll_track(editor: &mut Editor, ui: &Ui, area: Rect, track: Rect) {
    let state = &mut editor.ui.timeline;
    let width = track.width();
    if ui.rect_contains_pointer(area) && state.active_drag.is_none() {
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

/// Rename window opened by double-clicking a cue marker.
fn show_cue_rename_window(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(mut rename) = editor.ui.timeline.cue_rename.take() else {
        return;
    };
    let mut commit = false;
    let mut cancel = false;
    Dialog::new("Rename cue").show(ui.ctx(), |ui| {
        ui.label("Cue name");
        let response =
            ui.add(TextEdit::singleline(&mut rename.text).id_salt("timeline_cue_rename_text"));
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            commit = true;
        }
        if let Some(error) = &rename.error {
            ui.colored_label(coursecraft_render::palette(ui.ctx()).danger, error.clone());
        }
        ui.horizontal(|ui| {
            if ui.button("Rename").clicked() {
                commit = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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

fn show_timeline_content(
    editor: &mut Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
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

    let objects: Vec<(String, String, Timing, Option<String>)> = match &selected_layer_id {
        Some(lid) => match slide.layer(lid) {
            Some(l) => l
                .objects
                .iter()
                .map(|o| (o.id.clone(), o.name.clone(), o.timing, o.group.clone()))
                .collect(),
            None => Vec::new(),
        },
        None => slide
            .objects
            .iter()
            .map(|o| (o.id.clone(), o.name.clone(), o.timing, o.group.clone()))
            .collect(),
    };

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

    if editor.ui.bottom_tab != BottomTab::Timeline {
        match editor.ui.bottom_tab {
            BottomTab::Timeline => {}
            BottomTab::Triggers => {
                crate::triggers::show_triggers_tab(editor, ui, actions);
            }
            BottomTab::Variables => {
                crate::variables::show_variables_tab(editor, ui, actions);
            }
            BottomTab::Question => {
                crate::question::show_question_tab(editor, ui, actions);
            }
        }
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

    let track_rect = Rect::from_min_max(
        pos2(total_area.min.x + TRACK_HEADER_WIDTH, total_area.min.y),
        total_area.max,
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
    let playhead_response = ui.interact(
        ruler_rect,
        ui.scope_id().with("timeline_playhead"),
        Sense::click_and_drag(),
    );
    if (playhead_response.clicked() || playhead_response.dragged())
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        // Dragging within PLAYHEAD_SNAP_PX of a cue marker snaps to it;
        // Shift disables snapping. Clicking or dragging while playing
        // seeks and keeps playing.
        let cue_ms: Vec<u32> = objects
            .iter()
            .flat_map(|(oid, _, _, _)| object_cues(editor, oid))
            .map(|cue| cue.at_ms_u64().min(u64::from(u32::MAX)) as u32)
            .collect();
        let shift = ui.input(|i| i.modifiers.shift);
        let snapped = snapped_playhead_ms(
            pos.x,
            track_rect.min.x,
            track_width,
            duration_ms,
            &cue_ms,
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

    handle_active_drag(editor, ui, duration_ms, track_width, &slide_id, actions);

    show_cue_rename_window(editor, ui, actions);

    let row_area = Rect::from_min_max(pos2(total_area.min.x, ruler_rect.max.y), total_area.max);
    if row_area.height() > 0.0 {
        // The ruler is only painted, so step past it or the top row hides beneath it.
        ui.allocate_space(vec2(row_area.width(), row_area.min.y - total_area.min.y));
        egui::ScrollArea::vertical()
            .id_salt("timeline_object_rows_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let origin = ui.available_rect_before_wrap();
                let row_painter =
                    ui.painter_at(row_area.expand2(vec2(coursecraft_render::PANEL_MARGIN, 0.0)));
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
                let content_height =
                    (objects.len() as f32 * OBJECT_ROW_HEIGHT).max(row_area.height());
                ui.allocate_space(vec2(row_area.width(), content_height));
            });
    }
}
