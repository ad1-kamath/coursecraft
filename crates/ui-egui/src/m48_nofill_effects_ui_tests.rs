use crate::dock::Panel;
use crate::panels::{Section, open_at};
use crate::test_harness::TestHarness;
use coursecraft_course::{Course, Object, ObjectKind};
use egui::pos2;

fn base_course(objects: serde_json::Value) -> Course {
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-m48-s5",
        "title": "M48 S5 Effects UI",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene-1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "objects": objects
            }]
        }]
    });
    serde_json::from_value(raw).expect("valid course")
}

fn shape_obj(fill: &str, outline: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": "box-1",
        "name": "Box 1",
        "frame": { "x": 100, "y": 100, "w": 200, "h": 120 },
        "kind": {
            "type": "shape",
            "shape": "rectangle",
            "fill": fill,
            "outline": outline
        },
        "shadow": {
            "color": "#000000FF",
            "offset_x": 4.0,
            "offset_y": 4.0,
            "blur": 6.0
        },
        "glow": {
            "color": "#FFAA00FF",
            "size": 8.0
        }
    })
}

fn first_object(harness: &TestHarness) -> &Object {
    &harness.editor.session.course.scenes[0].slides[0].objects[0]
}

fn scroll_right_panel_to(harness: &mut TestHarness, target: &str) {
    let panel = harness.right_panel_rect();
    for _ in 0..25 {
        if harness
            .find_text_in(target, panel)
            .is_some_and(|p| p.y < panel.min.y + 300.0)
        {
            return;
        }
        harness.scroll_over(panel.center(), -120.0);
        harness.idle_frame();
    }
}

fn scroll_right_panel_top(harness: &mut TestHarness) {
    let panel = harness.right_panel_rect();
    for _ in 0..25 {
        if harness
            .find_text_in("Fill", panel)
            .is_some_and(|p| p.y >= panel.min.y + 40.0)
        {
            return;
        }
        harness.scroll_over(panel.center(), 120.0);
        harness.idle_frame();
    }
}

#[test]
fn m48_s5_01_unticking_fill_keeps_shadow_and_glow_settings_and_shows_notes() {
    let mut harness = TestHarness::new_with_size(
        base_course(serde_json::json!([shape_obj(
            "#2F6FEB",
            serde_json::Value::Null
        )])),
        (1280.0, 1200.0),
    );
    harness.editor.selection.set_many(["box-1"]);
    harness.idle_frame();

    let original_shadow = first_object(&harness).shadow.clone();
    let original_glow = first_object(&harness).glow.clone();
    assert!(original_shadow.is_some());
    assert!(original_glow.is_some());

    scroll_right_panel_to(&mut harness, "Effects");
    let panel = harness.right_panel_rect();
    assert!(harness.find_text_in("Shadow", panel).is_some());
    assert!(harness.find_text_in("Glow", panel).is_some());
    assert!(
        harness
            .find_text("Add a fill or outline to see the shadow.")
            .is_none()
    );
    assert!(
        harness
            .find_text("Add a fill or outline to see the glow.")
            .is_none()
    );

    scroll_right_panel_top(&mut harness);
    let control_x = panel.min.x + 144.0;
    let fill_label = harness.find_text_in("Fill", panel).expect("Fill label");
    harness.click_at(pos2(control_x, fill_label.y));
    harness.idle_frame();

    assert!(
        matches!(&first_object(&harness).kind, ObjectKind::Shape { fill, .. } if fill.0 == "#00000000"),
        "unticking Fill should set shape fill to #00000000"
    );
    assert_eq!(first_object(&harness).shadow, original_shadow);
    assert_eq!(first_object(&harness).glow, original_glow);

    scroll_right_panel_to(&mut harness, "Effects");
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the shadow.", panel)
            .is_some(),
        "shadow note should appear when both fill and outline are off"
    );
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the glow.", panel)
            .is_some(),
        "glow note should appear when both fill and outline are off"
    );

    scroll_right_panel_top(&mut harness);
    let fill_label = harness.find_text_in("Fill", panel).expect("Fill label");
    harness.click_at(pos2(control_x, fill_label.y));
    harness.idle_frame();

    assert!(
        matches!(&first_object(&harness).kind, ObjectKind::Shape { fill, .. } if fill.0 == "#2F6FEB"),
        "re-ticking Fill should restore the previous shape fill"
    );
    assert_eq!(first_object(&harness).shadow, original_shadow);
    assert_eq!(first_object(&harness).glow, original_glow);

    scroll_right_panel_to(&mut harness, "Effects");
    assert!(harness.find_text_in("Shadow", panel).is_some());
    assert!(harness.find_text_in("Glow", panel).is_some());
    assert!(
        harness
            .find_text("Add a fill or outline to see the shadow.")
            .is_none()
    );
    assert!(
        harness
            .find_text("Add a fill or outline to see the glow.")
            .is_none()
    );
}

#[test]
fn m48_s5_02_no_fill_with_outline_shows_no_hidden_note() {
    let mut harness = TestHarness::new_with_size(
        base_course(serde_json::json!([shape_obj(
            "#00000000",
            serde_json::json!({ "color": "#1F1F1F", "width": 2.0 })
        )])),
        (1280.0, 1200.0),
    );
    harness.editor.selection.set_many(["box-1"]);
    harness.idle_frame();

    scroll_right_panel_to(&mut harness, "Effects");
    let panel = harness.right_panel_rect();
    assert!(harness.find_text_in("Shadow", panel).is_some());
    assert!(harness.find_text_in("Glow", panel).is_some());
    assert!(
        harness
            .find_text("Add a fill or outline to see the shadow.")
            .is_none()
    );
    assert!(
        harness
            .find_text("Add a fill or outline to see the glow.")
            .is_none()
    );
}

#[test]
fn m48_s5_03_effects_still_editable_when_greyed() {
    let mut harness = TestHarness::new_with_size(
        base_course(serde_json::json!([shape_obj(
            "#00000000",
            serde_json::Value::Null
        )])),
        (1280.0, 1200.0),
    );
    harness.editor.selection.set_many(["box-1"]);
    harness.idle_frame();

    scroll_right_panel_to(&mut harness, "Effects");
    let panel = harness.right_panel_rect();
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the shadow.", panel)
            .is_some()
    );

    let shadow_label = harness.find_text_in("Shadow", panel).expect("Shadow label");
    harness.click_at(pos2(panel.min.x + 144.0, shadow_label.y));
    harness.idle_frame();

    assert!(
        first_object(&harness).shadow.is_none(),
        "toggling Shadow checkbox while greyed should still update obj.shadow"
    );
}

#[test]
fn m48_s5_04_shape_style_panel_also_shows_notes_and_image_unaffected() {
    let mut harness = TestHarness::new_with_size(
        base_course(serde_json::json!([shape_obj(
            "#00000000",
            serde_json::Value::Null
        )])),
        (1600.0, 1200.0),
    );
    harness.editor.ui.dock.enabled = true;
    harness.editor.selection.set_many(["box-1"]);
    harness.idle_frame();
    open_at(&mut harness.editor, Panel::ShapeStyle, Some(Section::Fill));
    for _ in 0..3 {
        harness.idle_frame();
    }

    let style_rect = harness
        .editor
        .ui
        .dock
        .rect(Panel::ShapeStyle)
        .expect("Shape Style panel rect");
    for _ in 0..10 {
        if harness
            .find_text_in("Add a fill or outline to see the glow.", style_rect)
            .is_some()
        {
            break;
        }
        harness.scroll_over(style_rect.center(), -120.0);
        harness.idle_frame();
    }
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the shadow.", style_rect)
            .is_some(),
        "Shape Style panel should show shadow note"
    );
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the glow.", style_rect)
            .is_some(),
        "Shape Style panel should show glow note"
    );

    // Image object with shadow/glow should never show the hidden notes.
    let img_course = base_course(serde_json::json!([{
        "id": "img-1",
        "name": "Image 1",
        "frame": { "x": 100, "y": 100, "w": 200, "h": 120 },
        "kind": { "type": "image", "src": "photo.png" },
        "shadow": {
            "color": "#000000FF",
            "offset_x": 4.0,
            "offset_y": 4.0,
            "blur": 6.0
        },
        "glow": {
            "color": "#FFAA00FF",
            "size": 8.0
        }
    }]));
    let mut img_harness = TestHarness::new_with_size(img_course, (1280.0, 1200.0));
    img_harness.editor.selection.set_many(["img-1"]);
    img_harness.idle_frame();
    scroll_right_panel_to(&mut img_harness, "Effects");
    let img_panel = img_harness.right_panel_rect();
    assert!(img_harness.find_text_in("Shadow", img_panel).is_some());
    assert!(img_harness.find_text_in("Glow", img_panel).is_some());
    assert!(
        img_harness
            .find_text("Add a fill or outline to see the shadow.")
            .is_none()
    );
    assert!(
        img_harness
            .find_text("Add a fill to see the shadow.")
            .is_none()
    );
    assert!(
        img_harness
            .find_text("Add a fill or outline to see the glow.")
            .is_none()
    );
    assert!(
        img_harness
            .find_text("Add a fill to see the glow.")
            .is_none()
    );
}

#[test]
fn m48_s5_05_button_no_fill_shows_button_notes_and_preserves_effects() {
    let course = base_course(serde_json::json!([{
        "id": "btn-1",
        "name": "Button 1",
        "frame": { "x": 100, "y": 100, "w": 200, "h": 60 },
        "kind": {
            "type": "button",
            "label": "Continue",
            "fill": "#2F6FEB"
        },
        "shadow": {
            "color": "#000000FF",
            "offset_x": 4.0,
            "offset_y": 4.0,
            "blur": 6.0
        },
        "glow": {
            "color": "#FFAA00FF",
            "size": 8.0
        }
    }]));
    let mut harness = TestHarness::new_with_size(course, (1280.0, 1200.0));
    harness.editor.selection.set_many(["btn-1"]);
    harness.idle_frame();

    let original_shadow = first_object(&harness).shadow.clone();
    let original_glow = first_object(&harness).glow.clone();

    let panel = harness.right_panel_rect();
    let fill_label = harness.find_text_in("Fill", panel).expect("Fill label");
    harness.click_at(pos2(panel.min.x + 144.0, fill_label.y));
    harness.idle_frame();

    assert!(
        matches!(&first_object(&harness).kind, ObjectKind::Button { fill, .. } if fill.0 == "#00000000")
    );
    assert_eq!(first_object(&harness).shadow, original_shadow);
    assert_eq!(first_object(&harness).glow, original_glow);

    scroll_right_panel_to(&mut harness, "Effects");
    assert!(
        harness
            .find_text_in("Add a fill to see the shadow.", panel)
            .is_some(),
        "button with fill off should show button shadow note"
    );
    assert!(
        harness
            .find_text_in("Add a fill to see the glow.", panel)
            .is_some(),
        "button with fill off should show button glow note"
    );
    assert!(
        harness
            .find_text("Add a fill or outline to see the shadow.")
            .is_none()
    );

    scroll_right_panel_top(&mut harness);
    let fill_label = harness.find_text_in("Fill", panel).expect("Fill label");
    harness.click_at(pos2(panel.min.x + 144.0, fill_label.y));
    harness.idle_frame();

    assert!(
        matches!(&first_object(&harness).kind, ObjectKind::Button { fill, .. } if fill.0 == "#2F6FEB")
    );
    assert_eq!(first_object(&harness).shadow, original_shadow);
    assert_eq!(first_object(&harness).glow, original_glow);

    scroll_right_panel_to(&mut harness, "Effects");
    assert!(harness.find_text_in("Shadow", panel).is_some());
    assert!(harness.find_text("Add a fill to see the shadow.").is_none());
    assert!(harness.find_text("Add a fill to see the glow.").is_none());
}

#[test]
fn m48_s5_06_state_transparent_fill_suppresses_base_gradient_and_shows_notes() {
    // Base shape has a linear gradient fill_style and no outline; editing a state whose
    // fill override is #00000000 (the exact patch produced by unticking Fill on a state)
    // suppresses the base fill_style per ST-05, so Effects must show the hidden notes.
    let course = base_course(serde_json::json!([{
        "id": "box-1",
        "name": "Box 1",
        "frame": { "x": 100, "y": 100, "w": 200, "h": 120 },
        "kind": {
            "type": "shape",
            "shape": "rectangle",
            "fill": "#2F6FEB",
            "fill_style": {
                "type": "linear",
                "angle": 90.0,
                "stops": [
                    { "position": 0.0, "color": "#2F6FEB" },
                    { "position": 1.0, "color": "#1B4DB1" }
                ]
            },
            "outline": null
        },
        "shadow": {
            "color": "#000000FF",
            "offset_x": 4.0,
            "offset_y": 4.0,
            "blur": 6.0
        },
        "glow": {
            "color": "#FFAA00FF",
            "size": 8.0
        },
        "states": [{
            "name": "hover",
            "fill": "#00000000",
            "fill_style": null,
            "clear_fill_style": false
        }]
    }]));
    let mut harness = TestHarness::new_with_size(course, (1280.0, 1200.0));
    harness.editor.selection.set_many(["box-1"]);
    harness.editor.ui.editing_state = Some(crate::StateEditSession {
        slide_id: "s1".to_owned(),
        object_id: "box-1".to_owned(),
        state_name: "hover".to_owned(),
        selected_extra: None,
    });
    harness.idle_frame();

    scroll_right_panel_to(&mut harness, "Effects");
    let panel = harness.right_panel_rect();
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the shadow.", panel)
            .is_some(),
        "state with transparent fill overriding base gradient should show shadow note"
    );
    assert!(
        harness
            .find_text_in("Add a fill or outline to see the glow.", panel)
            .is_some(),
        "state with transparent fill overriding base gradient should show glow note"
    );
}

#[test]
fn m48_s5_07_path_and_non_drawable_outline_show_notes() {
    // Open path with a fill colour (ignored for open paths) and zero-width outline draws nothing.
    let open_path_course = base_course(serde_json::json!([{
        "id": "path-1",
        "name": "Path 1",
        "frame": { "x": 100, "y": 100, "w": 200, "h": 120 },
        "kind": {
            "type": "path",
            "points": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }],
            "closed": false,
            "fill": "#2F6FEB",
            "outline": { "color": "#1F1F1F", "width": 0.0 }
        },
        "shadow": { "color": "#000000FF", "offset_x": 4.0, "offset_y": 4.0, "blur": 6.0 },
        "glow": { "color": "#FFAA00FF", "size": 8.0 }
    }]));
    let mut path_harness = TestHarness::new_with_size(open_path_course, (1280.0, 1200.0));
    path_harness.editor.selection.set_many(["path-1"]);
    path_harness.idle_frame();
    scroll_right_panel_to(&mut path_harness, "Effects");
    let panel = path_harness.right_panel_rect();
    assert!(
        path_harness
            .find_text_in("Add a fill or outline to see the shadow.", panel)
            .is_some(),
        "open path with zero-width outline should show shadow note"
    );
    assert!(
        path_harness
            .find_text_in("Add a fill or outline to see the glow.", panel)
            .is_some(),
        "open path with zero-width outline should show glow note"
    );

    // Shape with no fill and an out-of-range outline width (> 50 px, dropped by resolve_outline).
    let hostile_outline_course = base_course(serde_json::json!([shape_obj(
        "#00000000",
        serde_json::json!({ "color": "#1F1F1F", "width": 120.0 })
    )]));
    let mut hostile_harness = TestHarness::new_with_size(hostile_outline_course, (1280.0, 1200.0));
    hostile_harness.editor.selection.set_many(["box-1"]);
    hostile_harness.idle_frame();
    scroll_right_panel_to(&mut hostile_harness, "Effects");
    let panel = hostile_harness.right_panel_rect();
    assert!(
        hostile_harness
            .find_text_in("Add a fill or outline to see the shadow.", panel)
            .is_some(),
        "shape with non-drawable >50px outline width should show shadow note"
    );
}
