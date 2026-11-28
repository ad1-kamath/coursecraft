//! Keynote-style editor toolbar.
//!
//! Provides the 52 px toolbar panel below the menu row:
//! - Left: Add Slide (with caret dropdown for New slide/scene/layer) and Preview;
//! - Centre: Text, Shape (dropdown), Image, Media (dropdown), Button, Question, Trigger;
//! - Right: Undo, Redo, thin separator, and Inspector toggles (Format, Animate, Slide, Course).
//!
//! Shares actions with the menu bar and navigator.

use coursecraft_course::ShapeKind;
use coursecraft_render::Icon;
use egui::{Align, Layout, Rect, Ui, UiBuilder, pos2, vec2};

use crate::menu::tooltip_with_shortcut;
use crate::shortcuts::{ShortcutAction, shortcut_for_action, shortcut_for_command};
use crate::{DrawTool, Editor, EditorRequest, InspectorTab, PlacementTool, widgets};

pub const TOOLBAR_HEIGHT: f32 = 52.0;

// ----------------------------------------------------------------------------
// Shared editor actions
// ----------------------------------------------------------------------------

/// Name for the next slide added from the editor: `Slide {count + 1}`.
#[must_use]
pub fn next_slide_title(course: &coursecraft_course::Course) -> String {
    format!("Slide {}", course.slides().count().saturating_add(1))
}

/// Add a new slide to the current or first scene, after current selection if any.
pub fn add_new_slide(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let scene_id = editor
        .session
        .course
        .scenes
        .iter()
        .find(|sc| sc.slides.iter().any(|s| s.id == editor.selection.slide))
        .map(|sc| sc.id.clone())
        .or_else(|| editor.session.course.scenes.first().map(|s| s.id.clone()));
    if let Some(sc_id) = scene_id {
        let mut params = serde_json::json!({
            "scene": sc_id,
            "title": next_slide_title(&editor.session.course),
        });
        if !editor.selection.slide.is_empty() {
            params["after"] = serde_json::json!(editor.selection.slide);
        }
        editor.run("slide.add", params);
    }
}

/// Add a new scene to the course.
///
/// The new scene starts with one slide, which becomes the selection, so
/// New Slide (toolbar, Ctrl+M) then keeps adding to this scene. Scene and
/// slide are one undo step.
pub fn add_new_scene(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let course = &editor.session.course;
    let scene_id = first_free_id("scene", course.scenes.iter().map(|sc| sc.id.as_str()));
    let slide_id = first_free_id("slide", course.all_slide_ids());
    let title = next_slide_title(course);
    editor.run_batch(&[
        (
            "scene.add".to_owned(),
            serde_json::json!({ "id": scene_id }),
        ),
        (
            "slide.add".to_owned(),
            serde_json::json!({ "id": slide_id, "scene": scene_id, "title": title }),
        ),
    ]);
}

/// The first `prefix<N>` not in `taken`.
fn first_free_id<'a>(prefix: &str, taken: impl Iterator<Item = &'a str>) -> String {
    let taken: std::collections::BTreeSet<&str> = taken.collect();
    (1u64..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id.as_str()))
        .unwrap_or_else(|| prefix.to_owned())
}

/// Add a new layer to the currently selected slide.
pub fn add_new_layer(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    if !editor.selection.slide.is_empty() {
        editor.run(
            "layer.add",
            serde_json::json!({ "slide": editor.selection.slide }),
        );
    }
}

/// Arm the text box placement tool.
pub fn insert_text_box(editor: &mut Editor) {
    editor.ui.placement_tool = Some(PlacementTool::TextBox);
}

/// Insert a 3x3 table centred on the slide.
pub fn insert_table(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let size = editor.session.course.slide_size;
    let width = (size.w as f32 * 0.6).clamp(120.0, 900.0);
    let height = (size.h as f32 * 0.4).clamp(90.0, 500.0);
    let frame = coursecraft_course::Rect {
        x: ((size.w as f32 - width) * 0.5).max(0.0),
        y: ((size.h as f32 - height) * 0.5).max(0.0),
        w: width,
        h: height,
    };
    let columns = vec![1.0 / 3.0; 3];
    let rows = (0..3)
        .map(|_| coursecraft_course::TableRow {
            height: 1.0 / 3.0,
            cells: vec![coursecraft_course::TableCell::default(); 3],
        })
        .collect();
    let outline = coursecraft_course::Outline {
        color: coursecraft_course::Color("#1F1F1F".to_owned()),
        width: 1.0,
        dash: coursecraft_course::DashStyle::Solid,
        dash_pattern: None,
        join: coursecraft_course::LineJoin::Miter,
        miter_limit: None,
    };
    let kind = coursecraft_course::ObjectKind::Table {
        columns,
        rows,
        borders: coursecraft_course::TableBorders {
            outer: Some(outline.clone()),
            inner: Some(outline),
        },
        header_row: false,
    };
    let Ok(kind) = serde_json::to_value(kind) else {
        return;
    };
    let result = editor.run(
        "object.add",
        serde_json::json!({
            "slide": editor.selection.slide,
            "layer": editor.selection.layer,
            "kind": kind,
            "frame": frame,
        }),
    );
    if let Some(id) = result
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
    {
        editor.selection.set_many([id.to_owned()]);
        editor.ui.table_cell = Some((0, 0));
    }
}

/// Arm the button placement tool.
pub fn insert_button(editor: &mut Editor) {
    editor.ui.placement_tool = Some(PlacementTool::Button);
}

/// Arm the placement tool for a shape kind.
pub fn insert_shape(editor: &mut Editor, kind: ShapeKind) {
    editor.ui.placement_tool = Some(PlacementTool::Shape(kind));
    editor.ui.draw = None;
}

/// Arm a drawing tool.
pub fn insert_draw(editor: &mut Editor, tool: DrawTool) {
    editor.ui.placement_tool = Some(PlacementTool::Draw(tool));
    editor.ui.placement_drag = None;
    editor.ui.draw = None;
    editor.ui.path_edit = None;
}

/// Arm the rectangle placement tool.
pub fn insert_rectangle(editor: &mut Editor) {
    insert_shape(editor, ShapeKind::Rectangle);
}

/// Arm the rounded rectangle placement tool.
pub fn insert_rounded_rectangle(editor: &mut Editor) {
    insert_shape(editor, ShapeKind::RoundedRectangle);
}

/// Arm the ellipse placement tool.
pub fn insert_ellipse(editor: &mut Editor) {
    insert_shape(editor, ShapeKind::Ellipse);
}

/// Request image file picker for insertion.
pub fn insert_image(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    editor.pending_image_pick =
        editor
            .ui
            .editing_state
            .as_ref()
            .map(|session| crate::PendingImagePick::StateExtraImage {
                slide: session.slide_id.clone(),
                object: session.object_id.clone(),
                state: session.state_name.clone(),
            });
    requests.push(EditorRequest::PickImage);
}

/// Request video file picker for insertion.
pub fn insert_video(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    editor.pending_image_pick = None;
    requests.push(EditorRequest::PickVideo);
}

/// Request audio file picker for insertion.
pub fn insert_audio(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    editor.pending_image_pick = None;
    requests.push(EditorRequest::PickAudio);
}

/// Request Lottie file picker for insertion.
pub fn insert_lottie(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    editor.pending_image_pick = None;
    requests.push(EditorRequest::PickLottie);
}

/// Insert a question on the current slide and open the Question panel.
pub fn insert_question(editor: &mut Editor, kind: coursecraft_course::QuestionKind) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let slide_id = editor.selection.slide.clone();
    if slide_id.is_empty() {
        return;
    }
    // A draw slide gets its questions from a bank, never its own.
    if editor
        .session
        .course
        .slide(&slide_id)
        .is_some_and(|slide| slide.draw.is_some())
    {
        editor.set_status("This slide draws questions from a bank");
        return;
    }
    if editor
        .run(
            "question.insert",
            serde_json::json!({
                "slide": slide_id,
                "kind": kind,
            }),
        )
        .is_some()
    {
        editor.ui.bottom_tab = crate::BottomTab::Question;
    }
}

/// Insert a results slide after the selected slide and select it.
pub fn insert_results_slide(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let mut params = serde_json::json!({});
    if !editor.selection.slide.is_empty() {
        params["after"] = serde_json::json!(editor.selection.slide);
    }
    let Some(result) = editor.run("results.insert", params) else {
        return;
    };
    let Some(id) = result.get("slide").and_then(serde_json::Value::as_str) else {
        return;
    };
    editor.selection.slide = id.to_owned();
    editor.selection.layer = None;
    editor.selection.clear_objects();
}

/// Insert a certificate slide after the selected slide and select it.
pub fn insert_certificate_slide(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    let mut params = serde_json::json!({});
    if !editor.selection.slide.is_empty() {
        params["after"] = serde_json::json!(editor.selection.slide);
    }
    let Some(result) = editor.run("certificate.insert", params) else {
        return;
    };
    let Some(id) = result.get("slide").and_then(serde_json::Value::as_str) else {
        return;
    };
    editor.selection.slide = id.to_owned();
    editor.selection.layer = None;
    editor.selection.clear_objects();
}

/// Insert a trigger on the current slide/layer.
pub fn insert_trigger(editor: &mut Editor) {
    if editor.ui.master_view {
        editor.set_status("Not available in slide masters");
        return;
    }
    if let Some(slide) = editor.session.course.slide(&editor.selection.slide) {
        let slide_clone = slide.clone();
        let selected_layer = editor.selection.layer.clone();
        let default_target = crate::triggers::default_trigger_target(
            &editor.selection,
            &editor.session.course,
            &slide_clone,
        );
        editor.ui.trigger_form = Some(
            crate::triggers::TriggerFormState::new_for_owner(
                &slide_clone,
                selected_layer.as_deref(),
                &editor.session.course,
            )
            .with_selected_object(&slide_clone, default_target.as_deref()),
        );
    }
}

/// Open preview of the currently selected slide.
pub fn preview_slide(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let slide_id = editor.selection.slide.clone();
    editor.open_preview_at(Some(&slide_id));
    requests.push(EditorRequest::Preview { slide: slide_id });
}

/// A click on a Format Painter button: click arms it once, double-click keeps it
/// armed, a click while armed turns it off. Shared by the toolbar and the ribbon.
pub(crate) fn format_painter_clicked(editor: &mut Editor, painter_resp: &egui::Response) {
    painter_clicked(
        editor,
        painter_resp,
        crate::format_painter::PainterKind::Format,
    );
}

/// [`format_painter_clicked`] for either painter; a click on one while the other is armed
/// switches to it.
pub(crate) fn painter_clicked(
    editor: &mut Editor,
    painter_resp: &egui::Response,
    kind: crate::format_painter::PainterKind,
) {
    let arm = |editor: &mut Editor, sticky: bool| match kind {
        crate::format_painter::PainterKind::Format => editor.arm_format_painter(sticky),
        crate::format_painter::PainterKind::Animation => editor.arm_animation_painter(sticky),
    };
    if painter_resp.double_clicked() {
        arm(editor, true);
        if let Some(p) = editor.ui.format_painter.as_mut() {
            p.sticky = true;
            p.just_double_clicked = true;
        }
    } else if painter_resp.clicked() {
        if let Some(p) = editor.ui.format_painter.as_mut() {
            if p.kind != kind {
                arm(editor, false);
            } else if p.just_double_clicked {
                p.just_double_clicked = false;
            } else {
                editor.ui.format_painter = None;
            }
        } else {
            arm(editor, false);
        }
    }
}

// ----------------------------------------------------------------------------
// Toolbar display
// ----------------------------------------------------------------------------

/// Render the Keynote-style toolbar inside `ui`.
#[allow(clippy::cognitive_complexity)]
pub fn show_toolbar(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let max_rect = ui.max_rect();
    let center_x = max_rect.center().x;
    let label_threshold = if editor.file_ops_available {
        1220.0
    } else {
        1100.0
    };
    let show_label = ui.ctx().content_rect().width() >= label_threshold;
    widgets::set_toolbar_show_labels(ui.ctx(), show_label);

    // Center group width calculation
    let center_w = if show_label {
        // Text: 52, Shape: 52, Image: 52, Media: 52, Button: 52, Question: ~62, Trigger: 52
        // 6 * 52 + 62 = 374 + 6 gaps * 6 = 410
        414.0
    } else {
        // 7 * 38 = 266 + 6 gaps * 6 = 302
        302.0
    };

    let center_rect = Rect::from_center_size(
        pos2(center_x, max_rect.center().y),
        vec2(center_w, TOOLBAR_HEIGHT),
    );

    let left_rect = Rect::from_min_max(max_rect.min, pos2(center_rect.min.x - 8.0, max_rect.max.y));
    let right_rect =
        Rect::from_min_max(pos2(center_rect.max.x + 8.0, max_rect.min.y), max_rect.max);

    // 1. Left group: Add Slide (+ Caret menu) and Preview
    let mut left_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(left_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    left_ui.spacing_mut().item_spacing.x = 2.0;

    let master_view = editor.ui.master_view;

    let add_slide_tooltip = if master_view {
        "Not available in slide masters".to_owned()
    } else {
        tooltip_with_shortcut(
            left_ui.ctx(),
            "New slide",
            shortcut_for_command("slide.add"),
        )
    };
    let add_slide_resp = widgets::toolbar_button_enabled(
        &mut left_ui,
        Icon::Plus,
        "New slide",
        false,
        !master_view,
        &add_slide_tooltip,
    );
    if add_slide_resp.clicked() {
        add_new_slide(editor);
    }

    let caret_resp = widgets::toolbar_caret_button(&mut left_ui, "More slide options");
    egui::Popup::menu(&caret_resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            if master_view {
                let p = coursecraft_render::palette(ui.ctx());
                ui.label(
                    coursecraft_render::small("Not available in slide masters")
                        .color(p.text_disabled),
                );
                return;
            }
            crate::slides::show_new_slide_popup(ui, editor);
            ui.separator();
            if ui.button("New scene").clicked() {
                add_new_scene(editor);
                ui.close();
            }
            let can_add_layer = !editor.selection.slide.is_empty();
            if ui
                .add_enabled(can_add_layer, egui::Button::new("New layer"))
                .on_disabled_hover_text("Select a slide first")
                .clicked()
            {
                add_new_layer(editor);
                ui.close();
            }
        });

    left_ui.add_space(coursecraft_render::SPACE_MD);

    let preview_tooltip = if master_view {
        "Not available in slide masters".to_owned()
    } else {
        tooltip_with_shortcut(
            left_ui.ctx(),
            "Preview this slide",
            Some(shortcut_for_action(ShortcutAction::PreviewThisSlide)),
        )
    };
    let preview_resp = widgets::toolbar_button_enabled(
        &mut left_ui,
        Icon::Play,
        "Preview",
        false,
        !master_view,
        &preview_tooltip,
    );
    if preview_resp.clicked() {
        preview_slide(editor, requests);
    }
    let preview_caret = widgets::toolbar_caret_button(&mut left_ui, "More preview options");
    egui::Popup::menu(&preview_caret).show(|ui| {
        crate::menu::show_preview_menu_items(editor, ui, requests);
    });

    if editor.file_ops_available {
        left_ui.add_space(coursecraft_render::SPACE_MD);
        let publish_tooltip = tooltip_with_shortcut(
            left_ui.ctx(),
            "Publish course",
            Some(shortcut_for_action(ShortcutAction::Publish)),
        );
        let publish_resp = widgets::toolbar_primary_button(
            &mut left_ui,
            Icon::UploadSimple,
            "Publish",
            &publish_tooltip,
        );
        if publish_resp.clicked() {
            editor.open_publish_dialog();
        }
    }

    // Course map toggle: the course map replaces the slide editor.
    left_ui.add_space(coursecraft_render::SPACE_MD);
    let map_tooltip = if master_view {
        "Not available in slide masters".to_owned()
    } else {
        tooltip_with_shortcut(
            left_ui.ctx(),
            "Course map",
            Some(shortcut_for_action(ShortcutAction::CourseMap)),
        )
    };
    if widgets::toolbar_button_enabled(
        &mut left_ui,
        Icon::SquaresFour,
        "Map",
        editor.ui.story_view,
        !master_view,
        &map_tooltip,
    )
    .clicked()
    {
        editor.ui.story_view = !editor.ui.story_view;
    }

    // Format Painter: click arms once, double click keeps it armed.
    left_ui.add_space(coursecraft_render::SPACE_MD);
    let painter_armed = editor
        .ui
        .format_painter
        .as_ref()
        .is_some_and(|p| p.kind == crate::format_painter::PainterKind::Format);
    let state_edit_locked = editor.ui.editing_state.is_some();
    let painter_enabled =
        !master_view && !state_edit_locked && (painter_armed || editor.can_arm_format_painter());
    let painter_tooltip = if state_edit_locked {
        crate::STATE_LOCK_TOOLTIP.to_owned()
    } else if master_view {
        "Not available in slide masters".to_owned()
    } else if !painter_enabled {
        "Select an object to copy its format".to_owned()
    } else {
        tooltip_with_shortcut(
            left_ui.ctx(),
            "Format painter",
            Some(shortcut_for_action(ShortcutAction::FormatPainter)),
        )
    };
    let painter_resp = widgets::toolbar_button_enabled(
        &mut left_ui,
        Icon::PaintBrush,
        "Paint",
        painter_armed,
        painter_enabled,
        &painter_tooltip,
    );
    format_painter_clicked(editor, &painter_resp);

    // 2. Right group: Undo, Redo, separator, and inspector toggles (Format, Animate, Slide, Course)
    let mut right_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(right_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    right_ui.spacing_mut().item_spacing.x = 6.0;

    // Course tab toggle
    let course_active = editor.ui.inspector_tab == InspectorTab::Course;
    if widgets::toolbar_toggle(
        &mut right_ui,
        Icon::GraduationCap,
        "Course",
        course_active,
        "Course inspector",
    )
    .clicked()
    {
        crate::dock::request_inspector_tab(editor, InspectorTab::Course);
    }

    // Slide tab toggle
    let slide_active = editor.ui.inspector_tab == InspectorTab::Slide;
    if widgets::toolbar_toggle(
        &mut right_ui,
        Icon::Presentation,
        "Slide",
        slide_active,
        "Slide inspector",
    )
    .clicked()
    {
        crate::dock::request_inspector_tab(editor, InspectorTab::Slide);
    }

    // Animate tab toggle
    let anim_active = editor.ui.inspector_tab == InspectorTab::Animate;
    if widgets::toolbar_toggle(
        &mut right_ui,
        Icon::Sparkle,
        "Animate",
        anim_active,
        "Animate inspector",
    )
    .clicked()
    {
        crate::dock::request_inspector_tab(editor, InspectorTab::Animate);
    }

    // Format tab toggle
    let fmt_active = editor.ui.inspector_tab == InspectorTab::Format;
    if widgets::toolbar_toggle(
        &mut right_ui,
        Icon::SlidersHorizontal,
        "Format",
        fmt_active,
        "Format inspector",
    )
    .clicked()
    {
        crate::dock::request_inspector_tab(editor, InspectorTab::Format);
    }

    // Thin vertical separator
    right_ui.separator();

    // Redo button
    let can_redo = editor.session.can_redo();
    let redo_tooltip =
        tooltip_with_shortcut(right_ui.ctx(), "Redo", shortcut_for_command("edit.redo"));
    if widgets::icon_button_enabled_reason(
        &mut right_ui,
        Icon::ArrowClockwise,
        can_redo,
        &redo_tooltip,
        "Nothing to redo",
    )
    .clicked()
        && let Some(p) = crate::menu::adapter(editor, "edit.redo")
    {
        editor.run("edit.redo", p);
    }

    // Undo button
    let can_undo = editor.session.can_undo();
    let undo_tooltip =
        tooltip_with_shortcut(right_ui.ctx(), "Undo", shortcut_for_command("edit.undo"));
    if widgets::icon_button_enabled_reason(
        &mut right_ui,
        Icon::ArrowCounterClockwise,
        can_undo,
        &undo_tooltip,
        "Nothing to undo",
    )
    .clicked()
        && let Some(p) = crate::menu::adapter(editor, "edit.undo")
    {
        editor.run("edit.undo", p);
    }

    // 3. Centre group: Text, Shape, Image, Media, Button, Question, Trigger
    let mut center_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(center_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    center_ui.spacing_mut().item_spacing.x = 6.0;

    // Text tool
    let text_active = editor.ui.placement_tool == Some(PlacementTool::TextBox);
    if widgets::toolbar_toggle(
        &mut center_ui,
        Icon::TextT,
        "Text",
        text_active,
        "Insert text box",
    )
    .clicked()
    {
        if text_active {
            editor.ui.placement_tool = None;
        } else {
            insert_text_box(editor);
        }
    }

    // Shape gallery
    let shape_active = matches!(
        editor.ui.placement_tool,
        Some(PlacementTool::Shape(_) | PlacementTool::Draw(_))
    );
    let shape_resp = widgets::toolbar_toggle(
        &mut center_ui,
        Icon::Shapes,
        "Shape",
        shape_active,
        "Insert shape",
    );
    egui::Popup::menu(&shape_resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut picked = None;
            let output = crate::shape_gallery::show_shape_gallery(
                ui,
                &mut editor.ui.shape_gallery_query,
                &mut |pick| picked = Some(pick),
            );
            editor.ui.shape_gallery_rects = output.icons;
            editor.ui.shape_gallery_draw_rects = output.draw_icons;
            editor.ui.shape_gallery_search_rect = output.search_rect;
            match picked {
                Some(crate::shape_gallery::GalleryPick::Shape(kind)) => insert_shape(editor, kind),
                Some(crate::shape_gallery::GalleryPick::Draw(tool)) => insert_draw(editor, tool),
                None => {}
            }
        });

    // Image button
    if widgets::toolbar_button(&mut center_ui, Icon::Image, "Image", "Insert image").clicked() {
        insert_image(editor, requests);
    }

    // Media dropdown (Video…, Audio…)
    let non_visual_enabled = !master_view && !state_edit_locked;
    let media_tooltip = if state_edit_locked {
        "Not available inside a state"
    } else if master_view {
        "Not available in slide masters"
    } else {
        "Insert media"
    };
    let media_resp = widgets::toolbar_button_enabled(
        &mut center_ui,
        Icon::FilmStrip,
        "Media",
        false,
        non_visual_enabled,
        media_tooltip,
    );
    egui::Popup::menu(&media_resp).show(|ui| {
        if !non_visual_enabled {
            let p = coursecraft_render::palette(ui.ctx());
            ui.label(coursecraft_render::small(media_tooltip).color(p.text_disabled));
            return;
        }
        if ui.button("Video…").clicked() {
            insert_video(editor, requests);
            ui.close();
        }
        if ui.button("Audio…").clicked() {
            insert_audio(editor, requests);
            ui.close();
        }
        if ui.button("Lottie…").clicked() {
            insert_lottie(editor, requests);
            ui.close();
        }
        if ui.button("Record narration…").clicked() {
            editor.open_record_narration();
            ui.close();
        }
    });

    // Button placement tool
    let btn_active = editor.ui.placement_tool == Some(PlacementTool::Button);
    let btn_tooltip = if state_edit_locked {
        "Not available inside a state"
    } else {
        "Insert button"
    };
    if widgets::toolbar_button_enabled(
        &mut center_ui,
        Icon::CursorClick,
        "Button",
        btn_active,
        !state_edit_locked,
        btn_tooltip,
    )
    .clicked()
    {
        if btn_active {
            editor.ui.placement_tool = None;
        } else {
            insert_button(editor);
        }
    }

    // Question button. A slide with a question opens its panel; otherwise the
    // button lists the eight kinds like the Media popup. A draw slide has
    // no question action at all.
    let slide_is_draw = editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some_and(|slide| slide.draw.is_some());
    if !slide_is_draw {
        let slide_has_question = editor
            .session
            .course
            .slide(&editor.selection.slide)
            .is_some_and(|slide| slide.question.is_some());
        let question_tooltip = if state_edit_locked {
            "Not available inside a state"
        } else if master_view {
            "Not available in slide masters"
        } else {
            "Insert question"
        };
        let question_resp = widgets::toolbar_button_enabled(
            &mut center_ui,
            Icon::Question,
            "Question",
            false,
            non_visual_enabled,
            question_tooltip,
        );
        if non_visual_enabled {
            if slide_has_question {
                if question_resp.clicked() {
                    editor.ui.bottom_tab = crate::BottomTab::Question;
                }
            } else {
                egui::Popup::menu(&question_resp).show(|ui| {
                    for kind in crate::question::ALL_QUESTION_KINDS {
                        if ui
                            .button(crate::question::question_kind_name(kind))
                            .clicked()
                        {
                            insert_question(editor, kind);
                            ui.close();
                        }
                    }
                });
            }
        }
    }

    // Trigger button
    let trigger_tooltip = if state_edit_locked {
        "Not available inside a state"
    } else if master_view {
        "Not available in slide masters"
    } else {
        "Insert trigger"
    };
    let trigger_resp = widgets::toolbar_button_enabled(
        &mut center_ui,
        Icon::Lightning,
        "Trigger",
        false,
        non_visual_enabled,
        trigger_tooltip,
    );
    if trigger_resp.clicked() {
        insert_trigger(editor);
    }

    // Reserve the full toolbar rectangle so following panels don't overlap
    ui.advance_cursor_after_rect(max_rect);
}
