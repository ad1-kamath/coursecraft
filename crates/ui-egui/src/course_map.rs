//! Course map: the whole course as a map, one column per scene.
//!
//! Read-only for branching (triggers are edited on the slide): arrows are
//! computed from the course on each frame by
//! [`coursecraft_course::course_map`] and never stored. Clicking selects a
//! slide, double-clicking opens it in the slide editor, dragging reorders
//! through the `slide.move` engine command (undoable), and double-clicking
//! a scene title renames it through `scene.update`. Only thumbnails and
//! arrows near the viewport are drawn, so a 500-slide course stays
//! interactive.

use crate::{Editor, slides::SceneRename};
use coursecraft_course::{Course, CourseMap, MapEdgeKind};
use coursecraft_render::{Highlight, SlideTransform, compose_authoring, paint};
use egui::epaint::CubicBezierShape;
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, Modifiers, Rect, ScrollArea, Sense, Stroke,
    StrokeKind, TextEdit, Ui, pos2, vec2,
};
use std::collections::HashMap;

/// Active drag of a slide thumbnail in the Course map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDrag {
    /// Slide being dragged.
    pub slide_id: String,
    /// Scene it came from and its index there.
    pub source_scene: String,
    pub source_index: usize,
    /// Pointer position at the press, for the click threshold.
    pub start_pointer: egui::Pos2,
    /// True once the pointer moved past the click threshold.
    pub moved: bool,
    /// Insertion scene and index in that scene's current slide list.
    pub target_scene: String,
    pub target_index: usize,
}

/// Transient Course map state: drag, rename and the rects the last frame
/// laid out (test hooks), plus the cached course graph.
#[derive(Debug, Default)]
pub struct CourseMapState {
    pub drag: Option<MapDrag>,
    pub rename: Option<SceneRename>,
    pub thumb_rects: Vec<(String, Rect)>,
    pub scene_header_rects: Vec<(String, Rect)>,
    pub rename_edit_rect: Option<Rect>,
    /// Cached graph (ids, titles, edges); recomputed only when the course
    /// revision changes, so the map is not rebuilt every frame.
    pub cached_map: Option<CourseMap>,
    /// Course revision the cached map was built from (`u64::MAX` = never).
    pub cached_revision: u64,
    /// A text field had keyboard focus on the previous map-keys call.
    pub keyboard_was_focused: bool,
}

pub const MAP_COL_W: f32 = 204.0;
pub const MAP_THUMB_W: f32 = 176.0;
pub const MAP_THUMB_H: f32 = 99.0;
pub const MAP_TITLE_H: f32 = 18.0;
pub const MAP_ROW_H: f32 = 133.0;
pub const MAP_HEADER_H: f32 = 26.0;
pub const MAP_COL_GAP: f32 = 28.0;
pub const MAP_PAD: f32 = 16.0;
/// Outgoing arrows drawn per slide; past this they are grouped.
pub const MAP_MAX_ARROWS_PER_SLIDE: usize = 12;

/// Colour for an edge kind, from the design tokens.
fn edge_color(p: coursecraft_render::Palette, kind: MapEdgeKind) -> Color32 {
    match kind {
        MapEdgeKind::Jump => p.accent,
        MapEdgeKind::Lightbox => p.success,
        MapEdgeKind::Link => p.warning,
        MapEdgeKind::Next => p.text_tertiary,
    }
}

/// One line of the legend: a sample stroke and its label.
fn legend_entry(ui: &mut Ui, color: Color32, dashed: bool, label: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 2.0), Sense::hover());
    let y = rect.center().y;
    let stroke = Stroke::new(1.5, color);
    if dashed {
        for shape in egui::Shape::dashed_line(
            &[pos2(rect.min.x, y), pos2(rect.max.x, y)],
            stroke,
            5.0,
            3.0,
        ) {
            ui.painter().add(shape);
        }
    } else {
        ui.painter()
            .line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], stroke);
    }
    ui.label(coursecraft_render::small(label.to_owned()));
}

/// Render the Course map in the central area.
pub fn show_course_map(editor: &mut Editor, ui: &mut Ui) {
    // The map shows play slides only: in Master View it must not draw or
    // handle clicks (they would select a play slide under the masters),
    // and entering Master View leaves the Course map.
    if editor.ui.master_view {
        editor.ui.story_view = false;
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());

    // Legend, pinned above the scroll area.
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::small("Course map"));
        legend_entry(ui, edge_color(p, MapEdgeKind::Jump), false, "Jump");
        legend_entry(ui, edge_color(p, MapEdgeKind::Lightbox), false, "Lightbox");
        legend_entry(ui, edge_color(p, MapEdgeKind::Link), false, "Link");
        legend_entry(ui, edge_color(p, MapEdgeKind::Next), true, "Next");
        legend_entry(ui, edge_color(p, MapEdgeKind::Jump), true, "Conditional");
    });

    // Cached graph: ids, titles and edges only (never the whole course).
    // Recomputed only when the course revision changes, not every frame.
    let revision = editor.session.revision;
    let stale = match &editor.ui.course_map.cached_map {
        Some(_) => editor.ui.course_map.cached_revision != revision,
        None => true,
    };
    if stale {
        editor.ui.course_map.cached_map =
            Some(coursecraft_course::course_map(&editor.session.course));
        editor.ui.course_map.cached_revision = revision;
    }
    let map = editor
        .ui
        .course_map
        .cached_map
        .take()
        .unwrap_or_else(|| coursecraft_course::course_map(&editor.session.course));
    let selected = editor.selection.slide.clone();

    let out = ScrollArea::both()
        .id_salt("course_map_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let origin = ui.available_rect_before_wrap();
            let clip = ui.clip_rect();
            let scenes_len = map.scenes.len();
            let mut rects: HashMap<String, (usize, Rect)> = HashMap::new();
            let mut rows: Vec<(String, String, usize, Rect)> = Vec::new();
            let mut headers: Vec<(String, Rect)> = Vec::new();
            // Each column starts at the top; the scroll height is the
            // tallest column, not the staircase of stacked columns.
            let top = origin.min.y + MAP_PAD;
            let mut tallest = top;

            for (scene_idx, scene) in map.scenes.iter().enumerate() {
                let mut y = top;
                let col_x = origin.min.x + MAP_PAD + scene_idx as f32 * (MAP_COL_W + MAP_COL_GAP);
                let header_rect =
                    Rect::from_min_size(pos2(col_x, y), vec2(MAP_COL_W, MAP_HEADER_H));
                headers.push((scene.id.clone(), header_rect));
                if clip.intersects(header_rect.expand(2.0)) {
                    show_scene_header(editor, ui, &scene.id, &scene.title, header_rect);
                }
                y += MAP_HEADER_H + 8.0;
                for (slide_idx, slide) in scene.slides.iter().enumerate() {
                    let thumb_rect = Rect::from_min_size(
                        pos2(col_x + (MAP_COL_W - MAP_THUMB_W) * 0.5, y),
                        vec2(MAP_THUMB_W, MAP_THUMB_H),
                    );
                    rects.insert(slide.id.clone(), (scene_idx, thumb_rect));
                    rows.push((scene.id.clone(), slide.id.clone(), slide_idx, thumb_rect));
                    if clip.intersects(thumb_rect.expand(2.0)) {
                        show_map_thumbnail(
                            editor,
                            ui,
                            &slide.id,
                            thumb_rect,
                            &selected,
                            slide.unreachable,
                        );
                    }
                    y += MAP_ROW_H;
                }
                tallest = tallest.max(y + 16.0);
            }

            let total_w = MAP_PAD * 2.0
                + scenes_len as f32 * MAP_COL_W
                + (scenes_len.saturating_sub(1) as f32) * MAP_COL_GAP;
            let total_h = (tallest - origin.min.y) + MAP_PAD;
            // Keep the laid-out rects for tests and for keyboard-free checks.
            editor.ui.course_map.thumb_rects = rows
                .iter()
                .map(|(_, id, _, rect)| (id.clone(), *rect))
                .collect();
            editor.ui.course_map.scene_header_rects = headers.clone();
            ui.allocate_space(vec2(total_w, total_h));

            // Arrows above the thumbnails, in the gaps between columns.
            draw_arrows(ui, &map, &rects, &clip, &selected);

            // Insertion marker while dragging.
            handle_map_drag(editor, ui, &rows, &headers);
        });
    let _ = out;
    editor.ui.course_map.cached_map = Some(map);
}

/// A scene title header: double-click renames through `scene.update`.
fn show_scene_header(editor: &mut Editor, ui: &mut Ui, scene_id: &str, title: &str, rect: Rect) {
    let p = coursecraft_render::palette(ui.ctx());
    let resp = ui.interact(
        rect,
        ui.scope_id().with("map_scene_header").with(scene_id),
        Sense::click(),
    );
    if resp.double_clicked() {
        editor.ui.course_map.rename = Some(SceneRename {
            scene: scene_id.to_owned(),
            buffer: title.to_owned(),
            original: title.to_owned(),
            just_entered: true,
        });
    }
    let renaming = editor
        .ui
        .course_map
        .rename
        .as_ref()
        .is_some_and(|rename| rename.scene == scene_id);
    if renaming {
        show_map_rename_editor(editor, ui, rect);
    } else {
        ui.painter_at(rect).text(
            rect.left_center(),
            Align2::LEFT_CENTER,
            title,
            FontId::new(
                coursecraft_render::SIZE_CAPTION,
                egui::FontFamily::Name("inter-bold".into()),
            ),
            p.text,
        );
    }
}

fn show_map_rename_editor(editor: &mut Editor, ui: &mut Ui, rect: Rect) {
    let Some(rename) = editor.ui.course_map.rename.clone() else {
        return;
    };
    editor.ui.course_map.rename_edit_rect = Some(rect);
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("map_scene_rename", &rename.scene));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let mut output = TextEdit::singleline(&mut buffer)
        .id(edit_id)
        .font(FontId::new(
            coursecraft_render::SIZE_CAPTION,
            egui::FontFamily::Name("inter-bold".into()),
        ))
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
        if let Some(state) = editor.ui.course_map.rename.as_mut() {
            state.just_entered = false;
        }
    }
    if let Some(state) = editor.ui.course_map.rename.as_mut() {
        state.buffer = buffer.clone();
    }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    if escaped {
        editor.ui.course_map.rename = None;
        editor.ui.course_map.rename_edit_rect = None;
    } else if enter || lost {
        let title = buffer.trim().to_owned();
        if !title.is_empty() && title != rename.original {
            editor.run(
                "scene.update",
                serde_json::json!({ "scene": rename.scene, "title": title }),
            );
        }
        editor.ui.course_map.rename = None;
        editor.ui.course_map.rename_edit_rect = None;
    }
}

/// One slide thumbnail with its title, selection ring and interactions.
/// The full slide is looked up only when drawn: the caller keeps ids and
/// titles, so the course itself is never cloned per frame.
fn show_map_thumbnail(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    thumb_rect: Rect,
    selected: &str,
    unreachable: bool,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let Some(slide) = editor.session.course.play_slide(slide_id) else {
        return;
    };
    let slide_title = slide.title.clone();
    let is_selected = selected == slide_id;
    let title_rect = Rect::from_min_size(
        pos2(thumb_rect.min.x, thumb_rect.max.y + 4.0),
        vec2(MAP_THUMB_W, MAP_TITLE_H),
    );
    let hit_rect = Rect::from_min_max(thumb_rect.min, title_rect.max);

    // The existing thumbnail rendering and cache, as in the navigator.
    let items = compose_authoring(&editor.session.course, slide, None);
    let thumb_bg = coursecraft_render::parse_color(editor.session.course.slide_background(slide))
        .unwrap_or(Color32::WHITE);
    let painter = ui.painter_at(thumb_rect);
    painter.rect_filled(
        thumb_rect,
        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
        thumb_bg,
    );
    if let Some(t) = SlideTransform::fit(thumb_rect, editor.session.course.slide_size) {
        let source = crate::MemoryMediaSource {
            media: &editor.media,
        };
        paint(
            &painter,
            &t,
            &items,
            Highlight::default(),
            Some((&mut editor.media_cache, &source)),
            Some(&editor.session.course.theme),
            Some(editor.session.course.slide_background(slide)),
        );
    }

    crate::content_region::mark_content_region(ui.ctx(), ui.layer_id(), thumb_rect);
    let resp = ui.interact(
        hit_rect,
        ui.scope_id().with("map_thumb").with(slide_id),
        Sense::click(),
    );
    // Tab reachability: a clicked thumbnail can take keyboard focus, and
    // Enter on it opens the slide (handled with the global map keys too).
    let resp = resp.on_hover_text(slide_title.clone());
    if editor.ui.course_map.drag.is_none()
        && ui.input(|i| i.pointer.primary_pressed())
        && resp.contains_pointer()
    {
        editor.ui.last_active_area = crate::ActiveArea::Navigator;
        editor.selection.slide = slide_id.to_owned();
        editor.selection.layer = None;
        editor.selection.clear_objects();
        let start = ui
            .input(|i| i.pointer.interact_pos())
            .unwrap_or(thumb_rect.center());
        let (source_scene, source_index) = find_slide_position(&editor.session.course, slide_id);
        let source_scene = source_scene.unwrap_or_default();
        editor.ui.course_map.drag = Some(MapDrag {
            slide_id: slide_id.to_owned(),
            source_scene: source_scene.clone(),
            source_index,
            start_pointer: start,
            moved: false,
            target_scene: source_scene,
            target_index: source_index,
        });
    }
    if resp.clicked() {
        editor.ui.last_active_area = crate::ActiveArea::Navigator;
        editor.selection.slide = slide_id.to_owned();
        editor.selection.layer = None;
        editor.selection.clear_objects();
    }
    if resp.double_clicked() {
        editor.selection.slide = slide_id.to_owned();
        editor.selection.layer = None;
        editor.selection.clear_objects();
        editor.ui.story_view = false;
    }

    let border = if resp.hovered() && !is_selected {
        p.border_strong
    } else {
        p.border
    };
    ui.painter().rect_stroke(
        thumb_rect,
        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
        Stroke::new(1.0, border),
        StrokeKind::Outside,
    );
    if is_selected {
        ui.painter().rect_stroke(
            thumb_rect.expand(1.0),
            CornerRadius::same((coursecraft_render::RADIUS_MD + 2.0) as u8),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    }

    // Unreachable warning mark.
    if unreachable {
        let mark_rect = Rect::from_center_size(
            pos2(thumb_rect.min.x + 10.0, thumb_rect.min.y + 10.0),
            vec2(16.0, 16.0),
        );
        ui.painter()
            .circle_filled(mark_rect.center(), 8.0, p.warning);
        ui.painter().text(
            mark_rect.center(),
            Align2::CENTER_CENTER,
            "!",
            FontId::new(
                coursecraft_render::SIZE_SMALL,
                egui::FontFamily::Name("inter-bold".into()),
            ),
            p.app,
        );
        ui.interact(
            mark_rect,
            ui.scope_id().with("map_unreachable").with(slide_id),
            Sense::hover(),
        )
        .on_hover_text("Unreachable: nothing leads to this slide");
    }

    let title_color = if is_selected {
        p.text
    } else {
        p.text_secondary
    };
    ui.painter_at(title_rect).text(
        title_rect.left_top(),
        Align2::LEFT_TOP,
        &slide_title,
        FontId::new(
            coursecraft_render::SIZE_SMALL,
            egui::FontFamily::Name("inter".into()),
        ),
        title_color,
    );
}

/// Where a slide sits: (scene id, index in that scene).
fn find_slide_position(course: &Course, slide_id: &str) -> (Option<String>, usize) {
    for scene in &course.scenes {
        if let Some(index) = scene.slides.iter().position(|slide| slide.id == slide_id) {
            return (Some(scene.id.clone()), index);
        }
    }
    (None, 0)
}

/// Route points for an edge, avoiding thumbnails where that is easy: a
/// gentle curve between columns, a side loop within a column, a loop
/// above for a self jump.
fn edge_points(from: Rect, to: Rect) -> [egui::Pos2; 4] {
    if from == to {
        let cx = from.center().x;
        let top = from.min.y;
        return [
            pos2(cx - 20.0, top),
            pos2(cx - 44.0, top - 36.0),
            pos2(cx + 44.0, top - 36.0),
            pos2(cx + 20.0, top),
        ];
    }
    let from_cx = from.center().x;
    let to_cx = to.center().x;
    if (from_cx - to_cx).abs() < 1.0 {
        // Same column: loop out to the right.
        let p0 = pos2(from.max.x, from.center().y);
        let p1 = pos2(to.max.x, to.center().y);
        [p0, pos2(p0.x + 30.0, p0.y), pos2(p1.x + 30.0, p1.y), p1]
    } else if to_cx > from_cx {
        let p0 = pos2(from.max.x, from.center().y);
        let p1 = pos2(to.min.x, to.center().y);
        [p0, pos2(p0.x + 44.0, p0.y), pos2(p1.x - 44.0, p1.y), p1]
    } else {
        let p0 = pos2(from.min.x, from.center().y);
        let p1 = pos2(to.max.x, to.center().y);
        [p0, pos2(p0.x - 44.0, p0.y), pos2(p1.x + 44.0, p1.y), p1]
    }
}

/// Draw every visible arrow, dimming the ones that do not touch the
/// selection. Outgoing arrows past the per-slide cap are grouped.
#[allow(clippy::too_many_arguments)]
fn draw_arrows(
    ui: &mut Ui,
    map: &coursecraft_course::CourseMap,
    rects: &HashMap<String, (usize, Rect)>,
    clip: &Rect,
    selected: &str,
) {
    let mut outgoing: HashMap<String, usize> = HashMap::new();
    let mut grouped: HashMap<String, usize> = HashMap::new();
    for edge in &map.edges {
        *outgoing.entry(edge.from.clone()).or_insert(0) += 1;
    }
    for (edge_idx, edge) in map.edges.iter().enumerate() {
        let Some((_, from_rect)) = rects.get(&edge.from) else {
            continue;
        };
        let Some((_, to_rect)) = rects.get(&edge.to) else {
            continue;
        };
        let from_rect = *from_rect;
        let to_rect = *to_rect;
        // Grouping: only the first N outgoing arrows are drawn.
        let count = grouped.entry(edge.from.clone()).or_insert(0);
        *count += 1;
        if *count > MAP_MAX_ARROWS_PER_SLIDE {
            continue;
        }
        // Cull arrows far outside the viewport.
        if !clip.intersects(from_rect.expand(160.0)) && !clip.intersects(to_rect.expand(160.0)) {
            continue;
        }
        let touches_selection =
            !selected.is_empty() && (edge.from == selected || edge.to == selected);
        let dimmed = !selected.is_empty() && !touches_selection;
        let palette = coursecraft_render::palette(ui.ctx());
        let mut color = edge_color(palette, edge.kind);
        if dimmed {
            color = color.linear_multiply(0.35);
        }
        let conditional = edge.condition.is_some();
        let dashed =
            conditional || edge.kind == MapEdgeKind::Next || edge.kind == MapEdgeKind::Lightbox;
        let stroke = Stroke::new(if touches_selection { 2.0 } else { 1.5 }, color);
        let points = edge_points(from_rect, to_rect);
        let curve =
            CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, stroke);
        let path = curve.flatten(Some(2.0));
        if path.len() < 2 {
            continue;
        }
        if dashed {
            for shape in egui::Shape::dashed_line(&path, stroke, 6.0, 4.0) {
                ui.painter().add(shape);
            }
        } else {
            ui.painter().add(egui::Shape::line(path.clone(), stroke));
        }
        // Arrowhead at the end, along the final direction.
        let end = *path.last().unwrap_or(&points[3]);
        let before = path
            .get(path.len().saturating_sub(2))
            .copied()
            .unwrap_or(points[2]);
        let dir = (end - before).normalized();
        if dir.x.is_finite() && dir.y.is_finite() && dir.length() > 0.0 {
            let back = end - dir * 7.0;
            let side = egui::Vec2::new(-dir.y, dir.x) * 3.0;
            ui.painter().line_segment([end, back + side], stroke);
            ui.painter().line_segment([end, back - side], stroke);
        }
        // Lightboxes end in a small box.
        if edge.kind == MapEdgeKind::Lightbox {
            let box_rect = Rect::from_center_size(end - dir * 6.0, vec2(12.0, 12.0));
            ui.painter().rect_stroke(
                box_rect,
                CornerRadius::same(2),
                Stroke::new(1.5, color),
                StrokeKind::Inside,
            );
        }
        // Tooltip at the midpoint: the source, plus the condition.
        let mid = path.get(path.len() / 2).copied().unwrap_or(end);
        let tip_rect = Rect::from_center_size(mid, vec2(14.0, 14.0));
        let tip = match &edge.condition {
            Some(condition) => format!("{} — if {condition}", edge.via),
            None => edge.via.clone(),
        };
        ui.interact(
            tip_rect,
            ui.scope_id()
                .with("map_edge")
                .with(edge_idx)
                .with(&edge.from)
                .with(&edge.to)
                .with(&edge.via),
            Sense::hover(),
        )
        .on_hover_text(tip);
    }
    // Grouped arrows get a "+N more" chip on the source thumbnail.
    for (from, total) in &outgoing {
        if *total > MAP_MAX_ARROWS_PER_SLIDE {
            let Some((_, rect)) = rects.get(from) else {
                continue;
            };
            if !clip.intersects(rect.expand(2.0)) {
                continue;
            }
            let extra = total - MAP_MAX_ARROWS_PER_SLIDE;
            let chip =
                Rect::from_min_size(pos2(rect.max.x - 52.0, rect.max.y - 20.0), vec2(48.0, 16.0));
            let p = coursecraft_render::palette(ui.ctx());
            ui.painter().rect_filled(
                chip,
                CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                p.raised,
            );
            ui.painter().text(
                chip.center(),
                Align2::CENTER_CENTER,
                format!("+{extra} more"),
                FontId::new(
                    coursecraft_render::SIZE_SMALL,
                    egui::FontFamily::Name("inter".into()),
                ),
                p.text_secondary,
            );
        }
    }
}

/// Insertion target for a pointer position across the laid-out rows.
/// Scene headers count as hit areas too, so a scene with no slides can
/// still receive a drop at index 0.
fn map_drop_target(
    rows: &[(String, String, usize, Rect)],
    headers: &[(String, Rect)],
    pointer: egui::Pos2,
) -> Option<(String, usize)> {
    // The column under the pointer: rows share an x centre per scene.
    let column_scene = rows
        .iter()
        .find(|(_, _, _, rect)| {
            pointer.x >= rect.min.x - MAP_COL_GAP * 0.5
                && pointer.x <= rect.max.x + MAP_COL_GAP * 0.5
        })
        .map(|(scene, _, _, _)| scene.clone())
        .or_else(|| {
            headers
                .iter()
                .find(|(_, rect)| {
                    pointer.x >= rect.min.x - MAP_COL_GAP * 0.5
                        && pointer.x <= rect.max.x + MAP_COL_GAP * 0.5
                })
                .map(|(scene, _)| scene.clone())
        })?;
    let column_rows: Vec<&(String, String, usize, Rect)> = rows
        .iter()
        .filter(|(scene, _, _, _)| *scene == column_scene)
        .collect();
    // An empty scene has no rows: it takes the drop at index 0.
    if column_rows.is_empty() {
        return Some((column_scene, 0));
    }
    for (scene, _, index, rect) in &column_rows {
        if pointer.y < rect.center().y {
            return Some((scene.clone(), *index));
        }
    }
    column_rows
        .last()
        .map(|(_, _, index, _)| (column_scene.clone(), index.saturating_add(1)))
}

/// y of the insertion line for a target, or `None` without rows.
fn map_drop_line_y(
    rows: &[(String, String, usize, Rect)],
    headers: &[(String, Rect)],
    scene: &str,
    index: usize,
) -> Option<f32> {
    if let Some((_, _, _, rect)) = rows
        .iter()
        .find(|(row_scene, _, row_index, _)| row_scene == scene && *row_index == index)
    {
        return Some(rect.min.y - 6.0);
    }
    if let Some(y) = rows
        .iter()
        .rev()
        .find(|(row_scene, _, _, _)| row_scene == scene)
        .map(|(_, _, _, rect)| rect.max.y + MAP_TITLE_H + 6.0)
    {
        return Some(y);
    }
    // No rows (empty scene): draw the line under the header.
    headers
        .iter()
        .find(|(header_scene, _)| header_scene == scene)
        .map(|(_, rect)| rect.max.y + 6.0)
}

/// Update, draw and finish a Course map drag.
fn handle_map_drag(
    editor: &mut Editor,
    ui: &mut Ui,
    rows: &[(String, String, usize, Rect)],
    headers: &[(String, Rect)],
) {
    let Some(mut drag) = editor.ui.course_map.drag.take() else {
        return;
    };
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_down = ui.input(|i| i.pointer.primary_down());
    if pointer_down {
        if let Some(pos) = pointer_pos {
            if (pos - drag.start_pointer).length() >= 4.0 {
                drag.moved = true;
            }
            if drag.moved
                && let Some((scene, index)) = map_drop_target(rows, headers, pos)
            {
                drag.target_scene = scene;
                drag.target_index = index;
            }
        }
        if drag.moved
            && let Some(y) = map_drop_line_y(rows, headers, &drag.target_scene, drag.target_index)
        {
            let p = coursecraft_render::palette(ui.ctx());
            let col_x = rows
                .iter()
                .find(|(scene, _, _, _)| *scene == drag.target_scene)
                .map(|(_, _, _, rect)| rect.min.x)
                .or_else(|| {
                    headers
                        .iter()
                        .find(|(scene, _)| *scene == drag.target_scene)
                        .map(|(_, rect)| rect.min.x + (MAP_COL_W - MAP_THUMB_W) * 0.5)
                })
                .unwrap_or(0.0);
            ui.painter().line_segment(
                [pos2(col_x, y), pos2(col_x + MAP_THUMB_W, y)],
                Stroke::new(2.0, p.accent),
            );
        }
        editor.ui.course_map.drag = Some(drag);
    } else if drag.moved {
        let mut index = drag.target_index;
        if drag.source_scene == drag.target_scene && drag.source_index < index {
            index = index.saturating_sub(1);
        }
        if drag.source_scene != drag.target_scene || index != drag.source_index {
            editor.run(
                "slide.move",
                serde_json::json!({
                    "slide": drag.slide_id,
                    "scene": drag.target_scene,
                    "index": index,
                }),
            );
        }
    }
}

/// Course map keyboard: arrows move the selection in play order, Enter
/// opens the slide in the editor. Called from the central panel while
/// the Course map is shown and no text field owns the keyboard.
pub fn handle_map_keys(editor: &mut Editor, ui: &mut Ui) {
    if editor.ui.master_view {
        editor.ui.story_view = false;
        return;
    }
    if editor.ui.course_map.rename.is_some() {
        return;
    }
    // A focused text field (for example in the inspector) owns arrows and
    // Enter: do not move the map selection or close the Course map while
    // the user is typing there. The inspector is drawn before the map, so
    // a singleline field's Enter commits and blurs *before* this check:
    // the blur frame is still the field's keypress, not a map key.
    if ui.ctx().egui_wants_keyboard_input() {
        editor.ui.course_map.keyboard_was_focused = true;
        return;
    }
    if editor.ui.course_map.keyboard_was_focused {
        editor.ui.course_map.keyboard_was_focused = false;
        return;
    }
    let next = ui.input_mut(|i| {
        i.consume_key(Modifiers::NONE, Key::ArrowRight)
            || i.consume_key(Modifiers::NONE, Key::ArrowDown)
    });
    let prev = ui.input_mut(|i| {
        i.consume_key(Modifiers::NONE, Key::ArrowLeft)
            || i.consume_key(Modifiers::NONE, Key::ArrowUp)
    });
    let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    if enter {
        editor.ui.story_view = false;
        return;
    }
    if !next && !prev {
        return;
    }
    let ids: Vec<String> = editor
        .session
        .course
        .slides()
        .map(|slide| slide.id.clone())
        .collect();
    let Some(current) = ids.iter().position(|id| *id == editor.selection.slide) else {
        return;
    };
    let target = if next {
        current.saturating_add(1)
    } else {
        current.saturating_sub(1)
    };
    if let Some(id) = ids.get(target) {
        editor.selection.slide = id.clone();
        editor.selection.layer = None;
        editor.selection.clear_objects();
        editor.ui.last_active_area = crate::ActiveArea::Navigator;
    }
}
