//! Document tabs under the ribbon: the Course Map, pinned slide tabs, and at
//! most one italic preview tab.
//!
//! The first tab is always the Course Map. Selecting a slide shows it in a
//! single italic preview tab, which becomes a pinned tab on double-click or
//! when the slide is edited. Clicking a tab switches the centre between the map
//! and that slide.

use std::collections::HashMap;

use coursecraft_render::Icon;
use egui::{RichText, Ui, vec2};

use crate::{Editor, Selection};

/// Height of the tab row.
pub const DOC_TABS_H: f32 = 28.0;

/// Minimum readable width for a slide tab.
const TAB_MIN_W: f32 = 48.0;

/// Maximum width for a slide tab before truncating with a tooltip.
const TAB_MAX_W: f32 = 180.0;

/// Step size in pixels when clicking the `‹` / `›` tab scroll buttons.
const TAB_SCROLL_STEP: f32 = 160.0;

/// The opened slides, in the order they were opened, plus at most one preview tab.
#[derive(Debug, Clone, Default)]
pub struct DocTabs {
    pub open: Vec<String>,
    pub preview: Option<String>,
    pub cached_revision: Option<u64>,
    #[cfg(test)]
    pub(crate) cache_rebuilds: u64,
    labels: HashMap<String, String>,
    last_active_tab: Option<String>,
    overflows: bool,
    pending_scroll_delta: f32,
}

impl PartialEq for DocTabs {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open && self.preview == other.preview
    }
}

/// "2.3", the slide's scene and place in it; `None` for a slide in no scene.
#[must_use]
pub fn slide_number(course: &coursecraft_course::Course, slide_id: &str) -> Option<String> {
    course.scenes.iter().enumerate().find_map(|(scene, sc)| {
        sc.slides
            .iter()
            .position(|s| s.id == slide_id)
            .map(|i| format!("{}.{}", scene + 1, i + 1))
    })
}

/// A slide tab's text: "1.2 Title".
#[must_use]
pub fn slide_tab_label(course: &coursecraft_course::Course, slide_id: &str) -> String {
    let title = course
        .slide(slide_id)
        .map_or(slide_id, |s| s.title.as_str());
    match slide_number(course, slide_id) {
        Some(number) => format!("{number} {title}"),
        None => title.to_owned(),
    }
}

/// Rebuild the cached `slide_id -> label` map when the course revision changes,
/// and prune deleted slides from `open` and `preview` in O(1) per tab.
fn ensure_cache(editor: &mut Editor) {
    let rev = editor.session.revision;
    if editor.ui.doc_tabs.cached_revision == Some(rev) {
        return;
    }
    let course = &editor.session.course;
    let mut labels = HashMap::new();
    for (scene_idx, scene) in course.scenes.iter().enumerate() {
        for (slide_idx, slide) in scene.slides.iter().enumerate() {
            labels.insert(
                slide.id.clone(),
                format!("{}.{} {}", scene_idx + 1, slide_idx + 1, slide.title),
            );
        }
    }
    for bank in &course.banks {
        for slide in &bank.slides {
            labels.insert(slide.id.clone(), slide.title.clone());
        }
    }
    editor.ui.doc_tabs.labels = labels;
    editor.ui.doc_tabs.cached_revision = Some(rev);
    #[cfg(test)]
    {
        editor.ui.doc_tabs.cache_rebuilds += 1;
    }

    let labels = &editor.ui.doc_tabs.labels;
    editor.ui.doc_tabs.open.retain(|id| labels.contains_key(id));
    if let Some(prev) = &editor.ui.doc_tabs.preview
        && !labels.contains_key(prev)
    {
        editor.ui.doc_tabs.preview = None;
    }
}

/// Keep the tabs in step with the course and the editor: a deleted slide loses
/// its tab, and the current slide is shown in the preview tab unless it is
/// already pinned in `open`.
pub fn sync(editor: &mut Editor) {
    ensure_cache(editor);
    let current = &editor.selection.slide;
    if !editor.ui.story_view
        && !editor.ui.master_view
        && editor.ui.doc_tabs.labels.contains_key(current)
        && !editor.ui.doc_tabs.open.iter().any(|id| id == current)
    {
        editor.ui.doc_tabs.preview = Some(current.clone());
    }
}

/// Pin `slide_id` as a normal tab, clearing it from `preview` if present.
pub fn pin(editor: &mut Editor, slide_id: &str) {
    if editor.session.course.play_slide(slide_id).is_none() {
        return;
    }
    if editor.ui.doc_tabs.preview.as_deref() == Some(slide_id) {
        editor.ui.doc_tabs.preview = None;
    }
    if !editor.ui.doc_tabs.open.iter().any(|id| id == slide_id) {
        editor.ui.doc_tabs.open.push(slide_id.to_owned());
    }
}

/// Show the Course Map in the centre.
pub fn open_map(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.toggle_master_view();
    }
    editor.ui.story_view = true;
}

/// Edit `slide_id` in the centre (shown in the preview tab unless already pinned).
pub fn open_slide(editor: &mut Editor, slide_id: &str) {
    if editor.ui.master_view {
        editor.toggle_master_view();
    }
    editor.ui.story_view = false;
    if editor.selection.slide != slide_id || editor.selection.is_multi() {
        editor.selection = Selection::new(slide_id);
    }
}

/// Close a slide's tab (pinned or preview); closing the shown slide shows its
/// neighbour, or the Course Map when none is left.
pub fn close(editor: &mut Editor, slide_id: &str) {
    let shown =
        !editor.ui.story_view && !editor.ui.master_view && editor.selection.slide == slide_id;
    if let Some(index) = editor.ui.doc_tabs.open.iter().position(|id| id == slide_id) {
        editor.ui.doc_tabs.open.remove(index);
        if !shown {
            return;
        }
        let next = editor
            .ui
            .doc_tabs
            .open
            .get(index)
            .or_else(|| {
                index
                    .checked_sub(1)
                    .and_then(|i| editor.ui.doc_tabs.open.get(i))
            })
            .cloned()
            .or_else(|| editor.ui.doc_tabs.preview.clone());
        match next {
            Some(id) => open_slide(editor, &id),
            None => open_map(editor),
        }
        return;
    }

    if editor.ui.doc_tabs.preview.as_deref() == Some(slide_id) {
        editor.ui.doc_tabs.preview = None;
        if !shown {
            return;
        }
        let next = editor.ui.doc_tabs.open.last().cloned();
        match next {
            Some(id) => open_slide(editor, &id),
            None => open_map(editor),
        }
    }
}

/// Draw one slide tab (pinned or preview) and its close `×` button.
#[allow(clippy::too_many_arguments)]
fn draw_slide_tab(
    ui: &mut Ui,
    id: &str,
    label: &str,
    active: bool,
    is_preview: bool,
    scroll_active_into_view: bool,
    open_now: &mut Option<String>,
    pin_now: &mut Option<String>,
    close_now: &mut Option<String>,
) {
    ui.separator();
    let mut text = RichText::new(label);
    if is_preview {
        text = text.italics();
    }
    if active {
        text = text.strong();
    }
    let text_w = egui::WidgetText::from(text.clone())
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        )
        .size()
        .x;
    let pad_x = ui.spacing().button_padding.x * 2.0 + 4.0;
    let desired_w = (text_w + pad_x).clamp(TAB_MIN_W, TAB_MAX_W);
    let resp = ui
        .add_sized(
            vec2(desired_w, ui.spacing().interact_size.y),
            egui::Button::selectable(active, text).truncate(),
        )
        .on_hover_text(label);
    if active && scroll_active_into_view {
        resp.scroll_to_me(Some(egui::Align::Center));
    }
    if is_preview && resp.double_clicked() {
        *pin_now = Some(id.to_owned());
        *open_now = Some(id.to_owned());
    } else if resp.clicked() {
        *open_now = Some(id.to_owned());
    }
    let tip = tl!("Close this slide's tab");
    if crate::widgets::icon_button(ui, Icon::X, tip).clicked() {
        *close_now = Some(id.to_owned());
    }
}

/// The tab row, drawn under the ribbon.
pub fn show(editor: &mut Editor, ui: &mut Ui) {
    sync(editor);
    let p = coursecraft_render::palette(ui.ctx());
    let mut open_map_now = false;
    let mut open_now: Option<String> = None;
    let mut pin_now: Option<String> = None;
    let mut close_now: Option<String> = None;

    let active_tab =
        (!editor.ui.story_view && !editor.ui.master_view).then(|| editor.selection.slide.clone());
    let active_changed = editor.ui.doc_tabs.last_active_tab != active_tab;
    if active_changed {
        editor.ui.doc_tabs.last_active_tab = active_tab;
    }

    let overflows = editor.ui.doc_tabs.overflows;
    let pending_delta = std::mem::take(&mut editor.ui.doc_tabs.pending_scroll_delta);
    let mut click_scroll_delta = 0.0_f32;
    let row_rect = ui.max_rect();

    // Translate vertical mouse wheel over the tab strip into horizontal scroll.
    let wheel_delta = if ui.rect_contains_pointer(row_rect) {
        ui.input(|i| {
            let d = i.smooth_scroll_delta;
            if d.x == 0.0 && d.y != 0.0 { d.y } else { 0.0 }
        })
    } else {
        0.0
    };

    let full_avail_w = ui.available_width();
    let arrow_w = if overflows { 22.0 } else { 0.0 };
    let scroll_w = (full_avail_w - arrow_w * 2.0).max(40.0);

    let mut scroll_output = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if overflows
            && ui
                .add_sized(vec2(18.0, 20.0), egui::Button::new("‹"))
                .on_hover_text(tl!("Scroll tabs left"))
                .clicked()
        {
            click_scroll_delta += TAB_SCROLL_STEP;
        }

        let out = ui
            .allocate_ui(vec2(scroll_w, DOC_TABS_H), |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("doc_tabs_scroll")
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        let total_delta = pending_delta + wheel_delta;
                        if total_delta != 0.0 {
                            ui.scroll_with_delta(vec2(total_delta, 0.0));
                        }
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let map_active = editor.ui.story_view && !editor.ui.master_view;
                            ui.label(coursecraft_render::icon(Icon::SquaresFour, 14.0));
                            let map_text = RichText::new(tl!("Course Map"));
                            if ui
                                .add(egui::Button::selectable(map_active, map_text))
                                .on_hover_text(tl!("Every scene and slide, and how they connect"))
                                .clicked()
                            {
                                open_map_now = true;
                            }
                            let labels = &editor.ui.doc_tabs.labels;
                            for id in &editor.ui.doc_tabs.open {
                                let active = !editor.ui.story_view
                                    && !editor.ui.master_view
                                    && editor.selection.slide == *id;
                                let label = labels.get(id).map_or(id.as_str(), String::as_str);
                                draw_slide_tab(
                                    ui,
                                    id,
                                    label,
                                    active,
                                    false,
                                    active_changed,
                                    &mut open_now,
                                    &mut pin_now,
                                    &mut close_now,
                                );
                            }
                            if let Some(id) = &editor.ui.doc_tabs.preview {
                                let active = !editor.ui.story_view
                                    && !editor.ui.master_view
                                    && editor.selection.slide == *id;
                                let label = labels.get(id).map_or(id.as_str(), String::as_str);
                                draw_slide_tab(
                                    ui,
                                    id,
                                    label,
                                    active,
                                    true,
                                    active_changed,
                                    &mut open_now,
                                    &mut pin_now,
                                    &mut close_now,
                                );
                            }
                        });
                    })
            })
            .inner;
        scroll_output = Some(out.content_size.x);

        if overflows
            && ui
                .add_sized(vec2(18.0, 20.0), egui::Button::new("›"))
                .on_hover_text(tl!("Scroll tabs right"))
                .clicked()
        {
            click_scroll_delta -= TAB_SCROLL_STEP;
        }
    });

    if let Some(content_w) = scroll_output {
        let now_overflows = content_w > full_avail_w + 1.0;
        if now_overflows != overflows {
            editor.ui.doc_tabs.overflows = now_overflows;
            ui.ctx().request_repaint();
        }
    }
    if click_scroll_delta != 0.0 {
        editor.ui.doc_tabs.pending_scroll_delta += click_scroll_delta;
        ui.ctx().request_repaint();
    }

    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.max_rect().bottom(),
        egui::Stroke::new(1.0, p.border),
    );
    if let Some(id) = close_now {
        close(editor, &id);
    } else if let Some(id) = pin_now {
        open_slide(editor, &id);
        pin(editor, &id);
    } else if let Some(id) = open_now {
        open_slide(editor, &id);
    } else if open_map_now {
        open_map(editor);
    }
}
