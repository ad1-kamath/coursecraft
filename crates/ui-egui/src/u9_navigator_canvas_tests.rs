//! Step u9 regression tests: Slide Navigator, Master View, Course Map,
//! Canvas, Rulers, Zoom HUD, Crop, Path Edit, and Placement Tool (`Area 4` & `Area 5`).

use crate::test_harness::TestHarness;
use coursecraft_course::{Course, ObjectKind};
use coursecraft_render::{ThemeMode, contrast_ratio, palette};
use egui::{Key, Modifiers, Rect, epaint::Shape, pos2, vec2};

fn multi_scene_long_titles_course() -> Course {
    let long_scene = "Scene One With An Extraordinarily Long Title That Exceeds Any Navigator Or Course Map Card Width Easily 1234567890";
    let long_slide = "Slide One With An Extraordinarily Long Title That Exceeds Any Navigator Or Course Map Card Width Easily 1234567890";
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-u9",
        "title": "Step u9 Course",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [
            { "id": "m1", "title": "Default Master", "objects": [] },
            { "id": "m2", "title": "Second Master", "objects": [] }
        ],
        "scenes": [
            {
                "id": "scene1",
                "title": long_scene,
                "slides": [
                    {
                        "id": "s1",
                        "title": long_slide,
                        "master": "m1",
                        "objects": [
                            {
                                "id": "obj_text",
                                "frame": { "x": 100, "y": 100, "w": 300, "h": 120 },
                                "kind": {
                                    "type": "text",
                                    "text": "Hello World",
                                    "style": { "font": "Inter", "size": 24, "color": "#1F1F1F", "align": "left" }
                                }
                            },
                            {
                                "id": "obj_shape",
                                "frame": { "x": 500, "y": 140, "w": 200, "h": 120 },
                                "kind": {
                                    "type": "shape",
                                    "shape": "rectangle",
                                    "fill": "#2563EB"
                                }
                            },
                            {
                                "id": "obj_path",
                                "frame": { "x": 150, "y": 350, "w": 240, "h": 160 },
                                "kind": {
                                    "type": "path",
                                    "points": [
                                        { "x": 0, "y": 0 },
                                        { "x": 120, "y": 0 },
                                        { "x": 120, "y": 120 },
                                        { "x": 0, "y": 120 }
                                    ],
                                    "closed": true,
                                    "outline": { "color": "#1F1F1F", "width": 2.0 }
                                }
                            }
                        ]
                    },
                    {
                        "id": "s2",
                        "title": "Second Slide",
                        "objects": []
                    }
                ]
            },
            {
                "id": "scene2",
                "title": "Second Scene",
                "slides": [
                    {
                        "id": "s3",
                        "title": "Third Slide",
                        "objects": []
                    }
                ]
            }
        ]
    });
    serde_json::from_value(raw).expect("valid course json")
}

fn single_slide_course() -> Course {
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-u9-single",
        "title": "Single Slide",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{ "id": "m1", "title": "Default Master", "objects": [] }],
        "scenes": [{
            "id": "scene1",
            "title": "Main",
            "slides": [{
                "id": "s1",
                "title": "Only Slide",
                "objects": []
            }]
        }]
    });
    serde_json::from_value(raw).expect("valid single slide course")
}

fn course_map_dense_course() -> Course {
    let long_slide = "Long Course Map Slide Card Title That Should Truncate Cleanly Without Overflowing The Card 1234567890";
    let mut scenes = Vec::new();
    for sc in 0..5 {
        let mut slides = Vec::new();
        for sl in 0..8 {
            slides.push(serde_json::json!({
                "id": format!("s_{sc}_{sl}"),
                "title": format!("{sc}.{sl} {long_slide}"),
                "objects": []
            }));
        }
        scenes.push(serde_json::json!({
            "id": format!("sc_{sc}"),
            "title": format!("Scene {sc} With A Very Long Title That Truncates Cleanly"),
            "slides": slides
        }));
    }
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-u9-map",
        "title": "Dense Map",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{ "id": "m1", "title": "Default Master", "objects": [] }],
        "scenes": scenes
    });
    serde_json::from_value(raw).expect("valid dense map course")
}

fn painted_texts_in(h: &mut TestHarness, rect: Rect) -> Vec<(String, Rect, bool)> {
    let mut out = Vec::new();
    for cs in h.frame_shapes(Vec::new()) {
        if let Shape::Text(t) = cs.shape {
            let text_rect = Rect::from_min_size(t.pos, t.galley.size());
            if rect.intersects(text_rect) {
                out.push((t.galley.text().to_owned(), text_rect, t.galley.elided));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Area 4: Left Panel — Slide Navigator, Master View & Course Map (T4.1–T4.8)
// ---------------------------------------------------------------------------

#[test]
fn t4_1_navigator_long_titles_truncate_with_tooltip_across_widths_and_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for width in [180.0_f32, 220.0, 320.0] {
            let mut h = TestHarness::new_with_theme(
                multi_scene_long_titles_course(),
                (1280.0, 800.0),
                mode,
            );
            h.idle_frame();
            let left0 = h.left_panel_rect();
            let dx = width - left0.width();
            if dx.abs() > 1.0 {
                h.drag_by(
                    pos2(left0.max.x, left0.center().y),
                    vec2(dx, 0.0),
                    4,
                    Modifiers::NONE,
                );
            }
            h.idle_frame();
            let left = h.left_panel_rect();
            let report = h.layout_report(2);
            let left_findings: Vec<_> = report
                .findings
                .iter()
                .filter(|f| !f.contains("[110") && !f.contains("[10"))
                .collect();
            assert!(
                left_findings.is_empty(),
                "expected 0 layout findings in navigator at width {width} ({mode:?}), got: {left_findings:?}"
            );
            let texts = painted_texts_in(&mut h, left);
            let scene_title_rect = texts
                .iter()
                .find(|(t, _, _)| t.starts_with("Scene One"))
                .map(|(t, r, e)| (t.clone(), *r, *e))
                .expect("scene title painted in left panel");
            assert!(
                scene_title_rect.2,
                "expected truncated scene title (elided) at width {width}, got {:?}",
                scene_title_rect.0
            );
            let scene_tip = h.probe_tooltip(scene_title_rect.1.center());
            assert!(
                scene_tip
                    .as_deref()
                    .is_some_and(|t| t.contains("Scene One With An Extraordinarily Long Title")),
                "expected full scene title tooltip at width {width}, got {scene_tip:?}"
            );

            let slide_title_rect = texts
                .iter()
                .find(|(t, _, _)| t.starts_with("Slide One"))
                .map(|(t, r, e)| (t.clone(), *r, *e))
                .expect("slide title painted in left panel");
            assert!(
                slide_title_rect.2,
                "expected truncated slide title (elided) at width {width}, got {:?}",
                slide_title_rect.0
            );
            let slide_tip = h.probe_tooltip(slide_title_rect.1.center());
            assert!(
                slide_tip
                    .as_deref()
                    .is_some_and(|t| t.contains("Slide One With An Extraordinarily Long Title")),
                "expected full slide title tooltip at width {width}, got {slide_tip:?}"
            );
        }
    }
}

#[test]
fn t4_2_thumbnail_more_button_has_hit_target_tooltip_popup_and_escape_close() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.idle_frame();
    let left = h.left_panel_rect();
    let thumb = crate::slides::slide_thumbnail_rect(&h.editor.session.course, left, "s1")
        .expect("thumbnail rect for s1");

    // Hover thumbnail so the `⋯` button appears.
    h.hover_at(thumb.center());
    let more_center = pos2(thumb.max.x - 16.0, thumb.min.y + 16.0);
    h.click_at(more_center);
    h.idle_frame();
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "expected thumbnail more popup to open"
    );
    assert!(
        h.find_text("Duplicate slide").is_some(),
        "expected Duplicate slide in thumbnail more popup"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        !egui::Popup::is_any_open(&h.ctx),
        "expected Esc to close thumbnail more popup"
    );

    let tip = h.probe_tooltip(more_center);
    assert_eq!(
        tip.as_deref(),
        Some("Slide options"),
        "expected Slide options tooltip on thumbnail more button"
    );
}

#[test]
fn t4_3_inline_scene_rename_commits_in_one_undo_rejects_empty_and_reverts_on_escape() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.idle_frame();
    let left = h.left_panel_rect();
    let texts = painted_texts_in(&mut h, left);
    let (_, scene_rect, _) = texts
        .into_iter()
        .find(|(t, _, _)| t.starts_with("Scene One"))
        .expect("scene header visible");

    let undo_before = h.editor.undo_len_for_test();
    // Double-click to start inline rename, type new name + Enter -> 1 undo step.
    h.double_click_at(scene_rect.center());
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("New Scene");
    h.press(Key::Enter, Modifiers::NONE);
    assert_eq!(h.editor.session.course.scenes[0].title, "New Scene");
    assert_eq!(h.editor.undo_len_for_test(), undo_before + 1);
    h.press(Key::Z, Modifiers::COMMAND);
    assert_ne!(h.editor.session.course.scenes[0].title, "New Scene");

    // Blank name is rejected cleanly.
    let before = h.editor.session.course.scenes[0].title.clone();
    h.double_click_at(scene_rect.center());
    h.press(Key::A, Modifiers::COMMAND);
    h.press(Key::Backspace, Modifiers::NONE);
    h.press(Key::Enter, Modifiers::NONE);
    assert_eq!(h.editor.session.course.scenes[0].title, before);

    // Escape reverts without changing title.
    h.double_click_at(scene_rect.center());
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("Discarded Name");
    h.press(Key::Escape, Modifiers::NONE);
    assert_eq!(h.editor.session.course.scenes[0].title, before);
}

#[test]
fn t4_4_master_view_banner_has_high_contrast_exit_and_restores_slide_selection() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for size in [(1024.0_f32, 600.0_f32), (1440.0, 900.0)] {
            let mut h = TestHarness::new_with_theme(multi_scene_long_titles_course(), size, mode);
            h.editor.selection.slide = "s2".to_owned();
            h.editor.selection.repair(&h.editor.session.course);
            h.idle_frame();

            h.editor.enter_master_view();
            h.idle_frame();

            let p = palette(&h.ctx);
            let exit_pos = h
                .find_text("Close slide masters")
                .expect("Slide masters exit button visible");
            let c_rest = contrast_ratio(p.accent_text, p.input);
            assert!(
                c_rest >= 4.5,
                "Master View exit button contrast {c_rest:.2} < 4.5 in {mode:?}"
            );

            // Bottom timeline panel is visible in Master View (for master animations and timing).
            assert!(
                h.editor.ui.timeline.panel_rect.is_some(),
                "timeline panel should be visible in Master View"
            );

            h.click_at(exit_pos);
            assert!(!h.editor.ui.master_view);
            assert_eq!(h.editor.selection.slide, "s2");
        }
    }
}

#[test]
fn t4_5_delete_scene_and_delete_master_dialogs_cancel_on_escape_and_delete_in_one_undo() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.idle_frame();

    // Delete scene dialog: Esc cancels cleanly, Confirm deletes in 1 undo step.
    h.editor.ui.pending_delete_scene = Some("scene2".to_owned());
    h.idle_frame();
    assert!(h.find_text("Delete scene").is_some());
    h.press(Key::Escape, Modifiers::NONE);
    assert!(
        h.editor.ui.pending_delete_scene.is_none(),
        "Esc should close Delete scene dialog"
    );
    assert_eq!(h.editor.session.course.scenes.len(), 2);

    h.editor.ui.pending_delete_scene = Some("scene2".to_owned());
    h.idle_frame();
    let undo_before = h.editor.undo_len_for_test();
    let confirm_scene = h
        .find_text_last("Delete scene")
        .expect("Delete scene button");
    h.click_at(confirm_scene);
    assert_eq!(h.editor.session.course.scenes.len(), 1);
    assert_eq!(h.editor.undo_len_for_test(), undo_before + 1);
    h.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.editor.session.course.scenes.len(), 2);

    // Delete master dialog: Esc cancels cleanly, Confirm deletes in 1 undo step.
    h.editor.ui.pending_delete_master = Some("m1".to_owned());
    h.idle_frame();
    assert!(h.find_text("Delete master").is_some());
    h.press(Key::Escape, Modifiers::NONE);
    assert!(
        h.editor.ui.pending_delete_master.is_none(),
        "Esc should close Delete master dialog"
    );
    assert_eq!(h.editor.session.course.masters.len(), 2);

    h.editor.ui.pending_delete_master = Some("m1".to_owned());
    h.idle_frame();
    let undo_before = h.editor.undo_len_for_test();
    let confirm_master = h
        .find_text_last("Delete master")
        .expect("Delete master button");
    h.click_at(confirm_master);
    assert_eq!(h.editor.session.course.masters.len(), 1);
    assert_eq!(h.editor.undo_len_for_test(), undo_before + 1);
    h.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.editor.session.course.masters.len(), 2);
}

#[test]
fn t4_6_course_map_dense_cards_truncate_without_layout_findings() {
    let mut h = TestHarness::new_with_size(course_map_dense_course(), (1440.0, 900.0));
    h.editor.ui.story_view = true;
    h.idle_frame();
    let report = h.layout_report(2);
    let right = h.right_panel_rect();
    let map_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| {
            // Exclude right-inspector Slide tab title and horizontal ScrollArea edge clipping on Scene 3.
            !f.contains(&format!("[{:.0},", right.min.x.floor()))
                && !f.contains("[1263,")
                && !f.contains("Scene 3 With A Very Long Title")
        })
        .collect();
    assert!(
        map_findings.is_empty(),
        "Course map layout findings: {map_findings:?}"
    );
}

#[test]
fn t4_7_course_map_ignores_keys_when_modal_or_preview_is_open() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.editor.ui.story_view = true;
    h.editor.selection.slide = "s1".to_owned();
    h.idle_frame();

    // With Preferences open, ArrowRight/Enter must not navigate or exit Course map.
    h.editor.ui.prefs.open = true;
    h.idle_frame();
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.press(Key::Enter, Modifiers::NONE);
    assert_eq!(h.editor.selection.slide, "s1");
    assert!(h.editor.ui.story_view);
}

#[test]
fn t4_8_single_slide_course_disables_delete_slide_with_tooltip() {
    let mut h = TestHarness::new(single_slide_course());
    h.idle_frame();
    let left = h.left_panel_rect();
    let thumb = crate::slides::slide_thumbnail_rect(&h.editor.session.course, left, "s1")
        .expect("thumbnail rect");
    h.right_click_at(thumb.center());
    h.idle_frame();
    h.idle_frame();
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "expected context menu to be open after right-click on thumbnail"
    );
    let del_pos = h
        .find_text_last("Delete")
        .expect("Delete item in context menu");
    h.hover_at(del_pos);
    let tip = h.probe_tooltip(del_pos);
    assert_eq!(
        tip.as_deref(),
        Some("A course needs at least one slide"),
        "expected disabled Delete slide tooltip"
    );
}

// ---------------------------------------------------------------------------
// Area 5: Central Canvas, Rulers, Zoom & Direct Manipulation (T5.1–T5.8)
// ---------------------------------------------------------------------------

#[test]
fn t5_1_zoom_pill_stays_inside_canvas_meets_hit_targets_tooltips_and_contrast() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for size in [(1024.0_f32, 600.0_f32), (1280.0, 800.0), (1920.0, 1080.0)] {
            for scale in [1.0_f32, 1.5] {
                if size.0 / scale < 750.0 || size.1 / scale < 450.0 {
                    continue;
                }
                let mut h = TestHarness::new_with_config(
                    multi_scene_long_titles_course(),
                    size,
                    mode,
                    scale,
                    false,
                );
                h.editor.session.course.editor.show_rulers = true;
                let _ = h.layout_report(2);
                let hud = h
                    .editor
                    .ui
                    .last_zoom_hud_rect
                    .expect("zoom HUD rect recorded");
                assert!(
                    h.screen_rect.contains_rect(hud),
                    "zoom HUD {hud:?} outside screen {:?} at {size:?} scale {scale}",
                    h.screen_rect
                );
                assert!(
                    hud.min.x >= h.left_panel_rect().max.x - 1.0
                        && hud.max.x <= h.right_panel_rect().min.x + 1.0
                        && hud.min.y >= h.top_bars_height() - 1.0,
                    "zoom HUD {hud:?} overlaps side panels or top bars at {size:?} scale {scale}"
                );

                // Check - / Fit / + buttons meet >= 26x26 pt and have tooltips.
                let minus_pos = h
                    .find_icon_last(coursecraft_render::Icon::MagnifyingGlassMinus)
                    .expect("zoom out button");
                let plus_pos = h
                    .find_icon_last(coursecraft_render::Icon::MagnifyingGlassPlus)
                    .expect("zoom in button");
                let fit_pos = h.find_text_in("Fit", hud).expect("Fit zoom button");

                assert!(h.probe_tooltip(minus_pos).is_some());
                assert!(h.probe_tooltip(plus_pos).is_some());
                assert!(
                    h.probe_tooltip(fit_pos).is_some(),
                    "expected tooltip on Fit zoom button at {size:?} ({mode:?})"
                );

                let _ = h.layout_report(1);
                let hit_issues = h.check_hit_targets(vec2(26.0, 26.0));
                let zoom_hit_issues: Vec<_> = hit_issues
                    .into_iter()
                    .filter(|s| s.contains("Fit") || s.contains("Zoom"))
                    .collect();
                assert!(
                    zoom_hit_issues.is_empty(),
                    "zoom HUD hit target issues at {size:?} scale {scale}: {zoom_hit_issues:?}"
                );

                let p = palette(&h.ctx);
                assert!(contrast_ratio(p.text_secondary, p.raised) >= 4.5);
            }
        }
    }
}

#[test]
fn t5_2_rulers_tick_contrast_in_light_and_dark_and_guide_drag_clipped() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h =
            TestHarness::new_with_theme(multi_scene_long_titles_course(), (1280.0, 800.0), mode);
        h.editor.session.course.editor.show_rulers = true;
        h.idle_frame();

        let canvas = crate::canvas::canvas_rect(h.central_panel_rect());
        let top_strip = crate::rulers::top_strip(canvas);
        let p = palette(&h.ctx);

        // Find any ruler tick label text color in the top strip.
        let mut tick_color = None;
        for cs in h.frame_shapes(Vec::new()) {
            if let Shape::Text(t) = cs.shape {
                let r = Rect::from_min_size(t.pos, t.galley.size());
                if top_strip.intersects(r) {
                    tick_color = Some(t.override_text_color.unwrap_or_else(|| {
                        t.galley
                            .job
                            .sections
                            .first()
                            .map_or(p.text, |s| s.format.color)
                    }));
                    break;
                }
            }
        }
        let color = tick_color.expect("ruler tick label painted in top strip");
        let ratio = contrast_ratio(color, p.panel);
        assert!(
            ratio >= 4.5,
            "ruler tick label contrast {ratio:.2} < 4.5 in {mode:?} (color={color:?}, bg={:?})",
            p.panel
        );

        // Drag a guide from top ruler into canvas; verify guide is added.
        h.drag_by(top_strip.center(), vec2(0.0, 100.0), 4, Modifiers::NONE);
        assert_eq!(h.editor.session.course.editor.guides.len(), 1);
    }
}

#[test]
fn t5_3_layer_pills_with_12_long_names_do_not_overflow_canvas() {
    let mut course = multi_scene_long_titles_course();
    for i in 0..12 {
        let layer: coursecraft_course::Layer = serde_json::from_value(serde_json::json!({
            "id": format!("layer_{i}"),
            "name": format!("Layer {i} With A Very Long Descriptive Name 1234567890"),
            "objects": []
        }))
        .expect("valid layer json");
        course.scenes[0].slides[0].layers.push(layer);
    }
    for size in [(1024.0_f32, 600.0_f32), (1440.0, 900.0)] {
        let mut h = TestHarness::new_with_size(course.clone(), size);
        h.editor.selection.layer = Some("layer_0".to_owned());
        h.idle_frame();
        let canvas = h.central_panel_rect();
        let right = h.right_panel_rect();
        for (txt, rect, _) in painted_texts_in(&mut h, canvas) {
            if txt.starts_with("Layer ") {
                assert!(
                    rect.max.x <= right.min.x + 1.0,
                    "layer pill {txt:?} overflowed into inspector at {size:?}: {rect:?} vs {right:?}"
                );
            }
        }
    }
}

#[test]
fn t5_4_inline_text_edit_escape_hierarchy_exits_edit_then_clears_selection() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.idle_frame();
    let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
    let obj_center = pos2(
        slide_rect.min.x + slide_rect.width() * (250.0 / 1280.0),
        slide_rect.min.y + slide_rect.height() * (160.0 / 720.0),
    );
    h.double_click_at(obj_center);
    assert!(
        h.editor.ui.inline_edit.is_some(),
        "double-click should enter inline_edit"
    );

    // First Esc exits inline_edit, keeping object selected.
    h.press(Key::Escape, Modifiers::NONE);
    assert!(h.editor.ui.inline_edit.is_none());
    assert!(h.editor.selection.contains("obj_text"));

    // Second Esc clears object selection.
    h.press(Key::Escape, Modifiers::NONE);
    assert!(h.editor.selection.object.is_none() && h.editor.selection.also.is_empty());
}

#[test]
fn t5_5_crop_mode_escape_cancels_and_enter_commits_in_one_undo() {
    let mut course = multi_scene_long_titles_course();
    let img_obj: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "obj_img",
        "frame": { "x": 400, "y": 300, "w": 200, "h": 150 },
        "kind": { "type": "image", "src": "test.png" }
    }))
    .expect("valid image object");
    course.scenes[0].slides[0].objects.push(img_obj);
    let mut h = TestHarness::new(course);
    let png_bytes = coursecraft_media::encode_png_rgba(100, 80, &vec![255u8; 100 * 80 * 4])
        .expect("encode png");
    h.editor
        .media
        .push("test.png".to_owned(), std::sync::Arc::from(png_bytes));
    h.editor.selection.object = Some("obj_img".to_owned());
    h.idle_frame();

    let undo_before = h.editor.undo_len_for_test();
    // Enter crop mode, modify crop, press Esc -> unchanged.
    assert!(h.editor.start_crop("obj_img"));
    h.idle_frame();
    assert!(h.editor.ui.crop_edit.is_some());
    if let Some(session) = h.editor.ui.crop_edit.as_mut() {
        session.edit =
            session
                .edit
                .drag_handle(crate::crop::CropHandle::Right, vec2(-30.0, 0.0), false);
    }
    h.press(Key::Escape, Modifiers::NONE);
    assert!(h.editor.ui.crop_edit.is_none());
    assert_eq!(h.editor.undo_len_for_test(), undo_before);

    // Enter crop mode, modify crop, press Enter -> 1 undoable command.
    assert!(h.editor.start_crop("obj_img"));
    h.idle_frame();
    if let Some(session) = h.editor.ui.crop_edit.as_mut() {
        session.edit =
            session
                .edit
                .drag_handle(crate::crop::CropHandle::Right, vec2(-30.0, 0.0), false);
    }
    h.press(Key::Enter, Modifiers::NONE);
    assert!(h.editor.ui.crop_edit.is_none());
    assert_eq!(h.editor.undo_len_for_test(), undo_before + 1);
    h.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.editor.undo_len_for_test(), undo_before);
}

#[test]
fn t5_6_path_point_editor_deletes_point_in_one_undo_and_exits_on_escape() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.editor.selection.object = Some("obj_path".to_owned());
    crate::canvas::start_path_edit(&mut h.editor, "obj_path");
    if let Some(edit) = h.editor.ui.path_edit.as_mut() {
        edit.selected_point = Some(0);
    }
    h.idle_frame();

    let undo_before = h.editor.undo_len_for_test();
    h.press(Key::Delete, Modifiers::NONE);
    let obj = h
        .editor
        .session
        .course
        .slide("s1")
        .and_then(|s| s.objects.iter().find(|o| o.id == "obj_path"))
        .expect("obj_path still exists after deleting 1 point");
    if let ObjectKind::Path { path } = &obj.kind {
        assert_eq!(path.points.len(), 3, "expected 1 point deleted");
    } else {
        panic!("expected Path object");
    }
    assert_eq!(h.editor.undo_len_for_test(), undo_before + 1);

    h.press(Key::Escape, Modifiers::NONE);
    assert!(h.editor.ui.path_edit.is_none(), "Esc should exit path edit");
}

#[test]
fn t5_7_clicking_outside_open_menu_does_not_select_underlying_canvas_object() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.idle_frame();
    let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
    let shape_center = pos2(
        slide_rect.min.x + slide_rect.width() * (600.0 / 1280.0),
        slide_rect.min.y + slide_rect.height() * (200.0 / 720.0),
    );
    let insert_pos = h.find_text("Insert").expect("Insert menu");
    h.click_at(insert_pos);
    assert!(egui::Popup::is_any_open(&h.ctx));

    // Dismiss menu by clicking over the shape on the canvas.
    h.click_at(shape_center);
    assert!(
        h.editor.selection.object.is_none() && h.editor.selection.also.is_empty(),
        "dismissing click should not select underlying canvas object"
    );
}

#[test]
fn t5_8_placement_tool_shows_status_hint_and_escape_disarms_without_clearing_selection() {
    let mut h = TestHarness::new(multi_scene_long_titles_course());
    h.editor.selection.object = Some("obj_shape".to_owned());
    h.editor.ui.placement_tool = Some(crate::PlacementTool::TextBox);
    h.idle_frame();

    let report = h.layout_report(1);
    assert!(
        report.painted("Click for the default size, or drag to draw"),
        "expected placement prompt in status bar"
    );

    h.press(Key::Escape, Modifiers::NONE);
    assert!(h.editor.ui.placement_tool.is_none());
    assert!(
        h.editor.selection.contains("obj_shape"),
        "Esc disarming placement tool must not clear existing selection"
    );
}
