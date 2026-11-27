//! Headless UI tests for Step f24: Exit animation effects are named as exits
//! ("Fly out", "Fade out", "Zoom out", "Wipe out", "Float out") everywhere in
//! the UI while keeping stored `AnimEffect` ids unchanged.

use crate::properties::InspectorTab;
use crate::test_harness::TestHarness;
use coursecraft_course::{AnimEffect, Animation, Course, Direction, Easing};

fn hello() -> Course {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    Course::from_json(&json).expect("valid course")
}

fn select_button_animate(harness: &mut TestHarness) {
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.set_many(["btn-start"]);
    harness.editor.ui.inspector_tab = InspectorTab::Animate;
    harness.idle_frame();
    harness.idle_frame();
}

/// Find the "None" (or current label) effect picker in the Exit Animation section
/// by looking below the "Exit Animation" header inside the right panel.
fn find_text_below_in_inspector(
    harness: &mut TestHarness,
    needle: &str,
    min_y: f32,
) -> Option<egui::Pos2> {
    let panel = harness.right_panel_rect();
    let mut best: Option<egui::Pos2> = None;
    for pos in harness.find_texts(needle) {
        if panel.contains(pos) && pos.y > min_y && best.is_none_or(|b| pos.y < b.y) {
            best = Some(pos);
        }
    }
    best
}

#[test]
fn exit_effect_dropdown_shows_exit_names_and_entrance_shows_entrance_names() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 900.0));
    select_button_animate(&mut harness);

    let ent_header = harness
        .find_text_in("Entrance animation", harness.right_panel_rect())
        .expect("Entrance animation header");

    // Open Entrance Effect dropdown.
    let ent_combo = find_text_below_in_inspector(&mut harness, "None", ent_header.y)
        .expect("entrance effect combo");
    harness.click_at(ent_combo);
    harness.idle_frame();
    harness.idle_frame();

    for expected in [
        "None", "Fade in", "Fly in", "Zoom in", "Wipe in", "Float in",
    ] {
        assert!(
            harness.find_text_last(expected).is_some(),
            "entrance dropdown should contain '{expected}'"
        );
    }

    // Select "Fly in" for entrance.
    let fly_in = harness.find_text_last("Fly in").expect("Fly in option");
    harness.click_at(fly_in);
    harness.idle_frame();

    // The Entrance and Emphasis cards push Exit down: scroll the inspector to it.
    let panel = harness.right_panel_rect();
    harness.scroll_over(panel.center(), -300.0);
    harness.idle_frame();
    let exit_header = harness
        .find_text_in("Exit animation", harness.right_panel_rect())
        .expect("Exit animation header after scrolling");

    // Now open Exit Effect dropdown.
    let exit_combo = find_text_below_in_inspector(&mut harness, "None", exit_header.y)
        .expect("exit effect combo");
    harness.click_at(exit_combo);
    // The effect gallery lays out its scroll area on its first frame.
    harness.idle_frame();
    harness.idle_frame();

    for expected in [
        "None",
        "Fade out",
        "Fly out",
        "Zoom out",
        "Wipe out",
        "Float out",
    ] {
        assert!(
            harness.find_text_last(expected).is_some(),
            "exit dropdown should contain '{expected}'"
        );
    }

    // Select "Fly out" for exit and verify stored fly_in is unchanged.
    let fly_out = harness.find_text_last("Fly out").expect("Fly out option");
    harness.click_at(fly_out);
    harness.idle_frame();

    let obj = &harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|s| s.object("btn-start"))
        .expect("btn-start")
        .0;
    assert!(
        matches!(
            obj.exit.as_ref().map(|a| &a.effect),
            Some(e) if e.id == "fly_in"
        ),
        "selecting 'Fly out' stores fly_in: {:?}",
        obj.exit
    );

    // Selected text on the closed exit combo should now read "Fly out".
    assert!(
        find_text_below_in_inspector(&mut harness, "Fly out", exit_header.y).is_some(),
        "closed exit combo should display 'Fly out'"
    );
}

#[test]
fn existing_course_with_fly_in_exit_loads_and_shows_fly_out() {
    let mut course = hello();
    let slide = course.slide_mut("s-intro").expect("s-intro");
    let obj = slide
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-start")
        .expect("btn-start");
    obj.entrance = Some(Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::EaseOut,
        sequence: Default::default(),
    });
    obj.exit = Some(Animation {
        effect: AnimEffect::with_from("fly_in", Direction::Right),
        duration_ms: 600,
        easing: Easing::EaseOut,
        sequence: Default::default(),
    });

    // Round-trip through JSON to verify no file format change.
    let json = course.to_json().expect("serialize");
    let reloaded = Course::from_json(&json).expect("deserialize");

    let mut harness = TestHarness::new_with_size(reloaded, (1280.0, 900.0));
    select_button_animate(&mut harness);

    let ent_header = harness
        .find_text_in("Entrance animation", harness.right_panel_rect())
        .expect("Entrance animation header");
    let exit_header = harness
        .find_text_in("Exit animation", harness.right_panel_rect())
        .expect("Exit animation header");

    assert!(
        find_text_below_in_inspector(&mut harness, "Fade in", ent_header.y).is_some(),
        "entrance Fade should display 'Fade in'"
    );
    assert!(
        find_text_below_in_inspector(&mut harness, "Fly out", exit_header.y).is_some(),
        "exit fly_in should display 'Fly out'"
    );
}

#[test]
fn multi_selection_shows_entrance_and_exit_names_and_undo_redo() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 900.0));
    harness.editor.selection.slide = "s-intro".into();
    let ids: Vec<String> = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .objects
        .iter()
        .take(2)
        .map(|o| o.id.clone())
        .collect();
    assert_eq!(ids.len(), 2, "need at least 2 objects on s-intro");
    harness.editor.selection.set_many(ids.clone());
    harness.editor.ui.inspector_tab = InspectorTab::Animate;
    harness.idle_frame();
    harness.idle_frame();

    let exit_header = harness
        .find_text_in("Exit animation", harness.right_panel_rect())
        .expect("Exit animation header");
    let exit_combo = find_text_below_in_inspector(&mut harness, "None", exit_header.y)
        .expect("exit effect combo");
    harness.click_at(exit_combo);
    // The effect gallery lays out its scroll area on its first frame.
    harness.idle_frame();
    harness.idle_frame();

    let zoom_out = harness.find_text_last("Zoom out").expect("Zoom out option");
    harness.click_at(zoom_out);
    harness.idle_frame();

    for id in &ids {
        let obj = harness
            .editor
            .session
            .course
            .slide("s-intro")
            .and_then(|s| s.object(id))
            .expect("obj")
            .0;
        assert_eq!(
            obj.exit.as_ref().map(|a| &a.effect),
            Some(&AnimEffect::new("zoom")),
            "object {id} should have Zoom exit"
        );
    }
    assert!(
        find_text_below_in_inspector(&mut harness, "Zoom out", exit_header.y).is_some(),
        "multi-selection exit combo should display 'Zoom out'"
    );

    // Undo and redo preserve the label (multi-selection batches into one undo step).
    assert!(
        harness
            .editor
            .run("edit.undo", serde_json::json!({}))
            .is_some()
    );
    harness.idle_frame();
    assert!(
        find_text_below_in_inspector(&mut harness, "None", exit_header.y).is_some(),
        "after undo exit combo should display 'None'"
    );
    assert!(
        harness
            .editor
            .run("edit.redo", serde_json::json!({}))
            .is_some()
    );
    harness.idle_frame();
    assert!(
        find_text_below_in_inspector(&mut harness, "Zoom out", exit_header.y).is_some(),
        "after redo exit combo should display 'Zoom out'"
    );
}

#[test]
fn the_emphasis_card_adds_and_lists_an_emphasis() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 900.0));
    select_button_animate(&mut harness);
    let add = harness
        .find_text_in("+ Add emphasis", harness.right_panel_rect())
        .expect("Add emphasis button");
    harness.click_at(add);
    harness.idle_frame();
    let tile = harness.find_text_last("Heartbeat").expect("Heartbeat tile");
    harness.click_at(tile);
    harness.idle_frame();
    let emphasis = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|s| s.object("btn-start"))
        .map(|(o, _)| o.emphasis.clone())
        .expect("btn-start");
    assert_eq!(emphasis.len(), 1);
    assert_eq!(emphasis[0].effect.id, "heartbeat");
    assert!(
        harness
            .find_text_in("Heartbeat", harness.right_panel_rect())
            .is_some(),
        "the card lists it"
    );
}

/// `N96`: dragging an emphasis field keeps the dragged value until the drag ends.
#[test]
fn dragging_the_emphasis_duration_keeps_the_dragged_value() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 900.0));
    select_button_animate(&mut harness);
    let add = harness
        .find_text_in("+ Add emphasis", harness.right_panel_rect())
        .expect("Add emphasis button");
    harness.click_at(add);
    harness.idle_frame();
    let tile = harness.find_text_last("Heartbeat").expect("Heartbeat tile");
    harness.click_at(tile);
    harness.idle_frame();
    let duration = |h: &TestHarness| {
        h.editor
            .session
            .course
            .slide("s-intro")
            .and_then(|s| s.object("btn-start"))
            .map(|(o, _)| o.emphasis[0].duration_ms)
            .expect("emphasis")
    };
    let before = duration(&harness);
    // The emphasis card sits below the entrance card, so its row is the last one.
    let label = harness
        .find_texts("Duration")
        .last()
        .copied()
        .expect("Duration row");
    harness.drag_by(
        egui::pos2(label.x + 130.0, label.y),
        egui::vec2(60.0, 0.0),
        6,
        egui::Modifiers::NONE,
    );
    harness.idle_frame();
    assert!(
        duration(&harness) >= before + 2000,
        "a 60 px drag must add about 3 s, got {} from {before}",
        duration(&harness)
    );
}
