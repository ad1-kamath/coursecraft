//! Headless UI tests for Find and Replace.

use coursecraft_course::Course;
use coursecraft_engine::Session;
use egui::Key;
use serde_json::json;

use crate::test_harness::TestHarness;

fn frame() -> serde_json::Value {
    json!({ "x": 100.0, "y": 100.0, "w": 300.0, "h": 60.0 })
}

/// A course with a title "Cat course", a text box "Hello cat world" and a
/// button "Cat button", all on the first slide.
fn harness() -> TestHarness {
    let mut session = Session::blank("Cat course");
    session
        .execute(
            "object.add",
            &json!({ "slide": "slide1", "id": "plain", "kind": { "type": "text", "text": "Hello cat world" }, "frame": frame() }),
        )
        .expect("add text");
    session
        .execute(
            "object.add",
            &json!({ "slide": "slide1", "id": "btn", "kind": { "type": "button", "label": "Cat button" }, "frame": frame() }),
        )
        .expect("add button");
    let course: Course = session.course;
    TestHarness::new(course)
}

fn object_text(harness: &TestHarness, id: &str) -> String {
    let slide = harness
        .editor
        .session
        .course
        .play_slide("slide1")
        .expect("slide1");
    match slide.object(id).map(|(object, _)| &object.kind) {
        Some(coursecraft_course::ObjectKind::Text { text, .. }) => text.clone(),
        Some(coursecraft_course::ObjectKind::Button { label, .. }) => label.clone(),
        _ => panic!("no text object {id}"),
    }
}

#[test]
fn ctrl_f_opens_the_window_and_typing_shows_the_count() {
    let mut harness = harness();
    harness.idle_frame();
    assert!(!harness.editor.ui.find.open);

    harness.press(Key::F, egui::Modifiers::COMMAND);
    assert!(harness.editor.ui.find.open);

    harness.type_text("cat");
    harness.idle_frame();
    assert_eq!(harness.editor.ui.find.query, "cat");
    assert!(
        harness
            .find_text_containing("3 matches on 1 slide")
            .is_some(),
        "count line not shown"
    );
}

#[test]
fn escape_closes_the_window() {
    let mut harness = harness();
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    assert!(harness.editor.ui.find.open);
    harness.press(Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.editor.ui.find.open);
}

#[test]
fn no_matches_says_so() {
    let mut harness = harness();
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("zebra");
    harness.idle_frame();
    assert!(harness.find_text("No matches").is_some());
}

#[test]
fn clicking_a_row_selects_its_object() {
    let mut harness = harness();
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("hello");
    harness.idle_frame();
    // The canvas paints the same text first; the window is painted last.
    let row = harness
        .find_text_last("Hello cat world")
        .expect("result row");
    harness.click_at(row);
    assert_eq!(harness.editor.selection.object.as_deref(), Some("plain"));
    assert_eq!(harness.editor.selection.slide, "slide1");
}

#[test]
fn replace_all_then_undo_restores_the_text_and_the_list_refreshes() {
    let mut harness = harness();
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("cat");
    harness.editor.ui.find.replacement = "dog".to_owned();
    harness.idle_frame();

    let button = harness.find_text("Replace All").expect("Replace All");
    harness.click_at(button);
    assert_eq!(object_text(&harness, "plain"), "Hello dog world");
    assert_eq!(object_text(&harness, "btn"), "dog button");
    assert!(
        harness.find_text_containing("No matches").is_some(),
        "the list must refresh after a replace"
    );
    assert!(
        harness.find_text_containing("Replaced 3.").is_some(),
        "the result line is shown"
    );

    harness.editor.run("edit.undo", json!({})).expect("undo");
    harness.idle_frame();
    assert_eq!(object_text(&harness, "plain"), "Hello cat world");
    assert!(
        harness
            .find_text_containing("3 matches on 1 slide")
            .is_some(),
        "the list refreshes after undo"
    );
    assert!(
        harness.find_text_containing("Replaced 3.").is_none(),
        "the replace message clears after undo"
    );
    assert!(
        harness.editor.ui.find.message.is_none(),
        "the replace message is cleared"
    );
}

#[test]
fn typing_in_the_find_field_does_not_nudge_the_selection() {
    let mut harness = harness();
    harness.idle_frame();
    harness.editor.selection.set_many(["plain".to_owned()]);
    harness.idle_frame();
    let before = harness
        .editor
        .session
        .course
        .play_slide("slide1")
        .and_then(|slide| slide.object("plain"))
        .map(|(object, _)| object.frame.x)
        .expect("plain frame");

    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("a");
    harness.press(Key::ArrowRight, egui::Modifiers::NONE);
    harness.idle_frame();

    let after = harness
        .editor
        .session
        .course
        .play_slide("slide1")
        .and_then(|slide| slide.object("plain"))
        .map(|(object, _)| object.frame.x)
        .expect("plain frame");
    assert_eq!(before, after, "arrow keys in the field must not nudge");
    assert_eq!(harness.editor.ui.find.query, "a");
}
