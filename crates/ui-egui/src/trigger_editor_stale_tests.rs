//! The trigger editor must not save over a different trigger when the list
//! changed under it (Undo or Redo while the editor is open).

use crate::test_harness::TestHarness;
use crate::triggers::TriggerFormState;
use coursecraft_course::{Action, Course, Event, Trigger};
use serde_json::json;

fn url_trigger(url: &str) -> Trigger {
    Trigger {
        action: Action::OpenUrl {
            url: url.to_owned(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    }
}

fn course_with_three_triggers() -> Course {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    let mut course = Course::from_json(&json).expect("valid course");
    let slide = course.slide_mut("s-intro").expect("s-intro");
    slide.triggers = vec![
        url_trigger("https://t0.example"),
        url_trigger("https://t1.example"),
        url_trigger("https://t2.example"),
    ];
    course
}

fn open_editor(harness: &mut TestHarness, index: usize) -> TriggerFormState {
    let course = &harness.editor.session.course;
    let slide = course.slide("s-intro").expect("s-intro");
    let trigger = slide.triggers.get(index).expect("trigger");
    TriggerFormState::from_existing(index, trigger, slide, None, course)
}

fn urls(harness: &TestHarness) -> Vec<String> {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .triggers
        .iter()
        .map(|t| match &t.action {
            Action::OpenUrl { url } => url.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn save_after_undo_does_not_overwrite_another_trigger() {
    let mut harness = TestHarness::new(course_with_three_triggers());
    harness.editor.selection.slide = "s-intro".into();
    harness
        .editor
        .run("trigger.delete", json!({"slide": "s-intro", "index": 0}))
        .expect("delete T0");

    // Open T2, which now sits at index 1.
    let mut form = open_editor(&mut harness, 1);
    assert_eq!(form.open_url, "https://t2.example");
    form.open_url = "https://t2-edited.example".to_owned();

    // Undo brings T0 back, so index 1 is T1 again.
    harness.editor.run("edit.undo", json!({}));
    assert_eq!(
        urls(&harness),
        [
            "https://t0.example",
            "https://t1.example",
            "https://t2.example"
        ]
    );

    let saved = form.commit_ok(&mut harness.editor);
    let after = urls(&harness);
    assert_eq!(
        after[1], "https://t1.example",
        "T1 was overwritten: {after:?}"
    );
    // Either the edit reaches T2 or the save is refused with a message.
    if saved {
        assert_eq!(after[2], "https://t2-edited.example");
    } else {
        assert!(form.error_message.is_some());
        assert_eq!(after[2], "https://t2.example");
    }
}

#[test]
fn save_refuses_when_the_trigger_was_removed() {
    let mut harness = TestHarness::new(course_with_three_triggers());
    harness.editor.selection.slide = "s-intro".into();
    let mut form = open_editor(&mut harness, 2);
    form.open_url = "https://t2-edited.example".to_owned();
    harness
        .editor
        .run("trigger.delete", json!({"slide": "s-intro", "index": 2}))
        .expect("delete T2");
    harness
        .editor
        .run(
            "trigger.add",
            json!({"slide": "s-intro", "trigger": url_trigger("https://t3.example")}),
        )
        .expect("add T3");

    assert!(!form.commit_ok(&mut harness.editor));
    assert!(form.error_message.is_some());
    assert_eq!(
        urls(&harness),
        [
            "https://t0.example",
            "https://t1.example",
            "https://t3.example"
        ]
    );
}

#[test]
fn plain_edit_still_saves_at_its_index() {
    let mut harness = TestHarness::new(course_with_three_triggers());
    harness.editor.selection.slide = "s-intro".into();
    let mut form = open_editor(&mut harness, 1);
    form.open_url = "https://t1-edited.example".to_owned();
    assert!(form.commit_ok(&mut harness.editor));
    assert_eq!(
        urls(&harness),
        [
            "https://t0.example",
            "https://t1-edited.example",
            "https://t2.example"
        ]
    );
}

#[test]
fn open_editor_closes_with_a_message_when_its_trigger_is_undone() {
    let mut harness = TestHarness::new(course_with_three_triggers());
    harness.editor.selection.slide = "s-intro".into();
    harness
        .editor
        .run(
            "trigger.add",
            json!({"slide": "s-intro", "trigger": url_trigger("https://t3.example")}),
        )
        .expect("add T3");
    let form = open_editor(&mut harness, 3);
    harness.editor.ui.trigger_form = Some(form);
    harness.idle_frame();
    assert!(harness.editor.ui.trigger_form.is_some());
    harness.editor.ui.last_error = None;

    harness.editor.run("edit.undo", json!({}));
    harness.idle_frame();
    assert!(harness.editor.ui.trigger_form.is_none());
    assert!(harness.editor.ui.last_error.is_some());
}
