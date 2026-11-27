//! Slides left panel: scenes, slides list, thumbnails, and reordering.

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::{Course, Slide};
use coursecraft_render::{DrawItem, Highlight, SlideTransform, compose_authoring, paint};
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, Rect, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, Ui, pos2, vec2,
};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::sync::Arc;

/// Frames the slide list may stand still while scrolling a slide into view, before giving up.
const MAX_SCROLL_STALLED: u8 = 3;

/// Maximum number of composed thumbnails retained in [`ThumbnailCache`].
pub(crate) const MAX_CACHED_THUMBNAILS: usize = 128;

/// Deterministic 64-bit FNV-1a hasher used for thumbnail fingerprints.
struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    fn write_sep(&mut self, tag: u8) {
        self.write_bytes(&[tag]);
    }

    fn finish(self) -> u64 {
        self.0
    }
}

impl std::io::Write for Fnv1a64 {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

macro_rules! fnv_hash_json {
    ($hasher:expr, $tag:expr, $value:expr) => {{
        $hasher.write_sep($tag);
        if serde_json::to_writer(&mut $hasher, $value).is_err() {
            let _ = std::io::Write::write_fmt(&mut $hasher, format_args!("{:?}", $value));
        }
    }};
}

/// Deterministic 64-bit fingerprint of course-level visual state that affects thumbnails.
fn course_visual_fingerprint(course: &Course) -> u64 {
    let mut h = Fnv1a64::new();
    fnv_hash_json!(h, 1, &course.slide_size);
    fnv_hash_json!(h, 2, &course.theme);
    fnv_hash_json!(h, 3, &course.text_styles);
    fnv_hash_json!(h, 4, &course.variables);
    fnv_hash_json!(h, 5, &course.strings);
    h.finish()
}

/// Deterministic 64-bit fingerprint of one slide, its master, and `course_visual_fp`.
fn thumbnail_fingerprint(course: &Course, slide: &Slide, course_visual_fp: u64) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_u64(course_visual_fp);
    fnv_hash_json!(h, 10, &slide.background);
    fnv_hash_json!(h, 11, &slide.master);
    fnv_hash_json!(h, 12, &slide.objects);
    fnv_hash_json!(h, 13, &slide.question);
    fnv_hash_json!(h, 14, &slide.draw);
    if let Some(draw) = slide.draw.as_ref() {
        h.write_sep(15);
        if let Some(bank) = course.bank(&draw.bank) {
            h.write_bytes(bank.title.as_bytes());
            h.write_sep(0);
            h.write_u64(bank.slides.len() as u64);
        }
    }
    if let Some(master) = slide.master.as_deref().and_then(|id| course.master(id)) {
        fnv_hash_json!(h, 16, &master.background);
        fnv_hash_json!(h, 17, &master.objects);
    }
    h.finish()
}

struct CachedThumbnail {
    verified_revision: u64,
    fingerprint: u64,
    bg: Color32,
    items: Arc<[DrawItem]>,
    last_used: u64,
}

/// Bounded LRU cache of composed authoring thumbnails, keyed by slide id and
/// validated cheaply by session revision and deterministic content fingerprint.
pub(crate) struct ThumbnailCache {
    visual_revision: Option<u64>,
    course_visual_fp: u64,
    /// True when `course_visual_fp` at `visual_revision` matched the previous revision.
    course_visual_unchanged: bool,
    tick: u64,
    max_entries: usize,
    entries: HashMap<String, CachedThumbnail>,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self {
            visual_revision: None,
            course_visual_fp: 0,
            course_visual_unchanged: false,
            tick: 0,
            max_entries: MAX_CACHED_THUMBNAILS,
            entries: HashMap::new(),
        }
    }
}

impl ThumbnailCache {
    /// Drop all cached thumbnails (called when replacing the open course).
    pub(crate) fn clear(&mut self) {
        self.visual_revision = None;
        self.course_visual_fp = 0;
        self.course_visual_unchanged = false;
        self.tick = 0;
        self.entries.clear();
    }

    /// Configure the maximum number of cached thumbnails, evicting oldest entries if needed.
    pub(crate) fn set_max_entries(&mut self, max_entries: usize) {
        self.max_entries = max_entries.max(1);
        while self.entries.len() > self.max_entries {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.entries.remove(&oldest_key);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return the cached background colour and composed authoring items for `slide`,
    /// composing only when the slide (or its master / course visual settings) changed.
    pub(crate) fn get_or_compose(
        &mut self,
        revision: u64,
        last_changed_slide: Option<&str>,
        course: &Course,
        slide: &Slide,
    ) -> (Color32, Arc<[DrawItem]>) {
        // Fast path: already verified at this course revision (0 hashing, 0 composes).
        if let Some(entry) = self.entries.get_mut(&slide.id)
            && entry.verified_revision == revision
        {
            self.tick = self.tick.saturating_add(1);
            entry.last_used = self.tick;
            return (entry.bg, Arc::clone(&entry.items));
        }

        if self.visual_revision != Some(revision) {
            let new_fp = course_visual_fingerprint(course);
            self.course_visual_unchanged = self.visual_revision == Some(revision.wrapping_sub(1))
                && self.course_visual_fp == new_fp;
            self.course_visual_fp = new_fp;
            self.visual_revision = Some(revision);
        }

        // Single-slide edit fast path: if course-level visual state did not change
        // and only a different slide (not this slide or its master) was edited,
        // bump `verified_revision` in O(1) without hashing this slide.
        if self.course_visual_unchanged
            && let Some(changed_id) = last_changed_slide
            && slide.id != changed_id
            && slide.master.as_deref() != Some(changed_id)
            && let Some(entry) = self.entries.get_mut(&slide.id)
            && entry.verified_revision == revision.wrapping_sub(1)
        {
            self.tick = self.tick.saturating_add(1);
            entry.verified_revision = revision;
            entry.last_used = self.tick;
            return (entry.bg, Arc::clone(&entry.items));
        }

        let fp = thumbnail_fingerprint(course, slide, self.course_visual_fp);
        if let Some(entry) = self.entries.get_mut(&slide.id)
            && entry.fingerprint == fp
        {
            self.tick = self.tick.saturating_add(1);
            entry.verified_revision = revision;
            entry.last_used = self.tick;
            return (entry.bg, Arc::clone(&entry.items));
        }

        #[cfg(test)]
        crate::guard_counters::note_thumbnail_compose();
        let items: Arc<[DrawItem]> = compose_authoring(course, slide, None).into();
        let bg = coursecraft_render::parse_color(course.slide_background(slide))
            .unwrap_or(Color32::WHITE);
        self.tick = self.tick.saturating_add(1);
        let used = self.tick;
        self.entries.insert(
            slide.id.clone(),
            CachedThumbnail {
                verified_revision: revision,
                fingerprint: fp,
                bg,
                items: Arc::clone(&items),
                last_used: used,
            },
        );
        while self.entries.len() > self.max_entries {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.entries.remove(&oldest_key);
        }
        (bg, items)
    }
}

struct SlideSummary {
    id: String,
    title: String,
    is_question: bool,
}

struct SceneSummary {
    id: String,
    title: String,
    slides: Vec<SlideSummary>,
}

struct BankSlideSummary {
    id: String,
    title: String,
}

struct BankSummary {
    id: String,
    title: String,
    slides: Vec<BankSlideSummary>,
}

#[derive(Default)]
struct NavigatorData {
    scenes: Vec<SceneSummary>,
    bank_menu: Vec<(String, String)>,
    banks: Vec<BankSummary>,
    total_course_slides: usize,
}

/// Revision-cached summary of scenes, slides, and question banks for the navigator,
/// avoiding per-frame 300-slide string clones on idle and scroll frames.
#[derive(Default)]
pub(crate) struct NavigatorIndex {
    revision: crate::SyncRevision,
    data: Arc<NavigatorData>,
}

impl NavigatorIndex {
    pub(crate) fn clear(&mut self) {
        self.revision = crate::SyncRevision::default();
        self.data = Arc::default();
    }

    fn snapshot(&mut self, revision: u64, course: &Course) -> Arc<NavigatorData> {
        if !self.revision.is(revision) {
            let scenes: Vec<SceneSummary> = course
                .scenes
                .iter()
                .map(|scene| SceneSummary {
                    id: scene.id.clone(),
                    title: scene.title.clone(),
                    slides: scene
                        .slides
                        .iter()
                        .map(|slide| SlideSummary {
                            id: slide.id.clone(),
                            title: slide.title.clone(),
                            is_question: slide.question.is_some(),
                        })
                        .collect(),
                })
                .collect();
            let bank_menu: Vec<(String, String)> = course
                .banks
                .iter()
                .map(|bank| (bank.id.clone(), bank.title.clone()))
                .collect();
            let banks: Vec<BankSummary> = course
                .banks
                .iter()
                .map(|bank| BankSummary {
                    id: bank.id.clone(),
                    title: bank.title.clone(),
                    slides: bank
                        .slides
                        .iter()
                        .map(|slide| BankSlideSummary {
                            id: slide.id.clone(),
                            title: slide.title.clone(),
                        })
                        .collect(),
                })
                .collect();
            let total_course_slides = course.slides().count();
            self.data = Arc::new(NavigatorData {
                scenes,
                bank_menu,
                banks,
                total_course_slides,
            });
            self.revision.set(revision);
        }
        Arc::clone(&self.data)
    }
}

/// In-place rename of a scene header in the navigator.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRename {
    pub scene: String,
    pub buffer: String,
    pub original: String,
    pub just_entered: bool,
}

/// Active drag of a slide thumbnail in the navigator.
#[derive(Debug, Clone, PartialEq)]
pub struct SlideDrag {
    /// Slide being dragged.
    pub slide_id: String,
    /// Scene it came from and its index there.
    pub source_scene: String,
    pub source_index: usize,
    /// Pointer y at the press, used for the plain-click threshold.
    pub start_pointer_y: f32,
    /// True once the pointer moved past the click threshold.
    pub moved: bool,
    /// Insertion scene and index in that scene's current slide list.
    pub target_scene: String,
    pub target_index: usize,
}

pub const SLIDES_PANEL_WIDTH: f32 = 220.0;
pub const SLIDES_PANEL_MIN_WIDTH: f32 = 180.0;
pub const SLIDES_PANEL_MAX_WIDTH: f32 = 320.0;
pub const GUTTER_WIDTH: f32 = 22.0;
pub const ROW_GAP: f32 = 10.0;
pub const TITLE_GAP: f32 = 4.0;
pub const TITLE_HEIGHT: f32 = 16.0;
pub const SCENE_FIRST_TOP_GAP: f32 = 12.0;
pub const SCENE_TOP_GAP: f32 = 16.0;
pub const SCENE_BOTTOM_GAP: f32 = 8.0;
pub const SCENE_HEADER_HEIGHT: f32 = 18.0;
pub const FOOTER_HEIGHT: f32 = 36.0;

// Backwards-compatible constants for tests and harness
pub const PANEL_PADDING_X: f32 = 22.0;
pub const PANEL_PADDING_Y: f32 = 12.0;
pub const SLIDE_INFO_HEIGHT: f32 = 20.0;
pub const SLIDE_ROW_GAP: f32 = 10.0;
pub const THUMBNAIL_WIDTH: f32 = 186.0;
pub const THUMBNAIL_HEIGHT: f32 = 105.0;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlideMenuAction {
    Copy(String),
    Cut(String),
    Paste,
    Rename(String),
    DeleteSelected,
    PreviewSelected,
    DuplicateSelected,
    MoveSelected { up: bool },
}

enum SlideClick {
    Single(String),
    Toggle(String),
    Range(String),
}

/// A move of the slide selection by keyboard in the slide list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlideNav {
    Prev,
    Next,
    First,
    Last,
}

/// A slide menu acts on the whole selection when its slide is part of it, and on
/// that slide alone otherwise.
fn select_slide_for_menu(editor: &mut Editor, id: String) {
    if !editor.selection.is_slide_selected(&id) {
        editor.selection.slide = id;
        editor.selection.also_slides.clear();
    }
    editor.selection.layer = None;
    editor.selection.clear_objects();
}

/// The moves that shift every selected slide one place up or down in its scene,
/// as `(slide, scene, new index)` in the order to run them. A selected slide at the
/// edge of its scene, or behind one that cannot move, stays; the rest move.
#[must_use]
pub(crate) fn plan_slide_moves(
    course: &Course,
    selected: &[String],
    up: bool,
) -> Vec<(String, String, usize)> {
    let mut plan = Vec::new();
    for scene in &course.scenes {
        let mut order: Vec<&str> = scene.slides.iter().map(|s| s.id.as_str()).collect();
        let mut walk: Vec<&str> = order
            .iter()
            .copied()
            .filter(|id| selected.iter().any(|s| s == id))
            .collect();
        if !up {
            walk.reverse();
        }
        let mut stuck: Vec<&str> = Vec::new();
        for id in walk {
            let Some(pos) = order.iter().position(|other| *other == id) else {
                continue;
            };
            let neighbour = if up {
                pos.checked_sub(1)
            } else {
                Some(pos + 1).filter(|next| *next < order.len())
            };
            let Some(neighbour) = neighbour else {
                stuck.push(id);
                continue;
            };
            if order
                .get(neighbour)
                .is_some_and(|other| stuck.contains(other))
            {
                stuck.push(id);
                continue;
            }
            order.remove(pos);
            order.insert(neighbour, id);
            plan.push((id.to_owned(), scene.id.clone(), neighbour));
        }
    }
    plan
}

impl Editor {
    /// Slides in the order the slide list shows them: scenes, then the slides of
    /// the question banks that are expanded.
    #[must_use]
    pub(crate) fn navigator_slide_order(&self) -> Vec<String> {
        let mut order: Vec<String> = self
            .session
            .course
            .slides()
            .map(|slide| slide.id.clone())
            .collect();
        for bank in &self.session.course.banks {
            if self.ui.bank_expanded.contains(&bank.id) {
                order.extend(bank.slides.iter().map(|slide| slide.id.clone()));
            }
        }
        order
    }

    /// Every selected slide, in the order of the slide list.
    #[must_use]
    pub(crate) fn selected_slides_in_order(&self) -> Vec<String> {
        let ids: Vec<String> = self
            .navigator_slide_order()
            .into_iter()
            .filter(|id| self.selection.is_slide_selected(id))
            .collect();
        if ids.is_empty() && !self.selection.slide.is_empty() {
            vec![self.selection.slide.clone()]
        } else {
            ids
        }
    }

    /// Move the slide selection by key: previous, next, first or last slide in list
    /// order, across scenes. `extend` grows or shrinks the range from the slide that
    /// was selected first (Shift+arrow). Like a click, it clears the object and layer.
    pub(crate) fn navigate_slides(&mut self, nav: SlideNav, extend: bool) {
        let order = self.navigator_slide_order();
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let cursor = self
            .ui
            .slides_cursor
            .clone()
            .filter(|id| self.selection.is_slide_selected(id) && order.contains(id))
            .unwrap_or_else(|| self.selection.slide.clone());
        let at = order.iter().position(|id| id == &cursor);
        let index = match (nav, at) {
            (SlideNav::First, _) => 0,
            (SlideNav::Last, _) => last,
            (SlideNav::Next, Some(i)) => (i + 1).min(last),
            (SlideNav::Prev, Some(i)) => i.saturating_sub(1),
            (SlideNav::Next, None) => 0,
            (SlideNav::Prev, None) => last,
        };
        let Some(target) = order.get(index).cloned() else {
            return;
        };
        let in_scenes = |id: &str| self.session.course.slide_index(id).is_some();
        if extend && in_scenes(&self.selection.slide) && in_scenes(&target) {
            self.selection
                .select_slide_range(&self.session.course, &target);
            self.ui.slides_cursor = Some(target);
        } else {
            self.selection.slide = target;
            self.selection.also_slides.clear();
            self.ui.slides_cursor = None;
        }
        self.selection.layer = None;
        self.selection.clear_objects();
        self.ui.slides_scrolled_to = None;
        self.ui.slides_scroll_stalled = 0;
        self.ui.slides_scroll_gap = f32::NAN;
    }

    /// Duplicate every selected slide, each right after its original, as one undo step.
    pub(crate) fn duplicate_selected_slides(&mut self) {
        let ids = self.selected_slides_in_order();
        let steps: Vec<(String, Json)> = ids
            .iter()
            .filter(|id| self.session.course.slide(id).is_some())
            .map(|id| {
                (
                    "slide.duplicate".to_owned(),
                    serde_json::json!({ "slide": id }),
                )
            })
            .collect();
        match steps.as_slice() {
            [] => {}
            [(_, params)] => {
                self.run("slide.duplicate", params.clone());
            }
            _ => {
                self.run_batch(&steps);
            }
        }
    }

    /// Move every selected slide one place up or down in its scene as one undo step.
    pub(crate) fn move_selected_slides(&mut self, up: bool) {
        let selected = self.selected_slides_in_order();
        let steps: Vec<(String, Json)> = plan_slide_moves(&self.session.course, &selected, up)
            .into_iter()
            .map(|(slide, scene, index)| {
                (
                    "slide.move".to_owned(),
                    serde_json::json!({ "slide": slide, "scene": scene, "index": index }),
                )
            })
            .collect();
        match steps.as_slice() {
            [] => {}
            [(_, params)] => {
                self.run("slide.move", params.clone());
            }
            _ => {
                self.run_batch(&steps);
            }
        }
    }
}

/// Returns the thumbnail rect for a given slide id, given the left panel rect.
///
/// Both the slides panel rendering and tests use this helper so layout is deterministic.
#[must_use]
pub fn slide_thumbnail_rect(course: &Course, panel_rect: Rect, slide_id: &str) -> Option<Rect> {
    let mut current_y = panel_rect.min.y;
    let thumb_w = (panel_rect.width() - GUTTER_WIDTH - coursecraft_render::PANEL_MARGIN).max(64.0);
    let thumb_h = (thumb_w * 9.0 / 16.0).round();
    let row_height = thumb_h + TITLE_GAP + TITLE_HEIGHT + ROW_GAP;
    let thumb_x = panel_rect.min.x + GUTTER_WIDTH;

    for (scene_idx, scene) in course.scenes.iter().enumerate() {
        if scene_idx == 0 {
            current_y += SCENE_FIRST_TOP_GAP + SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;
        } else {
            current_y += SCENE_TOP_GAP + SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;
        }
        for slide in &scene.slides {
            let rect = Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, thumb_h));
            if slide.id == slide_id {
                return Some(rect);
            }
            current_y += row_height;
        }
    }
    None
}

/// Render the slides panel.
///
/// Thumbnails scroll inside a vertical `ScrollArea` while the "+ New slide" footer is pinned
/// to the panel bottom with a hairline above.
pub fn show_slides_panel(editor: &mut Editor, ui: &mut Ui, panel_rect: Rect) {
    if editor.ui.master_view {
        show_masters_panel(editor, ui, panel_rect);
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());

    // 1. Pinned footer at the bottom (hairline above)
    egui::Panel::bottom(ui.auto_id_with("slides_panel_footer"))
        .exact_size(FOOTER_HEIGHT)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(p.panel))
        .show_separator_line(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let caret_w = 24.0;
                ui.spacing_mut().item_spacing.x = 0.0;
                let add_btn = egui::Button::new(
                    coursecraft_render::icon_text(coursecraft_render::Icon::Plus, "New slide")
                        .color(p.accent_text)
                        .size(coursecraft_render::SIZE_BODY),
                )
                .frame(false)
                .min_size(vec2(
                    (ui.available_width() - caret_w).max(1.0),
                    coursecraft_render::CONTROL_COMPACT,
                ));
                if ui.add(add_btn).clicked() {
                    crate::toolbar::add_new_slide(editor);
                }
                let caret = ui
                    .add(
                        egui::Button::new(
                            coursecraft_render::icon(
                                coursecraft_render::Icon::CaretDown,
                                coursecraft_render::ICON_SM,
                            )
                            .color(p.text_secondary),
                        )
                        .frame(false)
                        .min_size(vec2(caret_w, coursecraft_render::CONTROL_COMPACT)),
                    )
                    .on_hover_text("More slide options");
                caret.widget_info(|| {
                    let mut info = egui::WidgetInfo::new(egui::Role::Button);
                    info.label = Some("More slide options".to_owned());
                    info
                });
                egui::Popup::menu(&caret)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        show_new_slide_popup(ui, editor);
                    });
            });
        });

    // 2. Scrolling thumbnails area
    let out = ScrollArea::vertical()
        .id_salt("slides_panel_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let origin = ui.available_rect_before_wrap();
            show_slides_content(editor, ui, origin);
        });
    editor.ui.slides_scroll_y = out.state.clamped_offset().y;
}

fn show_masters_panel(editor: &mut Editor, ui: &mut Ui, _panel_rect: Rect) {
    let p = coursecraft_render::palette(ui.ctx());

    // Footer: add a master.
    egui::Panel::bottom(ui.auto_id_with("masters_panel_footer"))
        .exact_size(FOOTER_HEIGHT)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(p.panel))
        .show_separator_line(true)
        .show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                let add_btn = egui::Button::new(
                    coursecraft_render::icon_text(coursecraft_render::Icon::Plus, "New master")
                        .color(p.accent_text)
                        .size(coursecraft_render::SIZE_BODY),
                )
                .frame(false)
                .min_size(vec2(
                    ui.available_width(),
                    coursecraft_render::CONTROL_COMPACT,
                ));
                if ui.add(add_btn).clicked() {
                    editor.create_master();
                }
            });
        });

    let mut select: Option<String> = None;
    let mut rename: Option<String> = None;
    let mut delete: Option<String> = None;

    let out = ScrollArea::vertical()
        .id_salt("masters_panel_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let origin = ui.available_rect_before_wrap();
            let mut y = origin.min.y + SCENE_FIRST_TOP_GAP;
            let thumb_w =
                (origin.width() - GUTTER_WIDTH - coursecraft_render::PANEL_MARGIN).max(64.0);
            let thumb_h = (thumb_w * 9.0 / 16.0).round();
            let row_height = thumb_h + TITLE_GAP + TITLE_HEIGHT + ROW_GAP;
            let thumb_x = origin.min.x + GUTTER_WIDTH;

            // A clearly visible way out, pinned at the top of the navigator.
            let close_rect = Rect::from_min_size(
                pos2(origin.min.x + coursecraft_render::PANEL_MARGIN, y),
                vec2(
                    (origin.width() - coursecraft_render::PANEL_MARGIN * 2.0).max(1.0),
                    30.0,
                ),
            );
            let close = ui.interact(
                close_rect,
                ui.scope_id().with("close_master_view"),
                Sense::click(),
            );
            ui.painter().rect_filled(
                close_rect,
                CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                if close.hovered() { p.hover } else { p.input },
            );
            ui.painter().text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "Close slide masters",
                FontId::new(
                    coursecraft_render::SIZE_BODY,
                    egui::FontFamily::Name("inter-bold".into()),
                ),
                p.accent_text,
            );
            if close.clicked() {
                editor.leave_master_view();
            }
            y += 30.0 + SCENE_TOP_GAP;

            ui.painter().text(
                pos2(thumb_x, y),
                Align2::LEFT_TOP,
                "Masters",
                FontId::new(
                    coursecraft_render::SIZE_CAPTION,
                    egui::FontFamily::Name("inter-bold".into()),
                ),
                p.text_secondary,
            );
            y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

            let clip_rect = ui.clip_rect();
            let revision = editor.session.revision;
            let master_count = editor.session.course.masters.len();
            if master_count == 0 {
                ui.painter().text(
                    pos2(thumb_x, y),
                    Align2::LEFT_TOP,
                    "No masters yet.",
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter".into()),
                    ),
                    p.text_secondary,
                );
                let new_rect = Rect::from_min_size(
                    pos2(thumb_x, y + TITLE_HEIGHT + TITLE_GAP),
                    vec2(thumb_w, 30.0),
                );
                let resp = ui.interact(
                    new_rect,
                    ui.scope_id().with("empty_new_master"),
                    Sense::click(),
                );
                ui.painter().rect_filled(
                    new_rect,
                    CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                    if resp.hovered() { p.hover } else { p.input },
                );
                ui.painter().text(
                    new_rect.center(),
                    Align2::CENTER_CENTER,
                    "+ New master",
                    FontId::new(
                        coursecraft_render::SIZE_BODY,
                        egui::FontFamily::Name("inter-bold".into()),
                    ),
                    p.accent_text,
                );
                if resp.clicked() {
                    editor.create_master();
                }
                y += row_height;
            }

            for master_idx in 0..master_count {
                let Some((master_id, master_title)) = editor
                    .session
                    .course
                    .masters
                    .get(master_idx)
                    .map(|m| (m.id.clone(), m.title.clone()))
                else {
                    continue;
                };
                let thumb_rect = Rect::from_min_size(pos2(thumb_x, y), vec2(thumb_w, thumb_h));
                let title_rect = Rect::from_min_size(
                    pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                    vec2(thumb_w, TITLE_HEIGHT),
                );
                let row_rect = Rect::from_min_max(thumb_rect.min, title_rect.max);
                let renaming = editor
                    .ui
                    .master_rename
                    .as_ref()
                    .is_some_and(|state| state.scene == master_id);
                if !row_rect.expand(4.0).intersects(clip_rect) && !renaming {
                    y += row_height;
                    continue;
                }

                let is_selected = editor.selection.slide == master_id;

                if thumb_rect.intersects(clip_rect)
                    && let Some(master) = editor.session.course.masters.get(master_idx)
                {
                    let (thumb_bg, items) = editor.ui.thumbnail_cache.get_or_compose(
                        revision,
                        editor.session.last_changed_slide(),
                        &editor.session.course,
                        master,
                    );
                    let painter = ui.painter_at(thumb_rect);
                    painter.rect_filled(
                        thumb_rect,
                        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                        thumb_bg,
                    );
                    if let Some(t) =
                        SlideTransform::fit(thumb_rect, editor.session.course.slide_size)
                    {
                        #[cfg(test)]
                        crate::guard_counters::note_thumbnail_paint(&master_id, master);
                        let source = crate::MemoryMediaSource::from_store(&editor.media);
                        paint(
                            &painter,
                            &t,
                            &items,
                            Highlight::default(),
                            Some((&mut editor.media_cache, &source)),
                            Some(&editor.session.course.theme),
                            Some(editor.session.course.slide_background(master)),
                        );
                    }
                }

                let resp = ui.interact(
                    thumb_rect,
                    ui.scope_id().with("master_thumb").with(&master_id),
                    Sense::click(),
                );
                if resp.clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    select = Some(master_id.clone());
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

                // ⋯ menu: Rename and Delete.
                let more_btn_rect = Rect::from_min_size(
                    pos2(thumb_rect.max.x - 24.0 - 4.0, thumb_rect.min.y + 4.0),
                    vec2(24.0, 24.0),
                );
                let more_id = ui.scope_id().with("master_more").with(&master_id);
                let is_menu_open = egui::Popup::is_id_open(ui.ctx(), more_id);
                if resp.hovered() || is_selected || is_menu_open {
                    ui.painter().rect_filled(
                        more_btn_rect,
                        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                        p.raised,
                    );
                    let more = ui
                        .interact(more_btn_rect, more_id, Sense::click())
                        .on_hover_text("Master options");
                    let icon_color = if more.hovered() {
                        p.text
                    } else {
                        p.text_secondary
                    };
                    let font_id = FontId::new(
                        coursecraft_render::ICON_SM,
                        egui::FontFamily::Name("phosphor".into()),
                    );
                    let galley = ui.painter().layout_no_wrap(
                        coursecraft_render::Icon::DotsThree.codepoint().to_string(),
                        font_id,
                        icon_color,
                    );
                    ui.painter().galley(
                        pos2(
                            more_btn_rect.center().x - galley.size().x * 0.5,
                            more_btn_rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        icon_color,
                    );
                    egui::Popup::menu(&more).show(|ui| {
                        if ui.button("Rename").clicked() {
                            rename = Some(master_id.clone());
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(master_id.clone());
                            ui.close();
                        }
                    });
                }

                // Title below the thumbnail, or an in-place rename field.
                if renaming {
                    show_master_rename_editor(editor, ui, title_rect);
                } else {
                    let title_color = if is_selected {
                        p.text
                    } else {
                        p.text_secondary
                    };
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        master_title.clone(),
                        FontId::new(
                            coursecraft_render::SIZE_SMALL,
                            egui::FontFamily::Name("inter".into()),
                        ),
                        title_color,
                    );
                    job.wrap =
                        egui::text::TextWrapping::truncate_at_width(title_rect.width().max(16.0));
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    let elided = galley.elided;
                    ui.painter_at(title_rect)
                        .galley(title_rect.left_top(), galley, title_color);
                    if elided {
                        ui.interact(
                            title_rect,
                            ui.scope_id().with("master_title").with(&master_id),
                            Sense::hover(),
                        )
                        .on_hover_text(&master_title);
                    }
                }

                y += row_height;
            }

            let total_height = y - origin.min.y;
            ui.allocate_space(vec2(origin.width(), total_height));
        });
    editor.ui.slides_scroll_y = out.state.clamped_offset().y;

    if let Some(id) = select {
        editor.selection.slide = id;
        editor.selection.layer = None;
        editor.selection.clear_objects();
    }

    if let Some(id) = rename {
        start_master_rename(editor, &id);
    }
    if let Some(id) = delete {
        request_master_delete(editor, &id);
    }
}

pub(crate) fn start_master_rename(editor: &mut Editor, master_id: &str) {
    let title = editor
        .session
        .course
        .master(master_id)
        .map(|master| master.title.clone())
        .unwrap_or_default();
    editor.ui.master_rename = Some(SceneRename {
        scene: master_id.to_owned(),
        buffer: title.clone(),
        original: title,
        just_entered: true,
    });
}

fn show_master_rename_editor(editor: &mut Editor, ui: &mut Ui, rect: Rect) {
    let Some(rename) = editor.ui.master_rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("master_rename", &rename.scene));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let mut output = TextEdit::singleline(&mut buffer)
        .id(edit_id)
        .font(FontId::new(
            coursecraft_render::SIZE_SMALL,
            egui::FontFamily::Name("inter".into()),
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
        if let Some(state) = editor.ui.master_rename.as_mut() {
            state.just_entered = false;
        }
    }
    if let Some(state) = editor.ui.master_rename.as_mut() {
        state.buffer = buffer.clone();
    }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    if escaped {
        editor.ui.master_rename = None;
    } else if enter || lost {
        let title = buffer.trim().to_owned();
        if !title.is_empty() && title != rename.original {
            editor.run(
                "slide.update",
                serde_json::json!({ "slide": rename.scene, "title": title }),
            );
        }
        editor.ui.master_rename = None;
    }
}

fn start_slide_rename(editor: &mut Editor, slide_id: &str) {
    let title = editor
        .session
        .course
        .slide(slide_id)
        .map(|slide| slide.title.clone())
        .unwrap_or_default();
    editor.ui.slide_rename = Some(SceneRename {
        scene: slide_id.to_owned(),
        buffer: title.clone(),
        original: title,
        just_entered: true,
    });
}

fn show_slide_rename_editor(editor: &mut Editor, ui: &mut Ui, rect: Rect) {
    let Some(rename) = editor.ui.slide_rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("slide_rename", &rename.scene));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let mut output = TextEdit::singleline(&mut buffer)
        .id(edit_id)
        .font(FontId::new(
            coursecraft_render::SIZE_SMALL,
            egui::FontFamily::Name("inter".into()),
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
        if let Some(state) = editor.ui.slide_rename.as_mut() {
            state.just_entered = false;
        }
    }
    if let Some(state) = editor.ui.slide_rename.as_mut() {
        state.buffer = buffer.clone();
    }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    if escaped {
        editor.ui.slide_rename = None;
    } else if enter || lost {
        let title = buffer.trim().to_owned();
        if !title.is_empty() && title != rename.original {
            editor.run(
                "slide.update",
                serde_json::json!({ "slide": rename.scene, "title": title }),
            );
        }
        editor.ui.slide_rename = None;
    }
}

fn slides_using_master(course: &Course, master_id: &str) -> usize {
    course
        .slides()
        .chain(course.templates.iter())
        .filter(|slide| slide.master.as_deref() == Some(master_id))
        .count()
}

pub(crate) fn request_master_delete(editor: &mut Editor, master_id: &str) {
    if slides_using_master(&editor.session.course, master_id) > 0 {
        editor.ui.pending_delete_master = Some(master_id.to_owned());
    } else {
        editor.run("master.delete", serde_json::json!({ "master": master_id }));
    }
}

/// Confirmation dialog before deleting a master that slides still use.
pub fn show_master_delete_dialog(editor: &mut Editor, ctx: &egui::Context) {
    let Some(master_id) = editor.ui.pending_delete_master.clone() else {
        return;
    };
    let Some(master) = editor.session.course.master(&master_id) else {
        editor.ui.pending_delete_master = None;
        return;
    };
    let title = master.title.clone();
    let count = slides_using_master(&editor.session.course, &master_id);
    let mut close = false;
    let mut delete = false;
    let mut open = true;
    Dialog::new("Delete master")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(format!(
            "Delete the master \"{title}\"? {count} slide(s) use it and will draw with no master."
        ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Delete master").clicked() {
                    delete = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if !open {
        close = true;
    }
    if delete {
        editor.run("master.delete", serde_json::json!({ "master": master_id }));
        editor.ui.pending_delete_master = None;
    } else if close {
        editor.ui.pending_delete_master = None;
    }
}

/// The one layout popup shared by the toolbar caret and the navigator's
/// `+ New slide` caret: six layouts plus saved templates.
pub fn show_new_slide_popup(ui: &mut Ui, editor: &mut Editor) {
    let after = if editor
        .session
        .course
        .play_slide(&editor.selection.slide)
        .is_some()
    {
        Some(editor.selection.slide.clone())
    } else {
        None
    };
    let scene = editor
        .session
        .course
        .scenes
        .iter()
        .find(|scene| scene.slides.iter().any(|s| s.id == editor.selection.slide))
        .map(|scene| scene.id.clone())
        .or_else(|| editor.session.course.scenes.first().map(|s| s.id.clone()));

    let mut add: Option<(&'static str, Json)> = None;
    ui.label(coursecraft_render::small("Layouts").strong());
    for (label, layout) in [
        ("Blank", "blank"),
        ("Title", "title"),
        ("Title and content", "title_content"),
        ("Two columns", "two_columns"),
        ("Picture and text", "picture_text"),
        ("Question", "question"),
    ] {
        if ui.button(label).clicked() {
            let mut params = serde_json::json!({ "layout": layout });
            if let Some(ref after) = after {
                params["after"] = serde_json::json!(after);
            }
            if let Some(ref scene) = scene {
                params["scene"] = serde_json::json!(scene);
            }
            add = Some(("slide.add_from_layout", params));
            ui.close();
        }
    }

    let library_available = !cfg!(target_arch = "wasm32")
        && editor.file_ops_available
        && editor.library_templates_available;

    if !editor.session.course.templates.is_empty() {
        ui.separator();
        ui.label(coursecraft_render::small("Your templates").strong());
        let templates = editor.session.course.templates.clone();
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .id_salt("layout_popup_templates")
            .show(ui, |ui| {
                for template in &templates {
                    ui.horizontal(|ui| {
                        if ui.button(&template.title).clicked() {
                            let mut params = serde_json::json!({ "template": template.id });
                            if let Some(ref after) = after {
                                params["after"] = serde_json::json!(after);
                            }
                            if let Some(ref scene) = scene {
                                params["scene"] = serde_json::json!(scene);
                            }
                            add = Some(("template.add_slide", params));
                            ui.close();
                        }
                        if library_available
                            && crate::quiet_button(
                                ui,
                                "Save to my library",
                                crate::QuietTone::Accent,
                                true,
                            )
                            .clicked()
                        {
                            editor.pending_requests.push(
                                crate::EditorRequest::SaveTemplateToLibrary {
                                    template_id: template.id.clone(),
                                },
                            );
                            ui.close();
                        }
                        if crate::quiet_button(ui, "Delete", crate::QuietTone::Danger, true)
                            .clicked()
                        {
                            add = Some((
                                "template.delete",
                                serde_json::json!({ "template": template.id }),
                            ));
                            ui.close();
                        }
                    });
                }
            });
    }

    if library_available {
        ui.separator();
        ui.label(coursecraft_render::small("My library").strong());
        if editor.library_templates.is_empty() {
            ui.label(coursecraft_render::design::caption(
                "No saved templates yet.",
            ));
        } else {
            let library = editor.library_templates.clone();
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .id_salt("layout_popup_library_templates")
                .show(ui, |ui| {
                    for tpl in &library {
                        ui.horizontal(|ui| {
                            if ui.button(&tpl.title).clicked() {
                                editor.pending_requests.push(
                                    crate::EditorRequest::InsertLibraryTemplate {
                                        id: tpl.id.clone(),
                                        scene: scene.clone(),
                                        after: after.clone(),
                                    },
                                );
                                editor.ui.confirm_delete_library_template = None;
                                ui.close();
                            }
                            if editor.ui.confirm_delete_library_template.as_deref()
                                == Some(tpl.id.as_str())
                            {
                                if crate::quiet_button(
                                    ui,
                                    "Confirm",
                                    crate::QuietTone::Danger,
                                    true,
                                )
                                .clicked()
                                {
                                    editor.pending_requests.push(
                                        crate::EditorRequest::DeleteLibraryTemplate {
                                            id: tpl.id.clone(),
                                        },
                                    );
                                    editor.ui.confirm_delete_library_template = None;
                                    ui.close();
                                }
                                if crate::quiet_button(
                                    ui,
                                    "Cancel",
                                    crate::QuietTone::Secondary,
                                    true,
                                )
                                .clicked()
                                {
                                    editor.ui.confirm_delete_library_template = None;
                                }
                            } else if crate::quiet_button(
                                ui,
                                "Delete",
                                crate::QuietTone::Danger,
                                true,
                            )
                            .clicked()
                            {
                                editor.ui.confirm_delete_library_template = Some(tpl.id.clone());
                            }
                        });
                    }
                });
        }
    }

    if let Some((cmd, params)) = add {
        editor.run(cmd, params);
    }
}

#[allow(clippy::too_many_arguments)]
fn show_slide_menu_items(
    ui: &mut Ui,
    slide_id: &str,
    scene_id: &str,
    slide_idx: usize,
    total_in_scene: usize,
    total_slides: usize,
    multi_selected: bool,
    selected_count: usize,
    multi_move: Option<(bool, bool)>,
    can_paste: bool,
    has_question: bool,
    banks: &[(String, String)],
    action_to_run: &mut Option<(&'static str, Json)>,
    slide_action: &mut Option<SlideMenuAction>,
) {
    if crate::menu::menu_button(
        ui,
        "New slide after",
        crate::shortcuts::shortcut_for_command("slide.add"),
        true,
        None,
    )
    .clicked()
    {
        *action_to_run = Some((
            "slide.add",
            serde_json::json!({
                "scene": scene_id,
                "after": slide_id,
                "title": format!("Slide {}", total_slides.saturating_add(1)),
            }),
        ));
        ui.close();
    }
    let duplicate_label = if multi_selected {
        "Duplicate slides"
    } else {
        "Duplicate slide"
    };
    if ui.button(duplicate_label).clicked() {
        if multi_selected {
            *slide_action = Some(SlideMenuAction::DuplicateSelected);
        } else {
            *action_to_run = Some(("slide.duplicate", serde_json::json!({ "slide": slide_id })));
        }
        ui.close();
    }
    if crate::menu::menu_button(
        ui,
        "Rename",
        Some(crate::shortcuts::shortcut_for_action(
            crate::shortcuts::ShortcutAction::RenameSlide,
        )),
        true,
        None,
    )
    .clicked()
    {
        *slide_action = Some(SlideMenuAction::Rename(slide_id.to_owned()));
        ui.close();
    }
    ui.separator();

    let can_cut = if multi_selected {
        total_slides > selected_count
    } else {
        total_slides > 1
    };
    if crate::menu::menu_button(
        ui,
        "Cut",
        Some(crate::shortcuts::shortcut_for_action(
            crate::shortcuts::ShortcutAction::Cut,
        )),
        can_cut,
        Some("A course needs at least one slide"),
    )
    .clicked()
    {
        *slide_action = Some(SlideMenuAction::Cut(slide_id.to_owned()));
        ui.close();
    }
    if crate::menu::menu_button(
        ui,
        "Copy",
        Some(crate::shortcuts::shortcut_for_action(
            crate::shortcuts::ShortcutAction::Copy,
        )),
        true,
        None,
    )
    .clicked()
    {
        *slide_action = Some(SlideMenuAction::Copy(slide_id.to_owned()));
        ui.close();
    }
    if crate::menu::menu_button(
        ui,
        "Paste",
        Some(crate::shortcuts::shortcut_for_action(
            crate::shortcuts::ShortcutAction::Paste,
        )),
        can_paste,
        Some("Nothing copied yet"),
    )
    .clicked()
    {
        *slide_action = Some(SlideMenuAction::Paste);
        ui.close();
    }
    ui.separator();

    let can_up = multi_move.map_or(slide_idx > 0, |(up, _)| up);
    if ui
        .add_enabled(can_up, egui::Button::new("Move up"))
        .on_disabled_hover_text("Slide is already at the top of this scene")
        .clicked()
    {
        if multi_move.is_some() {
            *slide_action = Some(SlideMenuAction::MoveSelected { up: true });
        } else {
            *action_to_run = Some((
                "slide.move",
                serde_json::json!({
                    "slide": slide_id,
                    "scene": scene_id,
                    "index": slide_idx.saturating_sub(1),
                }),
            ));
        }
        ui.close();
    }
    let can_down = multi_move.map_or(slide_idx + 1 < total_in_scene, |(_, down)| down);
    if ui
        .add_enabled(can_down, egui::Button::new("Move down"))
        .on_disabled_hover_text("Slide is already at the bottom of this scene")
        .clicked()
    {
        if multi_move.is_some() {
            *slide_action = Some(SlideMenuAction::MoveSelected { up: false });
        } else {
            *action_to_run = Some((
                "slide.move",
                serde_json::json!({
                    "slide": slide_id,
                    "scene": scene_id,
                    "index": slide_idx + 1,
                }),
            ));
        }
        ui.close();
    }
    ui.separator();

    if ui.button("Save as template").clicked() {
        *action_to_run = Some(("template.save", serde_json::json!({ "slide": slide_id })));
        ui.close();
    }
    // Only a question slide can move into a bank.
    if has_question && !banks.is_empty() {
        ui.menu_button("Move to bank ▸", |ui| {
            for (bank_id, title) in banks {
                if ui.button(title).clicked() {
                    *action_to_run = Some((
                        "bank.move_slide",
                        serde_json::json!({ "slide": slide_id, "bank": bank_id }),
                    ));
                    ui.close();
                }
            }
        });
    }
    ui.separator();

    if ui
        .add_enabled(
            multi_selected && selected_count >= 2,
            egui::Button::new("Preview selected slides"),
        )
        .on_disabled_hover_text("Select two or more slides to preview them")
        .clicked()
    {
        *slide_action = Some(SlideMenuAction::PreviewSelected);
        ui.close();
    }
    ui.separator();

    let can_delete = if multi_selected {
        total_slides > selected_count
    } else {
        total_slides > 1
    };
    if ui
        .add_enabled(can_delete, egui::Button::new("Delete"))
        .on_disabled_hover_text("A course needs at least one slide")
        .clicked()
    {
        if multi_selected {
            *slide_action = Some(SlideMenuAction::DeleteSelected);
        } else {
            *action_to_run = Some((
                "slide.delete",
                serde_json::json!({
                    "slide": slide_id,
                }),
            ));
        }
        ui.close();
    }
}

/// Insertion point for a pointer y across every laid-out slide row.
fn slide_drop_target(
    rows: &[(String, String, usize, Rect)],
    pointer_y: f32,
) -> Option<(String, usize)> {
    for (scene, _slide, index, rect) in rows {
        if pointer_y < rect.center().y {
            return Some((scene.clone(), *index));
        }
    }
    rows.last()
        .map(|(scene, _slide, index, _rect)| (scene.clone(), index.saturating_add(1)))
}

/// y of the 2 px drop line for an insertion point, or `None` when the scene has no rows.
fn slide_drop_line_y(
    rows: &[(String, String, usize, Rect)],
    scene: &str,
    index: usize,
) -> Option<f32> {
    if let Some((_, _, _, rect)) = rows
        .iter()
        .find(|(row_scene, _slide, row_index, _rect)| row_scene == scene && *row_index == index)
    {
        return Some(rect.min.y - ROW_GAP * 0.5);
    }
    rows.iter()
        .rev()
        .find(|(row_scene, _slide, _index, _rect)| row_scene == scene)
        .map(|(_, _slide, _index, rect)| rect.max.y + TITLE_GAP + TITLE_HEIGHT + ROW_GAP * 0.5)
}

/// Update, draw and finish a slide-thumbnail drag.
fn handle_slide_drag(
    editor: &mut Editor,
    ui: &mut Ui,
    rows: &[(String, String, usize, Rect)],
    panel_rect: Rect,
) {
    let Some(mut drag) = editor.ui.slide_drag.take() else {
        return;
    };
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_down = ui.input(|i| i.pointer.primary_down());

    if pointer_down {
        if let Some(pos) = pointer_pos {
            if (pos.y - drag.start_pointer_y).abs() >= 4.0 {
                drag.moved = true;
            }
            if drag.moved
                && let Some((scene, index)) = slide_drop_target(rows, pos.y)
            {
                drag.target_scene = scene;
                drag.target_index = index;
            }
        }
        if drag.moved
            && let Some(y) = slide_drop_line_y(rows, &drag.target_scene, drag.target_index)
        {
            let p = coursecraft_render::palette(ui.ctx());
            ui.painter().line_segment(
                [
                    pos2(panel_rect.min.x + GUTTER_WIDTH, y),
                    pos2(panel_rect.max.x - coursecraft_render::PANEL_MARGIN, y),
                ],
                Stroke::new(2.0, p.accent),
            );
        }
        editor.ui.slide_drag = Some(drag);
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

#[allow(clippy::too_many_arguments)]
fn show_scene_header_menu(
    ui: &mut Ui,
    scene_id: &str,
    can_delete: bool,
    next_title: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
    scene_to_rename: &mut Option<String>,
    scene_to_delete: &mut Option<String>,
) {
    if ui.button("Rename").clicked() {
        *scene_to_rename = Some(scene_id.to_owned());
        ui.close();
    }
    if crate::menu::menu_button(
        ui,
        "New slide",
        crate::shortcuts::shortcut_for_command("slide.add"),
        true,
        None,
    )
    .clicked()
    {
        *action_to_run = Some((
            "slide.add",
            serde_json::json!({
                "scene": scene_id,
                "title": next_title,
            }),
        ));
        ui.close();
    }
    if ui
        .add_enabled(can_delete, egui::Button::new("Delete scene"))
        .on_disabled_hover_text("A course needs at least one scene")
        .clicked()
    {
        *scene_to_delete = Some(scene_id.to_owned());
        ui.close();
    }
}

fn show_slides_empty_area(editor: &mut Editor, ui: &mut Ui, panel_rect: Rect, content_height: f32) {
    let empty_top = panel_rect.min.y + content_height;
    if empty_top >= panel_rect.max.y {
        return;
    }
    let empty_rect = Rect::from_min_max(pos2(panel_rect.min.x, empty_top), panel_rect.max);
    let empty_response = ui.interact(
        empty_rect,
        ui.scope_id().with("slides_empty_area"),
        Sense::click(),
    );
    empty_response.context_menu(|ui| show_slides_empty_menu(editor, ui));
}

fn show_slides_empty_menu(editor: &mut Editor, ui: &mut Ui) {
    if ui.button("New slide").clicked() {
        crate::toolbar::add_new_slide(editor);
        ui.close();
    }
    let can_paste = editor.ui.clipboard.is_some();
    if crate::menu::menu_button(
        ui,
        "Paste",
        Some(crate::shortcuts::shortcut_for_action(
            crate::shortcuts::ShortcutAction::Paste,
        )),
        can_paste,
        Some("Nothing copied yet"),
    )
    .clicked()
    {
        crate::clipboard::paste_private(editor);
        ui.close();
    }
}

#[allow(clippy::cognitive_complexity)]
fn show_slides_content(editor: &mut Editor, ui: &mut Ui, panel_rect: Rect) {
    let mut selected_rect: Option<Rect> = None;
    // The slide to keep in view: the moving end of a Shift+arrow range, else the primary.
    let scroll_target = editor
        .ui
        .slides_cursor
        .clone()
        .filter(|id| editor.selection.is_slide_selected(id))
        .unwrap_or_else(|| editor.selection.slide.clone());
    // With several slides selected: can Move up and Move down shift any of them?
    let multi_move_all = (!editor.selection.also_slides.is_empty()).then(|| {
        let selected = editor.selected_slides_in_order();
        (
            !plan_slide_moves(&editor.session.course, &selected, true).is_empty(),
            !plan_slide_moves(&editor.session.course, &selected, false).is_empty(),
        )
    });
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let mut slide_action: Option<SlideMenuAction> = None;
    let mut slide_to_select: Option<SlideClick> = None;
    let mut slide_to_pin: Option<String> = None;
    let mut slide_to_rename: Option<String> = None;
    let mut scene_to_rename: Option<String> = None;
    let mut scene_to_delete: Option<String> = None;
    let mut bank_to_rename: Option<String> = None;
    // Every slide row laid out this frame when a slide drag is active.
    let dragging_slide = editor.ui.slide_drag.is_some();
    let mut slide_rows: Vec<(String, String, usize, Rect)> = Vec::new();

    let p = coursecraft_render::palette(ui.ctx());
    let mode = coursecraft_render::theme_mode(ui.ctx());
    let clip_rect = ui.clip_rect();
    let thumb_w = (panel_rect.width() - GUTTER_WIDTH - coursecraft_render::PANEL_MARGIN).max(64.0);
    let thumb_h = (thumb_w * 9.0 / 16.0).round();
    let row_height = thumb_h + TITLE_GAP + TITLE_HEIGHT + ROW_GAP;
    let thumb_x = panel_rect.min.x + GUTTER_WIDTH;

    let revision = editor.session.revision;
    let nav = editor
        .ui
        .navigator_index
        .snapshot(revision, &editor.session.course);
    let total_course_slides = nav.total_course_slides;
    let scene_count = nav.scenes.len();
    let mut current_y = panel_rect.min.y;
    let mut global_slide_idx: usize = 0;

    for (scene_idx, scene) in nav.scenes.iter().enumerate() {
        if scene_idx == 0 {
            current_y += SCENE_FIRST_TOP_GAP;
        } else {
            current_y += SCENE_TOP_GAP;
        }

        // Scene caption header: double-click renames, right-click gives rename/delete.
        let header_rect =
            Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
        let renaming = editor
            .ui
            .scene_rename
            .as_ref()
            .is_some_and(|rename| rename.scene == scene.id);
        if header_rect.expand(4.0).intersects(clip_rect) || renaming {
            let header_id = ui.scope_id().with("scene_header").with(&scene.id);
            let header_resp = ui.interact(header_rect, header_id, Sense::click());
            if header_resp.double_clicked() {
                scene_to_rename = Some(scene.id.clone());
            }
            header_resp.context_menu(|ui| {
                show_scene_header_menu(
                    ui,
                    &scene.id,
                    scene_count > 1,
                    &crate::toolbar::next_slide_title(&editor.session.course),
                    &mut action_to_run,
                    &mut scene_to_rename,
                    &mut scene_to_delete,
                );
            });

            if renaming {
                show_scene_rename_editor(editor, ui, header_rect);
            } else {
                let painter = ui.painter_at(header_rect);
                // One line, ending in "…" before the + so a long title never
                // runs under it.
                let mut job = egui::text::LayoutJob::simple_singleline(
                    scene.title.clone(),
                    FontId::new(
                        coursecraft_render::SIZE_CAPTION,
                        egui::FontFamily::Name("inter-bold".into()),
                    ),
                    p.text_secondary,
                );
                job.wrap = egui::text::TextWrapping::truncate_at_width(
                    (header_rect.width() - 28.0).max(16.0),
                );
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let elided = galley.elided;
                let title_pos = pos2(
                    header_rect.min.x,
                    header_rect.center().y - galley.size().y * 0.5,
                );
                painter.galley(title_pos, galley, p.text_secondary);
                if elided {
                    header_resp.on_hover_text(&scene.title);
                }
                // "+" at the right of the header: a new slide at the end of this
                // scene, so a slide never has to be made elsewhere and dragged in.
                let add_size = vec2(26.0, 26.0);
                let add_rect = Rect::from_center_size(
                    pos2(header_rect.max.x - add_size.x * 0.5, header_rect.center().y),
                    add_size,
                );
                let add_tooltip = format!("New slide in {}", scene.title);
                let add_resp = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(add_rect), |ui| {
                        crate::widgets::icon_button(
                            ui,
                            coursecraft_render::Icon::Plus,
                            &add_tooltip,
                        )
                    })
                    .inner;
                if add_resp.clicked() && !editor.ui.master_view {
                    action_to_run = Some((
                        "slide.add",
                        serde_json::json!({
                            "scene": scene.id,
                            "title": crate::toolbar::next_slide_title(&editor.session.course),
                        }),
                    ));
                }
            }
        }
        current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

        let total_in_scene = scene.slides.len();
        for (slide_idx, slide) in scene.slides.iter().enumerate() {
            global_slide_idx += 1;
            let thumb_rect = Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, thumb_h));
            let title_rect = Rect::from_min_size(
                pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                vec2(thumb_w, TITLE_HEIGHT),
            );
            let row_rect = Rect::from_min_max(
                pos2(panel_rect.min.x, thumb_rect.min.y),
                pos2(thumb_rect.max.x, title_rect.max.y),
            );
            if scroll_target == slide.id {
                selected_rect = Some(Rect::from_min_size(
                    thumb_rect.min,
                    vec2(thumb_w, thumb_h + TITLE_GAP + TITLE_HEIGHT),
                ));
            }
            if dragging_slide {
                slide_rows.push((scene.id.clone(), slide.id.clone(), slide_idx, thumb_rect));
            }

            let visible = row_rect.expand(4.0).intersects(clip_rect);
            let renaming = editor
                .ui
                .slide_rename
                .as_ref()
                .is_some_and(|state| state.scene == slide.id);
            if !visible && !renaming {
                current_y += row_height;
                continue;
            }

            let is_selected = editor.selection.is_slide_selected(&slide.id);

            // Gutter with slide number (vertically centred on the thumbnail)
            let num_pos = pos2(panel_rect.min.x + GUTTER_WIDTH - 4.0, thumb_rect.center().y);
            let num_painter = ui.painter();
            if is_selected {
                num_painter.text(
                    num_pos,
                    Align2::RIGHT_CENTER,
                    global_slide_idx.to_string(),
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter-bold".into()),
                    ),
                    p.accent_text,
                );
            } else {
                num_painter.text(
                    num_pos,
                    Align2::RIGHT_CENTER,
                    global_slide_idx.to_string(),
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter".into()),
                    ),
                    p.text_tertiary,
                );
            }

            // Thumbnail background & items (composed only when visible in viewport)
            if thumb_rect.intersects(clip_rect)
                && let Some(slide_ref) = editor
                    .session
                    .course
                    .scenes
                    .get(scene_idx)
                    .and_then(|sc| sc.slides.get(slide_idx))
            {
                let (thumb_bg, items) = editor.ui.thumbnail_cache.get_or_compose(
                    revision,
                    editor.session.last_changed_slide(),
                    &editor.session.course,
                    slide_ref,
                );
                let painter = ui.painter_at(thumb_rect);
                painter.rect_filled(
                    thumb_rect,
                    CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                    thumb_bg,
                );
                if let Some(t) = SlideTransform::fit(thumb_rect, editor.session.course.slide_size) {
                    #[cfg(test)]
                    crate::guard_counters::note_thumbnail_paint(&slide.id, slide_ref);
                    let source = crate::MemoryMediaSource::from_store(&editor.media);
                    paint(
                        &painter,
                        &t,
                        &items,
                        Highlight::default(),
                        Some((&mut editor.media_cache, &source)),
                        Some(&editor.session.course.theme),
                        Some(editor.session.course.slide_background(slide_ref)),
                    );
                }
            }

            let visible_thumb = thumb_rect.intersect(clip_rect);
            if visible_thumb.is_positive() {
                crate::content_region::mark_content_region(ui.ctx(), ui.layer_id(), visible_thumb);
            }

            // Thumbnail click, drag and context menu interaction
            let thumb_response =
                ui.interact(thumb_rect, ui.scope_id().with(&slide.id), Sense::click());
            let raw_pointer = ui.input(|i| i.pointer.interact_pos());
            let covered = raw_pointer.is_some_and(|pos| !crate::widgets::pointer_reaches(ui, pos));
            let more_btn_size = vec2(24.0, 24.0);
            let more_btn_rect = Rect::from_min_size(
                pos2(thumb_rect.max.x - 24.0 - 4.0, thumb_rect.min.y + 4.0),
                more_btn_size,
            );
            let over_more_btn = raw_pointer.is_some_and(|pos| more_btn_rect.contains(pos));
            let click_mods = ui.input(|i| i.modifiers);
            let multi_mod = click_mods.shift || click_mods.command || click_mods.ctrl;
            if !covered
                && !over_more_btn
                && !multi_mod
                && editor.ui.slide_drag.is_none()
                && ui.input(|i| i.pointer.primary_pressed())
                && thumb_response.contains_pointer()
            {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
                if !editor.selection.is_slide_selected(&slide.id) {
                    slide_to_select = Some(SlideClick::Single(slide.id.clone()));
                }
                editor.ui.slide_drag = Some(SlideDrag {
                    slide_id: slide.id.clone(),
                    source_scene: scene.id.clone(),
                    source_index: slide_idx,
                    start_pointer_y: ui
                        .input(|i| i.pointer.interact_pos())
                        .map_or(0.0, |pos| pos.y),
                    moved: false,
                    target_scene: scene.id.clone(),
                    target_index: slide_idx,
                });
            }
            if !covered && thumb_response.clicked() {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
                slide_to_select = Some(if click_mods.shift {
                    SlideClick::Range(slide.id.clone())
                } else if click_mods.command || click_mods.ctrl {
                    SlideClick::Toggle(slide.id.clone())
                } else {
                    SlideClick::Single(slide.id.clone())
                });
            }
            if !covered && thumb_response.double_clicked() {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
                slide_to_pin = Some(slide.id.clone());
            }
            if (!covered && thumb_response.secondary_clicked()) || thumb_response.has_focus() {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
            }

            let is_hovered = thumb_response.hovered();

            // Right-clicking thumbnail opens context menu
            let can_paste = editor.ui.clipboard.is_some();
            let multi_selected = !editor.selection.also_slides.is_empty()
                && editor.selection.is_slide_selected(&slide.id);
            let selected_slide_count = editor.selection.slide_ids().len();
            let multi_move = multi_selected.then_some(multi_move_all).flatten();
            thumb_response.context_menu(|ui| {
                show_slide_menu_items(
                    ui,
                    &slide.id,
                    &scene.id,
                    slide_idx,
                    total_in_scene,
                    total_course_slides,
                    multi_selected,
                    selected_slide_count,
                    multi_move,
                    can_paste,
                    slide.is_question,
                    &nav.bank_menu,
                    &mut action_to_run,
                    &mut slide_action,
                );
            });

            // Outline strokes based on states:
            // normal: 1 px border; hover: 1 px border_strong
            // selected: the 1 px border plus a 2 px accent ring outside it. The ring is painted
            // with the unclipped panel painter (the thumbnail painter clips to the thumbnail).
            let thumb_radius = CornerRadius::same(coursecraft_render::RADIUS_MD as u8);
            let border = if is_hovered && !is_selected {
                p.border_strong
            } else {
                p.border
            };
            ui.painter().rect_stroke(
                thumb_rect,
                thumb_radius,
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

            // DotsThree button at thumbnail top-right on hover or when selected
            let more_id = ui.scope_id().with("more").with(&slide.id);
            let is_menu_open = egui::Popup::is_id_open(ui.ctx(), more_id);

            if is_hovered || is_selected || is_menu_open {
                let shadow = coursecraft_render::popover_shadow(mode);
                ui.painter().add(shadow.as_shape(
                    more_btn_rect,
                    CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                ));
                ui.painter().rect_filled(
                    more_btn_rect,
                    CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                    p.raised,
                );

                let more_resp = ui
                    .interact(more_btn_rect, more_id, Sense::click())
                    .on_hover_text("Slide options");
                more_resp.widget_info(|| {
                    let mut info = egui::WidgetInfo::new(egui::Role::Button);
                    info.label = Some("Slide options".to_owned());
                    info
                });
                let icon_color = if more_resp.hovered() {
                    p.text
                } else {
                    p.text_secondary
                };
                let font_id = FontId::new(
                    coursecraft_render::ICON_SM,
                    egui::FontFamily::Name("phosphor".into()),
                );
                let icon_galley = ui.painter().layout_no_wrap(
                    coursecraft_render::Icon::DotsThree.codepoint().to_string(),
                    font_id,
                    icon_color,
                );
                let icon_pos = pos2(
                    more_btn_rect.center().x - icon_galley.size().x * 0.5,
                    more_btn_rect.center().y - icon_galley.size().y * 0.5,
                );
                ui.painter().galley(icon_pos, icon_galley, icon_color);

                egui::Popup::menu(&more_resp).show(|ui| {
                    show_slide_menu_items(
                        ui,
                        &slide.id,
                        &scene.id,
                        slide_idx,
                        total_in_scene,
                        total_course_slides,
                        multi_selected,
                        selected_slide_count,
                        multi_move,
                        can_paste,
                        slide.is_question,
                        &nav.bank_menu,
                        &mut action_to_run,
                        &mut slide_action,
                    );
                });
            }

            // Slide title below thumbnail, or an in-place rename field.
            if renaming {
                show_slide_rename_editor(editor, ui, title_rect);
            } else {
                let title_resp = ui.interact(
                    title_rect,
                    ui.scope_id().with("slide_title").with(&slide.id),
                    Sense::click(),
                );
                if title_resp.clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    slide_to_select = Some(if click_mods.shift {
                        SlideClick::Range(slide.id.clone())
                    } else if click_mods.command || click_mods.ctrl {
                        SlideClick::Toggle(slide.id.clone())
                    } else {
                        SlideClick::Single(slide.id.clone())
                    });
                }
                if title_resp.double_clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    slide_to_select = Some(SlideClick::Single(slide.id.clone()));
                    slide_to_rename = Some(slide.id.clone());
                }
                if title_resp.secondary_clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                }
                title_resp.context_menu(|ui| {
                    show_slide_menu_items(
                        ui,
                        &slide.id,
                        &scene.id,
                        slide_idx,
                        total_in_scene,
                        total_course_slides,
                        multi_selected,
                        selected_slide_count,
                        multi_move,
                        can_paste,
                        slide.is_question,
                        &nav.bank_menu,
                        &mut action_to_run,
                        &mut slide_action,
                    );
                });
                let title_color = if is_selected {
                    p.text
                } else {
                    p.text_secondary
                };
                let painter = ui.painter_at(title_rect);
                let mut job = egui::text::LayoutJob::simple_singleline(
                    slide.title.clone(),
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter".into()),
                    ),
                    title_color,
                );
                job.wrap =
                    egui::text::TextWrapping::truncate_at_width(title_rect.width().max(16.0));
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let elided = galley.elided;
                painter.galley(title_rect.left_top(), galley, title_color);
                if elided {
                    title_resp.on_hover_text(&slide.title);
                }
            }

            current_y += row_height;
        }
    }

    // Question banks, below the scenes.
    current_y += SCENE_TOP_GAP;
    let banks_header_rect =
        Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
    if banks_header_rect.expand(4.0).intersects(clip_rect) {
        ui.painter_at(banks_header_rect).text(
            banks_header_rect.left_center(),
            Align2::LEFT_CENTER,
            "Question banks",
            FontId::new(
                coursecraft_render::SIZE_CAPTION,
                egui::FontFamily::Name("inter-bold".into()),
            ),
            p.text_secondary,
        );
        let plus_rect = Rect::from_min_size(
            pos2(banks_header_rect.max.x - 24.0, current_y),
            vec2(24.0, SCENE_HEADER_HEIGHT),
        );
        let plus_resp = ui.interact(plus_rect, ui.scope_id().with("bank_create"), Sense::click());
        ui.painter_at(plus_rect).text(
            plus_rect.center(),
            Align2::CENTER_CENTER,
            "+",
            FontId::new(
                coursecraft_render::SIZE_BODY,
                egui::FontFamily::Name("inter-bold".into()),
            ),
            p.accent_text,
        );
        if plus_resp.clicked() {
            action_to_run = Some(("bank.create", serde_json::json!({})));
        }
    }
    current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

    for (bank_idx, bank) in nav.banks.iter().enumerate() {
        let header_rect =
            Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
        let expanded = editor.ui.bank_expanded.contains(&bank.id);
        let renaming = editor
            .ui
            .bank_rename
            .as_ref()
            .is_some_and(|rename| rename.scene == bank.id);
        if header_rect.expand(4.0).intersects(clip_rect) || renaming {
            let header_resp = ui.interact(
                header_rect,
                ui.scope_id().with("bank_header").with(&bank.id),
                Sense::click(),
            );
            if header_resp.clicked() && !header_resp.double_clicked() {
                if expanded {
                    editor.ui.bank_expanded.retain(|id| id != &bank.id);
                } else {
                    editor.ui.bank_expanded.push(bank.id.clone());
                }
            }
            if header_resp.double_clicked() {
                bank_to_rename = Some(bank.id.clone());
            }
            let draw_params = draw_insert_params(editor, &bank.id);
            header_resp.context_menu(|ui| {
                show_bank_header_menu(ui, &bank.id, draw_params, &mut action_to_run);
            });

            if renaming {
                show_bank_rename_editor(editor, ui, header_rect);
            } else {
                // Disclosure triangle (drawn, so no icon-font glyph is needed).
                let tri_cx = thumb_x + 6.0;
                let tri_cy = header_rect.center().y;
                let tri = if expanded {
                    [
                        pos2(tri_cx - 5.0, tri_cy - 3.0),
                        pos2(tri_cx + 5.0, tri_cy - 3.0),
                        pos2(tri_cx, tri_cy + 4.0),
                    ]
                } else {
                    [
                        pos2(tri_cx - 3.0, tri_cy - 5.0),
                        pos2(tri_cx - 3.0, tri_cy + 5.0),
                        pos2(tri_cx + 4.0, tri_cy),
                    ]
                };
                let tri_painter = ui.painter();
                tri_painter.line_segment([tri[0], tri[1]], Stroke::new(1.5, p.text_secondary));
                tri_painter.line_segment([tri[1], tri[2]], Stroke::new(1.5, p.text_secondary));
                tri_painter.line_segment([tri[2], tri[0]], Stroke::new(1.5, p.text_secondary));
                let title_painter = ui.painter_at(header_rect);
                title_painter.text(
                    pos2(thumb_x + 16.0, header_rect.center().y),
                    Align2::LEFT_CENTER,
                    &bank.title,
                    FontId::new(
                        coursecraft_render::SIZE_CAPTION,
                        egui::FontFamily::Name("inter-bold".into()),
                    ),
                    p.text_secondary,
                );
                title_painter.text(
                    header_rect.right_center(),
                    Align2::RIGHT_CENTER,
                    format!("{} questions", bank.slides.len()),
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter".into()),
                    ),
                    p.text_tertiary,
                );
            }
        }
        current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

        if expanded {
            for (bank_slide_idx, slide) in bank.slides.iter().enumerate() {
                let thumb_rect =
                    Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, thumb_h));
                let title_rect = Rect::from_min_size(
                    pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                    vec2(thumb_w, TITLE_HEIGHT),
                );
                let hit_rect = Rect::from_min_max(thumb_rect.min, title_rect.max);
                let is_selected = editor.selection.slide == slide.id;
                if scroll_target == slide.id {
                    selected_rect = Some(hit_rect);
                }

                if !hit_rect.expand(4.0).intersects(clip_rect) {
                    current_y += row_height;
                    continue;
                }

                if thumb_rect.intersects(clip_rect)
                    && let Some(slide_ref) = editor
                        .session
                        .course
                        .banks
                        .get(bank_idx)
                        .and_then(|b| b.slides.get(bank_slide_idx))
                {
                    let (thumb_bg, items) = editor.ui.thumbnail_cache.get_or_compose(
                        revision,
                        editor.session.last_changed_slide(),
                        &editor.session.course,
                        slide_ref,
                    );
                    let painter = ui.painter_at(thumb_rect);
                    painter.rect_filled(
                        thumb_rect,
                        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                        thumb_bg,
                    );
                    if let Some(t) =
                        SlideTransform::fit(thumb_rect, editor.session.course.slide_size)
                    {
                        let source = crate::MemoryMediaSource::from_store(&editor.media);
                        paint(
                            &painter,
                            &t,
                            &items,
                            Highlight::default(),
                            Some((&mut editor.media_cache, &source)),
                            Some(&editor.session.course.theme),
                            Some(editor.session.course.slide_background(slide_ref)),
                        );
                    }
                }

                let row_resp = ui.interact(
                    hit_rect,
                    ui.scope_id().with("bank_slide").with(&slide.id),
                    Sense::click(),
                );
                if row_resp.clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    slide_to_select = Some(SlideClick::Single(slide.id.clone()));
                }
                if row_resp.double_clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    slide_to_pin = Some(slide.id.clone());
                }
                row_resp.context_menu(|ui| {
                    if ui.button("Delete").clicked() {
                        action_to_run = Some((
                            "bank.remove_slide",
                            serde_json::json!({ "slide": slide.id }),
                        ));
                        ui.close();
                    }
                });

                let thumb_radius = CornerRadius::same(coursecraft_render::RADIUS_MD as u8);
                let border = if row_resp.hovered() && !is_selected {
                    p.border_strong
                } else {
                    p.border
                };
                ui.painter().rect_stroke(
                    thumb_rect,
                    thumb_radius,
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

                let title_color = if is_selected {
                    p.text
                } else {
                    p.text_secondary
                };
                ui.painter_at(title_rect).text(
                    title_rect.left_top(),
                    Align2::LEFT_TOP,
                    &slide.title,
                    FontId::new(
                        coursecraft_render::SIZE_SMALL,
                        egui::FontFamily::Name("inter".into()),
                    ),
                    title_color,
                );

                current_y += row_height;
            }
        }
    }

    handle_slide_drag(editor, ui, &slide_rows, panel_rect);

    let total_height = current_y - panel_rect.min.y;
    ui.allocate_space(vec2(panel_rect.width(), total_height));

    // Blank space below the last slide: the usual way to add or paste.
    show_slides_empty_area(editor, ui, panel_rect, total_height);

    if editor.ui.slides_scrolled_to.as_deref() != Some(scroll_target.as_str())
        && let Some(r) = selected_rect
    {
        let clip = ui.clip_rect();
        // A thumbnail taller than the panel can never show whole: its top edge is enough.
        let fits = r.min.y >= clip.min.y - 1.0
            && (r.max.y <= clip.max.y + 1.0
                || (r.height() > clip.height() && r.min.y <= clip.max.y));
        // The list moves while it scrolls; a list that stands still has nowhere left to go.
        let gap = r.min.y - clip.min.y;
        let moving = !editor.ui.slides_scroll_gap.is_finite()
            || (gap - editor.ui.slides_scroll_gap).abs() > 0.5;
        editor.ui.slides_scroll_gap = gap;
        if moving {
            editor.ui.slides_scroll_stalled = 0;
        } else {
            editor.ui.slides_scroll_stalled += 1;
        }
        if fits || editor.ui.slides_scroll_stalled >= MAX_SCROLL_STALLED {
            editor.ui.slides_scrolled_to = Some(scroll_target.clone());
            editor.ui.slides_scroll_stalled = 0;
            editor.ui.slides_scroll_gap = f32::NAN;
        } else {
            ui.scroll_to_rect(r.expand2(vec2(0.0, 12.0)), None);
            ui.ctx().request_repaint();
        }
    }

    if let Some(click) = slide_to_select {
        editor.ui.slides_cursor = None;
        match click {
            SlideClick::Single(sid) => {
                editor.selection.slide = sid;
                editor.selection.also_slides.clear();
            }
            SlideClick::Toggle(sid) => {
                editor.selection.toggle_slide(&sid);
            }
            SlideClick::Range(sid) => {
                editor
                    .selection
                    .select_slide_range(&editor.session.course, &sid);
            }
        }
        editor.selection.layer = None;
        editor.selection.clear_objects();
    }

    if let Some(id) = slide_to_pin {
        crate::doc_tabs::open_slide(editor, &id);
        crate::doc_tabs::pin(editor, &id);
    }

    if let Some(id) = slide_to_rename {
        start_slide_rename(editor, &id);
    }
    if let Some(id) = scene_to_rename {
        start_scene_rename(editor, &id);
    }
    if let Some(id) = bank_to_rename {
        start_bank_rename(editor, &id);
    }
    if let Some(id) = scene_to_delete {
        editor.ui.pending_delete_scene = Some(id);
    }

    if let Some(action) = slide_action {
        editor.ui.last_active_area = crate::ActiveArea::Navigator;
        match action {
            SlideMenuAction::Copy(id) => {
                select_slide_for_menu(editor, id);
                crate::clipboard::copy(editor);
            }
            SlideMenuAction::Cut(id) => {
                select_slide_for_menu(editor, id);
                crate::clipboard::cut(editor);
            }
            SlideMenuAction::DuplicateSelected => editor.duplicate_selected_slides(),
            SlideMenuAction::MoveSelected { up } => editor.move_selected_slides(up),
            SlideMenuAction::Paste => crate::clipboard::paste_private(editor),
            SlideMenuAction::Rename(id) => start_slide_rename(editor, &id),
            SlideMenuAction::DeleteSelected => editor.delete_selected_slides(),
            SlideMenuAction::PreviewSelected => editor.open_preview_selected(),
        }
    }

    if !ui.ctx().egui_wants_keyboard_input()
        && editor.ui.last_active_area == crate::ActiveArea::Navigator
        && !editor.ui.master_view
        && !editor.ui.story_view
        && editor.ui.slide_rename.is_none()
        && editor.ui.scene_rename.is_none()
        && editor.ui.bank_rename.is_none()
        // Question bank rows have no rename field: F2 there would leave `slide_rename` set
        // with nothing to clear it (`N105`).
        && editor
            .session
            .course
            .slides()
            .any(|slide| slide.id == editor.selection.slide)
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F2))
    {
        let sid = editor.selection.slide.clone();
        start_slide_rename(editor, &sid);
    }

    if let Some((cmd, params)) = action_to_run
        && let Some(result) = editor.run(cmd, params)
        && cmd == "bank.create"
        && let Some(id) = result.get("id").and_then(Json::as_str)
    {
        editor.ui.bank_expanded.push(id.to_owned());
    }
}

/// `draw.insert` parameters for a draw of one question after the selected slide.
fn draw_insert_params(editor: &Editor, bank_id: &str) -> Json {
    let mut params = serde_json::json!({ "bank": bank_id, "count": 1 });
    for scene in &editor.session.course.scenes {
        if let Some(index) = scene
            .slides
            .iter()
            .position(|slide| slide.id == editor.selection.slide)
        {
            params["scene"] = serde_json::json!(scene.id);
            params["index"] = serde_json::json!(index + 1);
            break;
        }
    }
    params
}

/// Context menu for a question bank header.
fn show_bank_header_menu(
    ui: &mut Ui,
    bank_id: &str,
    draw_params: Json,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    if ui.button("Add question").clicked() {
        *action_to_run = Some(("bank.add_question", serde_json::json!({ "bank": bank_id })));
        ui.close();
    }
    if ui.button("Draw from this bank").clicked() {
        *action_to_run = Some(("draw.insert", draw_params));
        ui.close();
    }
    if ui.button("Delete").clicked() {
        *action_to_run = Some(("bank.delete", serde_json::json!({ "bank": bank_id })));
        ui.close();
    }
}

fn start_bank_rename(editor: &mut Editor, bank_id: &str) {
    let title = editor
        .session
        .course
        .bank(bank_id)
        .map(|bank| bank.title.clone())
        .unwrap_or_default();
    editor.ui.bank_rename = Some(SceneRename {
        scene: bank_id.to_owned(),
        buffer: title.clone(),
        original: title,
        just_entered: true,
    });
}

fn show_bank_rename_editor(editor: &mut Editor, ui: &mut Ui, rect: Rect) {
    let Some(rename) = editor.ui.bank_rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("bank_rename", &rename.scene));
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
        if let Some(state) = editor.ui.bank_rename.as_mut() {
            state.just_entered = false;
        }
    }

    if let Some(state) = editor.ui.bank_rename.as_mut() {
        state.buffer = buffer.clone();
    }

    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    let clicked_outside = ui.input(|i| {
        i.pointer.primary_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !rect.contains(pos))
    });

    if escaped {
        editor.ui.bank_rename = None;
    } else if enter || lost || clicked_outside {
        let title = buffer.trim().to_owned();
        if !title.is_empty() && title != rename.original {
            editor.run(
                "bank.update",
                serde_json::json!({ "bank": rename.scene, "title": title }),
            );
        }
        editor.ui.bank_rename = None;
    }
}

fn start_scene_rename(editor: &mut Editor, scene_id: &str) {
    let title = editor
        .session
        .course
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
        .map(|scene| scene.title.clone())
        .unwrap_or_default();
    editor.ui.scene_rename = Some(SceneRename {
        scene: scene_id.to_owned(),
        buffer: title.clone(),
        original: title,
        just_entered: true,
    });
}

fn show_scene_rename_editor(editor: &mut Editor, ui: &mut Ui, rect: Rect) {
    let Some(rename) = editor.ui.scene_rename.clone() else {
        return;
    };
    let mut buffer = rename.buffer.clone();
    let edit_id = Id::unique(("scene_rename", &rename.scene));
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
        if let Some(state) = editor.ui.scene_rename.as_mut() {
            state.just_entered = false;
        }
    }

    if let Some(state) = editor.ui.scene_rename.as_mut() {
        state.buffer = buffer.clone();
    }

    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let escaped = ui.input(|i| i.key_pressed(Key::Escape));
    let lost = output.response.lost_focus();
    let clicked_outside = ui.input(|i| {
        i.pointer.primary_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !rect.contains(pos))
    });

    if escaped {
        editor.ui.scene_rename = None;
    } else if enter || lost || clicked_outside {
        let title = buffer.trim().to_owned();
        if !title.is_empty() && title != rename.original {
            editor.run(
                "scene.update",
                serde_json::json!({ "scene": rename.scene, "title": title }),
            );
        }
        editor.ui.scene_rename = None;
    }
}

/// Small confirmation dialog shown before deleting a scene that holds slides.
pub fn show_scene_delete_dialog(editor: &mut Editor, ctx: &egui::Context) {
    let Some(scene_id) = editor.ui.pending_delete_scene.clone() else {
        return;
    };
    let Some(scene) = editor
        .session
        .course
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
    else {
        editor.ui.pending_delete_scene = None;
        return;
    };
    let slide_count = scene.slides.len();
    let title = scene.title.clone();
    let mut close = false;
    let mut delete = false;
    let mut open = true;
    Dialog::new("Delete scene").open(&mut open).show(ctx, |ui| {
        if slide_count == 0 {
            ui.label(format!("Delete the scene \"{title}\"?"));
        } else {
            ui.label(format!(
                "Delete the scene \"{title}\"? {slide_count} slide(s) will be deleted."
            ));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Delete scene").clicked() {
                delete = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if !open {
        close = true;
    }
    if delete {
        editor.run("scene.delete", serde_json::json!({ "scene": scene_id }));
        editor.ui.pending_delete_scene = None;
    } else if close {
        editor.ui.pending_delete_scene = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CACHED_THUMBNAILS, slide_thumbnail_rect};
    use crate::guard_counters as gc;
    use crate::test_harness::TestHarness;
    use coursecraft_course::Course;
    use egui::{Event, Modifiers, MouseWheelUnit, TouchPhase, pos2, vec2};

    fn slides_course(n: usize) -> Course {
        let mut slides = Vec::with_capacity(n);
        for i in 0..n {
            slides.push(format!(
                r##"{{"id":"s-{i}","title":"Slide {i}","objects":[{{"id":"t{i}","kind":{{"type":"text","text":"Slide {i}"}},"frame":{{"x":80,"y":80,"w":400,"h":80}}}}]}}"##
            ));
        }
        let json = format!(
            r#"{{"schema_version":1,"id":"slides-{n}","title":"Slides","slide_size":{{"w":1280,"h":720}},"scenes":[{{"id":"sc0","title":"Scene 0","slides":[{}]}}]}}"#,
            slides.join(",")
        );
        Course::from_json(&json).expect("generated slides course parses")
    }

    fn wheel(dy: f32) -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, dy),
            phase: TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        }
    }

    /// SL-05: ThumbnailCache stays capped at 128 in Standard mode (and 32 in Lite mode)
    /// across a 300-slide scroll, empties on `replace_course`, keeps off-screen
    /// `scroll_to_rect` working, and preserves slide drag-reorder.
    #[test]
    fn thumbnail_cache_cap_reset_scroll_to_rect_and_drag_reorder() {
        let mut h = TestHarness::new(slides_course(300));
        // 300 slides triggers Auto -> Lite (32); explicitly test Standard (128) here.
        h.editor
            .set_performance_mode(coursecraft_render::PerformanceMode::Standard);
        for _ in 0..3 {
            let _ = h.idle_frame();
        }
        assert!(
            h.editor.ui.thumbnail_cache.len() > 0
                && h.editor.ui.thumbnail_cache.len() <= MAX_CACHED_THUMBNAILS,
            "initial visible slides should populate bounded cache"
        );

        // Idle frames after settling perform zero thumbnail composes.
        gc::reset();
        for _ in 0..5 {
            let _ = h.idle_frame();
        }
        assert_eq!(
            gc::thumbnail_composes(),
            0,
            "settled idle frames should not compose thumbnails"
        );

        // (a) Scroll across all 300 slides and verify the cache never exceeds 128 entries.
        let list_pos = h.find_text("Slide 0").unwrap_or_else(|| pos2(90.0, 200.0));
        h.frame(vec![Event::PointerMoved(list_pos)]);
        for _ in 0..80 {
            h.frame(vec![wheel(-600.0)]);
            assert!(
                h.editor.ui.thumbnail_cache.len() <= MAX_CACHED_THUMBNAILS,
                "cache exceeded cap: {}",
                h.editor.ui.thumbnail_cache.len()
            );
        }
        assert_eq!(
            h.editor.ui.thumbnail_cache.len(),
            MAX_CACHED_THUMBNAILS,
            "scrolling through 300 slides should fill cache up to the 128-entry cap"
        );

        // Switching to Lite (or Auto on 300 slides) shrinks the cache to its 32-entry ceiling.
        h.editor
            .set_performance_mode(coursecraft_render::PerformanceMode::Lite);
        assert_eq!(
            h.editor.ui.thumbnail_cache.len(),
            32,
            "Lite mode must evict thumbnail cache down to 32 entries"
        );
        h.editor
            .set_performance_mode(coursecraft_render::PerformanceMode::Standard);

        // (b) `replace_course` empties the thumbnail cache.
        h.editor.replace_course(slides_course(300));
        assert_eq!(
            h.editor.ui.thumbnail_cache.len(),
            0,
            "replace_course must clear thumbnail_cache"
        );
        // Scrolling back to s-0 is animated; give it time to settle.
        for _ in 0..90 {
            let _ = h.idle_frame();
            if h.editor.ui.slides_scrolled_to.as_deref() == Some("s-0") {
                break;
            }
        }
        assert!(
            h.editor.ui.slides_scroll_y < 100.0 && h.find_text("Slide 0").is_some(),
            "fresh course should scroll back near the top and paint Slide 0, got y={}",
            h.editor.ui.slides_scroll_y
        );

        // (c) Selecting off-screen slide "s-250" computes selected_rect and scrolls it into view.
        h.editor.selection.slide = "s-250".into();
        for _ in 0..90 {
            let _ = h.idle_frame();
            if h.editor.ui.slides_scrolled_to.as_deref() == Some("s-250") {
                break;
            }
        }
        assert!(
            h.editor.ui.slides_scroll_y > 1000.0,
            "selecting off-screen s-250 should scroll the list down, got y={}",
            h.editor.ui.slides_scroll_y
        );
        assert_eq!(
            h.editor.ui.slides_scrolled_to.as_deref(),
            Some("s-250"),
            "s-250 should settle into the scroll viewport"
        );
        assert!(
            h.find_text("Slide 250").is_some(),
            "Slide 250 should be painted once scrolled into view"
        );

        // (d) Dragging slide 0 past slide 2 reorders slide 0 to index 2.
        let mut h_drag = TestHarness::new(slides_course(10));
        for _ in 0..3 {
            let _ = h_drag.idle_frame();
        }
        let left = h_drag.left_panel_rect();
        let r0 =
            slide_thumbnail_rect(&h_drag.editor.session.course, left, "s-0").expect("s-0 rect");
        let r2 =
            slide_thumbnail_rect(&h_drag.editor.session.course, left, "s-2").expect("s-2 rect");
        let delta = vec2(0.0, (r2.center().y + 20.0) - r0.center().y);
        h_drag.drag_by(r0.center(), delta, 6, Modifiers::NONE);
        let _ = h_drag.idle_frame();
        let order: Vec<&str> = h_drag.editor.session.course.scenes[0]
            .slides
            .iter()
            .take(3)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            order,
            vec!["s-1", "s-2", "s-0"],
            "dragging slide 0 past slide 2 should reorder it cleanly"
        );
    }
}
