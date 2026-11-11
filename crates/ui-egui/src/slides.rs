//! Slides left panel: scenes, slides list, thumbnails, and reordering.

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::Course;
use coursecraft_render::{Highlight, SlideTransform, compose_authoring, paint};
use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, Rect, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, Ui, pos2, vec2,
};
use serde_json::Value as Json;

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
                let caret_w = 22.0;
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
                let caret = ui.add(
                    egui::Button::new(
                        coursecraft_render::icon(
                            coursecraft_render::Icon::CaretDown,
                            coursecraft_render::ICON_SM,
                        )
                        .color(p.text_secondary),
                    )
                    .frame(false)
                    .min_size(vec2(caret_w, coursecraft_render::CONTROL_COMPACT)),
                );
                egui::Popup::menu(&caret).show(|ui| {
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
                "Close Master View",
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

            let masters = editor.session.course.masters.clone();
            if masters.is_empty() {
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

            for master in &masters {
                let thumb_rect = Rect::from_min_size(pos2(thumb_x, y), vec2(thumb_w, thumb_h));
                let is_selected = editor.selection.slide == master.id;

                let items = compose_authoring(&editor.session.course, master, None);
                let thumb_bg =
                    coursecraft_render::parse_color(editor.session.course.slide_background(master))
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
                        Some(editor.session.course.slide_background(master)),
                    );
                }

                let resp = ui.interact(
                    thumb_rect,
                    ui.scope_id().with("master_thumb").with(&master.id),
                    Sense::click(),
                );
                if resp.clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    select = Some(master.id.clone());
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
                let more_id = ui.scope_id().with("master_more").with(&master.id);
                let is_menu_open = egui::Popup::is_id_open(ui.ctx(), more_id);
                if resp.hovered() || is_selected || is_menu_open {
                    ui.painter().rect_filled(
                        more_btn_rect,
                        CornerRadius::same(coursecraft_render::RADIUS_MD as u8),
                        p.raised,
                    );
                    let more = ui.interact(more_btn_rect, more_id, Sense::click());
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
                            rename = Some(master.id.clone());
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(master.id.clone());
                            ui.close();
                        }
                    });
                }

                // Title below the thumbnail, or an in-place rename field.
                let title_rect = Rect::from_min_size(
                    pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                    vec2(thumb_w, TITLE_HEIGHT),
                );
                let renaming = editor
                    .ui
                    .master_rename
                    .as_ref()
                    .is_some_and(|state| state.scene == master.id);
                if renaming {
                    show_master_rename_editor(editor, ui, title_rect);
                } else {
                    ui.painter_at(title_rect).text(
                        title_rect.left_top(),
                        Align2::LEFT_TOP,
                        &master.title,
                        FontId::new(
                            coursecraft_render::SIZE_SMALL,
                            egui::FontFamily::Name("inter".into()),
                        ),
                        if is_selected {
                            p.text
                        } else {
                            p.text_secondary
                        },
                    );
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

fn start_master_rename(editor: &mut Editor, master_id: &str) {
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

fn slides_using_master(course: &Course, master_id: &str) -> usize {
    course
        .slides()
        .filter(|slide| slide.master.as_deref() == Some(master_id))
        .count()
}

fn request_master_delete(editor: &mut Editor, master_id: &str) {
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
    Dialog::new("Delete master").show(ctx, |ui| {
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
                        if crate::quiet_button(ui, "Remove", crate::QuietTone::Danger, true)
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
    can_paste: bool,
    has_question: bool,
    banks: &[(String, String)],
    action_to_run: &mut Option<(&'static str, Json)>,
    slide_action: &mut Option<SlideMenuAction>,
) {
    let new_after = egui::Button::new("New slide after")
        .shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+M"));
    if ui.add(new_after).clicked() {
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
    if ui.button("Duplicate slide").clicked() {
        *action_to_run = Some(("slide.duplicate", serde_json::json!({ "slide": slide_id })));
        ui.close();
    }
    ui.separator();

    let can_cut = total_slides > 1;
    let cut =
        egui::Button::new("Cut").shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+X"));
    if ui
        .add_enabled(can_cut, cut)
        .on_disabled_hover_text("A course needs at least one slide")
        .clicked()
    {
        *slide_action = Some(SlideMenuAction::Cut(slide_id.to_owned()));
        ui.close();
    }
    let copy =
        egui::Button::new("Copy").shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+C"));
    if ui.add(copy).clicked() {
        *slide_action = Some(SlideMenuAction::Copy(slide_id.to_owned()));
        ui.close();
    }
    let paste =
        egui::Button::new("Paste").shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+V"));
    if ui
        .add_enabled(can_paste, paste)
        .on_disabled_hover_text("Nothing copied yet")
        .clicked()
    {
        *slide_action = Some(SlideMenuAction::Paste);
        ui.close();
    }
    ui.separator();

    let can_up = slide_idx > 0;
    if ui
        .add_enabled(can_up, egui::Button::new("Move up"))
        .clicked()
    {
        *action_to_run = Some((
            "slide.move",
            serde_json::json!({
                "slide": slide_id,
                "scene": scene_id,
                "index": slide_idx.saturating_sub(1),
            }),
        ));
        ui.close();
    }
    let can_down = slide_idx + 1 < total_in_scene;
    if ui
        .add_enabled(can_down, egui::Button::new("Move down"))
        .clicked()
    {
        *action_to_run = Some((
            "slide.move",
            serde_json::json!({
                "slide": slide_id,
                "scene": scene_id,
                "index": slide_idx + 1,
            }),
        ));
        ui.close();
    }
    ui.separator();

    if ui.button("Save as Template").clicked() {
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

    let can_delete = total_slides > 1;
    if ui
        .add_enabled(can_delete, egui::Button::new("Delete"))
        .clicked()
    {
        *action_to_run = Some((
            "slide.delete",
            serde_json::json!({
                "slide": slide_id,
            }),
        ));
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
    let new_slide = egui::Button::new("New slide")
        .shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+M"));
    if ui.add(new_slide).clicked() {
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
    let paste =
        egui::Button::new("Paste").shortcut_text(crate::menu::shortcut_label(ui.ctx(), "Ctrl+V"));
    if ui
        .add_enabled(can_paste, paste)
        .on_disabled_hover_text("Nothing copied yet")
        .clicked()
    {
        crate::clipboard::paste_private(editor);
        ui.close();
    }
}

#[allow(clippy::cognitive_complexity)]
fn show_slides_content(editor: &mut Editor, ui: &mut Ui, panel_rect: Rect) {
    let mut selected_rect: Option<Rect> = None;
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let mut slide_action: Option<SlideMenuAction> = None;
    let mut slide_to_select: Option<String> = None;
    let mut scene_to_rename: Option<String> = None;
    let mut scene_to_delete: Option<String> = None;
    let mut bank_to_rename: Option<String> = None;
    // Every slide row laid out this frame, for slide drag-and-drop.
    let mut slide_rows: Vec<(String, String, usize, Rect)> = Vec::new();

    let p = coursecraft_render::palette(ui.ctx());
    let mode = coursecraft_render::theme_mode(ui.ctx());
    let thumb_w = (panel_rect.width() - GUTTER_WIDTH - coursecraft_render::PANEL_MARGIN).max(64.0);
    let thumb_h = (thumb_w * 9.0 / 16.0).round();
    let row_height = thumb_h + TITLE_GAP + TITLE_HEIGHT + ROW_GAP;
    let thumb_x = panel_rect.min.x + GUTTER_WIDTH;

    let total_course_slides = editor.session.course.slides().count();
    let scene_count = editor.session.course.scenes.len();
    // Clone the scenes so the header can start a rename or a delete while the
    // loop still needs the course for thumbnails.
    let scenes = editor.session.course.scenes.clone();
    // Question banks for the "Move to bank" submenu.
    let bank_menu: Vec<(String, String)> = editor
        .session
        .course
        .banks
        .iter()
        .map(|bank| (bank.id.clone(), bank.title.clone()))
        .collect();
    let mut current_y = panel_rect.min.y;
    let mut global_slide_idx: usize = 0;

    for (scene_idx, scene) in scenes.iter().enumerate() {
        if scene_idx == 0 {
            current_y += SCENE_FIRST_TOP_GAP;
        } else {
            current_y += SCENE_TOP_GAP;
        }

        // Scene caption header: double-click renames, right-click gives rename/delete.
        let header_rect =
            Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
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

        let renaming = editor
            .ui
            .scene_rename
            .as_ref()
            .is_some_and(|rename| rename.scene == scene.id);
        if renaming {
            show_scene_rename_editor(editor, ui, header_rect);
        } else {
            let painter = ui.painter_at(header_rect);
            painter.text(
                header_rect.left_center(),
                Align2::LEFT_CENTER,
                &scene.title,
                FontId::new(
                    coursecraft_render::SIZE_CAPTION,
                    egui::FontFamily::Name("inter-bold".into()),
                ),
                p.text_secondary,
            );
        }
        current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

        let total_in_scene = scene.slides.len();
        for (slide_idx, slide) in scene.slides.iter().enumerate() {
            global_slide_idx += 1;
            let thumb_rect = Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, thumb_h));
            let is_selected = editor.selection.slide == slide.id;
            if is_selected {
                selected_rect = Some(Rect::from_min_size(
                    thumb_rect.min,
                    vec2(thumb_w, thumb_h + TITLE_GAP + TITLE_HEIGHT),
                ));
            }

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

            // Thumbnail background & items
            let items = compose_authoring(&editor.session.course, slide, None);
            let thumb_bg =
                coursecraft_render::parse_color(editor.session.course.slide_background(slide))
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
            slide_rows.push((scene.id.clone(), slide.id.clone(), slide_idx, thumb_rect));

            // Thumbnail click, drag and context menu interaction
            let thumb_response =
                ui.interact(thumb_rect, ui.scope_id().with(&slide.id), Sense::click());
            if editor.ui.slide_drag.is_none()
                && ui.input(|i| i.pointer.primary_pressed())
                && thumb_response.contains_pointer()
            {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
                slide_to_select = Some(slide.id.clone());
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
            if thumb_response.clicked() {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
                slide_to_select = Some(slide.id.clone());
            }
            if thumb_response.secondary_clicked() || thumb_response.has_focus() {
                editor.ui.last_active_area = crate::ActiveArea::Navigator;
            }

            let is_hovered = thumb_response.hovered();

            // Right-clicking thumbnail opens context menu
            let can_paste = editor.ui.clipboard.is_some();
            thumb_response.context_menu(|ui| {
                show_slide_menu_items(
                    ui,
                    &slide.id,
                    &scene.id,
                    slide_idx,
                    total_in_scene,
                    total_course_slides,
                    can_paste,
                    slide.question.is_some(),
                    &bank_menu,
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
            let more_btn_size = vec2(24.0, 24.0);
            let more_btn_rect = Rect::from_min_size(
                pos2(thumb_rect.max.x - 24.0 - 4.0, thumb_rect.min.y + 4.0),
                more_btn_size,
            );
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

                let more_resp = ui.interact(more_btn_rect, more_id, Sense::click());
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
                        can_paste,
                        slide.question.is_some(),
                        &bank_menu,
                        &mut action_to_run,
                        &mut slide_action,
                    );
                });
            }

            // Slide title below thumbnail
            let title_rect = Rect::from_min_size(
                pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                vec2(thumb_w, TITLE_HEIGHT),
            );
            let title_color = if is_selected {
                p.text
            } else {
                p.text_secondary
            };
            let painter = ui.painter_at(title_rect);
            painter.text(
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

    // Question banks, below the scenes.
    current_y += SCENE_TOP_GAP;
    let banks_header_rect =
        Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
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
    current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

    let banks = editor.session.course.banks.clone();
    for bank in &banks {
        let header_rect =
            Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, SCENE_HEADER_HEIGHT));
        let expanded = editor.ui.bank_expanded.contains(&bank.id);
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

        let renaming = editor
            .ui
            .bank_rename
            .as_ref()
            .is_some_and(|rename| rename.scene == bank.id);
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
        current_y += SCENE_HEADER_HEIGHT + SCENE_BOTTOM_GAP;

        if expanded {
            for slide in &bank.slides {
                let thumb_rect =
                    Rect::from_min_size(pos2(thumb_x, current_y), vec2(thumb_w, thumb_h));
                let title_rect = Rect::from_min_size(
                    pos2(thumb_x, thumb_rect.max.y + TITLE_GAP),
                    vec2(thumb_w, TITLE_HEIGHT),
                );
                let hit_rect = Rect::from_min_max(thumb_rect.min, title_rect.max);
                let is_selected = editor.selection.slide == slide.id;
                if is_selected {
                    selected_rect = Some(hit_rect);
                }

                let items = compose_authoring(&editor.session.course, slide, None);
                let thumb_bg =
                    coursecraft_render::parse_color(editor.session.course.slide_background(slide))
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

                let row_resp = ui.interact(
                    hit_rect,
                    ui.scope_id().with("bank_slide").with(&slide.id),
                    Sense::click(),
                );
                if row_resp.clicked() {
                    editor.ui.last_active_area = crate::ActiveArea::Navigator;
                    slide_to_select = Some(slide.id.clone());
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

    if editor.ui.slides_scrolled_to.as_deref() != Some(editor.selection.slide.as_str())
        && let Some(r) = selected_rect
    {
        if ui.clip_rect().contains_rect(r) {
            editor.ui.slides_scrolled_to = Some(editor.selection.slide.clone());
        } else {
            ui.scroll_to_rect(r.expand2(vec2(0.0, 12.0)), None);
            ui.ctx().request_repaint();
        }
    }

    if let Some(sid) = slide_to_select {
        editor.selection.slide = sid;
        editor.selection.layer = None;
        editor.selection.clear_objects();
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
                editor.selection.slide = id;
                editor.selection.layer = None;
                editor.selection.clear_objects();
                crate::clipboard::copy(editor);
            }
            SlideMenuAction::Cut(id) => {
                editor.selection.slide = id;
                editor.selection.layer = None;
                editor.selection.clear_objects();
                crate::clipboard::cut(editor);
            }
            SlideMenuAction::Paste => crate::clipboard::paste_private(editor),
        }
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
    Dialog::new("Delete scene").show(ctx, |ui| {
        if slide_count == 0 {
            ui.label(format!("Delete the scene \"{title}\"?"));
        } else {
            ui.label(format!(
                "Delete the scene \"{title}\"? {slide_count} slide(s) will be deleted."
            ));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(true, egui::Button::new("Delete scene"))
                .clicked()
            {
                delete = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if delete {
        editor.run("scene.delete", serde_json::json!({ "scene": scene_id }));
        editor.ui.pending_delete_scene = None;
    } else if close {
        editor.ui.pending_delete_scene = None;
    }
}
