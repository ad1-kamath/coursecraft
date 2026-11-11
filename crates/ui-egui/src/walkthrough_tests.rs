//! Headless "build hello-course through the UI" test (M3 Step 8b).
//!
//! Rebuilds the entirety of `examples/hello-course/course.json` through the egui UI
//! using synthetic pointer, keyboard, and text events on an initially blank editor.
//! Then verifies course validation, structure parity, and exact simulation behaviour
//! against `crates/render/tests/fixtures/hello-course.pass.effects.jsonl`.

use crate::canvas::canvas_rect;
use crate::test_harness::TestHarness;
use coursecraft_course::{Course, ObjectKind, ShapeKind};
use coursecraft_engine::Session;
use coursecraft_render::SlideTransform;
use egui::{Event, Key, Modifiers, Pos2, pos2, vec2};
use std::collections::HashMap;

// ----------------------------------------------------------------------------
// Low-level UI interaction helpers (driven through egui Events only)
// ----------------------------------------------------------------------------

fn insert_object(harness: &mut TestHarness, tool_name: &str) {
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness
        .find_text_last(tool_name)
        .unwrap_or_else(|| panic!("{tool_name} menu item"));
    harness.click_at(item);
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let c_rect = canvas_rect(central);
    let transform = SlideTransform::fit(
        c_rect.shrink(16.0),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let click_pos = transform.slide_rect().center();
    harness.click_at(click_pos);
    harness.idle_frame();
}

fn insert_shape_object(harness: &mut TestHarness, kind: ShapeKind) {
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let shape_menu = harness.find_text_last("Shape").expect("Shape menu item");
    harness.click_at(shape_menu);
    harness.idle_frame();
    let icon = harness
        .editor
        .ui
        .shape_gallery_rects
        .iter()
        .find_map(|(candidate, rect)| (*candidate == kind).then_some(*rect))
        .unwrap_or_else(|| panic!("{kind:?} gallery icon"));
    harness.click_at(icon.center());
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let c_rect = canvas_rect(central);
    let transform = SlideTransform::fit(
        c_rect.shrink(16.0),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let click_pos = transform.slide_rect().center();
    harness.click_at(click_pos);
    harness.idle_frame();
}

fn scroll_to_label(harness: &mut TestHarness, label: &str) -> Pos2 {
    for _ in 0..80 {
        let panel = harness.right_panel_rect();
        if let Some(position) = harness.find_text_in(label, panel) {
            return position;
        }
        harness.scroll_over(panel.center(), -120.0);
    }
    panic!("{label} label")
}

fn set_object_frame(harness: &mut TestHarness, x: f32, y: f32, w: f32, h: f32) {
    // Newer shape panels put Outline and Text above Frame; return to the top
    // then scroll down until the frame fields are on screen.
    let panel = harness.right_panel_rect();
    for _ in 0..12 {
        harness.scroll_over(panel.center(), 300.0);
    }
    let fx = scroll_to_label(harness, "X") + vec2(25.0, 0.0);
    harness.set_field(fx, &x.to_string());
    let fy = scroll_to_label(harness, "Y") + vec2(25.0, 0.0);
    harness.set_field(fy, &y.to_string());
    let fw = scroll_to_label(harness, "W") + vec2(25.0, 0.0);
    harness.set_field(fw, &w.to_string());
    let fh = scroll_to_label(harness, "H") + vec2(25.0, 0.0);
    harness.set_field(fh, &h.to_string());
}

fn set_object_name(harness: &mut TestHarness, name: &str) {
    if harness
        .find_text_in("Name", harness.right_panel_rect())
        .is_none_or(|p| p.y < 120.0)
    {
        harness.scroll_over(pos2(1100.0, 300.0), 300.0);
        harness.idle_frame();
    }
    let n = harness
        .find_text_in("Name", harness.right_panel_rect())
        .expect("Name label");
    let name_pos = n + vec2(110.0, 0.0);
    harness.set_field(name_pos, name);
}

fn set_multiline_text(harness: &mut TestHarness, label_pos: Pos2, text: &str) {
    harness.click_at(label_pos);
    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text(text.into()),
    ]);
    let frame_label = scroll_to_label(harness, "Frame");
    harness.click_at(frame_label);
    harness.idle_frame();
}

fn ensure_layers_panel_open(harness: &mut TestHarness) {
    if harness.editor.ui.inspector_tab != crate::InspectorTab::Slide
        && let Some(slide_tab) = harness.find_text("Slide")
    {
        harness.click_at(slide_tab);
        harness.idle_frame();
    }
}

fn add_layer(harness: &mut TestHarness, name: &str) {
    ensure_layers_panel_open(harness);
    let add_pos = harness
        .editor
        .ui
        .layers
        .add_button_rect
        .map(|r| r.center())
        .or_else(|| harness.find_text("+ New layer"))
        .expect("Add layer button");
    harness.click_at(add_pos);
    harness.idle_frame();

    // Adding selects the new layer; rename it from the row's right-click menu.
    let layer_id = harness
        .editor
        .selection
        .layer
        .clone()
        .expect("new layer selected");
    let row_rect = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get(&layer_id)
        .expect("new layer row");
    harness.right_click_at(row_rect.center());
    harness.idle_frame();
    let rename_item = harness.find_text_last("Rename").expect("Rename menu item");
    harness.click_at(rename_item);
    harness.idle_frame();
    let rename_rect = harness
        .editor
        .ui
        .layers
        .rename_edit_rect
        .expect("rename edit rect");
    harness.set_field(rename_rect.center(), name);
}

fn add_button_with_visited(
    harness: &mut TestHarness,
    name: &str,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    insert_object(harness, "Button");
    set_object_name(harness, name);
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, label);
    set_object_frame(harness, x, y, w, h);

    // Add state "visited"
    // Scroll down properties panel to ensure States section is in view
    harness.scroll_over(pos2(1100.0, 300.0), -900.0);
    harness.idle_frame();

    let state_name_rect = harness
        .editor
        .ui
        .states
        .new_state_name_rect
        .expect("new state name rect");
    harness.set_field(state_name_rect.center(), "visited");

    // Edit fill to #2E8B57
    // Scroll down properties panel to ensure States fill is in view
    harness.scroll_over(pos2(1100.0, 300.0), -200.0);
    for _ in 0..5 {
        harness.idle_frame();
    }

    let buf = harness
        .editor
        .ui
        .states
        .edit_buffers
        .get("visited")
        .expect("visited buffer");
    let fill_rect = buf.fill_rect.expect("fill rect");
    harness.click_at(fill_rect.center());
    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("#2E8B57".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    harness.idle_frame();
}

fn deselect_object_via_thumbnail(harness: &mut TestHarness, slide_id: &str) {
    let left_rect = harness.left_panel_rect();
    let thumb_rect =
        crate::slides::slide_thumbnail_rect(&harness.editor.session.course, left_rect, slide_id)
            .expect("thumbnail rect");
    harness.click_at(thumb_rect.center());
    harness.idle_frame();
}

// ----------------------------------------------------------------------------
// Task-level authoring builders
// ----------------------------------------------------------------------------

/// Task 1: Course settings (title, id, quiz passing score 80%).
fn configure_course_settings(harness: &mut TestHarness) {
    let proj = harness.find_text("Project").expect("Project menu");
    harness.click_at(proj);
    harness.idle_frame();
    let settings = harness
        .find_text("Course Settings")
        .expect("Course Settings menu item");
    harness.click_at(settings);
    // The window sizes itself over a few frames now that it holds more options.
    for _ in 0..4 {
        harness.idle_frame();
    }

    let title_pos = harness
        .find_text("Course Title")
        .map(|p| p + vec2(130.0, 0.0))
        .expect("Course Title field");
    harness.set_field(title_pos, "Workplace Safety Basics");

    let id_pos = harness
        .find_text("Course ID")
        .map(|p| p + vec2(130.0, 0.0))
        .expect("Course ID field");
    harness.set_field(id_pos, "hello-course");

    let quiz_chk = harness
        .find_text("Enable quiz grading and passing percentage")
        .expect("Quiz checkbox");
    harness.click_at(quiz_chk);
    harness.idle_frame();

    let close = harness.find_text_last("Close").expect("Close button");
    harness.click_at(close);
    harness.idle_frame();
}

/// Task 2: Slide 1 "Welcome" (title text, start button, about button, about layer and triggers).
fn build_intro_slide(harness: &mut TestHarness) {
    let title_pos = harness.find_text("Slide title").expect("Slide title label") + vec2(110.0, 0.0);
    harness.set_field(title_pos, "Welcome");

    let dur_pos = harness
        .find_text("5000")
        .or_else(|| {
            harness
                .editor
                .ui
                .timeline
                .duration_edit_rect
                .map(|r| r.center())
        })
        .expect("Duration field");
    harness.set_field(dur_pos, "4000");

    // Text object `title`
    insert_object(harness, "Text Box");
    set_object_name(harness, "title");
    set_object_frame(harness, 80.0, 120.0, 1120.0, 80.0);
    let size_pos = harness
        .find_text_in("Font size", harness.right_panel_rect())
        .expect("Font size label")
        + vec2(140.0, 0.0);
    harness.set_field(size_pos, "48");
    // Focusing the size field scrolled the inspector; bring the Style row back into view.
    let panel_mid = harness.right_panel_rect().center();
    harness.scroll_over(panel_mid, 400.0);
    let bold_pos = harness
        .find_text_in("B", harness.right_panel_rect())
        .expect("Bold button");
    harness.click_at(bold_pos);
    harness.idle_frame();
    let text_pos = harness
        .find_text("Text Content")
        .expect("Text Content label")
        + vec2(110.0, 0.0);
    set_multiline_text(harness, text_pos, "Workplace Safety Basics");

    // Button `btn-start`
    insert_object(harness, "Button");
    set_object_name(harness, "btn-start");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "Start");
    set_object_frame(harness, 80.0, 520.0, 200.0, 60.0);
    let start_pos = scroll_to_label(harness, "Start (ms)") + vec2(120.0, 0.0);
    harness.set_field(start_pos, "1000");

    // Button `btn-about`
    insert_object(harness, "Button");
    set_object_name(harness, "btn-about");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "About this course");
    let fill_pos = harness
        .find_text_in("Fill", harness.right_panel_rect())
        .expect("Fill label")
        + vec2(140.0, 0.0);
    harness.set_field(fill_pos, "#5A5A5A");
    set_object_frame(harness, 300.0, 520.0, 260.0, 60.0);

    // Layer "About": its settings live in the gear's window.
    add_layer(harness, "About");
    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("layer1")
        .expect("About layer gear");
    harness.click_at(gear.center());
    harness.idle_frame();

    let layer_dur_rect = harness
        .editor
        .ui
        .layer_settings
        .duration_rect
        .expect("layer duration rect");
    harness.set_field(layer_dur_rect.center(), "3000");

    let prev_chk = harness
        .find_text("Stop clicks on the base layer")
        .expect("Stop clicks checkbox");
    harness.click_at(prev_chk);
    harness.idle_frame();
    let ok = harness.find_text_last("OK").expect("Layer Settings OK");
    harness.click_at(ok);
    harness.idle_frame();

    // Rounded Rectangle `about-panel`
    insert_shape_object(harness, ShapeKind::RoundedRectangle);
    set_object_name(harness, "about-panel");
    let fill_pos = harness
        .find_text_in("Fill", harness.right_panel_rect())
        .expect("Fill label")
        + vec2(140.0, 0.0);
    harness.set_field(fill_pos, "#FFFFFFF0");
    set_object_frame(harness, 240.0, 140.0, 800.0, 440.0);

    // Text `about-text`
    insert_object(harness, "Text Box");
    set_object_name(harness, "about-text");
    let text_pos = harness
        .find_text("Text Content")
        .expect("Text Content label")
        + vec2(110.0, 0.0);
    set_multiline_text(harness, text_pos, "Ten minutes. One quiz. 80% to pass.");
    set_object_frame(harness, 280.0, 200.0, 720.0, 200.0);

    // Button `btn-close`
    insert_object(harness, "Button");
    set_object_name(harness, "btn-close");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "Close");
    set_object_frame(harness, 820.0, 500.0, 180.0, 56.0);

    // Layer trigger: hide-this-layer on click btn-close
    let trig_tab = harness.find_text("Triggers").expect("Triggers tab");
    harness.click_at(trig_tab);
    harness.idle_frame();

    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    harness.pick_combo("Jump to slide", "Hide layer");
    let current_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let current_label = crate::triggers::object_label(
        harness.editor.session.course.slide("slide1").unwrap(),
        &current_obj,
    );
    harness.pick_combo(&current_label, "btn-close (About)");
    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();

    // Base triggers on Slide 1:
    let base_row = harness
        .find_text("Base")
        .or_else(|| harness.find_text("Base layer"))
        .expect("Base layer item");
    harness.click_at(base_row);
    harness.idle_frame();

    // Trigger 1: jump-next on click btn-start
    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();
    let current_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let current_label = crate::triggers::object_label(
        harness.editor.session.course.slide("slide1").unwrap(),
        &current_obj,
    );
    harness.pick_combo(&current_label, "btn-start");
    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();

    // Trigger 2: show About on click btn-about
    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();
    harness.pick_combo("Jump to slide", "Show layer");
    let current_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let current_label = crate::triggers::object_label(
        harness.editor.session.course.slide("slide1").unwrap(),
        &current_obj,
    );
    harness.pick_combo(&current_label, "btn-about");
    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();
}

/// Task 3: Slide 2 "Three hazards" (tabs, next button, variable, layers, triggers).
fn build_tabs_slide(harness: &mut TestHarness) {
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let new_slide = harness.find_text("New Slide").expect("New Slide menu item");
    harness.click_at(new_slide);
    harness.idle_frame();

    let slide2_id = harness.editor.selection.slide.clone();

    // Slide 2 properties
    let title_pos = harness.find_text("Slide title").expect("Slide title label") + vec2(110.0, 0.0);
    harness.set_field(title_pos, "Three hazards");

    let dur_pos = harness
        .find_text("5000")
        .or_else(|| {
            harness
                .editor
                .ui
                .timeline
                .duration_edit_rect
                .map(|r| r.center())
        })
        .expect("Duration field");
    harness.set_field(dur_pos, "3000");

    // Add variable tabs_seen = 0
    let var_tab = harness.find_text("Variables").expect("Variables tab");
    harness.click_at(var_tab);
    harness.idle_frame();

    let add_var_label = harness
        .find_text("Add Variable")
        .expect("Add Variable label");
    harness.set_field(add_var_label + vec2(80.0, 0.0), "tabs_seen");

    let add_btn = harness
        .find_text("+ Add variable")
        .or_else(|| harness.find_text("+ Add"))
        .expect("+ Add variable button");
    harness.click_at(add_btn);
    harness.idle_frame();

    // Buttons tab1, tab2, tab3
    add_button_with_visited(harness, "tab1", "Slips", 80.0, 100.0, 220.0, 56.0);
    add_button_with_visited(harness, "tab2", "Fire", 320.0, 100.0, 220.0, 56.0);
    add_button_with_visited(harness, "tab3", "Electrical", 560.0, 100.0, 220.0, 56.0);

    // Button btn-next
    insert_object(harness, "Button");
    set_object_name(harness, "btn-next");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "Next");
    set_object_frame(harness, 1000.0, 600.0, 200.0, 60.0);

    harness.scroll_over(pos2(1100.0, 300.0), -900.0);

    let state_name_rect = harness
        .editor
        .ui
        .states
        .new_state_name_rect
        .expect("new state name rect");
    harness.set_field(state_name_rect.center(), "disabled");

    harness.scroll_over(pos2(1100.0, 300.0), -200.0);
    for _ in 0..5 {
        harness.idle_frame();
    }

    let buf = harness
        .editor
        .ui
        .states
        .edit_buffers
        .get("disabled")
        .expect("disabled buffer");
    let set_init_btn = buf.set_initial_rect.expect("set initial button rect");
    harness.click_at(set_init_btn.center());
    harness.idle_frame();

    let buf = harness
        .editor
        .ui
        .states
        .edit_buffers
        .get("disabled")
        .expect("disabled buffer");
    let fill_rect = buf.fill_rect.expect("fill rect");
    harness.click_at(fill_rect.center());
    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("#B0B0B0".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    harness.idle_frame();

    // Deselect object so Layers section is open
    deselect_object_via_thumbnail(harness, &slide2_id);

    // Layers "Slips", "Fire", "Electrical"
    let layers_data = [
        (
            "Slips",
            "l1-text",
            "Wipe spills right away and put up a sign.",
        ),
        ("Fire", "l2-text", "Know your two nearest exits."),
        ("Electrical", "l3-text", "Never overload a socket."),
    ];

    for (layer_name, text_obj_name, text_body) in layers_data {
        deselect_object_via_thumbnail(harness, &slide2_id);
        add_layer(harness, layer_name);

        insert_object(harness, "Text Box");
        set_object_name(harness, text_obj_name);
        let text_pos = harness
            .find_text("Text Content")
            .expect("Text Content label")
            + vec2(110.0, 0.0);
        set_multiline_text(harness, text_pos, text_body);
        set_object_frame(harness, 80.0, 200.0, 1000.0, 300.0);
    }

    // Base triggers on Slide 2:
    let base_row = harness
        .find_text("Base")
        .or_else(|| harness.find_text("Base layer"))
        .expect("Base layer item");
    harness.click_at(base_row);
    harness.idle_frame();

    let trig_tab = harness.find_text("Triggers").expect("Triggers tab");
    harness.click_at(trig_tab);
    harness.idle_frame();

    let tabs = ["tab1", "tab2", "tab3"];
    // 3× set tabs_seen += 1 on click tabN if tabN is not visited
    for tab in tabs {
        let new_trig = harness
            .find_text("+ Add trigger")
            .expect("New trigger button");
        harness.click_at(new_trig);
        harness.idle_frame();

        harness.pick_combo("Jump to slide", "Set variable");
        harness.pick_combo("Assign (=)", "Add (+)");
        let val_pos = harness.find_text("Value").expect("Value label") + vec2(115.0, 0.0);
        harness.set_field(val_pos, "1");

        let cur_obj = harness
            .editor
            .ui
            .trigger_form
            .as_ref()
            .unwrap()
            .event_object
            .clone();
        let cur_label = crate::triggers::object_label(
            harness.editor.session.course.slide(&slide2_id).unwrap(),
            &cur_obj,
        );
        harness.pick_combo(&cur_label, tab);

        let cond_chk = harness
            .find_text("Enable condition")
            .expect("Enable condition");
        harness.click_at(cond_chk);
        harness.idle_frame();

        let cur_cond_obj = match &harness
            .editor
            .ui
            .trigger_form
            .as_ref()
            .unwrap()
            .condition_root
        {
            crate::triggers::ConditionNode::ObjectState { object, .. } => object.clone(),
            _ => panic!("expected ObjectState condition"),
        };
        let cur_cond_label = crate::triggers::object_label(
            harness.editor.session.course.slide(&slide2_id).unwrap(),
            &cur_cond_obj,
        );
        harness.pick_combo(&cur_cond_label, tab);

        let ok_btn = harness.find_text_last("OK").expect("OK button");
        harness.click_at(ok_btn);
        harness.idle_frame();
    }

    // 3× show layer N on click tabN
    let show_data = [("tab1", "Slips"), ("tab2", "Fire"), ("tab3", "Electrical")];
    for (tab, layer) in show_data {
        let new_trig = harness
            .find_text("+ Add trigger")
            .expect("New trigger button");
        harness.click_at(new_trig);
        harness.idle_frame();

        harness.pick_combo("Jump to slide", "Show layer");
        let cur_layer = harness
            .editor
            .ui
            .trigger_form
            .as_ref()
            .unwrap()
            .show_layer_id
            .clone();
        let cur_layer_name = crate::triggers::layer_name(
            harness.editor.session.course.slide(&slide2_id).unwrap(),
            &cur_layer,
        );
        harness.pick_combo(&cur_layer_name, layer);

        let cur_obj = harness
            .editor
            .ui
            .trigger_form
            .as_ref()
            .unwrap()
            .event_object
            .clone();
        let cur_label = crate::triggers::object_label(
            harness.editor.session.course.slide(&slide2_id).unwrap(),
            &cur_obj,
        );
        harness.pick_combo(&cur_label, tab);

        let ok_btn = harness.find_text_last("OK").expect("OK button");
        harness.click_at(ok_btn);
        harness.idle_frame();
    }

    // Change btn-next to normal when tabs_seen changes if tabs_seen >= 3
    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    harness.pick_combo("User clicks", "Variable changes");
    harness.pick_combo("Jump to slide", "Change state");

    let cur_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .change_state_object
        .clone();
    let cur_label = crate::triggers::object_label(
        harness.editor.session.course.slide(&slide2_id).unwrap(),
        &cur_obj,
    );
    harness.pick_combo(&cur_label, "btn-next");

    let cond_chk = harness
        .find_text("Enable condition")
        .expect("Enable condition");
    harness.click_at(cond_chk);
    harness.idle_frame();

    harness.pick_combo("Object state", "Compare variable");
    let zero = harness
        .find_text_last("0")
        .expect("compare value DragValue");
    harness.set_field(zero, "3");

    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();

    // Jump next on click btn-next
    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    let cur_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let cur_label = crate::triggers::object_label(
        harness.editor.session.course.slide(&slide2_id).unwrap(),
        &cur_obj,
    );
    harness.pick_combo(&cur_label, "btn-next");

    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();
}

/// Task 4: Slide 3 "Check your knowledge" (submit button, correct/incorrect layers, question form).
fn build_quiz_slide(harness: &mut TestHarness) {
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let new_slide = harness.find_text("New Slide").expect("New Slide menu item");
    harness.click_at(new_slide);
    harness.idle_frame();

    let slide3_id = harness.editor.selection.slide.clone();

    // Slide 3 properties
    let title_pos = harness.find_text("Slide title").expect("Slide title label") + vec2(110.0, 0.0);
    harness.set_field(title_pos, "Check your knowledge");

    let dur_pos = harness
        .find_text("5000")
        .or_else(|| {
            harness
                .editor
                .ui
                .timeline
                .duration_edit_rect
                .map(|r| r.center())
        })
        .expect("Duration field");
    harness.set_field(dur_pos, "2000");

    // Deselect object so Layers section is open
    deselect_object_via_thumbnail(harness, &slide3_id);

    // Layer "Correct"
    add_layer(harness, "Correct");
    insert_object(harness, "Button");
    set_object_name(harness, "btn-continue-ok");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "Continue");
    set_object_frame(harness, 540.0, 420.0, 200.0, 60.0);

    // Trigger on layer Correct: jump next on click btn-continue-ok
    let trig_tab = harness.find_text("Triggers").expect("Triggers tab");
    harness.click_at(trig_tab);
    harness.idle_frame();

    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    let cur_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let cur_label = crate::triggers::object_label(
        harness.editor.session.course.slide(&slide3_id).unwrap(),
        &cur_obj,
    );
    harness.pick_combo(&cur_label, "btn-continue-ok (Correct)");
    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();

    // Layer "Incorrect"
    deselect_object_via_thumbnail(harness, &slide3_id);
    add_layer(harness, "Incorrect");
    insert_object(harness, "Button");
    set_object_name(harness, "btn-try-again");
    let label_pos = harness
        .find_text_in("Label", harness.right_panel_rect())
        .expect("Label label")
        + vec2(110.0, 0.0);
    harness.set_field(label_pos, "Try again");
    set_object_frame(harness, 540.0, 420.0, 200.0, 60.0);

    // Trigger on layer Incorrect: hide this layer on click btn-try-again
    let trig_tab = harness.find_text("Triggers").expect("Triggers tab");
    harness.click_at(trig_tab);
    harness.idle_frame();

    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    harness.pick_combo("Jump to slide", "Hide layer");
    let cur_obj = harness
        .editor
        .ui
        .trigger_form
        .as_ref()
        .unwrap()
        .event_object
        .clone();
    let cur_label = crate::triggers::object_label(
        harness.editor.session.course.slide(&slide3_id).unwrap(),
        &cur_obj,
    );
    harness.pick_combo(&cur_label, "btn-try-again (Incorrect)");
    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();

    // Question tab
    let q_tab = harness.find_text_last("Question").expect("Question tab");
    harness.click_at(q_tab);
    harness.idle_frame();

    let add_q_btn = harness
        .find_text_last("Question")
        .expect("Add Question button");
    let layers_before: Vec<String> = harness
        .editor
        .session
        .course
        .slide(&slide3_id)
        .map(|slide| slide.layers.iter().map(|layer| layer.id.clone()).collect())
        .unwrap_or_default();
    harness.click_at(add_q_btn);
    harness.idle_frame();

    // `question.insert` now seeds its own Correct/Incorrect feedback layers.
    // This walkthrough builds the course's richer feedback layers itself, so
    // remove the seeded pair again before recording the question's layer links.
    let seeded: Vec<String> = harness
        .editor
        .session
        .course
        .slide(&slide3_id)
        .map(|slide| {
            slide
                .layers
                .iter()
                .filter(|layer| !layers_before.contains(&layer.id))
                .map(|layer| layer.id.clone())
                .collect()
        })
        .unwrap_or_default();
    if !seeded.is_empty() {
        if let Some(mut question) = harness
            .editor
            .session
            .course
            .slide(&slide3_id)
            .and_then(|slide| slide.question.clone())
        {
            question.correct_layer = None;
            question.incorrect_layer = None;
            harness
                .editor
                .run(
                    "question.set",
                    serde_json::json!({ "slide": slide3_id.clone(), "question": question }),
                )
                .expect("clear seeded feedback links");
        }
        for layer in seeded {
            harness
                .editor
                .run(
                    "layer.delete",
                    serde_json::json!({ "slide": slide3_id.clone(), "layer": layer }),
                )
                .expect("remove seeded feedback layer");
        }
        harness.idle_frame();
    }

    // question.insert adds the Submit button and its click trigger. Turn the
    // default button into the course's btn-submit. The button is on the base
    // layer, so switch away from the Incorrect layer first.
    let base_pill = harness
        .find_text_in("Base", harness.central_panel_rect())
        .expect("Base layer pill");
    harness.click_at(base_pill);
    harness.idle_frame();
    let submit_pos = harness
        .find_text_in("Submit", harness.central_panel_rect())
        .expect("Submit button on the slide");
    harness.click_at(submit_pos);
    harness.idle_frame();
    set_object_name(harness, "btn-submit");
    set_object_frame(harness, 1000.0, 600.0, 200.0, 60.0);

    // Prompt
    let prompt_pos = harness.find_text("Prompt").expect("Prompt label") + vec2(0.0, 25.0);
    harness.click_at(prompt_pos);
    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("You see a spill in the corridor. What do you do first?".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
    ]);
    harness.idle_frame();

    // Choices
    let c1_pos = harness.find_text("Choice 1").expect("Choice 1 field");
    harness.set_field(c1_pos, "Walk around it");

    let c2_pos = harness.find_text("Choice 2").expect("Choice 2 field");
    harness.set_field(c2_pos, "Clean it or mark it, then report it");

    let add_c_btn = harness
        .find_text("+ Add choice")
        .expect("Add choice button");
    harness.click_at(add_c_btn);
    harness.idle_frame();

    if harness.find_text("c.").is_none() {
        harness.scroll_over(pos2(600.0, 750.0), -120.0);
        harness.idle_frame();
    }

    let c3_pos = harness.find_text("c.").expect("c. label") + vec2(60.0, 0.0);
    harness.set_field(c3_pos, "Email facilities tomorrow");

    // Mark choice b as correct
    let b_label = harness.find_text("b.").expect("b. label");
    harness.click_at(b_label + vec2(15.0, 0.0));
    harness.idle_frame();

    // Scroll down to reveal attempts and layer combos
    harness.scroll_over(pos2(600.0, 750.0), -200.0);

    // Attempts: 2
    let att_pos = harness
        .find_text("Attempts (0 = unlimited)")
        .map(|p| p + vec2(100.0, 0.0))
        .expect("Attempts field");
    harness.set_field(att_pos, "2");

    // Correct layer: Correct (first (None))
    let first_none = harness.find_text("(None)").expect("Correct layer combo");
    harness.click_at(first_none);
    harness.idle_frame();
    let corr_item = harness
        .find_text_last("Correct")
        .expect("Correct layer item");
    harness.click_at(corr_item);
    harness.idle_frame();

    // Incorrect layer: Incorrect (second (None))
    let second_none = harness.find_text("(None)").expect("Incorrect layer combo");
    harness.click_at(second_none);
    harness.idle_frame();
    let incorr_item = harness
        .find_text_last("Incorrect")
        .expect("Incorrect layer item");
    harness.click_at(incorr_item);
    harness.idle_frame();

    // The submit_question trigger on btn-submit was created with the question.
}

/// Task 5: Slide 4 "Results" (results text and submit results trigger).
fn build_results_slide(harness: &mut TestHarness) {
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let new_slide = harness.find_text("New Slide").expect("New Slide menu item");
    harness.click_at(new_slide);
    harness.idle_frame();

    // Slide 4 properties
    let title_pos = harness.find_text("Slide title").expect("Slide title label") + vec2(110.0, 0.0);
    harness.set_field(title_pos, "Results");

    let dur_pos = harness
        .find_text("5000")
        .or_else(|| {
            harness
                .editor
                .ui
                .timeline
                .duration_edit_rect
                .map(|r| r.center())
        })
        .expect("Duration field");
    harness.set_field(dur_pos, "1000");

    // Text object results-text
    insert_object(harness, "Text Box");
    set_object_name(harness, "results-text");
    let text_pos = harness
        .find_text("Text Content")
        .expect("Text Content label")
        + vec2(110.0, 0.0);
    set_multiline_text(harness, text_pos, "Thanks for completing the course.");
    set_object_frame(harness, 80.0, 200.0, 1120.0, 200.0);

    // Trigger submit_results on timeline_starts
    let trig_tab = harness.find_text("Triggers").expect("Triggers tab");
    harness.click_at(trig_tab);
    harness.idle_frame();

    let new_trig = harness
        .find_text("+ Add trigger")
        .expect("New trigger button");
    harness.click_at(new_trig);
    harness.idle_frame();

    harness.pick_combo("Jump to slide", "Submit results");
    harness.pick_combo("User clicks", "Timeline starts");

    let ok_btn = harness.find_text_last("OK").expect("OK button");
    harness.click_at(ok_btn);
    harness.idle_frame();
}

// ----------------------------------------------------------------------------
// Assertions: Validation, Structure, Behaviour
// ----------------------------------------------------------------------------

fn assert_objects_match(
    slide_idx: usize,
    built_objects: &[coursecraft_course::Object],
    expected_objects: &[coursecraft_course::Object],
) {
    assert_eq!(
        built_objects.len(),
        expected_objects.len(),
        "slide {slide_idx} objects count"
    );
    for (bo, eo) in built_objects.iter().zip(expected_objects) {
        assert_eq!(bo.name, eo.id, "slide {slide_idx} obj name vs id");
        assert_eq!(bo.frame, eo.frame, "slide {slide_idx} obj frame");
        assert_eq!(bo.timing, eo.timing, "slide {slide_idx} obj timing");
        assert_eq!(bo.states, eo.states, "slide {slide_idx} obj states");
        assert_eq!(
            bo.initial_state, eo.initial_state,
            "slide {slide_idx} obj initial_state"
        );
        match (&bo.kind, &eo.kind) {
            (
                ObjectKind::Text {
                    text: bt,
                    style: bs,
                    runs: br,
                    ..
                },
                ObjectKind::Text {
                    text: et,
                    style: es,
                    runs: er,
                    ..
                },
            ) => {
                assert_eq!(bt, et);
                if br == er {
                    assert_eq!(bs, es);
                } else {
                    let b_bold = br.first().and_then(|r| r.bold).unwrap_or(bs.bold);
                    let e_bold = er.first().and_then(|r| r.bold).unwrap_or(es.bold);
                    assert_eq!(b_bold, e_bold, "bold mismatch");

                    let b_size = br.first().and_then(|r| r.size).unwrap_or(bs.size);
                    let e_size = er.first().and_then(|r| r.size).unwrap_or(es.size);
                    assert_eq!(b_size, e_size, "size mismatch");

                    let b_italic = br.first().and_then(|r| r.italic).unwrap_or(bs.italic);
                    let e_italic = er.first().and_then(|r| r.italic).unwrap_or(es.italic);
                    assert_eq!(b_italic, e_italic, "italic mismatch");
                }
            }
            (
                ObjectKind::Button {
                    label: bl,
                    fill: bf,
                    ..
                },
                ObjectKind::Button {
                    label: el,
                    fill: ef,
                    ..
                },
            ) => {
                assert_eq!(bl, el);
                assert_eq!(bf, ef);
            }
            (
                ObjectKind::Shape {
                    shape: bsh,
                    fill: bf,
                    ..
                },
                ObjectKind::Shape {
                    shape: esh,
                    fill: ef,
                    ..
                },
            ) => {
                assert_eq!(bsh, esh);
                assert_eq!(bf, ef);
            }
            _ => panic!("slide {slide_idx} obj kind mismatch"),
        }
    }
}

fn assert_layers_match(
    slide_idx: usize,
    built_layers: &[coursecraft_course::Layer],
    expected_layers: &[coursecraft_course::Layer],
) {
    assert_eq!(
        built_layers.len(),
        expected_layers.len(),
        "slide {slide_idx} layers count"
    );
    for (bl, el) in built_layers.iter().zip(expected_layers) {
        assert_eq!(bl.name, el.name, "slide {slide_idx} layer name");
        assert_eq!(
            bl.duration_ms, el.duration_ms,
            "slide {slide_idx} layer duration"
        );
        assert_eq!(bl.prevent_base_clicks, el.prevent_base_clicks);
        assert_eq!(bl.hide_other_layers, el.hide_other_layers);
        assert_eq!(bl.pause_base_timeline, el.pause_base_timeline);

        assert_eq!(bl.objects.len(), el.objects.len());
        for (bo, eo) in bl.objects.iter().zip(&el.objects) {
            assert_eq!(bo.name, eo.id);
            assert_eq!(bo.frame, eo.frame);
        }
    }
}

fn assert_question_match(bs: &coursecraft_course::Slide, es: &coursecraft_course::Slide) {
    if let Some(ref eq) = es.question {
        let bq = bs.question.as_ref().expect("question on built slide");
        assert_eq!(bq.kind, eq.kind);
        assert_eq!(bq.prompt, eq.prompt);
        assert_eq!(bq.choices, eq.choices);
        assert_eq!(bq.points, eq.points);
        assert_eq!(bq.attempts, eq.attempts);

        let b_corr_name = bq
            .correct_layer
            .as_ref()
            .and_then(|id| bs.layer(id))
            .map(|l| l.name.as_str());
        let e_corr_name = eq
            .correct_layer
            .as_ref()
            .and_then(|id| es.layer(id))
            .map(|l| l.name.as_str());
        assert_eq!(b_corr_name, e_corr_name);

        let b_incorr_name = bq
            .incorrect_layer
            .as_ref()
            .and_then(|id| bs.layer(id))
            .map(|l| l.name.as_str());
        let e_incorr_name = eq
            .incorrect_layer
            .as_ref()
            .and_then(|id| es.layer(id))
            .map(|l| l.name.as_str());
        assert_eq!(b_incorr_name, e_incorr_name);
    } else {
        assert!(bs.question.is_none());
    }
}

fn assert_course_structure(built: &Course, expected: &Course) {
    assert_eq!(built.title, expected.title);
    assert_eq!(
        built.quiz.as_ref().map(|q| q.passing_percent),
        expected.quiz.as_ref().map(|q| q.passing_percent)
    );
    assert_eq!(built.variables.len(), expected.variables.len());
    for (bv, ev) in built.variables.iter().zip(&expected.variables) {
        assert_eq!(bv.name, ev.name);
        assert_eq!(bv.value, ev.value);
    }

    assert_eq!(built.scenes.len(), 1);
    let built_slides = &built.scenes[0].slides;
    let expected_slides = &expected.scenes[0].slides;
    assert_eq!(built_slides.len(), 4);
    assert_eq!(built_slides.len(), expected_slides.len());

    for (slide_idx, (bs, es)) in built_slides.iter().zip(expected_slides).enumerate() {
        assert_eq!(bs.title, es.title, "slide {slide_idx} title");
        assert_eq!(bs.duration_ms, es.duration_ms, "slide {slide_idx} duration");

        assert_objects_match(slide_idx, &bs.objects, &es.objects);
        assert_layers_match(slide_idx, &bs.layers, &es.layers);
        assert_question_match(bs, es);

        assert_eq!(
            bs.triggers.len(),
            es.triggers.len(),
            "slide {slide_idx} triggers count"
        );
    }
}

fn assert_course_behaviour(course: &Course) {
    let inputs_json = include_str!("../../../examples/hello-course/pass.inputs.json");
    let golden_jsonl = include_str!("../../render/tests/fixtures/hello-course.pass.effects.jsonl");

    let mut obj_name_to_id = HashMap::new();
    let mut obj_id_to_name: HashMap<(String, String), String> = HashMap::new();
    let mut layer_id_to_hello_id: HashMap<(String, String), String> = HashMap::new();
    let mut slide_id_to_hello_id = HashMap::new();

    for scene in &course.scenes {
        for slide in &scene.slides {
            let hello_slide_id = match slide.title.as_str() {
                "Welcome" => "s-intro",
                "Three hazards" => "s-tabs",
                "Check your knowledge" => "s-quiz",
                "Results" => "s-results",
                other => panic!("unexpected slide title: {other}"),
            };
            slide_id_to_hello_id.insert(slide.id.clone(), hello_slide_id.to_owned());

            for obj in &slide.objects {
                obj_name_to_id.insert(obj.name.clone(), obj.id.clone());
                obj_id_to_name.insert((slide.id.clone(), obj.id.clone()), obj.name.clone());
            }

            for layer in &slide.layers {
                let hello_layer_id = match layer.name.as_str() {
                    "About" => "about",
                    "Slips" => "l1",
                    "Fire" => "l2",
                    "Electrical" => "l3",
                    "Correct" => "correct",
                    "Incorrect" => "incorrect",
                    other => panic!("unexpected layer name: {other}"),
                };
                layer_id_to_hello_id.insert(
                    (slide.id.clone(), layer.id.clone()),
                    hello_layer_id.to_owned(),
                );

                for obj in &layer.objects {
                    obj_name_to_id.insert(obj.name.clone(), obj.id.clone());
                    obj_id_to_name.insert((slide.id.clone(), obj.id.clone()), obj.name.clone());
                }
            }
        }
    }

    let raw_inputs: Vec<serde_json::Value> =
        serde_json::from_str(inputs_json).expect("valid inputs json");
    let mut inputs = Vec::new();
    for raw in raw_inputs {
        let mut modified = raw.clone();
        if let Some(obj_name) = raw.get("object").and_then(|v| v.as_str()) {
            let real_id = obj_name_to_id
                .get(obj_name)
                .unwrap_or_else(|| panic!("object '{obj_name}' not found in UI course"));
            modified["object"] = serde_json::json!(real_id);
        }
        let input: coursecraft_logic::Input =
            serde_json::from_value(modified).expect("deserialize input");
        inputs.push(input);
    }

    let mut player = coursecraft_logic::Player::new(course).expect("valid player");
    let mut actual_effects = Vec::new();
    let mut current_slide = String::new();

    for input in &inputs {
        for effect in player.handle(input) {
            let translated = match effect {
                coursecraft_logic::Effect::SlideEntered { ref slide } => {
                    current_slide = slide.clone();
                    let s = slide_id_to_hello_id.get(slide).unwrap_or(slide);
                    coursecraft_logic::Effect::SlideEntered { slide: s.clone() }
                }
                coursecraft_logic::Effect::LayerShown { layer } => {
                    let l = layer_id_to_hello_id
                        .get(&(current_slide.clone(), layer.clone()))
                        .unwrap_or(&layer);
                    coursecraft_logic::Effect::LayerShown { layer: l.clone() }
                }
                coursecraft_logic::Effect::LayerHidden { layer } => {
                    let l = layer_id_to_hello_id
                        .get(&(current_slide.clone(), layer.clone()))
                        .unwrap_or(&layer);
                    coursecraft_logic::Effect::LayerHidden { layer: l.clone() }
                }
                coursecraft_logic::Effect::StateChanged { object, state } => {
                    let o = obj_id_to_name
                        .get(&(current_slide.clone(), object.clone()))
                        .unwrap_or(&object);
                    coursecraft_logic::Effect::StateChanged {
                        object: o.clone(),
                        state,
                    }
                }
                coursecraft_logic::Effect::QuestionAnswered {
                    slide,
                    correct,
                    points,
                    attempts_left,
                } => {
                    let s = slide_id_to_hello_id.get(&slide).unwrap_or(&slide);
                    coursecraft_logic::Effect::QuestionAnswered {
                        slide: s.clone(),
                        correct,
                        points,
                        attempts_left,
                    }
                }
                other => other,
            };
            let json = serde_json::to_string(&translated).expect("serialize effect");
            actual_effects.push(json);
        }
    }

    let expected_lines: Vec<&str> = golden_jsonl
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(
        actual_effects.len(),
        expected_lines.len(),
        "effect count mismatch: actual {} vs expected {}",
        actual_effects.len(),
        expected_lines.len()
    );

    for (i, (act, exp)) in actual_effects.iter().zip(&expected_lines).enumerate() {
        assert_eq!(act, exp, "effect mismatch at step {i}");
    }
}

// ----------------------------------------------------------------------------
// The main walkthrough test
// ----------------------------------------------------------------------------

#[test]
fn walkthrough_builds_hello_course_through_ui() {
    let blank = Session::blank("Untitled course").course;
    let mut harness = TestHarness::new_with_size(blank, (1280.0, 860.0));
    harness.idle_frame();

    // 1. Course settings
    configure_course_settings(&mut harness);

    // 2. Slide 1 "Welcome"
    build_intro_slide(&mut harness);

    // 3. Slide 2 "Three hazards"
    build_tabs_slide(&mut harness);

    // 4. Slide 3 "Check your knowledge"
    build_quiz_slide(&mut harness);

    // 5. Slide 4 "Results"
    build_results_slide(&mut harness);

    let built_course = &harness.editor.session.course;

    // 6. Validation: course.validate() has no errors
    let issues = built_course.validate();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == coursecraft_course::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "validation errors: {errors:?}");

    // 7. Structure parity with examples/hello-course/course.json
    let expected_json = include_str!("../../../examples/hello-course/course.json");
    let expected_course = Course::from_json(expected_json).expect("valid hello-course");
    assert_course_structure(built_course, &expected_course);

    // 8. Behaviour parity: run pass.inputs.json and verify effect stream matches golden effects exactly
    assert_course_behaviour(built_course);
}
