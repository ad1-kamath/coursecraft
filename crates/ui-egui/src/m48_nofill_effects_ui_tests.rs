use crate::dock::Panel;
use crate::panels::{Section, open_at};
use crate::test_harness::TestHarness;
use coursecraft_course::{Course, Object};
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
    let panel = harness.right_panel_rect();
    let control_x = panel.min.x + 144.0;
    let fill_label = harness.find_text_in("Fill", panel).expect("Fill label");
    harness.click_at(pos2(control_x, fill_label.y));
    harness.idle_frame();

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

    assert_eq!(first_object(&harness).shadow, original_shadow);
    assert_eq!(first_object(&harness).glow, original_glow);

    scroll_right_panel_to(&mut harness, "Effects");
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
