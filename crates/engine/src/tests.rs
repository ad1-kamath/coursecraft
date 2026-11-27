use coursecraft_course::{ObjectKind, ShapeKind};
use serde_json::{Value as Json, json};

use crate::{EngineError, Session, registry};

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    s.execute(id, &p)
        .unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

#[test]
fn course_map_returns_the_graph_as_json() {
    let mut s = Session::blank("Map");
    ok(&mut s, "slide.add", json!({ "title": "Two" }));
    ok(&mut s, "slide.add", json!({ "title": "Three" }));
    let map = ok(&mut s, "course.map", json!({}));
    assert_eq!(map["scenes"].as_array().map(Vec::len), Some(1));
    let slides = &map["scenes"][0]["slides"];
    assert_eq!(slides.as_array().map(Vec::len), Some(3));
    assert_eq!(slides[0]["unreachable"], false);
    let edges = map["edges"].as_array().expect("edges array");
    assert_eq!(edges.len(), 2, "two Next edges: {edges:?}");
    assert_eq!(edges[0]["kind"], "next");
    assert_eq!(edges[0]["via"], "player_next");
    assert_eq!(edges[0]["from"], "slide1");
    assert_eq!(edges[0]["to"], edges[1]["from"]);
    // A non-mutating command makes no undo step.
    assert_eq!(s.undo_len(), 2, "only the two slide.add steps");
}

#[test]
fn course_map_reorder_is_one_undoable_move() {
    let mut s = Session::blank("Map");
    ok(&mut s, "slide.add", json!({ "title": "Two" }));
    ok(&mut s, "slide.add", json!({ "title": "Three" }));
    // The reorder the Course map performs: slide3 to the front, cross-check
    // the map sees it, then undo it.
    ok(
        &mut s,
        "slide.move",
        json!({ "slide": "slide3", "scene": "scene1", "index": 0 }),
    );
    let order: Vec<&str> = s.course.slides().map(|slide| slide.id.as_str()).collect();
    assert_eq!(order, vec!["slide3", "slide1", "slide2"]);
    let map = ok(&mut s, "course.map", json!({}));
    assert_eq!(map["scenes"][0]["slides"][0]["id"], "slide3");
    ok(&mut s, "edit.undo", json!({}));
    let order: Vec<&str> = s.course.slides().map(|slide| slide.id.as_str()).collect();
    assert_eq!(order, vec!["slide1", "slide2", "slide3"]);
}

#[test]
fn registry_ids_are_unique_and_documented() {
    let mut ids = std::collections::BTreeSet::new();
    for c in registry() {
        assert!(ids.insert(c.id), "duplicate command id {}", c.id);
        assert!(c.id.contains('.'), "{} should be namespaced", c.id);
        assert!(!c.label.is_empty() && !c.params.is_empty(), "{}", c.id);
    }
}

#[test]
fn add_and_remove_font_faces_with_undo() {
    let mut s = Session::blank("Fonts");
    let result = ok(
        &mut s,
        "course.add_font",
        json!({
            "src": "fonts/lato.ttf",
            "family": "  Lato  ",
            "style": "regular",
        }),
    );
    assert_eq!(result["font"]["family"], "Lato");
    assert_eq!(s.course.fonts.len(), 1);
    assert_eq!(s.course.fonts[0].src, "fonts/lato.ttf");

    let duplicate = s.execute(
        "course.add_font",
        &json!({
            "src": "fonts/lato-2.ttf",
            "family": "lato",
            "style": "regular",
        }),
    );
    assert!(duplicate.is_err(), "duplicate family/style must fail");

    ok(&mut s, "edit.undo", json!({}));
    assert!(s.course.fonts.is_empty(), "undo must remove the face");
    ok(&mut s, "edit.redo", json!({}));
    assert_eq!(s.course.fonts.len(), 1);

    ok(
        &mut s,
        "course.remove_font",
        json!({ "family": "Lato", "style": "regular" }),
    );
    assert!(s.course.fonts.is_empty());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.fonts.len(), 1, "undo must restore the face");
}

#[test]
fn add_font_rejects_bad_paths_and_too_many_faces() {
    let mut s = Session::blank("Fonts");
    for src in ["../font.ttf", "photos/font.ttf", "fonts/font.png", "fonts/"] {
        let result = s.execute(
            "course.add_font",
            &json!({ "src": src, "family": "Lato", "style": "regular" }),
        );
        assert!(result.is_err(), "{src} must be rejected");
    }
    for i in 0..max_faces() {
        ok(
            &mut s,
            "course.add_font",
            json!({
                "src": format!("fonts/font{i}.ttf"),
                "family": format!("Family {i}"),
                "style": "regular",
            }),
        );
    }
    let result = s.execute(
        "course.add_font",
        &json!({
            "src": "fonts/extra.ttf",
            "family": "Extra",
            "style": "regular",
        }),
    );
    assert!(result.is_err(), "the 41st face must be rejected");
}

fn max_faces() -> usize {
    coursecraft_course::MAX_FONT_FACES
}

#[test]
fn build_a_course_with_commands_only() {
    let mut s = Session::blank("Fire Safety");
    let slide2 = ok(&mut s, "slide.add", json!({ "title": "Quiz" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let btn = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Next" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": { "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": btn } }
        }),
    );
    ok(
        &mut s,
        "question.set",
        json!({
            "slide": slide2,
            "question": { "kind": "multiple_choice", "prompt": "Pull the pin first?", "choices": [
                { "id": "y", "text": "Yes", "correct": true }, { "id": "n", "text": "No" } ] }
        }),
    );
    assert!(
        s.course
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error),
        "{:#?}",
        s.course.validate()
    );
    // question.set wired up grading automatically
    assert_eq!(s.course.slide(&slide2).unwrap().triggers.len(), 1);
    assert_eq!(s.revision, 4);
}

#[test]
fn rotate_and_flip_work_on_one_and_many_objects_with_undo() {
    let mut s = Session::blank("Rotation");
    let mut ids = Vec::new();
    for (x, y) in [(10, 10), (200, 10)] {
        let id = ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": x, "y": y, "w": 100, "h": 80 }
            }),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ids.push(id);
    }
    let first = ids.first().unwrap().clone();
    let second = ids.get(1).unwrap().clone();

    let before = s.undo_len();
    ok(
        &mut s,
        "object.set_rotation",
        json!({ "slide": "slide1", "object": first.clone(), "degrees": -30 }),
    );
    let object = s.course.slide("slide1").unwrap().object(&first).unwrap().0;
    assert!((object.rotation - 330.0).abs() < 0.001);
    assert_eq!(s.undo_len(), before + 1);

    ok(
        &mut s,
        "object.set_rotation",
        json!({ "slide": "slide1", "objects": ids.clone(), "degrees": 450 }),
    );
    for id in [&first, &second] {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!((object.rotation - 90.0).abs() < 0.001);
    }
    assert_eq!(s.undo_len(), before + 2);

    ok(
        &mut s,
        "object.flip",
        json!({ "slide": "slide1", "objects": [first.clone(), second.clone()], "axis": "horizontal" }),
    );
    for id in [&first, &second] {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!(object.flip_h);
        assert!(!object.flip_v);
    }

    ok(&mut s, "edit.undo", json!({}));
    for id in [&first, &second] {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!(!object.flip_h);
        assert!((object.rotation - 90.0).abs() < 0.001);
    }
}

const TRANSLATION_XLIFF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" datatype="plaintext" source-language="en" target-language="fr">
    <body>
      <trans-unit id="course/title" xml:space="preserve"><source>T</source><target>Traduit</target></trans-unit>
      <trans-unit id="slide/slide1/title" xml:space="preserve"><source>Slide 1</source><target>Diapositive</target></trans-unit>
    </body>
  </file>
</xliff>"#;

#[test]
fn xliff_import_is_one_undo_step() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let report = ok(
        &mut s,
        "course.import_xliff",
        json!({ "xliff": TRANSLATION_XLIFF }),
    );
    assert_eq!(report["applied"], json!(2));
    assert_eq!(s.course.title, "Traduit");
    assert_eq!(s.course.language.as_deref(), Some("fr"));
    assert_eq!(
        s.course.slide("slide1").map(|slide| slide.title.as_str()),
        Some("Diapositive")
    );
    assert!(s.can_undo());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn xliff_import_refuses_hostile_xml_without_changing_the_course() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let error = s
        .execute(
            "course.import_xliff",
            &json!({ "xliff": "<!DOCTYPE x [<!ENTITY y 'z'>]><xliff/>" }),
        )
        .unwrap_err();
    assert!(matches!(error, EngineError::Invalid(_)), "{error}");
    assert_eq!(s.course, before);
    assert!(!s.can_undo());
}

#[test]
fn failed_commands_roll_back_and_name_the_problem() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let e = s
        .execute(
            "trigger.add",
            &json!({
                "slide": "slide1",
                "trigger": { "action": { "action": "show_layer", "layer": "ghost" }, "when": { "event": "submit" } }
            }),
        )
        .unwrap_err();
    assert!(
        matches!(e, EngineError::Invalid(ref m) if m.contains("ghost")),
        "{e}"
    );
    assert_eq!(s.course, before);
    assert!(!s.can_undo());
}

#[test]
fn deleting_a_referenced_object_takes_its_triggers_in_one_undo() {
    let mut s = Session::blank("T");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "button", "label": "Go" }, "frame": { "x": 0, "y": 0, "w": 1, "h": 1 } }),
    );
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "button", "label": "Other" }, "frame": { "x": 0, "y": 0, "w": 1, "h": 1 } }),
    );
    ok(&mut s, "slide.add", json!({}));
    // From the button, acting on the button, a condition-free unrelated one.
    for trigger in [
        json!({ "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "button1" } }),
        json!({ "action": { "action": "change_state", "object": "button1", "state": "hidden" }, "when": { "event": "click", "object": "button2" } }),
        json!({ "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "button2" } }),
    ] {
        ok(
            &mut s,
            "trigger.add",
            json!({ "slide": "slide1", "trigger": trigger }),
        );
    }
    let result = ok(
        &mut s,
        "object.delete",
        json!({ "slide": "slide1", "object": "button1" }),
    );
    assert_eq!(result["removed_triggers"], json!(2));
    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.object("button1").is_none());
    assert_eq!(slide.triggers.len(), 1, "the unrelated trigger stays");
    assert!(s.course.is_playable());
    ok(&mut s, "edit.undo", json!({}));
    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.object("button1").is_some());
    assert_eq!(slide.triggers.len(), 3, "one undo brings both back");
}

#[test]
fn object_update_sets_accessibility_fields_and_clears_focus_order() {
    let mut s = Session::blank("Accessibility");
    let id = ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "button", "label": "Go" }, "frame": { "x": 0, "y": 0, "w": 10, "h": 10 } }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.update",
        json!({
            "slide": "slide1",
            "object": &id,
            "description": "Moves to the next slide",
            "decorative": true,
            "focus_order": 3,
        }),
    );
    let obj = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(obj, _)| obj)
        .expect("object exists");
    assert_eq!(obj.description.as_deref(), Some("Moves to the next slide"));
    assert!(obj.decorative);
    assert_eq!(obj.focus_order, Some(3));

    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": &id, "focus_order": Json::Null, "description": Json::Null }),
    );
    let obj = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(obj, _)| obj)
        .expect("object exists");
    assert!(obj.focus_order.is_none());
    assert!(obj.description.is_none());
}

#[test]
fn undo_redo() {
    let mut s = Session::blank("A");
    ok(&mut s, "course.set_title", json!({ "title": "B" }));
    ok(&mut s, "course.set_title", json!({ "title": "C" }));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.title, "B");
    ok(&mut s, "edit.redo", json!({}));
    assert_eq!(s.course.title, "C");
    ok(&mut s, "edit.undo", json!({}));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.title, "A");
    assert_eq!(
        s.execute("edit.undo", &json!({})),
        Err(EngineError::NothingTo("undo"))
    );
    // a new edit clears redo
    ok(&mut s, "course.set_title", json!({ "title": "D" }));
    assert!(!s.can_redo());
}

#[test]
fn execute_batch_success_one_undo_step() {
    let mut s = Session::blank("Batch Test");
    let initial_course = s.course.clone();
    let initial_rev = s.revision;

    let steps = vec![
        ("slide.add".to_string(), json!({ "title": "Slide 2" })),
        ("slide.add".to_string(), json!({ "title": "Slide 3" })),
        ("course.validate".to_string(), json!({})),
    ];
    let results = s.execute_batch(&steps).expect("batch should succeed");
    assert_eq!(results.len(), 3);
    assert_eq!(s.revision, initial_rev + 1);
    assert_eq!(s.undo.len(), 1);

    // One undo step reverts everything
    s.undo().expect("undo should succeed");
    assert_eq!(s.course, initial_course);
    assert!(!s.can_undo());
}

#[test]
fn undo_names_follow_commands_and_move_with_undo_redo() {
    let mut s = Session::blank("A");
    assert!(s.undo_names().is_empty());
    assert!(s.redo_names().is_empty());

    ok(&mut s, "course.set_title", json!({ "title": "B" }));
    assert_eq!(s.undo_names(), vec!["Course title"]);
    assert!(s.redo_names().is_empty());

    // A no-change command adds no name.
    ok(&mut s, "course.set_title", json!({ "title": "B" }));
    assert_eq!(s.undo_names(), vec!["Course title"]);

    ok(&mut s, "slide.add", json!({ "title": "Slide 2" }));
    assert_eq!(s.undo_names(), vec!["New slide", "Course title"]);

    s.undo().expect("undo 1");
    assert_eq!(s.undo_names(), vec!["Course title"]);
    assert_eq!(s.redo_names(), vec!["New slide"]);

    s.undo().expect("undo 2");
    assert!(s.undo_names().is_empty());
    assert_eq!(s.redo_names(), vec!["Course title", "New slide"]);

    s.redo().expect("redo 1");
    assert_eq!(s.undo_names(), vec!["Course title"]);
    assert_eq!(s.redo_names(), vec!["New slide"]);
}

#[test]
fn execute_batch_uses_first_changing_step_label_and_named_batch_overrides() {
    let mut s = Session::blank("A");
    let steps = vec![
        ("course.validate".to_string(), json!({})),
        ("course.set_title".to_string(), json!({ "title": "A" })), // no-op
        ("slide.add".to_string(), json!({ "title": "Slide 2" })),
        ("scene.add".to_string(), json!({ "title": "Scene 2" })),
    ];
    s.execute_batch(&steps).expect("batch");
    assert_eq!(s.undo_names(), vec!["New slide"]);

    s.execute_batch_named(
        "Import deck",
        &[("slide.add".to_string(), json!({ "title": "Slide 3" }))],
    )
    .expect("named batch");
    assert_eq!(s.undo_names(), vec!["Import deck", "New slide"]);
}

#[test]
fn undo_history_trimming_keeps_names_and_snapshots_in_step() {
    let mut s = Session::blank("Start");
    for i in 0..(super::MAX_HISTORY + 5) {
        if i % 2 == 0 {
            ok(
                &mut s,
                "course.set_title",
                json!({ "title": format!("T{i}") }),
            );
        } else {
            ok(
                &mut s,
                "slide.update",
                json!({ "slide": "slide1", "title": format!("S{i}") }),
            );
        }
    }
    assert_eq!(s.undo_len(), super::MAX_HISTORY);
    let names: Vec<String> = s.undo_names().into_iter().map(str::to_owned).collect();
    assert_eq!(names.len(), super::MAX_HISTORY);

    // Undo all MAX_HISTORY steps; the oldest kept snapshot is right before i = 5.
    for expected_name in &names {
        assert_eq!(
            s.undo_names().first().copied(),
            Some(expected_name.as_str())
        );
        s.undo().expect("undo");
    }
    assert!(!s.can_undo());
    // At i = 4 (last even before i = 5), course.title was "T4"; at i = 3, slide1.title was "S3".
    assert_eq!(s.course.title, "T4");
    assert_eq!(s.course.slide("slide1").expect("slide1").title, "S3");
}

#[test]
fn undo_steps_and_redo_steps_match_repeated_calls_and_reject_past_end() {
    let mut s1 = Session::blank("0");
    let mut s2 = Session::blank("0");
    for title in ["1", "2", "3", "4"] {
        ok(&mut s1, "course.set_title", json!({ "title": title }));
        ok(&mut s2, "course.set_title", json!({ "title": title }));
    }

    s1.undo_steps(3).expect("undo_steps(3)");
    s2.undo().expect("u1");
    s2.undo().expect("u2");
    s2.undo().expect("u3");
    assert_eq!(s1.course, s2.course);
    assert_eq!(s1.revision, s2.revision);
    assert_eq!(s1.undo_names(), s2.undo_names());
    assert_eq!(s1.redo_names(), s2.redo_names());

    // Undoing past the end returns an error and leaves the course untouched.
    let before_course = s1.course.clone();
    let before_rev = s1.revision;
    let before_undo = s1
        .undo_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let before_redo = s1
        .redo_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(s1.undo_steps(2), Err(EngineError::NothingTo("undo")));
    assert_eq!(s1.course, before_course);
    assert_eq!(s1.revision, before_rev);
    assert_eq!(
        s1.undo_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        before_undo
    );
    assert_eq!(
        s1.redo_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        before_redo
    );

    // Redo steps matches repeated redo() calls and rejects past the end.
    s1.redo_steps(2).expect("redo_steps(2)");
    s2.redo().expect("r1");
    s2.redo().expect("r2");
    assert_eq!(s1.course, s2.course);
    assert_eq!(s1.revision, s2.revision);
    assert_eq!(s1.redo_steps(2), Err(EngineError::NothingTo("redo")));
    assert_eq!(s1.course, s2.course);
    assert_eq!(s1.revision, s2.revision);
}

#[test]
fn execute_batch_failure_leaves_course_byte_identical() {
    let mut s = Session::blank("Batch Test");
    let initial_course = s.course.clone();
    let initial_rev = s.revision;

    let steps = vec![
        ("slide.add".to_string(), json!({ "title": "Slide 2" })),
        ("slide.add".to_string(), json!({ "title": "Slide 3" })),
        (
            "trigger.add".to_string(),
            json!({
                "slide": "slide1",
                "trigger": { "action": { "action": "show_layer", "layer": "ghost" }, "when": { "event": "submit" } }
            }),
        ),
    ];
    let err = s
        .execute_batch(&steps)
        .expect_err("batch should fail at step 3");
    match err {
        EngineError::BatchFailed {
            step,
            command,
            error,
        } => {
            assert_eq!(step, 2);
            assert_eq!(command, "trigger.add");
            assert!(
                matches!(*error, EngineError::Invalid(ref m) if m.contains("ghost")),
                "{error}"
            );
        }
        other => panic!("unexpected error: {other}"),
    }

    assert_eq!(s.course, initial_course);
    assert_eq!(s.revision, initial_rev);
    assert!(!s.can_undo());
}

#[test]
fn execute_batch_empty_is_noop() {
    let mut s = Session::blank("Batch Test");
    let initial_course = s.course.clone();
    let initial_rev = s.revision;

    let results = s.execute_batch(&[]).expect("empty batch succeeds");
    assert!(results.is_empty());
    assert_eq!(s.course, initial_course);
    assert_eq!(s.revision, initial_rev);
    assert!(!s.can_undo());
}

#[test]
fn ids_never_collide() {
    let mut s = Session::blank("T");
    let a = ok(&mut s, "slide.add", json!({}))["id"].clone();
    let b = ok(&mut s, "slide.add", json!({ "after": "slide1" }))["id"].clone();
    assert_ne!(a, b);
    let order: Vec<&str> = s.course.slides().map(|x| x.id.as_str()).collect();
    assert_eq!(order, ["slide1", b.as_str().unwrap(), a.as_str().unwrap()]);
}

#[test]
fn rebuild_script_recreates_hello_course() {
    let script_text = include_str!("../../../examples/hello-course/rebuild.script.json");
    let expected_text = include_str!("../../../examples/hello-course/course.json");
    let expected = coursecraft_course::Course::from_json(expected_text)
        .unwrap_or_else(|e| panic!("failed to parse expected hello-course: {e}"));

    let steps: Vec<serde_json::Value> = serde_json::from_str(script_text)
        .unwrap_or_else(|e| panic!("failed to parse rebuild.script.json: {e}"));

    let mut session = Session::blank("Initial Title");
    for (i, step) in steps.iter().enumerate() {
        let cmd = step["command"]
            .as_str()
            .unwrap_or_else(|| panic!("step {i} missing command"));
        let params = &step["params"];
        if let Err(e) = session.execute(cmd, params) {
            panic!("step {i} ({cmd}) failed: {e}");
        }
    }

    if session.course != expected {
        let actual_json = session
            .course
            .to_json()
            .unwrap_or_else(|e| format!("error: {e}"));
        let expected_json = expected.to_json().unwrap_or_else(|e| format!("error: {e}"));
        panic!(
            "rebuilt course differs from expected course.json:\n=== ACTUAL ===\n{actual_json}\n=== EXPECTED ===\n{expected_json}"
        );
    }
    assert_eq!(session.course, expected);
}

fn add_button(s: &mut Session, id: &str, label: &str, x: i32) {
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": id,
            "kind": { "type": "button", "label": label },
            "frame": { "x": x, "y": 10, "w": 50, "h": 20 }
        }),
    );
}

fn add_trigger(s: &mut Session, action: Json, event: &str) {
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": { "action": action, "when": { "event": event } }
        }),
    );
}

#[test]
fn course_update_changes_title_id_and_start_slide() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.update",
        json!({ "title": "New Title", "id": "my-course", "start_slide": "slide1" }),
    );
    assert_eq!(s.course.title, "New Title");
    assert_eq!(s.course.id, "my-course");
    assert_eq!(s.course.start_slide.as_deref(), Some("slide1"));
}

#[test]
fn course_update_null_clears_start_slide() {
    let mut s = Session::blank("Title");
    ok(&mut s, "course.update", json!({ "start_slide": "slide1" }));
    ok(&mut s, "course.update", json!({ "start_slide": null }));
    assert_eq!(s.course.start_slide, None);
}

#[test]
fn course_update_refuses_empty_title() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("course.update", &json!({ "title": "   " }))
            .is_err()
    );
}

#[test]
fn course_update_refuses_invalid_id() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("course.update", &json!({ "id": "invalid id!" }))
            .is_err()
    );
}

#[test]
fn course_update_refuses_start_slide_that_does_not_exist() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("course.update", &json!({ "start_slide": "ghost" }))
            .is_err()
    );
}

#[test]
fn course_set_player_toggles_nav_menu_and_seekbar() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_player",
        json!({ "show_nav": false, "show_menu": false, "show_seekbar": false }),
    );
    assert!(!s.course.player.show_nav);
    assert!(!s.course.player.show_menu);
    assert!(!s.course.player.show_seekbar);
}

#[test]
fn course_set_player_sets_seekbar_behaviour_and_undoes() {
    use coursecraft_course::SeekbarBehaviour;
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_player",
        json!({ "seekbar": "until_timeline_ends" }),
    );
    assert_eq!(s.course.player.seekbar, SeekbarBehaviour::UntilTimelineEnds);
    ok(&mut s, "course.set_player", json!({ "seekbar": "locked" }));
    assert_eq!(s.course.player.seekbar, SeekbarBehaviour::Locked);
    assert!(
        s.execute("course.set_player", &json!({ "seekbar": "sideways" }))
            .is_err()
    );
    assert_eq!(s.course.player.seekbar, SeekbarBehaviour::Locked);
    s.undo().unwrap();
    assert_eq!(s.course.player.seekbar, SeekbarBehaviour::UntilTimelineEnds);
}

#[test]
fn course_set_quiz_sets_the_passing_percent() {
    let mut s = Session::blank("Title");
    ok(&mut s, "course.set_quiz", json!({ "passing_percent": 80 }));
    assert_eq!(s.course.quiz.unwrap().passing_percent, 80);
}

#[test]
fn course_set_quiz_null_removes_the_quiz() {
    let mut s = Session::blank("Title");
    ok(&mut s, "course.set_quiz", json!({ "passing_percent": 80 }));
    ok(
        &mut s,
        "course.set_quiz",
        json!({ "passing_percent": null }),
    );
    assert_eq!(s.course.quiz, None);
}

#[test]
fn course_set_quiz_refuses_passing_percent_over_100() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("course.set_quiz", &json!({ "passing_percent": 105 }))
            .is_err()
    );
}

#[test]
fn course_set_quiz_refuses_missing_passing_percent() {
    let mut s = Session::blank("Title");
    assert!(s.execute("course.set_quiz", &json!({})).is_err());
}

#[test]
fn course_set_background_audio_sets_and_replaces_the_track() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "music.mp3" }),
    );
    let audio = s.course.background_audio.clone().expect("background audio");
    assert_eq!(audio.src, "music.mp3");
    assert!((audio.volume - 0.5).abs() < f32::EPSILON);
    assert!(audio.looping);

    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "other.mp3", "volume": 0.25, "loop": false }),
    );
    let audio = s.course.background_audio.clone().expect("background audio");
    assert_eq!(audio.src, "other.mp3");
    assert!((audio.volume - 0.25).abs() < f32::EPSILON);
    assert!(!audio.looping);

    // A later src-only change keeps the volume and loop choices.
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "third.mp3" }),
    );
    let audio = s.course.background_audio.clone().expect("background audio");
    assert_eq!(audio.src, "third.mp3");
    assert!((audio.volume - 0.25).abs() < f32::EPSILON);
    assert!(!audio.looping);
}

#[test]
fn course_set_background_audio_changes_volume_and_loop() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "music.mp3" }),
    );
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "volume": 0.8, "loop": false }),
    );
    let audio = s.course.background_audio.clone().expect("background audio");
    assert!((audio.volume - 0.8).abs() < f32::EPSILON);
    assert!(!audio.looping);
}

#[test]
fn course_set_background_audio_null_removes_the_track() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "music.mp3" }),
    );
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": null }),
    );
    assert_eq!(s.course.background_audio, None);
}

#[test]
fn course_set_background_audio_refuses_volume_without_a_track() {
    let mut s = Session::blank("Title");
    let error = s
        .execute("course.set_background_audio", &json!({ "volume": 0.5 }))
        .expect_err("no background audio");
    assert!(matches!(error, EngineError::NotFound(_)));
    let error = s
        .execute("course.set_background_audio", &json!({ "loop": false }))
        .expect_err("no background audio");
    assert!(matches!(error, EngineError::NotFound(_)));
    assert_eq!(s.course.background_audio, None);
}

#[test]
fn course_set_background_audio_undoes_each_change() {
    let mut s = Session::blank("Title");
    let empty = s.course.clone();

    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "music.mp3" }),
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, empty);

    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "music.mp3", "volume": 0.25 }),
    );
    let first = s.course.clone();
    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": "other.mp3", "loop": false }),
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, first);

    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "volume": 0.75, "loop": true }),
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, first);

    ok(
        &mut s,
        "course.set_background_audio",
        json!({ "src": null }),
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, first);
}

#[test]
fn scene_update_changes_the_title() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "scene.add",
        json!({ "id": "sc2", "title": "Scene 2" }),
    );
    ok(
        &mut s,
        "scene.update",
        json!({ "scene": "sc2", "title": "Updated Scene 2" }),
    );
    assert_eq!(s.course.scenes[1].title, "Updated Scene 2");
}

#[test]
fn scene_update_refuses_empty_title() {
    let mut s = Session::blank("Title");
    ok(&mut s, "scene.add", json!({ "id": "sc2" }));
    assert!(
        s.execute("scene.update", &json!({ "scene": "sc2", "title": "" }))
            .is_err()
    );
}

#[test]
fn scene_update_refuses_unknown_scene() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("scene.update", &json!({ "scene": "ghost", "title": "X" }))
            .is_err()
    );
}

#[test]
fn scene_delete_removes_the_scene() {
    let mut s = Session::blank("Title");
    ok(&mut s, "scene.add", json!({ "id": "sc2" }));
    ok(&mut s, "scene.delete", json!({ "scene": "sc2" }));
    assert_eq!(s.course.scenes.len(), 1);
}

#[test]
fn scene_delete_refuses_the_last_scene() {
    let mut s = Session::blank("Title");
    let e = s
        .execute("scene.delete", &json!({ "scene": "scene1" }))
        .unwrap_err();
    assert!(matches!(e, EngineError::Invalid(_)));
}

#[test]
fn slide_update_changes_title_and_duration() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "title": "Intro", "duration_ms": 3000, "revisit": "resume" }),
    );
    assert_eq!(s.course.slide("slide1").unwrap().title, "Intro");
    assert_eq!(s.course.slide("slide1").unwrap().duration_ms, 3000);
    assert_eq!(
        s.course.slide("slide1").unwrap().revisit,
        coursecraft_course::RevisitMode::Resume
    );
}

#[test]
fn slide_update_refuses_excessive_duration() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute(
            "slide.update",
            &json!({ "slide": "slide1", "duration_ms": 25 * 3600 * 1000 })
        )
        .is_err()
    );
}

#[test]
fn slide_move_can_cross_scenes() {
    let mut s = Session::blank("Title");
    ok(&mut s, "slide.add", json!({ "id": "slide2" }));
    ok(&mut s, "scene.add", json!({ "id": "sc2" }));
    ok(
        &mut s,
        "slide.move",
        json!({ "slide": "slide2", "scene": "sc2", "index": 0 }),
    );
    assert_eq!(s.course.scenes[1].slides[0].id, "slide2");
}

#[test]
fn slide_move_index_past_the_end_is_clamped() {
    let mut s = Session::blank("Title");
    ok(&mut s, "slide.add", json!({ "id": "slide2" }));
    ok(&mut s, "scene.add", json!({ "id": "sc2" }));
    ok(
        &mut s,
        "slide.move",
        json!({ "slide": "slide2", "scene": "sc2", "index": 0 }),
    );
    ok(
        &mut s,
        "slide.move",
        json!({ "slide": "slide2", "index": 999 }),
    );
    assert_eq!(s.course.scenes[1].slides[0].id, "slide2");
}

#[test]
fn slide_move_refuses_unknown_slide() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("slide.move", &json!({ "slide": "ghost", "index": 0 }))
            .is_err()
    );
}

#[test]
fn slide_move_refuses_unknown_target_scene() {
    let mut s = Session::blank("Title");
    ok(&mut s, "slide.add", json!({ "id": "slide2" }));
    assert!(
        s.execute(
            "slide.move",
            &json!({ "slide": "slide2", "scene": "ghost", "index": 0 })
        )
        .is_err()
    );
}

fn add_layer_l1(s: &mut Session) {
    ok(
        s,
        "layer.add",
        json!({ "slide": "slide1", "id": "l1", "name": "Layer 1" }),
    );
}

fn add_show_layer_trigger(s: &mut Session) {
    add_trigger(
        s,
        json!({ "action": "show_layer", "layer": "l1" }),
        "timeline_starts",
    );
}

#[test]
fn layer_update_changes_name_duration_and_flags() {
    let mut s = Session::blank("Title");
    add_layer_l1(&mut s);
    ok(
        &mut s,
        "layer.update",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "name": "Updated Layer",
            "duration_ms": 4000,
            "prevent_base_clicks": true,
            "hide_base_objects": true,
            "hide_when_timeline_ends": true,
            "allow_seek": "no",
            "revisit": "resume"
        }),
    );
    let layer = s.course.slide("slide1").unwrap().layer("l1").unwrap();
    assert_eq!(layer.name, "Updated Layer");
    assert_eq!(layer.duration_ms, 4000);
    assert!(layer.prevent_base_clicks);
    assert!(layer.hide_base_objects);
    assert!(layer.hide_when_timeline_ends);
    assert_eq!(layer.allow_seek, coursecraft_course::SeekMode::No);
    assert_eq!(layer.revisit, coursecraft_course::RevisitMode::Resume);
}

#[test]
fn layer_duplicate_copies_the_new_options() {
    let mut s = Session::blank("Title");
    add_layer_l1(&mut s);
    ok(
        &mut s,
        "layer.update",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "hide_base_objects": true,
            "hide_when_timeline_ends": true,
            "allow_seek": "yes",
            "revisit": "reset"
        }),
    );
    let result = ok(
        &mut s,
        "layer.duplicate",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    let new_id = result["id"].as_str().expect("copy id");
    let copy = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.layer(new_id))
        .expect("copy");
    assert!(copy.hide_base_objects);
    assert!(copy.hide_when_timeline_ends);
    assert_eq!(copy.allow_seek, coursecraft_course::SeekMode::Yes);
    assert_eq!(copy.revisit, coursecraft_course::RevisitMode::Reset);
}

#[test]
fn deleting_a_layer_takes_the_triggers_that_show_it() {
    let mut s = Session::blank("Title");
    add_layer_l1(&mut s);
    add_show_layer_trigger(&mut s);
    let result = ok(
        &mut s,
        "layer.delete",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    assert_eq!(result["removed_triggers"], json!(1));
    assert!(s.course.slide("slide1").unwrap().triggers.is_empty());
    assert!(s.course.is_playable());
}

#[test]
fn layer_delete_succeeds_once_unreferenced() {
    let mut s = Session::blank("Title");
    add_layer_l1(&mut s);
    add_show_layer_trigger(&mut s);
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "index": 0 }),
    );
    ok(
        &mut s,
        "layer.delete",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    assert!(s.course.slide("slide1").unwrap().layer("l1").is_none());
}

fn add_layer(s: &mut Session, id: &str, name: &str) {
    ok(
        s,
        "layer.add",
        json!({ "slide": "slide1", "id": id, "name": name }),
    );
}

#[test]
fn layer_move_reorders_and_a_no_op_adds_no_undo_step() {
    let mut s = Session::blank("Title");
    add_layer(&mut s, "l1", "Back");
    add_layer(&mut s, "l2", "Middle");
    add_layer(&mut s, "l3", "Front");

    ok(
        &mut s,
        "layer.move",
        json!({ "slide": "slide1", "layer": "l1", "to": 2 }),
    );
    let order: Vec<&str> = s
        .course
        .slide("slide1")
        .unwrap()
        .layers
        .iter()
        .map(|layer| layer.id.as_str())
        .collect();
    assert_eq!(order, ["l2", "l3", "l1"]);

    // Clamps a huge destination to the last index, and moving there is a no-op.
    let undo_before = s.undo_len();
    ok(
        &mut s,
        "layer.move",
        json!({ "slide": "slide1", "layer": "l1", "to": 999 }),
    );
    assert_eq!(s.undo_len(), undo_before);
    assert_eq!(s.course.slide("slide1").unwrap().layers[2].id, "l1");

    // Moving to its current index is also a no-op.
    ok(
        &mut s,
        "layer.move",
        json!({ "slide": "slide1", "layer": "l2", "to": 0 }),
    );
    assert_eq!(s.undo_len(), undo_before);
}

#[test]
fn layer_move_refuses_a_layer_on_another_slide() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute(
            "layer.move",
            &json!({ "slide": "slide1", "layer": "ghost", "to": 0 })
        )
        .is_err()
    );
}

#[test]
fn layer_duplicate_inserts_in_front_with_fresh_ids_and_remapped_triggers() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "base_btn", "Base", 10);
    add_layer(&mut s, "l1", "About");
    for (id, x) in [("a", 20), ("b", 80)] {
        ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "layer": "l1",
                "id": id,
                "kind": { "type": "button", "label": id },
                "frame": { "x": x, "y": 10, "w": 40, "h": 20 }
            }),
        );
    }
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    // Own object click changes own object's state: both references must remap.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "trigger": {
                "action": { "action": "change_state", "object": "b", "state": "hidden" },
                "when": { "event": "click", "object": "a" }
            }
        }),
    );
    // A trigger on the copy that reaches the base object keeps pointing at it.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "trigger": {
                "action": { "action": "change_state", "object": "base_btn", "state": "hidden" },
                "when": { "event": "click", "object": "base_btn" }
            }
        }),
    );
    // A trigger elsewhere still opens the original layer only.
    add_trigger(
        &mut s,
        json!({ "action": "show_layer", "layer": "l1" }),
        "timeline_starts",
    );

    let before = s.course.clone();
    let result = ok(
        &mut s,
        "layer.duplicate",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    let new_id = result["id"].as_str().unwrap().to_owned();
    assert_eq!(new_id, "layer1");

    let slide = s.course.slide("slide1").unwrap();
    assert_eq!(slide.layers.len(), 2);
    assert_eq!(slide.layers[1].id, new_id, "the copy sits in front");
    assert_eq!(slide.layers[1].name, "About copy");

    let copy = &slide.layers[1];
    let ids: Vec<&str> = copy.objects.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["button1", "button2"]);
    assert!(copy.objects.iter().all(|o| o.id != "a" && o.id != "b"));
    let group = copy.objects[0].group.clone().expect("copy keeps a group");
    assert_eq!(copy.objects[1].group.as_deref(), Some(group.as_str()));
    let original_group = slide.layers[0].objects[0].group.clone().unwrap();
    assert_ne!(group, original_group, "copied group ids are fresh");

    assert_eq!(copy.triggers.len(), 2);
    let own = copy
        .triggers
        .iter()
        .find(|t| matches!(&t.when, coursecraft_course::Event::Click { object } if object == "button1"))
        .expect("own click trigger remapped");
    assert!(matches!(
        &own.action,
        coursecraft_course::Action::ChangeState { object, .. } if object == "button2"
    ));
    assert!(copy.triggers.iter().any(|t| matches!(
        &t.when,
        coursecraft_course::Event::Click { object } if object == "base_btn"
    )));

    // The slide trigger did not change, and still shows the original layer.
    let slide_trigger = slide.triggers.last().expect("slide trigger");
    assert!(matches!(
        &slide_trigger.action,
        coursecraft_course::Action::ShowLayer { layer } if layer == "l1"
    ));
    // One undo step restores the course exactly.
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn layer_duplicate_names_each_copy_uniquely() {
    let mut s = Session::blank("Title");
    add_layer(&mut s, "l1", "About");
    ok(
        &mut s,
        "layer.duplicate",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    let second = ok(
        &mut s,
        "layer.duplicate",
        json!({ "slide": "slide1", "layer": "l1" }),
    );
    let slide = s.course.slide("slide1").unwrap();
    assert_eq!(
        slide.layer(second["id"].as_str().unwrap()).unwrap().name,
        "About copy 2"
    );
    assert_eq!(
        slide
            .layers
            .iter()
            .filter(|l| l.name.starts_with("About copy"))
            .count(),
        2
    );
}

#[test]
fn object_update_changes_name_and_timing() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "btn1", "B1", 10);
    ok(
        &mut s,
        "object.update",
        json!({
            "slide": "slide1",
            "object": "btn1",
            "name": "First Button",
            "timing": { "start_ms": 500 }
        }),
    );
    let (obj, _) = s.course.slide("slide1").unwrap().object("btn1").unwrap();
    assert_eq!(obj.name, "First Button");
    assert_eq!(obj.timing.start_ms, 500);
}

#[test]
fn object_set_captions_changes_audio_and_video_and_undoes() {
    let mut s = Session::blank("Title");
    let audio = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "audio", "src": "sound.wav", "autoplay": false },
            "frame": { "x": 0, "y": 0, "w": 10, "h": 10 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let video = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "video", "src": "clip.mp4", "autoplay": false },
            "frame": { "x": 0, "y": 0, "w": 10, "h": 10 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    ok(
        &mut s,
        "object.set_captions",
        json!({ "slide": "slide1", "object": audio, "captions": "sound.vtt" }),
    );
    ok(
        &mut s,
        "object.set_captions",
        json!({ "slide": "slide1", "object": video, "captions": "clip.vtt" }),
    );
    let slide = s.course.slide("slide1").unwrap();
    let captions_of = |id: &str| match &slide.object(id).unwrap().0.kind {
        coursecraft_course::ObjectKind::Audio { captions, .. }
        | coursecraft_course::ObjectKind::Video { captions, .. } => captions.clone(),
        _ => None,
    };
    assert_eq!(captions_of(&audio).as_deref(), Some("sound.vtt"));
    assert_eq!(captions_of(&video).as_deref(), Some("clip.vtt"));

    ok(&mut s, "edit.undo", json!({}));
    let slide = s.course.slide("slide1").unwrap();
    assert!(matches!(
        &slide.object(&video).unwrap().0.kind,
        coursecraft_course::ObjectKind::Video { captions: None, .. }
    ));

    ok(
        &mut s,
        "object.set_captions",
        json!({ "slide": "slide1", "object": audio, "captions": null }),
    );
    assert!(matches!(
        &s.course
            .slide("slide1")
            .unwrap()
            .object(&audio)
            .unwrap()
            .0
            .kind,
        coursecraft_course::ObjectKind::Audio { captions: None, .. }
    ));
}

#[test]
fn object_set_media_sets_volume_and_trim_refuses_non_media_and_undoes() {
    let mut s = Session::blank("Media");
    let audio = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "audio", "src": "sound.wav", "autoplay": false },
            "frame": { "x": 0, "y": 0, "w": 10, "h": 10 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let text = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "text", "text": "Hello" },
            "frame": { "x": 0, "y": 20, "w": 100, "h": 20 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    ok(
        &mut s,
        "object.set_media",
        json!({ "slide": "slide1", "object": audio, "volume": 0.4 }),
    );
    let slide = s.course.slide("slide1").unwrap();
    let coursecraft_course::ObjectKind::Audio { volume, trim, .. } =
        &slide.object(&audio).unwrap().0.kind
    else {
        panic!("audio");
    };
    assert!((*volume - 0.4).abs() < f32::EPSILON);
    assert!(trim.is_none());

    ok(
        &mut s,
        "object.set_media",
        json!({
            "slide": "slide1",
            "object": audio,
            "trim": { "start_ms": 1000, "end_ms": 3000 }
        }),
    );
    // An absent volume keeps the stored value.
    ok(
        &mut s,
        "object.set_media",
        json!({
            "slide": "slide1",
            "object": audio,
            "trim": { "start_ms": 500 }
        }),
    );
    let slide = s.course.slide("slide1").unwrap();
    let coursecraft_course::ObjectKind::Audio { volume, trim, .. } =
        &slide.object(&audio).unwrap().0.kind
    else {
        panic!("audio");
    };
    assert!((*volume - 0.4).abs() < f32::EPSILON);
    assert_eq!(
        trim,
        &Some(coursecraft_course::Trim {
            start_ms: 500,
            end_ms: None
        })
    );

    let error = s
        .execute(
            "object.set_media",
            &json!({ "slide": "slide1", "object": text, "volume": 0.5 }),
        )
        .expect_err("a text object is not media");
    assert!(matches!(error, EngineError::Invalid(_)), "{error}");

    ok(&mut s, "edit.undo", json!({}));
    let slide = s.course.slide("slide1").unwrap();
    let coursecraft_course::ObjectKind::Audio { trim, .. } = &slide.object(&audio).unwrap().0.kind
    else {
        panic!("audio");
    };
    assert_eq!(
        trim,
        &Some(coursecraft_course::Trim {
            start_ms: 1000,
            end_ms: Some(3000)
        })
    );

    ok(
        &mut s,
        "object.set_media",
        json!({ "slide": "slide1", "object": audio, "trim": null }),
    );
    let slide = s.course.slide("slide1").unwrap();
    let coursecraft_course::ObjectKind::Audio { trim, .. } = &slide.object(&audio).unwrap().0.kind
    else {
        panic!("audio");
    };
    assert!(trim.is_none());
}

#[test]
fn object_update_refuses_undefined_initial_state() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "btn1", "B1", 10);
    assert!(
        s.execute(
            "object.update",
            &json!({ "slide": "slide1", "object": "btn1", "initial_state": "undefined_state" })
        )
        .is_err()
    );
}

#[test]
fn reorder_index_past_the_end_is_clamped() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "btn1", "B1", 10);
    add_button(&mut s, "btn2", "B2", 70);
    ok(
        &mut s,
        "object.reorder",
        json!({ "slide": "slide1", "object": "btn1", "index": 999 }),
    );
    let slide = s.course.slide("slide1").unwrap();
    assert_eq!(slide.objects[0].id, "btn2");
    assert_eq!(slide.objects[1].id, "btn1");
}

#[test]
fn object_reorder_refuses_unknown_object() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "btn1", "B1", 10);
    assert!(
        s.execute(
            "object.reorder",
            &json!({ "slide": "slide1", "object": "ghost", "index": 0 })
        )
        .is_err()
    );
}

#[test]
fn object_duplicate_offsets_the_copy_by_twenty_points() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "btn1", "B1", 10);
    let dup_id = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": "btn1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (dup, _) = s.course.slide("slide1").unwrap().object(&dup_id).unwrap();
    assert_eq!(dup.frame.x, 30.0);
    assert_eq!(dup.frame.y, 30.0);
}

#[test]
fn object_duplicate_refuses_unknown_object() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute(
            "object.duplicate",
            &json!({ "slide": "slide1", "object": "ghost" })
        )
        .is_err()
    );
}

fn add_button_with_active_state(s: &mut Session) {
    add_button(s, "btn", "B", 0);
    ok(
        s,
        "object.add_state",
        json!({
            "slide": "slide1",
            "object": "btn",
            "state": { "name": "active", "fill": "#112233" }
        }),
    );
}

#[test]
fn object_update_state_changes_the_fill() {
    let mut s = Session::blank("Title");
    add_button_with_active_state(&mut s);
    ok(
        &mut s,
        "object.update_state",
        json!({
            "slide": "slide1",
            "object": "btn",
            "state": { "name": "active", "fill": "#445566" }
        }),
    );
    let (obj, _) = s.course.slide("slide1").unwrap().object("btn").unwrap();
    assert_eq!(obj.states[0].fill.as_ref().unwrap().0, "#445566");
}

#[test]
fn object_update_state_refuses_unknown_state() {
    let mut s = Session::blank("Title");
    add_button_with_active_state(&mut s);
    assert!(
        s.execute(
            "object.update_state",
            &json!({
                "slide": "slide1",
                "object": "btn",
                "state": { "name": "missing" }
            })
        )
        .is_err()
    );
}

#[test]
fn object_delete_state_removes_the_state() {
    let mut s = Session::blank("Title");
    add_button_with_active_state(&mut s);
    ok(
        &mut s,
        "object.delete_state",
        json!({ "slide": "slide1", "object": "btn", "name": "active" }),
    );
    let (obj, _) = s.course.slide("slide1").unwrap().object("btn").unwrap();
    assert!(obj.states.is_empty());
}

#[test]
fn object_delete_state_refuses_unknown_state() {
    let mut s = Session::blank("Title");
    add_button_with_active_state(&mut s);
    ok(
        &mut s,
        "object.delete_state",
        json!({ "slide": "slide1", "object": "btn", "name": "active" }),
    );
    assert!(
        s.execute(
            "object.delete_state",
            &json!({ "slide": "slide1", "object": "btn", "name": "active" })
        )
        .is_err()
    );
}

fn add_two_triggers(s: &mut Session) {
    add_trigger(s, json!({ "action": "pause_timeline" }), "timeline_starts");
    add_trigger(s, json!({ "action": "resume_timeline" }), "timeline_ends");
}

fn update_first_trigger_to_exit_course(s: &mut Session) {
    ok(
        s,
        "trigger.update",
        json!({
            "slide": "slide1",
            "index": 0,
            "trigger": { "action": { "action": "exit_course" }, "when": { "event": "timeline_starts" } }
        }),
    );
}

#[test]
fn trigger_update_replaces_the_action() {
    let mut s = Session::blank("Title");
    add_two_triggers(&mut s);
    update_first_trigger_to_exit_course(&mut s);
    assert_eq!(
        s.course.slide("slide1").unwrap().triggers[0].action,
        coursecraft_course::Action::ExitCourse
    );
}

#[test]
fn trigger_move_reorders_triggers() {
    let mut s = Session::blank("Title");
    add_two_triggers(&mut s);
    update_first_trigger_to_exit_course(&mut s);
    ok(
        &mut s,
        "trigger.move",
        json!({ "slide": "slide1", "from": 0, "to": 1 }),
    );
    assert_eq!(
        s.course.slide("slide1").unwrap().triggers[1].action,
        coursecraft_course::Action::ExitCourse
    );
}

#[test]
fn trigger_update_refuses_out_of_bounds_index() {
    let mut s = Session::blank("Title");
    add_two_triggers(&mut s);
    assert!(
        s.execute(
            "trigger.update",
            &json!({
                "slide": "slide1",
                "index": 99,
                "trigger": { "action": { "action": "exit_course" }, "when": { "event": "timeline_starts" } }
            })
        )
        .is_err()
    );
}

#[test]
fn trigger_move_refuses_out_of_bounds_index() {
    let mut s = Session::blank("Title");
    add_two_triggers(&mut s);
    assert!(
        s.execute(
            "trigger.move",
            &json!({ "slide": "slide1", "from": 99, "to": 0 })
        )
        .is_err()
    );
}

fn add_score_variable(s: &mut Session) {
    ok(s, "variable.add", json!({ "name": "score", "value": 0 }));
}

fn add_set_score_trigger(s: &mut Session) {
    add_trigger(
        s,
        json!({ "action": "set_variable", "variable": "score", "op": "assign", "value": 5 }),
        "timeline_starts",
    );
}

#[test]
fn variable_set_value_updates_the_value() {
    let mut s = Session::blank("Title");
    add_score_variable(&mut s);
    ok(
        &mut s,
        "variable.set_value",
        json!({ "name": "score", "value": 10 }),
    );
    assert_eq!(
        s.course.variable("score").unwrap().value,
        coursecraft_course::Value::Number(10.0)
    );
}

#[test]
fn variable_set_value_refuses_a_type_change() {
    let mut s = Session::blank("Title");
    add_score_variable(&mut s);
    assert!(
        s.execute(
            "variable.set_value",
            &json!({ "name": "score", "value": "text" })
        )
        .is_err()
    );
}

#[test]
fn deleting_a_variable_used_by_a_trigger_is_refused_and_names_it() {
    let mut s = Session::blank("Title");
    add_score_variable(&mut s);
    add_set_score_trigger(&mut s);
    let e = s
        .execute("variable.delete", &json!({ "name": "score" }))
        .unwrap_err();
    assert!(
        matches!(e, EngineError::Invalid(ref m) if m.contains("score")),
        "{e}"
    );
}

#[test]
fn variable_delete_succeeds_once_unreferenced() {
    let mut s = Session::blank("Title");
    add_score_variable(&mut s);
    add_set_score_trigger(&mut s);
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "index": 0 }),
    );
    ok(&mut s, "variable.delete", json!({ "name": "score" }));
    assert!(s.course.variable("score").is_none());
}

fn set_multiple_choice_question(s: &mut Session) {
    ok(
        s,
        "question.set",
        json!({
            "slide": "slide1",
            "question": {
                "kind": "multiple_choice",
                "prompt": "Q?",
                "choices": [
                    { "id": "a", "text": "A", "correct": true },
                    { "id": "b", "text": "B" }
                ]
            }
        }),
    );
}

#[test]
fn question_set_attaches_a_question_to_the_slide() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    assert!(s.course.slide("slide1").unwrap().question.is_some());
}

#[test]
fn deleting_a_question_removes_its_submit_trigger_and_button() {
    // question.set added a SubmitQuestion trigger; deleting the question now
    // removes that trigger and any Submit button that has nothing else to do.
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    let before = s.course.clone();
    ok(&mut s, "question.delete", json!({ "slide": "slide1" }));
    let slide = s.course.slide("slide1").expect("slide");
    assert!(slide.question.is_none());
    assert!(
        !slide
            .triggers
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
            .any(|t| t.action == coursecraft_course::Action::SubmitQuestion)
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_delete_removes_submit_furniture_for_every_kind_and_undo_restores_it() {
    use coursecraft_course::{Action, Event, ObjectKind, QuestionKind};

    let kinds = [
        QuestionKind::MultipleChoice,
        QuestionKind::MultipleResponse,
        QuestionKind::TrueFalse,
        QuestionKind::FillIn,
        QuestionKind::Sequence,
        QuestionKind::Matching,
        QuestionKind::Hotspot,
        QuestionKind::DragDrop,
    ];
    for kind in kinds {
        let mut s = Session::blank("Title");
        // A button with an ordinary trigger must survive the delete.
        ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "id": "keep",
                "kind": { "type": "button", "label": "Keep" },
                "frame": { "x": 20, "y": 20, "w": 100, "h": 40 }
            }),
        );
        ok(
            &mut s,
            "trigger.add",
            json!({
                "slide": "slide1",
                "trigger": {
                    "action": { "action": "pause_timeline" },
                    "when": { "event": "click", "object": "keep" }
                }
            }),
        );

        ok(
            &mut s,
            "question.insert",
            json!({ "slide": "slide1", "kind": kind }),
        );
        ok(
            &mut s,
            "question.add_feedback_layers",
            json!({ "slide": "slide1" }),
        );
        ok(
            &mut s,
            "question.add_submit_button",
            json!({ "slide": "slide1" }),
        );

        let before = s.course.clone();
        let before_slide = s.course.slide("slide1").expect("slide").clone();
        let before_layers = before_slide.layers.len();
        let had_submit_trigger = before_slide
            .triggers
            .iter()
            .chain(before_slide.layers.iter().flat_map(|l| l.triggers.iter()))
            .any(|t| t.action == Action::SubmitQuestion);
        assert!(
            had_submit_trigger,
            "{kind:?}: insert should add a submit trigger"
        );
        assert!(
            before_slide
                .objects
                .iter()
                .any(|o| o.name == "Submit" && matches!(o.kind, ObjectKind::Button { .. })),
            "{kind:?}: insert should add a Submit button"
        );

        ok(&mut s, "question.delete", json!({ "slide": "slide1" }));

        let slide = s.course.slide("slide1").expect("slide");
        assert!(
            slide.question.is_none(),
            "{kind:?}: question should be gone"
        );
        assert!(
            !slide
                .triggers
                .iter()
                .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
                .any(|t| t.action == Action::SubmitQuestion || t.when == Event::Submit),
            "{kind:?}: no submit trigger or submit event should remain"
        );
        assert!(
            !slide
                .objects
                .iter()
                .any(|o| o.name == "Submit" && matches!(o.kind, ObjectKind::Button { .. })),
            "{kind:?}: Submit button should be removed"
        );
        assert!(before_layers > 0, "{kind:?}: feedback layers were added");
        assert!(
            slide.layers.is_empty(),
            "{kind:?}: untouched feedback layers should go with the question"
        );
        assert!(
            slide.triggers.iter().any(|t| {
                t.action == Action::PauseTimeline
                    && t.when
                        == Event::Click {
                            object: "keep".to_owned(),
                        }
            }),
            "{kind:?}: unrelated trigger should stay"
        );
        assert!(
            slide.objects.iter().any(|o| o.id == "keep"),
            "{kind:?}: unrelated button should stay"
        );
        // Everything else that was on the slide is still there.
        let expected: Vec<_> = before_slide
            .objects
            .iter()
            .filter(|o| o.name != "Submit")
            .cloned()
            .collect();
        assert_eq!(
            slide.objects, expected,
            "{kind:?}: other objects should stay"
        );

        ok(&mut s, "edit.undo", json!({}));
        assert_eq!(s.course, before, "{kind:?}: undo should restore everything");
    }
}

#[test]
fn question_delete_keeps_a_submit_button_that_still_has_another_trigger() {
    use coursecraft_course::{Action, Event, ObjectKind};

    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "question.insert",
        json!({ "slide": "slide1", "kind": "multiple_choice" }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let submit_button = slide
        .objects
        .iter()
        .find(|o| o.name == "Submit")
        .map(|o| o.id.clone())
        .expect("submit button");
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "pause_timeline" },
                "when": { "event": "click", "object": submit_button }
            }
        }),
    );

    ok(&mut s, "question.delete", json!({ "slide": "slide1" }));
    let slide = s.course.slide("slide1").expect("slide");
    assert!(
        slide
            .objects
            .iter()
            .any(|o| o.id == submit_button && matches!(o.kind, ObjectKind::Button { .. })),
        "a button with another trigger stays"
    );
    assert!(
        slide.triggers.iter().any(|t| {
            t.action == Action::PauseTimeline
                && t.when
                    == Event::Click {
                        object: submit_button.clone(),
                    }
        }),
        "the ordinary trigger stays"
    );
    assert!(
        !slide
            .triggers
            .iter()
            .any(|t| t.action == Action::SubmitQuestion)
    );
}

#[test]
fn question_delete_succeeds_once_its_submit_trigger_is_gone() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "index": 0 }),
    );
    ok(&mut s, "question.delete", json!({ "slide": "slide1" }));
    assert!(s.course.slide("slide1").unwrap().question.is_none());
}

#[test]
fn question_delete_refuses_a_slide_without_a_question() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("question.delete", &json!({ "slide": "slide1" }))
            .is_err()
    );
}

fn quiz(s: &Session) -> coursecraft_course::Question {
    s.course
        .slide("slide1")
        .and_then(|sl| sl.question.clone())
        .expect("question")
}

fn mc_session() -> Session {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    s
}

#[test]
fn question_set_kind_true_false_undo_restores_the_original() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    let before = s.course.clone();
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "true_false" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::TrueFalse);
    assert_eq!(q.choices.len(), 2);
    assert_eq!(q.choices[0].id, "true");
    assert!(q.choices[0].correct);
    assert_eq!(q.choices[1].id, "false");
    assert!(!q.choices[1].correct);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn freeform_insert_defaults_condition_and_kind_switches_clear_fields() {
    use coursecraft_course::{CmpOp, Condition, QuestionKind, Value};

    let mut s = Session::blank("Title");
    add_score_variable(&mut s);
    ok(
        &mut s,
        "question.insert",
        json!({ "slide": "slide1", "kind": "freeform" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Freeform);
    assert!(q.choices.is_empty());
    assert!(q.correct_layer.is_some());
    assert!(q.incorrect_layer.is_some());
    let default_condition = Condition::Compare {
        variable: "score".into(),
        op: CmpOp::Ge,
        value: Value::Number(0.0).into(),
    };
    assert_eq!(q.correct_when.as_ref(), Some(&default_condition));

    let mut updated = q.clone();
    updated.correct_when = Some(Condition::Compare {
        variable: "Course.ScorePercent".into(),
        op: CmpOp::Ge,
        value: Value::Number(50.0).into(),
    });
    ok(
        &mut s,
        "question.set",
        json!({ "slide": "slide1", "question": updated }),
    );
    assert_eq!(quiz(&s).correct_when, updated.correct_when);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(quiz(&s).correct_when.as_ref(), Some(&default_condition));

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "multiple_choice" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::MultipleChoice);
    assert!(q.correct_when.is_none(), "leaving freeform clears it");
    assert!(q.choices.len() >= 2);

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "freeform" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Freeform);
    assert!(q.choices.is_empty(), "freeform keeps no choices");
    assert_eq!(q.correct_when.as_ref(), Some(&default_condition));
}

#[test]
fn question_fill_in_accepted_answers_undo_and_reject_the_last_removal() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "fill_in" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::FillIn);
    assert!(q.choices.is_empty());
    assert_eq!(q.accepted, vec!["answer".to_owned()]);
    assert!(!q.case_sensitive);
    ok(
        &mut s,
        "question.add_accepted",
        json!({ "slide": "slide1", "text": "Paris" }),
    );
    ok(
        &mut s,
        "question.update_accepted",
        json!({ "slide": "slide1", "index": 0, "text": "London" }),
    );
    ok(
        &mut s,
        "question.set_case_sensitive",
        json!({ "slide": "slide1", "case_sensitive": true }),
    );
    let q = quiz(&s);
    assert_eq!(q.accepted, vec!["London".to_owned(), "Paris".to_owned()]);
    assert!(q.case_sensitive);
    ok(
        &mut s,
        "question.remove_accepted",
        json!({ "slide": "slide1", "index": 1 }),
    );
    assert_eq!(quiz(&s).accepted, vec!["London".to_owned()]);
    let snap = s.course.clone();
    assert!(
        s.execute(
            "question.remove_accepted",
            &json!({ "slide": "slide1", "index": 0 })
        )
        .is_err()
    );
    assert_eq!(s.course, snap);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(quiz(&s).accepted.len(), 2);
}

#[test]
fn question_set_kind_sequence_pads_when_fewer_than_two_choices_remain() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "fill_in" }),
    );
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "sequence" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Sequence);
    assert!(q.choices.len() >= 2);
    assert!(q.choices.iter().all(|c| !c.correct));
}

#[test]
fn question_sequence_reorder_undo_restores_the_authored_order() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "sequence" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Sequence);
    assert!(q.choices.len() >= 2);
    assert!(q.choices.iter().all(|c| !c.correct));
    assert!(!q.shuffle);
    let first = q.choices[0].id.clone();
    ok(
        &mut s,
        "question.reorder_sequence",
        json!({ "slide": "slide1", "id": first, "to": 1 }),
    );
    assert_eq!(quiz(&s).choices[1].id, first);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(quiz(&s).choices[0].id, first);
}

#[test]
fn question_matching_pair_edits_undo_one_step_at_a_time() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "matching" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Matching);
    assert!(q.choices.is_empty());
    assert_eq!(q.pairs.len(), 2);
    assert!(!q.pairs[0].left.is_empty() && !q.pairs[0].right.is_empty());
    let added = ok(&mut s, "question.add_pair", json!({ "slide": "slide1" }));
    let id = added["id"].as_str().unwrap().to_owned();
    assert_eq!(quiz(&s).pairs.len(), 3);
    ok(
        &mut s,
        "question.update_pair",
        json!({ "slide": "slide1", "id": id, "left": "Cat", "right": "Meow" }),
    );
    ok(
        &mut s,
        "question.reorder_pair",
        json!({ "slide": "slide1", "id": id, "to": 0 }),
    );
    assert_eq!(quiz(&s).pairs[0].id, id);
    assert_eq!(quiz(&s).pairs[0].left, "Cat");
    ok(&mut s, "edit.undo", json!({}));
    assert_ne!(quiz(&s).pairs[0].id, id);
    ok(
        &mut s,
        "question.remove_pair",
        json!({ "slide": "slide1", "id": id }),
    );
    assert_eq!(quiz(&s).pairs.len(), 2);
}

#[test]
fn question_hotspot_lifecycle_and_undo() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    // Cannot switch to hotspot without an image on the slide
    assert!(
        s.execute(
            "question.set_kind",
            &json!({ "slide": "slide1", "kind": "hotspot" })
        )
        .is_err()
    );

    // Add an image object to the slide
    let img_id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "image", "src": "diagram.png" },
            "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 300.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Now switch to hotspot succeeds and seeds 1 region at image center
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "hotspot" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Hotspot);
    assert_eq!(q.hotspot_image.as_deref(), Some(img_id.as_str()));
    assert_eq!(q.regions.len(), 1);
    assert!(q.regions[0].correct);
    assert_eq!(q.markers, 1);

    // Add a second region
    let res = ok(
        &mut s,
        "question.add_region",
        json!({
            "slide": "slide1",
            "shape": "ellipse",
            "x": 200.0,
            "y": 200.0,
            "w": 50.0,
            "h": 50.0,
            "correct": false
        }),
    );
    let r2_id = res["id"].as_str().unwrap().to_owned();
    assert_eq!(quiz(&s).regions.len(), 2);

    // Update region
    ok(
        &mut s,
        "question.update_region",
        json!({
            "slide": "slide1",
            "id": r2_id,
            "correct": true,
            "w": 60.0
        }),
    );
    let r2 = quiz(&s)
        .regions
        .into_iter()
        .find(|r| r.id == r2_id)
        .unwrap();
    assert!(r2.correct);
    assert_eq!(r2.w, 60.0);

    // Set markers
    ok(
        &mut s,
        "question.set_markers",
        json!({ "slide": "slide1", "markers": 3 }),
    );
    assert_eq!(quiz(&s).markers, 3);

    // Remove region
    ok(
        &mut s,
        "question.remove_region",
        json!({ "slide": "slide1", "id": r2_id }),
    );
    assert_eq!(quiz(&s).regions.len(), 1);

    // Cannot remove the only remaining region
    let r1_id = quiz(&s).regions[0].id.clone();
    assert!(
        s.execute(
            "question.remove_region",
            &json!({ "slide": "slide1", "id": r1_id })
        )
        .is_err()
    );

    // Undo restores removed region
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(quiz(&s).regions.len(), 2);
}

#[test]
fn question_drag_drop_lifecycle_and_undo() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    // Cannot switch to drag_drop with < 2 objects on the slide (0 initially)
    assert!(
        s.execute(
            "question.set_kind",
            &json!({ "slide": "slide1", "kind": "drag_drop" })
        )
        .is_err()
    );

    // Add 1 object; still < 2 so it must still fail
    let _obj1 = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000ff" },
            "frame": { "x": 100.0, "y": 100.0, "w": 80.0, "h": 60.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        s.execute(
            "question.set_kind",
            &json!({ "slide": "slide1", "kind": "drag_drop" })
        )
        .is_err()
    );

    // Add a second object so there are 2 objects
    let obj2 = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#ff0000" },
            "frame": { "x": 200.0, "y": 200.0, "w": 100.0, "h": 100.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Now switch to drag_drop succeeds
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "drag_drop" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::DragDrop);
    assert_eq!(q.drag_items.len(), 1);
    assert_eq!(q.drop_targets.len(), 1);
    assert!(q.snap_to_center);

    // Add a third object for another target
    let obj3 = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#00ff00" },
            "frame": { "x": 400.0, "y": 200.0, "w": 100.0, "h": 100.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Update drag item's target to obj3
    let item_id = quiz(&s).drag_items[0].item.clone();
    ok(
        &mut s,
        "question.update_drag_item",
        json!({ "slide": "slide1", "item": item_id, "target": obj3 }),
    );
    assert_eq!(quiz(&s).drag_items[0].target, obj3);
    assert!(quiz(&s).drop_targets.contains(&obj3));

    // Set options
    ok(
        &mut s,
        "question.set_drag_options",
        json!({
            "slide": "slide1",
            "snap_to_center": false,
            "return_if_wrong": true,
            "one_per_target": false
        }),
    );
    let q = quiz(&s);
    assert!(!q.snap_to_center);
    assert!(q.return_if_wrong);
    assert!(!q.one_per_target);

    // Set drop targets (including a distractor target obj2)
    ok(
        &mut s,
        "question.set_drop_targets",
        json!({ "slide": "slide1", "targets": [obj3.clone(), obj2.clone()] }),
    );
    assert_eq!(quiz(&s).drop_targets.len(), 2);

    // Undo restores previous drop targets
    ok(&mut s, "edit.undo", json!({}));
    assert_ne!(quiz(&s).drop_targets, vec![obj3.clone(), obj2.clone()]);
}

#[test]
fn question_kind_commands_reject_hostile_input() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    let before = s.course.clone();
    let hostile = [
        json!(null),
        json!({ "slide": "slide1" }),
        json!({ "slide": "missing", "kind": "fill_in" }),
        json!({ "slide": "slide1", "kind": "nope" }),
        json!({ "slide": "slide1", "index": u64::MAX, "to": u64::MAX, "text": "x".repeat(10_000) }),
        json!({ "slide": "slide1", "id": "", "left": "", "right": "" }),
    ];
    for cmd in [
        "question.set_kind",
        "question.add_accepted",
        "question.remove_accepted",
        "question.update_accepted",
        "question.set_case_sensitive",
        "question.add_pair",
        "question.remove_pair",
        "question.update_pair",
        "question.reorder_pair",
        "question.reorder_sequence",
        "question.set_hotspot_image",
        "question.add_region",
        "question.remove_region",
        "question.update_region",
        "question.set_markers",
        "question.add_drag_item",
        "question.remove_drag_item",
        "question.update_drag_item",
        "question.set_drop_targets",
        "question.set_drag_options",
    ] {
        for p in &hostile {
            assert!(s.execute(cmd, p).is_err(), "{cmd} accepted {p}");
            assert_eq!(s.course, before, "{cmd} changed the course");
        }
    }
}

#[test]
fn custom_scene_id_is_used() {
    let mut s = Session::blank("Title");
    ok(&mut s, "scene.add", json!({ "id": "custom-scene" }));
    assert!(s.course.scenes.iter().any(|sc| sc.id == "custom-scene"));
}

#[test]
fn custom_scene_id_must_be_unique() {
    let mut s = Session::blank("Title");
    ok(&mut s, "scene.add", json!({ "id": "custom-scene" }));
    assert!(
        s.execute("scene.add", &json!({ "id": "custom-scene" }))
            .is_err()
    );
}

#[test]
fn custom_slide_id_is_used() {
    let mut s = Session::blank("Title");
    ok(&mut s, "slide.add", json!({ "id": "custom-slide" }));
    assert!(s.course.slide("custom-slide").is_some());
}

#[test]
fn custom_slide_id_must_be_unique() {
    let mut s = Session::blank("Title");
    ok(&mut s, "slide.add", json!({ "id": "custom-slide" }));
    assert!(
        s.execute("slide.add", &json!({ "id": "custom-slide" }))
            .is_err()
    );
}

#[test]
fn custom_layer_id_is_used() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "id": "custom-layer" }),
    );
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .layer("custom-layer")
            .is_some()
    );
}

#[test]
fn custom_layer_id_must_be_unique_on_the_slide() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "id": "custom-layer" }),
    );
    assert!(
        s.execute(
            "layer.add",
            &json!({ "slide": "slide1", "id": "custom-layer" })
        )
        .is_err()
    );
}

#[test]
fn custom_object_id_is_used() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "custom-obj", "B", 0);
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .object("custom-obj")
            .is_some()
    );
}

#[test]
fn custom_object_id_must_be_unique_on_the_slide() {
    let mut s = Session::blank("Title");
    add_button(&mut s, "custom-obj", "B", 0);
    assert!(
        s.execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "id": "custom-obj",
                "kind": { "type": "button", "label": "B2" },
                "frame": { "x": 0, "y": 0, "w": 10, "h": 10 }
            })
        )
        .is_err()
    );
}

#[test]
fn id_with_invalid_characters_is_refused() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("slide.add", &json!({ "id": "has space" }))
            .is_err()
    );
}

#[test]
fn empty_id_is_refused() {
    let mut s = Session::blank("Title");
    assert!(s.execute("slide.add", &json!({ "id": "" })).is_err());
}

#[test]
fn id_longer_than_64_characters_is_refused() {
    let mut s = Session::blank("Title");
    assert!(
        s.execute("slide.add", &json!({ "id": "a".repeat(65) }))
            .is_err()
    );
}

fn session_with_scene_variable_and_button() -> Session {
    let mut s = Session::blank("Title");
    ok(&mut s, "scene.add", json!({ "id": "sc2" }));
    ok(&mut s, "variable.add", json!({ "name": "v", "value": 1 }));
    add_button(&mut s, "btn", "B", 0);
    s
}

#[test]
fn undo_restores_the_course_after_object_duplicate() {
    let mut s = session_with_scene_variable_and_button();
    let snapshot = s.course.clone();
    ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": "btn" }),
    );
    assert_ne!(s.course, snapshot);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snapshot);
}

#[test]
fn undo_restores_the_course_after_scene_delete() {
    let mut s = session_with_scene_variable_and_button();
    let snapshot = s.course.clone();
    ok(&mut s, "scene.delete", json!({ "scene": "sc2" }));
    assert_ne!(s.course, snapshot);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snapshot);
}

#[test]
fn undo_restores_the_course_after_variable_delete() {
    let mut s = session_with_scene_variable_and_button();
    let snapshot = s.course.clone();
    ok(&mut s, "variable.delete", json!({ "name": "v" }));
    assert_ne!(s.course, snapshot);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snapshot);
}

/// Every command, fed hostile params, returns Ok or Err; nothing panics and a
/// failed command never changes the course.
#[test]
fn panic_hunt() {
    let hostile = [
        json!(null),
        json!([]),
        json!(42),
        json!("str"),
        json!({}),
        json!({ "slide": "slide1" }),
        json!({ "slide": "slide1", "index": usize::MAX, "ms": u64::MAX }),
        json!({ "slide": 7, "object": [], "layer": {}, "title": "x".repeat(10_000) }),
        json!({ "slide": "slide1", "kind": { "type": "button" }, "frame": { "x": "a" } }),
        json!({ "slide": "slide1", "frame": { "x": 1e308, "y": -1e308, "w": -5, "h": 0 }, "kind": { "type": "text", "text": "" } }),
        json!({ "name": "", "value": null }),
        json!({ "name": "x y", "value": [] }),
        json!({ "slide": "slide1", "question": { "kind": "multiple_choice", "prompt": "", "choices": [] } }),
        json!({ "w": 1e308, "h": -5, "mode": "scale" }),
        json!({ "w": 1e308, "h": -5, "mode": "fill" }),
        json!({ "w": 7680, "h": 4320, "mode": "scale" }),
        json!({ "w": 7680, "h": 4320, "mode": "fill" }),
        json!({ "w": 100, "h": 4320, "mode": "fill" }),
        json!({ "w": 7680, "h": 4320, "mode": 123 }),
        json!({ "w": 0, "h": 0, "mode": "keep" }),
    ];
    for cmd in registry() {
        for p in &hostile {
            let mut s = Session::blank("T");
            let before = s.course.clone();
            if s.execute(cmd.id, p).is_err() {
                assert_eq!(
                    s.course, before,
                    "{} changed the course on error with {p}",
                    cmd.id
                );
            }
        }
    }

    // M31-10: tiny and huge slide sizes across all insert commands.
    let insert_cmds: &[(&str, serde_json::Value)] = &[
        (
            "object.add",
            json!({
                "slide": "slide1",
                "kind": { "type": "rectangle" },
                "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 }
            }),
        ),
        ("question.insert", json!({ "slide": "slide1" })),
        (
            "interaction.insert",
            json!({ "slide": "slide1", "kind": "tabs" }),
        ),
        (
            "interaction.insert",
            json!({ "slide": "slide1", "kind": "accordion" }),
        ),
        ("results.insert", json!({ "after": "slide1" })),
        ("certificate.insert", json!({ "after": "slide1" })),
    ];
    for (w, h) in [(1u32, 1u32), (10, 10), (8192, 8192)] {
        for (cmd_id, params) in insert_cmds {
            let mut s = Session::blank("T");
            s.course.slide_size = coursecraft_course::Size { w, h };
            let before = s.course.clone();
            if s.execute(cmd_id, params).is_err() {
                assert_eq!(
                    s.course, before,
                    "{cmd_id} on {w}x{h} changed the course on error"
                );
            }
        }
    }
}

#[test]
fn text_set_updates_runs_and_undoes() {
    let mut s = Session::blank("Text Test");
    let txt = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "text", "text": "Hello World" },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Style first 5 chars
    ok(
        &mut s,
        "text.style_range",
        json!({
            "slide": "slide1",
            "object": txt,
            "start": 0,
            "end": 5,
            "bold": true
        }),
    );

    let sl = s.course.slide("slide1").unwrap();
    let obj = sl.objects.iter().find(|o| o.id == txt).unwrap();
    if let coursecraft_course::ObjectKind::Text { text, runs, .. } = &obj.kind {
        assert_eq!(text, "Hello World");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Hello");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[1].text, " World");
        assert_eq!(runs[1].bold, None);
    } else {
        panic!("not a text object");
    }

    let snap_styled = s.course.clone();

    // Now edit text with text.set
    ok(
        &mut s,
        "text.set",
        json!({
            "slide": "slide1",
            "object": txt,
            "text": "Hello Brave World"
        }),
    );

    let sl = s.course.slide("slide1").unwrap();
    let obj = sl.objects.iter().find(|o| o.id == txt).unwrap();
    if let coursecraft_course::ObjectKind::Text { text, runs, .. } = &obj.kind {
        assert_eq!(text, "Hello Brave World");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Hello");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[1].text, " Brave World");
        assert_eq!(runs[1].bold, None);
    } else {
        panic!("not a text object");
    }

    // Undo restores previous styled text
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snap_styled);
}

#[test]
fn text_style_range_and_paragraph_and_undo() {
    let mut s = Session::blank("Style & Paragraph");
    let txt = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "text", "text": "Testing" },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let snap_before = s.course.clone();

    ok(
        &mut s,
        "text.style_range",
        json!({
            "slide": "slide1",
            "object": txt,
            "start": 0,
            "end": 0,
            "italic": true,
            "underline": true,
            "size": 24.0,
            "color": "#123456"
        }),
    );

    let sl = s.course.slide("slide1").unwrap();
    let obj = sl.objects.iter().find(|o| o.id == txt).unwrap();
    if let coursecraft_course::ObjectKind::Text { runs, .. } = &obj.kind {
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].italic, Some(true));
        assert_eq!(runs[0].underline, Some(true));
        assert_eq!(runs[0].size, Some(24.0));
        assert_eq!(runs[0].color.as_ref().unwrap().0, "#123456");
    } else {
        panic!("not text");
    }

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snap_before);

    // Paragraph alignment
    ok(
        &mut s,
        "text.set_paragraph",
        json!({
            "slide": "slide1",
            "object": txt,
            "align": "center",
            "valign": "middle"
        }),
    );

    let sl = s.course.slide("slide1").unwrap();
    let obj = sl.objects.iter().find(|o| o.id == txt).unwrap();
    if let coursecraft_course::ObjectKind::Text { style, .. } = &obj.kind {
        assert_eq!(style.align, coursecraft_course::Align::Center);
        assert_eq!(style.valign, coursecraft_course::VAlign::Middle);
    } else {
        panic!("not text");
    }

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snap_before);
}

#[test]
fn text_paragraph_link_and_autofit_commands_undo() {
    let mut s = Session::blank("Paragraphs & Links");
    let txt = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "text", "text": "One\nTwo" },
            "frame": { "x": 0, "y": 0, "w": 300, "h": 120 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let snapshot = s.course.clone();
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({
            "slide": "slide1",
            "object": txt,
            "start": 0,
            "end": 1,
            "list": "bullet",
            "level": 2,
            "line_spacing": 1.5,
            "space_before": 4.0,
            "space_after": 8.0
        }),
    );
    let object = s.course.slide("slide1").unwrap().object(&txt).unwrap().0;
    let coursecraft_course::ObjectKind::Text { paragraphs, .. } = &object.kind else {
        panic!("not text");
    };
    assert_eq!(paragraphs.len(), 1);
    assert_eq!(paragraphs[0].list, coursecraft_course::ListKind::Bullet);
    assert_eq!(paragraphs[0].level, 2);
    assert_eq!(paragraphs[0].line_spacing, 1.5);
    assert_eq!(paragraphs[0].space_before, 4.0);
    assert_eq!(paragraphs[0].space_after, 8.0);

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snapshot);

    ok(
        &mut s,
        "text.set_link",
        json!({
            "slide": "slide1",
            "object": txt,
            "start": 0,
            "end": 3,
            "link": { "type": "url", "url": "https://example.com" }
        }),
    );
    let object = s.course.slide("slide1").unwrap().object(&txt).unwrap().0;
    let coursecraft_course::ObjectKind::Text { runs, .. } = &object.kind else {
        panic!("not text");
    };
    assert_eq!(
        runs[0].link,
        Some(coursecraft_course::Link::Url {
            url: "https://example.com".into()
        })
    );

    let err = s
        .execute(
            "text.set_link",
            &json!({
                "slide": "slide1",
                "object": txt,
                "start": 0,
                "end": 3,
                "link": { "type": "url", "url": "not a url" }
            }),
        )
        .expect_err("invalid url rejected");
    assert!(matches!(err, EngineError::BadParams { .. }));

    let err = s
        .execute(
            "text.set_link",
            &json!({
                "slide": "slide1",
                "object": txt,
                "start": 0,
                "end": 3,
                "link": { "type": "slide", "slide": "nope" }
            }),
        )
        .expect_err("missing slide rejected");
    assert!(matches!(err, EngineError::BadParams { .. }));

    ok(
        &mut s,
        "object.set_autofit",
        json!({ "slide": "slide1", "object": txt, "autofit": "shrink" }),
    );
    let object = s.course.slide("slide1").unwrap().object(&txt).unwrap().0;
    let coursecraft_course::ObjectKind::Text { autofit, .. } = &object.kind else {
        panic!("not text");
    };
    assert_eq!(*autofit, coursecraft_course::Autofit::Shrink);
}

#[test]
fn text_commands_work_on_layer_objects() {
    let mut s = Session::blank("Layer Text");
    let layer = ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "name": "Feedback" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let txt = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": layer,
            "kind": { "type": "text", "text": "Layer Text" },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    ok(
        &mut s,
        "text.style_range",
        json!({
            "slide": "slide1",
            "object": txt,
            "start": 0,
            "end": 5,
            "bold": true
        }),
    );

    let sl = s.course.slide("slide1").unwrap();
    let lay = sl.layers.iter().find(|l| l.id == layer).unwrap();
    let obj = lay.objects.iter().find(|o| o.id == txt).unwrap();
    if let coursecraft_course::ObjectKind::Text { runs, .. } = &obj.kind {
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Layer");
        assert_eq!(runs[0].bold, Some(true));
    } else {
        panic!("not text");
    }
}

#[test]
fn course_theme_and_slide_background_and_undo() {
    let mut s = Session::blank("Theme Test");
    let snap_before = s.course.clone();

    ok(
        &mut s,
        "course.set_theme",
        json!({
            "accent": "#FF5500",
            "background": "#F0F0F0",
            "text": "#222222",
            "font": "Roboto",
            "bar": "#333333",
            "bar_text": "#EEEEEE"
        }),
    );

    assert_eq!(s.course.theme.accent.0, "#FF5500");
    assert_eq!(s.course.theme.background.0, "#F0F0F0");
    assert_eq!(s.course.theme.text.0, "#222222");
    assert_eq!(s.course.theme.font, "Roboto");
    assert_eq!(s.course.theme.player.bar.0, "#333333");
    assert_eq!(s.course.theme.player.bar_text.0, "#EEEEEE");

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, snap_before);

    // Slide background
    ok(
        &mut s,
        "slide.set_background",
        json!({
            "slide": "slide1",
            "background": "#AABBCC"
        }),
    );
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .background
            .as_ref()
            .unwrap()
            .0,
        "#AABBCC"
    );

    // Reset slide background with null
    ok(
        &mut s,
        "slide.set_background",
        json!({
            "slide": "slide1",
            "background": null
        }),
    );
    assert!(s.course.slide("slide1").unwrap().background.is_none());

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .background
            .as_ref()
            .unwrap()
            .0,
        "#AABBCC"
    );

    ok(&mut s, "edit.undo", json!({}));
    assert!(s.course.slide("slide1").unwrap().background.is_none());
}

#[test]
fn hostile_input_handling_in_text_and_theme_commands() {
    let mut s = Session::blank("Hostile");
    let txt = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "text", "text": "Hi" },
            "frame": { "x": 0, "y": 0, "w": 50, "h": 20 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Invalid color
    assert!(
        s.execute(
            "text.style_range",
            &json!({ "slide": "slide1", "object": txt, "start": 0, "end": 1, "color": "not-a-color" })
        )
        .is_err()
    );

    // Negative size
    assert!(
        s.execute(
            "text.style_range",
            &json!({ "slide": "slide1", "object": txt, "start": 0, "end": 1, "size": -5.0 })
        )
        .is_err()
    );

    // Invalid theme color
    assert!(
        s.execute("course.set_theme", &json!({ "accent": "zzz" }))
            .is_err()
    );

    // Invalid slide background
    assert!(
        s.execute(
            "slide.set_background",
            &json!({ "slide": "slide1", "background": "invalid" })
        )
        .is_err()
    );

    // Invalid align / valign
    assert!(
        s.execute(
            "text.set_paragraph",
            &json!({ "slide": "slide1", "object": txt, "align": "upside-down" })
        )
        .is_err()
    );
}

#[test]
fn emphasis_commands_and_the_animation_painter() {
    let mut s = Session::blank("Test");
    let add = |s: &mut Session, x: f32| {
        s.execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": x, "y": 10.0, "w": 100.0, "h": 50.0 }
            }),
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let a = add(&mut s, 10.0);
    let b = add(&mut s, 200.0);
    let get = |s: &Session, id: &str| {
        s.course
            .slide("slide1")
            .unwrap()
            .object(id)
            .unwrap()
            .0
            .clone()
    };

    let r = s
        .execute(
            "object.add_emphasis",
            &json!({ "slide": "slide1", "object": a, "effect": "shake", "start_ms": 1500 }),
        )
        .unwrap();
    assert_eq!(r["index"], 0);
    assert_eq!(
        get(&s, &a).emphasis[0].duration_ms,
        600,
        "the registry default"
    );
    assert!(
        s.execute(
            "object.add_emphasis",
            &json!({ "slide": "slide1", "object": a, "effect": "wobble" }),
        )
        .is_err(),
        "unknown emphasis"
    );
    assert!(
        s.execute(
            "object.set_emphasis",
            &json!({ "slide": "slide1", "object": a, "index": 3, "emphasis": { "effect": { "type": "spin" } } }),
        )
        .is_err(),
        "no such index"
    );
    s.execute(
        "object.set_emphasis",
        &json!({ "slide": "slide1", "object": a, "index": 0, "emphasis": { "effect": { "type": "spin" }, "start_ms": 1000, "repeat": 2 } }),
    )
    .unwrap();
    assert_eq!(get(&s, &a).emphasis[0].effect.id, "spin");
    assert_eq!(get(&s, &a).emphasis[0].end_ms(), 3000);

    // The painter copies entrance, exit and emphasis; emphasis keeps its offset from the start.
    s.execute(
        "object.set_animation",
        &json!({ "slide": "slide1", "object": a, "which": "entrance",
                 "animation": { "effect": { "type": "wipe" }, "duration_ms": 700 } }),
    )
    .unwrap();
    s.execute(
        "timing.set",
        &json!({ "slide": "slide1", "object": b, "start_ms": 2000 }),
    )
    .ok();
    let b_start = get(&s, &b).timing.start_ms;
    s.execute(
        "object.copy_animation",
        &json!({ "slide": "slide1", "from": a, "objects": [b] }),
    )
    .unwrap();
    let copied = get(&s, &b);
    assert_eq!(copied.entrance, get(&s, &a).entrance);
    assert_eq!(copied.emphasis[0].start_ms, b_start + 1000);
    s.undo().unwrap();
    assert!(get(&s, &b).entrance.is_none() && get(&s, &b).emphasis.is_empty());

    s.execute(
        "object.remove_emphasis",
        &json!({ "slide": "slide1", "object": a, "index": 0 }),
    )
    .unwrap();
    assert!(get(&s, &a).emphasis.is_empty());
    s.undo().unwrap();
    assert_eq!(get(&s, &a).emphasis.len(), 1);
}

#[test]
fn object_set_animation_and_undo() {
    let mut s = Session::blank("Test");
    let obj_id = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 }
            }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Set entrance animation
    s.execute(
        "object.set_animation",
        &json!({
            "slide": "slide1",
            "object": obj_id,
            "which": "entrance",
            "animation": {
                "effect": { "type": "fade" },
                "duration_ms": 800,
                "easing": "linear"
            }
        }),
    )
    .unwrap();

    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.entrance.is_some());
    assert_eq!(obj.entrance.as_ref().unwrap().duration_ms, 800);

    // Set exit animation
    s.execute(
        "object.set_animation",
        &json!({
            "slide": "slide1",
            "object": obj_id,
            "which": "exit",
            "animation": {
                "effect": { "type": "fly_in", "from": "left" },
                "duration_ms": 600,
                "easing": "ease_in"
            }
        }),
    )
    .unwrap();

    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.exit.is_some());

    // Undo exit
    s.undo().unwrap();
    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.exit.is_none());
    assert!(obj.entrance.is_some());

    // Clear entrance animation
    s.execute(
        "object.set_animation",
        &json!({
            "slide": "slide1",
            "object": obj_id,
            "which": "entrance",
            "animation": null
        }),
    )
    .unwrap();
    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.entrance.is_none());

    // Hostile input: bad which, bad duration
    assert!(
        s.execute(
            "object.set_animation",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "which": "middle",
                "animation": null
            })
        )
        .is_err()
    );
    assert!(
        s.execute(
            "object.set_animation",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "which": "entrance",
                "animation": {
                    "effect": { "type": "fade" },
                    "duration_ms": 0
                }
            })
        )
        .is_err()
    );
    assert!(
        s.execute(
            "object.set_animation",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "which": "entrance",
                "animation": {
                    "effect": { "type": "fade" },
                    "duration_ms": 999999
                }
            })
        )
        .is_err()
    );
}

#[test]
fn motion_path_lifecycle_and_triggers() {
    let mut s = Session::blank("Test");
    let obj_id = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 }
            }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Add first motion path
    let res = s
        .execute(
            "object.add_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
            }),
        )
        .unwrap();
    let path1 = res["id"].as_str().unwrap().to_string();
    assert_eq!(path1, "path1");

    // Add second motion path
    let res2 = s
        .execute(
            "object.add_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "points": [{ "x": 0.0, "y": 0.0 }, { "x": 50.0, "y": 100.0 }],
                "duration_ms": 2500,
                "easing": "ease_in_out",
                "relative": false
            }),
        )
        .unwrap();
    let path2 = res2["id"].as_str().unwrap().to_string();
    assert_eq!(path2, "path2");

    // Update path1
    s.execute(
        "object.update_motion_path",
        &json!({
            "slide": "slide1",
            "object": obj_id,
            "path": path1,
            "duration_ms": 3000
        }),
    )
    .unwrap();

    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    let p1 = obj.motion_paths.iter().find(|p| p.id == path1).unwrap();
    assert_eq!(p1.duration_ms, 3000);

    // Add triggers using MoveOnPath and AnimationCompletes
    s.execute(
        "trigger.add",
        &json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "move_on_path", "object": obj_id, "path": path1 },
                "when": { "event": "timeline_starts" }
            }
        }),
    )
    .unwrap();

    s.execute(
        "trigger.add",
        &json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "jump_to_slide", "target": "next" },
                "when": { "event": "animation_completes", "object": obj_id, "path": path1 }
            }
        }),
    )
    .unwrap();

    // Removing path1 removes its referencing triggers in the same undo step
    // and keeps the course valid.
    s.execute(
        "object.remove_motion_path",
        &json!({
            "slide": "slide1",
            "object": obj_id,
            "path": path1
        }),
    )
    .unwrap();
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());
    assert!(s.course.slide("slide1").unwrap().triggers.is_empty());

    // Undo restores path1 and its referencing triggers.
    s.undo().unwrap();
    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.motion_paths.iter().any(|p| p.id == path1));
    assert_eq!(slide.triggers.len(), 2);

    // Hostile inputs
    assert!(
        s.execute(
            "object.add_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "points": [{ "x": 0.0, "y": 0.0 }] // less than 2 points
            })
        )
        .is_err()
    );
    assert!(
        s.execute(
            "object.add_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "points": [{ "x": 0.0, "y": 0.0 }, { "x": 1000000.0, "y": 0.0 }] // out of bounds
            })
        )
        .is_err()
    );
    assert!(
        s.execute(
            "object.update_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "path": "non-existent",
                "duration_ms": 500
            })
        )
        .is_err()
    );
}

#[test]
fn slide_transition_lifecycle_and_undo() {
    let mut s = Session::blank("Test");

    // Set transition
    s.execute(
        "slide.set_transition",
        &json!({
            "slide": "slide1",
            "transition": {
                "type": "push",
                "from": "right",
                "duration_ms": 750
            }
        }),
    )
    .unwrap();

    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.transition.is_some());
    assert_eq!(slide.transition.as_ref().unwrap().duration_ms, 750);

    // Undo
    s.undo().unwrap();
    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.transition.is_none());

    // Redo
    s.redo().unwrap();
    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.transition.is_some());

    // Clear transition
    s.execute(
        "slide.set_transition",
        &json!({
            "slide": "slide1",
            "transition": null
        }),
    )
    .unwrap();
    let slide = s.course.slide("slide1").unwrap();
    assert!(slide.transition.is_none());

    // Hostile duration
    assert!(
        s.execute(
            "slide.set_transition",
            &json!({
                "slide": "slide1",
                "transition": {
                    "effect": "fade",
                    "duration_ms": 0
                }
            })
        )
        .is_err()
    );
}

#[test]
fn course_audit_returns_the_report_as_json() {
    let mut s = Session::blank("Accessible?");
    assert!(
        ok(&mut s, "course.audit", json!({}))["findings"]
            .as_array()
            .expect("findings array")
            .is_empty()
    );

    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "image", "src": "photo.png" },
            "frame": { "x": 0, "y": 0, "w": 200, "h": 200 }
        }),
    );

    let report = ok(&mut s, "course.audit", json!({}));
    let findings = report["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f["check"] == "alt-text" && f["severity"] == "error"),
        "{findings:?}"
    );
    let manual = report["manual_checks"].as_array().expect("manual checks");
    assert_eq!(manual.len(), 7);
    for text in findings
        .iter()
        .flat_map(|f| [f["message"].as_str(), f["fix"].as_str()])
        .flatten()
    {
        let lower = text.to_ascii_lowercase();
        assert!(!lower.contains("compliant"));
        assert!(!lower.contains("passes wcag"));
        assert!(!lower.contains("accessible course"));
    }
}

#[test]
fn question_insert_adds_question_submit_button_and_click_trigger_and_undoes() {
    use coursecraft_course::{Action, Event, ObjectKind};

    let mut s = Session::blank("Title");
    let before = s.course.clone();
    let result = ok(&mut s, "question.insert", json!({ "slide": "slide1" }));
    let button = result["button"].as_str().expect("button id").to_owned();

    let slide = s.course.slide("slide1").expect("slide");
    assert!(slide.question.is_some());
    let (object, _) = slide.object(&button).expect("submit button object");
    assert!(matches!(object.kind, ObjectKind::Button { .. }));
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action == Action::SubmitQuestion
            && trigger.when
                == Event::Click {
                    object: button.clone(),
                }
    }));
    assert!(s.can_undo());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_insert_every_kind_stays_playable() {
    for kind in [
        "multiple_choice",
        "multiple_response",
        "true_false",
        "fill_in",
        "sequence",
        "matching",
        "hotspot",
        "drag_drop",
    ] {
        let mut s = Session::blank("Title");
        let result = s.execute(
            "question.insert",
            &json!({ "slide": "slide1", "kind": kind }),
        );
        assert!(result.is_ok(), "{kind}: {result:?}");
        assert!(s.course.is_playable(), "{kind}: {:#?}", s.course.validate());
        assert!(
            s.course
                .slide("slide1")
                .and_then(|sl| sl.question.as_ref())
                .is_some()
        );
    }
}

#[test]
fn question_insert_adds_two_hidden_feedback_layers_and_undoes() {
    use coursecraft_course::{Action, ObjectKind};

    let mut s = Session::blank("Title");
    let before = s.course.clone();
    ok(&mut s, "question.insert", json!({ "slide": "slide1" }));
    let slide = s.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    let correct = question.correct_layer.as_deref().expect("correct layer");
    let incorrect = question
        .incorrect_layer
        .as_deref()
        .expect("incorrect layer");
    for (id, name, text) in [
        (correct, "Correct", "Correct!"),
        (incorrect, "Incorrect", "Not quite."),
    ] {
        let layer = slide.layer(id).expect("feedback layer");
        assert_eq!(layer.name, name);
        assert!(layer.objects.iter().any(|object| {
            matches!(&object.kind, ObjectKind::Text { text: value, .. } if value == text)
        }));
    }
    assert!(
        slide
            .triggers
            .iter()
            .any(|trigger| trigger.action == Action::SubmitQuestion)
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_add_feedback_layers_sets_both_targets_and_undoes() {
    use coursecraft_course::ObjectKind;

    let mut s = Session::blank("Title");
    ok(&mut s, "question.insert", json!({ "slide": "slide1" }));
    {
        let slide = s.course.slide_mut("slide1").expect("slide");
        slide.layers.clear();
        if let Some(question) = slide.question.as_mut() {
            question.correct_layer = None;
            question.incorrect_layer = None;
        }
    }
    s.revision = s.revision.wrapping_add(1);
    let before = s.course.clone();
    ok(
        &mut s,
        "question.add_feedback_layers",
        json!({ "slide": "slide1" }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    let correct = question.correct_layer.as_deref().expect("correct layer");
    let incorrect = question
        .incorrect_layer
        .as_deref()
        .expect("incorrect layer");
    assert!(slide.layer(correct).is_some());
    assert!(slide.layer(incorrect).is_some());
    assert!(slide.layers.iter().any(|layer| {
        layer
            .objects
            .iter()
            .any(|o| matches!(&o.kind, ObjectKind::Text { text, .. } if text == "Not quite."))
    }));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_add_try_again_layer_creates_layer_button_and_undoes() {
    use coursecraft_course::{Action, Event, ObjectKind};

    let mut s = Session::blank("Title");
    // Refuses a slide without a question.
    assert!(
        s.execute(
            "question.add_try_again_layer",
            &json!({ "slide": "slide1" })
        )
        .is_err()
    );

    ok(&mut s, "question.insert", json!({ "slide": "slide1" }));
    let before = s.course.clone();
    let res = ok(
        &mut s,
        "question.add_try_again_layer",
        json!({ "slide": "slide1" }),
    );
    let layer_id = res["layer"].as_str().expect("layer id");
    let slide = s.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    assert_eq!(question.try_again_layer.as_deref(), Some(layer_id));

    let layer = slide.layer(layer_id).expect("try again layer");
    assert_eq!(layer.name, "Try Again");
    assert!(
        layer
            .objects
            .iter()
            .any(|o| matches!(&o.kind, ObjectKind::Text { .. }))
    );
    let btn = layer
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Button { label, .. } if label == "Try Again"))
        .expect("Try Again button");
    assert!(layer.triggers.iter().any(|t| {
        t.when
            == Event::Click {
                object: btn.id.clone(),
            }
            && t.action == Action::HideLayer { layer: None }
    }));
    assert!(s.course.is_playable());

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_insert_drag_drop_names_the_item_target_and_label() {
    use coursecraft_course::{ObjectKind, QuestionKind};

    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "question.insert",
        json!({ "slide": "slide1", "kind": "drag_drop" }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    assert_eq!(question.kind, QuestionKind::DragDrop);
    let item = question.drag_items.first().expect("drag item").item.clone();
    let target = question.drop_targets.first().expect("drop target").clone();
    let (item_object, _) = slide.object(&item).expect("item object");
    assert!(matches!(
        item_object.kind,
        ObjectKind::Button { ref label, .. } if label == "Drag me"
    ));
    let (target_object, _) = slide.object(&target).expect("target object");
    assert_eq!(target_object.name, "Drop target");
    assert!(matches!(
        target_object.kind,
        ObjectKind::Shape {
            shape: coursecraft_course::ShapeKind::RoundedRectangle,
            ..
        }
    ));
    assert!(slide.objects.iter().any(|object| {
        object.name == "Drop target label"
            && matches!(&object.kind, ObjectKind::Text { text, .. } if text == "Drop here")
    }));
    assert!(
        s.course
            .validate()
            .iter()
            .all(|issue| { issue.severity != coursecraft_course::Severity::Error })
    );
}

#[test]
fn question_insert_hotspot_keeps_a_placeholder_image() {
    use coursecraft_course::ObjectKind;

    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "question.insert",
        json!({ "slide": "slide1", "kind": "hotspot" }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    let image = question.hotspot_image.as_deref().expect("hotspot image");
    let (object, _) = slide.object(image).expect("image object");
    assert!(matches!(
        &object.kind,
        ObjectKind::Image { src, .. } if src == "placeholder.png"
    ));
    assert!(s.course.is_playable());
}

#[test]
fn question_insert_refuses_a_slide_that_already_has_a_question() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    let rev = s.revision;
    assert!(
        s.execute("question.insert", &json!({ "slide": "slide1" }))
            .is_err()
    );
    assert_eq!(s.revision, rev);
}

#[test]
fn results_insert_places_a_results_slide_after_the_given_slide() {
    use coursecraft_course::{Action, CmpOp, Condition, Event, Trigger, Value};

    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    let second = ok(&mut s, "slide.add", json!({ "title": "Second" }))["id"]
        .as_str()
        .expect("second slide id")
        .to_owned();
    let before = s.course.clone();
    let result = ok(&mut s, "results.insert", json!({ "after": "slide1" }));
    let id = result["slide"]
        .as_str()
        .expect("results slide id")
        .to_owned();
    assert_eq!(result["passed_layer"], "passed");
    assert_eq!(result["failed_layer"], "failed");

    let scene = &s.course.scenes[0];
    assert_eq!(scene.slides[1].id, id);
    assert_eq!(scene.slides[2].id, second);

    let slide = s.course.slide(&id).expect("results slide");
    assert_eq!(slide.title, "Results");
    assert_eq!(slide.duration_ms, 5000);
    assert_eq!(slide.layers.len(), 2);
    for (layer_id, name, message, labels) in [
        (
            "passed",
            "Passed",
            "Congratulations, you passed.",
            ["Review quiz", "Finish"],
        ),
        (
            "failed",
            "Failed",
            "You did not pass this time.",
            ["Retry quiz", "Review quiz"],
        ),
    ] {
        let layer = slide.layer(layer_id).expect("outcome layer");
        assert_eq!(layer.name, name);
        assert!(layer.hide_other_layers);
        assert!(layer.objects.iter().any(|object| {
            matches!(&object.kind, coursecraft_course::ObjectKind::Text { text, .. } if text == message)
        }));
        for label in labels {
            let button = layer
                .objects
                .iter()
                .find(|object| {
                    matches!(&object.kind, coursecraft_course::ObjectKind::Button { label: value, .. } if value == label)
                })
                .expect("outcome button");
            assert!(layer.triggers.iter().any(|trigger| {
                trigger.when
                    == Event::Click {
                        object: button.id.clone(),
                    }
            }));
        }
    }
    assert_eq!(slide.triggers.len(), 3);
    assert!(matches!(
        &slide.triggers[0],
        Trigger {
            action: Action::SubmitResults,
            when: Event::TimelineStarts { layer: None },
            condition: None,
            disabled: false,
        }
    ));
    for (index, layer) in [(1, "passed"), (2, "failed")] {
        match &slide.triggers[index] {
            Trigger {
                action: Action::ShowLayer { layer: shown },
                when: Event::TimelineStarts { layer: None },
                condition:
                    Some(Condition::Compare {
                        variable,
                        op: CmpOp::Eq,
                        value,
                    }),
                disabled: false,
            } => {
                assert_eq!(shown, layer);
                assert_eq!(variable, "Quiz.Passed");
                assert_eq!(*value, Value::Bool(layer == "passed").into());
            }
            other => panic!("unexpected trigger {other:?}"),
        }
    }
    assert!(
        s.course
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error)
    );

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn results_insert_sets_the_passing_score_to_80_only_when_unset() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    assert!(s.course.quiz.is_none());
    ok(&mut s, "results.insert", json!({}));
    assert_eq!(s.course.quiz.map(|quiz| quiz.passing_percent), Some(80));

    ok(&mut s, "course.set_quiz", json!({ "passing_percent": 55 }));
    ok(&mut s, "results.insert", json!({}));
    assert_eq!(s.course.quiz.map(|quiz| quiz.passing_percent), Some(55));
}

#[test]
fn results_insert_refuses_a_course_without_a_question() {
    let mut s = Session::blank("Title");
    let rev = s.revision;
    let err = s
        .execute("results.insert", &json!({}))
        .expect_err("course has no question");
    assert!(matches!(
        err,
        EngineError::Invalid(message) if message == "the course has no question to report"
    ));
    assert_eq!(s.revision, rev);
}

#[test]
fn results_insert_refuses_an_unknown_slide() {
    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    let before = s.course.clone();
    let err = s
        .execute("results.insert", &json!({ "after": "nope" }))
        .expect_err("unknown after");
    assert!(matches!(err, EngineError::NotFound(_)));
    assert_eq!(s.course, before);
}

#[test]
fn certificate_insert_places_a_certificate_slide_with_a_print_trigger_and_undoes() {
    use coursecraft_course::{Action, Event, ObjectKind};

    let mut s = Session::blank("Title");
    let second = ok(&mut s, "slide.add", json!({ "title": "Second" }))["id"]
        .as_str()
        .expect("second slide id")
        .to_owned();
    let before = s.course.clone();
    let result = ok(&mut s, "certificate.insert", json!({ "after": "slide1" }));
    let id = result["slide"]
        .as_str()
        .expect("certificate slide id")
        .to_owned();

    let scene = &s.course.scenes[0];
    assert_eq!(scene.slides[1].id, id);
    assert_eq!(scene.slides[2].id, second);

    let slide = s.course.slide(&id).expect("certificate slide");
    assert_eq!(slide.title, "Certificate");
    assert_eq!(slide.background, Some(s.course.theme.background.clone()));
    assert!(slide.question.is_none());

    let border = slide
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            ObjectKind::Shape {
                shape,
                fill,
                outline,
                ..
            } if *shape == coursecraft_course::ShapeKind::RoundedRectangle => Some((fill, outline)),
            _ => None,
        })
        .expect("border shape");
    assert_eq!(border.0.0, "#00000000");
    let outline = border.1.as_ref().expect("border outline");
    assert_eq!(outline.width, 6.0);
    assert_eq!(outline.color, s.course.theme.accent);

    for text in [
        "Certificate of Completion",
        "This certifies that",
        "%Learner.Name%",
        "has completed",
        "%Course.Title%",
        "Score: %Course.ScorePercent%%",
        "%Course.Date%",
    ] {
        assert!(
            slide.objects.iter().any(|object| matches!(
                &object.kind,
                ObjectKind::Text { text: value, .. } if value == text
            )),
            "missing text {text:?}"
        );
    }

    let button = slide
        .objects
        .iter()
        .find(|object| matches!(&object.kind, ObjectKind::Button { label, .. } if label == "Print"))
        .expect("Print button");
    assert_eq!(button.frame.w, 160.0);
    assert_eq!(button.frame.h, 48.0);
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action == Action::PrintSlide
            && trigger.when
                == Event::Click {
                    object: button.id.clone(),
                }
    }));
    assert!(
        s.course
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error)
    );

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_add_submit_button_removes_the_dead_submit_trigger_and_undoes() {
    use coursecraft_course::{Action, Event};

    let mut s = Session::blank("Title");
    set_multiple_choice_question(&mut s);
    assert!(s.course.slide("slide1").is_some_and(|slide| {
        slide
            .triggers
            .iter()
            .any(|trigger| trigger.when == Event::Submit)
    }));
    let before = s.course.clone();
    let result = ok(
        &mut s,
        "question.add_submit_button",
        json!({ "slide": "slide1" }),
    );
    let button = result["button"].as_str().expect("button id").to_owned();
    let slide = s.course.slide("slide1").expect("slide");
    assert!(!slide.triggers.iter().any(|t| t.when == Event::Submit));
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action == Action::SubmitQuestion
            && trigger.when
                == Event::Click {
                    object: button.clone(),
                }
    }));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn course_add_finish_button_uses_the_last_slide_reports_then_exits_and_undoes() {
    use coursecraft_course::{Action, Event};

    let mut s = Session::blank("Title");
    let second = ok(&mut s, "slide.add", json!({ "title": "Last" }))["id"]
        .as_str()
        .expect("second slide id")
        .to_owned();
    let before = s.course.clone();
    let result = ok(&mut s, "course.add_finish_button", json!({}));
    let button = result["button"]
        .as_str()
        .expect("finish button id")
        .to_owned();
    let slide = s.course.slide(&second).expect("last slide");
    let (object, _) = slide.object(&button).expect("finish object");
    assert!(matches!(
        object.kind,
        coursecraft_course::ObjectKind::Button { .. }
    ));
    let actions: Vec<Action> = slide
        .triggers
        .iter()
        .filter(|trigger| {
            trigger.when
                == Event::Click {
                    object: button.clone(),
                }
        })
        .map(|trigger| trigger.action.clone())
        .collect();
    assert_eq!(actions, vec![Action::SubmitResults, Action::ExitCourse]);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn course_add_finish_button_sits_left_of_an_existing_button() {
    use coursecraft_course::{Action, ObjectKind};

    let mut s = Session::blank("Title");
    let slide_size = s.course.slide_size;
    let submit = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Submit" },
            "frame": { "x": slide_size.w as f32 - 200.0, "y": slide_size.h as f32 - 88.0, "w": 160.0, "h": 48.0 }
        }),
    )["id"]
        .as_str()
        .expect("submit id")
        .to_owned();
    let before = s.course.clone();
    let finish = ok(&mut s, "course.add_finish_button", json!({}))["button"]
        .as_str()
        .expect("finish id")
        .to_owned();
    let slide = s.course.slide("slide1").expect("slide");
    let (finish_object, _) = slide.object(&finish).expect("finish object");
    let (submit_object, _) = slide.object(&submit).expect("submit object");
    assert!(matches!(finish_object.kind, ObjectKind::Button { .. }));
    assert!(
        finish_object.frame.x + finish_object.frame.w <= submit_object.frame.x,
        "finish {:?} must sit left of submit {:?}",
        finish_object.frame,
        submit_object.frame
    );
    assert!(
        slide
            .triggers
            .iter()
            .any(|trigger| trigger.action == Action::ExitCourse)
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn object_paste_gets_fresh_ids_offsets_clamps_and_undoes() {
    let mut s = Session::blank("Paste");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "name": "Audio",
            "kind": { "type": "audio", "src": "a.mp3" },
            "frame": { "x": 1000, "y": 700, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.clone())
        .expect("object");
    let before = s.course.clone();

    let result = ok(
        &mut s,
        "object.paste",
        json!({
            "slide": "slide1",
            "objects": [object],
            "offset": 20,
        }),
    );
    let new_id = result["ids"][0].as_str().expect("new id").to_owned();
    assert_ne!(new_id, id);
    let pasted = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&new_id))
        .map(|(object, _)| object.clone())
        .expect("pasted object");
    assert_eq!(pasted.frame.x, 1020.0);
    assert_eq!(
        pasted.frame.y, 719.0,
        "at least one pixel stays on the slide"
    );
    assert_eq!(pasted.name, "Audio");

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);

    // A paste to another slide with offset 0 keeps the position.
    ok(&mut s, "slide.add", json!({ "id": "slide2" }));
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.clone())
        .expect("object");
    let result = ok(
        &mut s,
        "object.paste",
        json!({ "slide": "slide2", "objects": [object], "offset": 0 }),
    );
    let new_id = result["ids"][0].as_str().expect("new id").to_owned();
    let pasted = s
        .course
        .slide("slide2")
        .and_then(|slide| slide.object(&new_id))
        .map(|(object, _)| object.clone())
        .expect("pasted object");
    assert_eq!(pasted.frame.x, 1000.0);
    assert_eq!(pasted.frame.y, 700.0);
}

#[test]
fn object_paste_rewrites_every_object_reference() {
    let mut s = Session::blank("Paste refs");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "audio", "src": "a.mp3" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .expect("id")
        .to_owned();
    {
        let slide = s.course.slide_mut("slide1").expect("slide1");
        let object = slide
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .expect("object");
        object.states.push(coursecraft_course::StateDef {
            name: "selected".into(),
            ..Default::default()
        });
        object.motion_paths.push(coursecraft_course::MotionPath {
            id: "p1".into(),
            points: vec![
                coursecraft_course::Point { x: 0.0, y: 0.0 },
                coursecraft_course::Point { x: 10.0, y: 10.0 },
            ],
            duration_ms: 1000,
            easing: coursecraft_course::Easing::Linear,
            relative: true,
        });
    }
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.clone())
        .expect("object");
    s.course
        .slide_mut("slide1")
        .expect("slide1")
        .triggers
        .clear();

    let click = |action| coursecraft_course::Trigger {
        action,
        when: coursecraft_course::Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    };
    let triggers = vec![
        click(coursecraft_course::Action::ChangeState {
            object: id.clone(),
            state: "selected".into(),
        }),
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::PlayMedia { object: id.clone() },
            when: coursecraft_course::Event::HoverStart { object: id.clone() },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::PauseMedia { object: id.clone() },
            when: coursecraft_course::Event::HoverEnd { object: id.clone() },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::StopMedia { object: id.clone() },
            when: coursecraft_course::Event::StateChanges {
                object: id.clone(),
                to: None,
            },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::MoveOnPath {
                object: id.clone(),
                path: "p1".into(),
            },
            when: coursecraft_course::Event::MediaEnds { object: id.clone() },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::JumpToSlide {
                target: coursecraft_course::SlideTarget::Next,
            },
            when: coursecraft_course::Event::AnimationCompletes {
                object: id.clone(),
                path: Some("p1".into()),
            },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::ChangeState {
                object: id.clone(),
                state: "selected".into(),
            },
            when: coursecraft_course::Event::Click { object: id.clone() },
            condition: Some(coursecraft_course::Condition::ObjectState {
                object: id.clone(),
                state: "selected".into(),
                is: true,
            }),
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::JumpToSlide {
                target: coursecraft_course::SlideTarget::Id("slide1".into()),
            },
            when: coursecraft_course::Event::Click { object: id.clone() },
            condition: None,
            disabled: false,
        },
        click(coursecraft_course::Action::OpenUrl {
            url: "https://example.com".into(),
        }),
        click(coursecraft_course::Action::CompleteCourse {
            status: coursecraft_course::CompletionStatus::Completed,
        }),
    ];
    let result = ok(
        &mut s,
        "object.paste",
        json!({
            "slide": "slide1",
            "objects": [object],
            "triggers": triggers,
            "offset": 0,
        }),
    );
    let new_id = result["ids"][0].as_str().expect("new id").to_owned();
    assert_eq!(result["skipped"], json!(0));
    let slide = s.course.slide("slide1").expect("slide1");
    assert_eq!(slide.triggers.len(), 10);
    for trigger in &slide.triggers {
        let text = serde_json::to_string(trigger).expect("trigger json");
        assert!(
            !text.contains(&format!("\"{id}\"")),
            "old object id left in {text}"
        );
        assert!(
            text.contains(&format!("\"{new_id}\"")),
            "new object id missing from {text}"
        );
    }
}

#[test]
fn object_paste_skips_triggers_that_refer_outside_the_copied_objects() {
    let mut s = Session::blank("Paste skip");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Go" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 50 }
        }),
    )["id"]
        .as_str()
        .expect("id")
        .to_owned();
    s.course
        .slide_mut("slide1")
        .expect("slide1")
        .objects
        .iter_mut()
        .find(|object| object.id == id)
        .expect("object")
        .states
        .push(coursecraft_course::StateDef {
            name: "selected".into(),
            ..Default::default()
        });
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.clone())
        .expect("object");
    s.course
        .slide_mut("slide1")
        .expect("slide1")
        .triggers
        .clear();
    let triggers = vec![
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::ChangeState {
                object: id.clone(),
                state: "selected".into(),
            },
            when: coursecraft_course::Event::Click { object: id.clone() },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::PlayMedia {
                object: "other".into(),
            },
            when: coursecraft_course::Event::Click { object: id.clone() },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::ShowLayer {
                layer: "ghost".into(),
            },
            when: coursecraft_course::Event::Click { object: id.clone() },
            condition: None,
            disabled: false,
        },
    ];
    let result = ok(
        &mut s,
        "object.paste",
        json!({
            "slide": "slide1",
            "objects": [object],
            "triggers": triggers,
            "offset": 0,
        }),
    );
    assert_eq!(result["skipped"], json!(2));
    assert_eq!(s.course.slide("slide1").expect("slide1").triggers.len(), 1);
}

#[test]
fn object_paste_rejects_non_finite_frames_without_changing_the_course() {
    let mut s = Session::blank("Paste hostile");
    let object = json!({
        "id": "bad",
        "name": "",
        "kind": { "type": "shape", "shape": "rectangle", "fill": "#D0D7DE" },
        "frame": { "x": 1e39, "y": 0, "w": 10, "h": 10 }
    });
    let before = s.course.clone();
    let error = s
        .execute(
            "object.paste",
            &json!({ "slide": "slide1", "objects": [object] }),
        )
        .unwrap_err();
    assert!(matches!(error, EngineError::BadParams { .. }), "{error}");
    assert_eq!(s.course, before);
}

#[test]
fn slide_duplicate_rewrites_internal_references_and_undoes() {
    let mut s = Session::blank("Duplicate");
    let button = ok(
        &mut s,
        "question.insert",
        json!({ "slide": "slide1", "kind": "multiple_choice" }),
    )["button"]
        .as_str()
        .expect("button")
        .to_owned();
    let old_correct = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.question.as_ref())
        .and_then(|question| question.correct_layer.clone())
        .expect("correct layer");
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "show_layer", "layer": old_correct },
                "when": { "event": "click", "object": button },
            }
        }),
    );
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "jump_to_slide", "target": { "id": "slide1" } },
                "when": { "event": "click", "object": button },
            }
        }),
    );
    let before = s.course.clone();
    let result = ok(&mut s, "slide.duplicate", json!({ "slide": "slide1" }));
    let new_id = result["id"].as_str().expect("new slide id").to_owned();
    assert_ne!(new_id, "slide1");
    let copy = s.course.slide(&new_id).expect("copy");
    assert!(copy.title.ends_with(" copy"));
    assert!(
        copy.objects.iter().all(|object| object.id != button),
        "objects get fresh ids"
    );
    assert!(copy.layers.iter().all(|layer| layer.id != old_correct));
    let new_correct = copy
        .question
        .as_ref()
        .and_then(|question| question.correct_layer.clone())
        .expect("copied correct layer");
    assert_ne!(new_correct, old_correct);
    assert!(copy.layer(&new_correct).is_some());
    assert!(copy.triggers.iter().any(|trigger| {
        matches!(
            &trigger.action,
            coursecraft_course::Action::JumpToSlide {
                target: coursecraft_course::SlideTarget::Id(id)
            } if id == &new_id
        )
    }));
    assert!(copy.triggers.iter().any(|trigger| {
        matches!(
            &trigger.action,
            coursecraft_course::Action::ShowLayer { layer } if layer == &new_correct
        )
    }));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn slide_paste_gives_fresh_ids_and_skips_missing_jump_targets() {
    let mut s = Session::blank("Paste slide");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Go" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 50 }
        }),
    );
    let mut source = s.course.slide("slide1").expect("slide1").clone();
    source.title = "Imported".into();
    source.triggers.push(coursecraft_course::Trigger {
        action: coursecraft_course::Action::JumpToSlide {
            target: coursecraft_course::SlideTarget::Id("missing".into()),
        },
        when: coursecraft_course::Event::Click {
            object: "button1".into(),
        },
        condition: None,
        disabled: false,
    });
    let before = s.course.clone();
    let result = ok(
        &mut s,
        "slide.paste",
        json!({ "slide": source, "after": "slide1" }),
    );
    let new_id = result["id"].as_str().expect("new slide id").to_owned();
    assert_eq!(result["skipped"], json!(1));
    let copy = s.course.slide(&new_id).expect("copy");
    assert_eq!(copy.title, "Imported copy");
    assert!(copy.triggers.is_empty());
    assert!(copy.objects.iter().all(|object| object.id != "button1"));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn course_update_sets_and_clears_language() {
    let mut s = Session::blank("Title");
    let before = s.course.clone();
    ok(&mut s, "course.update", json!({ "language": "pt-BR" }));
    assert_eq!(s.course.language.as_deref(), Some("pt-BR"));
    ok(&mut s, "course.update", json!({ "language": null }));
    assert_eq!(s.course.language, None);
    assert!(
        s.execute("course.update", &json!({ "language": "not a tag!" }))
            .is_err()
    );
    assert_eq!(s.course, before);
}

fn add_frame_button(s: &mut Session, id: &str, x: f32, y: f32, w: f32, h: f32) {
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": id,
            "kind": { "type": "button", "label": id },
            "frame": { "x": x, "y": y, "w": w, "h": h }
        }),
    );
}

fn frame_of(s: &Session, id: &str) -> coursecraft_course::Rect {
    s.course
        .slide("slide1")
        .unwrap()
        .object(id)
        .unwrap()
        .0
        .frame
}

fn group_of(s: &Session, id: &str) -> Option<String> {
    s.course
        .slide("slide1")
        .unwrap()
        .object(id)
        .unwrap()
        .0
        .group
        .clone()
}

fn add_test_image(s: &mut Session) -> String {
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "pic",
            "kind": { "type": "image", "src": "media/pic.png", "alt": "A picture" },
            "frame": { "x": 10, "y": 20, "w": 200, "h": 100 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn crop_of(s: &Session, id: &str) -> Option<coursecraft_course::CropRect> {
    match &s.course.slide("slide1").unwrap().object(id).unwrap().0.kind {
        coursecraft_course::ObjectKind::Image { crop, .. } => *crop,
        _ => None,
    }
}

#[test]
fn object_set_crop_sets_and_undo_restores_the_course() {
    let mut s = Session::blank("Crop");
    let id = add_test_image(&mut s);
    let before = s.course.clone();
    let crop = json!({ "x": 0.1, "y": 0.2, "w": 0.5, "h": 0.6 });
    ok(
        &mut s,
        "object.set_crop",
        json!({ "slide": "slide1", "object": id, "crop": crop }),
    );
    assert_eq!(
        crop_of(&s, &id),
        Some(coursecraft_course::CropRect {
            x: 0.1,
            y: 0.2,
            w: 0.5,
            h: 0.6
        })
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn object_set_crop_clears_and_can_change_the_frame_in_the_same_step() {
    let mut s = Session::blank("Crop");
    let id = add_test_image(&mut s);
    let set = json!({ "x": 0.1, "y": 0.2, "w": 0.5, "h": 0.6 });
    ok(
        &mut s,
        "object.set_crop",
        json!({ "slide": "slide1", "object": id, "crop": set }),
    );
    let before = s.course.clone();
    ok(
        &mut s,
        "object.set_crop",
        json!({
            "slide": "slide1",
            "object": id,
            "crop": null,
            "frame": { "x": 5, "y": 6, "w": 300, "h": 150 }
        }),
    );
    assert_eq!(crop_of(&s, &id), None);
    assert_eq!(
        frame_of(&s, &id),
        coursecraft_course::Rect {
            x: 5.0,
            y: 6.0,
            w: 300.0,
            h: 150.0
        }
    );
    // One undo step restores both the crop and the frame.
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn object_set_crop_refuses_other_kinds_and_bad_windows() {
    let mut s = Session::blank("Crop");
    add_frame_button(&mut s, "btn", 10.0, 10.0, 50.0, 20.0);
    assert!(
        s.execute(
            "object.set_crop",
            &json!({
                "slide": "slide1",
                "object": "btn",
                "crop": { "x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5 }
            })
        )
        .is_err()
    );

    let id = add_test_image(&mut s);
    let before = s.course.clone();
    for crop in [
        json!({ "x": -0.1, "y": 0.0, "w": 0.5, "h": 0.5 }),
        json!({ "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.5 }),
        json!({ "x": 0.0, "y": 0.0, "w": 0.5, "h": 0.0 }),
        json!({ "x": 0.8, "y": 0.0, "w": 0.5, "h": 0.5 }),
        json!({ "x": 0.0, "y": 0.8, "w": 0.5, "h": 0.5 }),
        json!({ "x": 0.0, "y": 0.0, "w": 1.5, "h": 0.5 }),
        json!({ "x": 1e308, "y": 0.0, "w": 0.5, "h": 0.5 }),
    ] {
        assert!(
            s.execute(
                "object.set_crop",
                &json!({ "slide": "slide1", "object": id, "crop": crop })
            )
            .is_err(),
            "hostile crop accepted: {crop}"
        );
    }
    assert_eq!(s.course, before);
}

#[test]
fn object_set_crop_stores_a_full_window_as_no_crop() {
    let mut s = Session::blank("Crop");
    let id = add_test_image(&mut s);
    ok(
        &mut s,
        "object.set_crop",
        json!({
            "slide": "slide1",
            "object": id,
            "crop": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }
        }),
    );
    assert_eq!(crop_of(&s, &id), None);
}

#[test]
fn object_duplicate_carries_the_crop() {
    let mut s = Session::blank("Crop");
    let id = add_test_image(&mut s);
    ok(
        &mut s,
        "object.set_crop",
        json!({
            "slide": "slide1",
            "object": id,
            "crop": { "x": 0.25, "y": 0.1, "w": 0.5, "h": 0.8 }
        }),
    );
    let copy = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": id }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(crop_of(&s, &copy), crop_of(&s, &id));
    assert!(crop_of(&s, &copy).is_some());
}

#[test]
fn align_each_edge_to_the_selection() {
    for (edge, ax, bx, ay, by) in [
        ("left", 10.0, 10.0, 10.0, 100.0),
        ("center", 60.0, 55.0, 10.0, 100.0),
        ("right", 110.0, 100.0, 10.0, 100.0),
        ("top", 10.0, 100.0, 10.0, 10.0),
        ("middle", 10.0, 100.0, 60.0, 55.0),
        ("bottom", 10.0, 100.0, 110.0, 100.0),
    ] {
        let mut s = Session::blank("Title");
        add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
        add_frame_button(&mut s, "b", 100.0, 100.0, 60.0, 30.0);
        ok(
            &mut s,
            "object.align",
            json!({ "slide": "slide1", "objects": ["a", "b"], "edge": edge }),
        );
        let a = frame_of(&s, "a");
        let b = frame_of(&s, "b");
        assert_eq!((a.x, b.x, a.y, b.y), (ax, bx, ay, by), "{edge}");
    }
}

#[test]
fn align_to_slide_uses_the_slide_bounds() {
    for (edge, ax, bx, ay, by) in [
        ("left", 0.0, 0.0, 10.0, 100.0),
        ("center", 615.0, 610.0, 10.0, 100.0),
        ("right", 1230.0, 1220.0, 10.0, 100.0),
        ("top", 10.0, 100.0, 0.0, 0.0),
        ("middle", 10.0, 100.0, 350.0, 345.0),
        ("bottom", 10.0, 100.0, 700.0, 690.0),
    ] {
        let mut s = Session::blank("Title");
        add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
        add_frame_button(&mut s, "b", 100.0, 100.0, 60.0, 30.0);
        ok(
            &mut s,
            "object.align",
            json!({ "slide": "slide1", "objects": ["a", "b"], "edge": edge, "to": "slide" }),
        );
        let a = frame_of(&s, "a");
        let b = frame_of(&s, "b");
        assert_eq!((a.x, b.x, a.y, b.y), (ax, bx, ay, by), "{edge}");
    }
}

#[test]
fn align_with_one_object_defaults_to_the_slide() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
    ok(
        &mut s,
        "object.align",
        json!({ "slide": "slide1", "objects": ["a"], "edge": "right" }),
    );
    assert_eq!(frame_of(&s, "a").x, 1230.0);
}

#[test]
fn align_is_one_undo_step() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "b", 100.0, 100.0, 60.0, 30.0);
    let before = s.course.clone();
    ok(
        &mut s,
        "object.align",
        json!({ "slide": "slide1", "objects": ["a", "b"], "edge": "left" }),
    );
    assert_ne!(s.course, before);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn distribute_equalises_gaps_with_unequal_widths() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "d1", 0.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "d2", 300.0, 10.0, 100.0, 20.0);
    add_frame_button(&mut s, "d3", 900.0, 10.0, 20.0, 20.0);
    ok(
        &mut s,
        "object.distribute",
        json!({ "slide": "slide1", "objects": ["d1", "d2", "d3"], "axis": "horizontal" }),
    );
    assert_eq!(frame_of(&s, "d1").x, 0.0);
    assert_eq!(frame_of(&s, "d2").x, 425.0);
    assert_eq!(frame_of(&s, "d3").x, 900.0);

    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "d1", 10.0, 0.0, 20.0, 20.0);
    add_frame_button(&mut s, "d2", 10.0, 300.0, 20.0, 100.0);
    add_frame_button(&mut s, "d3", 10.0, 900.0, 20.0, 20.0);
    ok(
        &mut s,
        "object.distribute",
        json!({ "slide": "slide1", "objects": ["d1", "d2", "d3"], "axis": "vertical" }),
    );
    assert_eq!(frame_of(&s, "d1").y, 0.0);
    assert_eq!(frame_of(&s, "d2").y, 410.0);
    assert_eq!(frame_of(&s, "d3").y, 900.0);
}

#[test]
fn distribute_needs_three_objects_and_undoes_in_one_step() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "d1", 0.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "d2", 300.0, 10.0, 100.0, 20.0);
    assert!(
        s.execute(
            "object.distribute",
            &json!({ "slide": "slide1", "objects": ["d1", "d2"], "axis": "horizontal" })
        )
        .is_err()
    );
    add_frame_button(&mut s, "d3", 900.0, 10.0, 20.0, 20.0);
    let before = s.course.clone();
    ok(
        &mut s,
        "object.distribute",
        json!({ "slide": "slide1", "objects": ["d1", "d2", "d3"], "axis": "horizontal" }),
    );
    assert_ne!(s.course, before);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn match_size_takes_the_primary_size() {
    for (dimension, w, h) in [
        ("width", 50.0, 40.0),
        ("height", 100.0, 20.0),
        ("both", 50.0, 20.0),
    ] {
        let mut s = Session::blank("Title");
        add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
        add_frame_button(&mut s, "b", 100.0, 100.0, 100.0, 40.0);
        let before = s.course.clone();
        ok(
            &mut s,
            "object.match_size",
            json!({ "slide": "slide1", "objects": ["a", "b"], "dimension": dimension }),
        );
        let b = frame_of(&s, "b");
        assert_eq!((b.w, b.h), (w, h), "{dimension}");
        ok(&mut s, "edit.undo", json!({}));
        assert_eq!(s.course, before);
    }
}

#[test]
fn group_makes_members_contiguous_at_the_topmost_position() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "b", 20.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "c", 40.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "d", 60.0, 0.0, 10.0, 10.0);
    let before = s.course.clone();
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "c"] }),
    );
    let order: Vec<String> = s
        .course
        .slide("slide1")
        .unwrap()
        .objects
        .iter()
        .map(|object| object.id.clone())
        .collect();
    assert_eq!(order, vec!["b", "a", "c", "d"]);
    let group = group_of(&s, "a").expect("group id");
    assert_eq!(group_of(&s, "c").as_deref(), Some(group.as_str()));
    assert_eq!(group_of(&s, "b"), None);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn grouping_an_existing_group_flattens_it() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "b", 20.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "c", 40.0, 0.0, 10.0, 10.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let first = group_of(&s, "a").expect("first group");
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["b", "c"] }),
    );
    let second = group_of(&s, "a").expect("second group");
    assert_ne!(first, second);
    assert_eq!(group_of(&s, "b").as_deref(), Some(second.as_str()));
    assert_eq!(group_of(&s, "c").as_deref(), Some(second.as_str()));
}

#[test]
fn ungroup_clears_the_whole_group_and_undoes() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "b", 20.0, 0.0, 10.0, 10.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let grouped = s.course.clone();
    ok(
        &mut s,
        "object.ungroup",
        json!({ "slide": "slide1", "objects": ["a"] }),
    );
    assert_eq!(group_of(&s, "a"), None);
    assert_eq!(group_of(&s, "b"), None);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, grouped);
}

#[test]
fn a_single_member_group_is_a_validation_warning() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    s.course
        .slide_mut("slide1")
        .unwrap()
        .objects
        .first_mut()
        .unwrap()
        .group = Some("g1".into());
    let issues = s.course.validate();
    assert!(
        issues.iter().any(
            |issue| issue.severity == coursecraft_course::Severity::Warning
                && issue.message.contains("only one member")
        ),
        "{issues:#?}"
    );
}

#[test]
fn duplicate_and_paste_give_grouped_copies_a_fresh_shared_group() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "b", 20.0, 0.0, 10.0, 10.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let original_group = group_of(&s, "a").expect("group");

    let duplicated = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let ids: Vec<String> = duplicated["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    let first = group_of(&s, &ids[0]).expect("copy group");
    assert_eq!(group_of(&s, &ids[1]).as_deref(), Some(first.as_str()));
    assert_ne!(first, original_group);

    let objects: Vec<coursecraft_course::Object> = ["a", "b"]
        .iter()
        .map(|id| {
            s.course
                .slide("slide1")
                .unwrap()
                .object(id)
                .unwrap()
                .0
                .clone()
        })
        .collect();
    let pasted = ok(
        &mut s,
        "object.paste",
        json!({ "slide": "slide1", "objects": objects, "source_slide": "slide1" }),
    );
    let pasted_ids: Vec<String> = pasted["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    let pasted_group = group_of(&s, &pasted_ids[0]).expect("pasted group");
    assert_eq!(
        group_of(&s, &pasted_ids[1]).as_deref(),
        Some(pasted_group.as_str())
    );
    assert_ne!(pasted_group, original_group);
}

#[test]
fn duplicating_a_slide_keeps_its_group_ids() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 0.0, 0.0, 10.0, 10.0);
    add_frame_button(&mut s, "b", 20.0, 0.0, 10.0, 10.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let original_group = group_of(&s, "a").expect("group");
    let copy = ok(&mut s, "slide.duplicate", json!({ "slide": "slide1" }));
    let copy_id = copy["id"].as_str().unwrap();
    let copy_slide = s.course.slide(copy_id).unwrap();
    let groups: Vec<Option<String>> = copy_slide
        .objects
        .iter()
        .map(|object| object.group.clone())
        .collect();
    assert_eq!(
        groups,
        vec![Some(original_group.clone()), Some(original_group.clone())]
    );

    let source = s.course.slide("slide1").unwrap().clone();
    let pasted = ok(
        &mut s,
        "slide.paste",
        json!({ "slide": source, "after": "slide1" }),
    );
    let pasted_id = pasted["id"].as_str().unwrap();
    let pasted_slide = s.course.slide(pasted_id).unwrap();
    let groups: Vec<Option<String>> = pasted_slide
        .objects
        .iter()
        .map(|object| object.group.clone())
        .collect();
    assert_eq!(
        groups,
        vec![Some(original_group.clone()), Some(original_group)]
    );
}

// ---------------------------------------------------------------- masters

#[test]
fn master_add_defaults_to_master_n_and_undoes_in_one_step() {
    let mut s = Session::blank("Title");
    let before = s.course.clone();
    let id = ok(&mut s, "master.add", json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(id, "master1");
    assert_eq!(s.course.master(&id).unwrap().title, "Master 1");
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn master_delete_clears_every_slide_master_in_one_undo_step() {
    let mut s = Session::blank("Title");
    let master = ok(&mut s, "master.add", json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(&mut s, "slide.add", json!({ "id": "slide2" }));
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["slide1", "slide2"], "master": master }),
    );
    let before = s.course.clone();
    ok(&mut s, "master.delete", json!({ "master": master }));
    assert!(s.course.master(&master).is_none());
    assert!(s.course.slide("slide1").unwrap().master.is_none());
    assert!(s.course.slide("slide2").unwrap().master.is_none());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn slide_set_master_applies_and_rejects_an_unknown_master() {
    let mut s = Session::blank("Title");
    let master = ok(&mut s, "master.add", json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["slide1"], "master": master }),
    );
    assert_eq!(
        s.course.slide("slide1").unwrap().master.as_deref(),
        Some(master.as_str())
    );
    assert!(
        s.execute(
            "slide.set_master",
            &json!({ "slides": ["slide1"], "master": "ghost" })
        )
        .is_err()
    );
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["slide1"], "master": Json::Null }),
    );
    assert!(s.course.slide("slide1").unwrap().master.is_none());
}

fn frames_inside(s: &Session, slide_id: &str) -> bool {
    let Some(slide) = s.course.slide(slide_id) else {
        return false;
    };
    let w = s.course.slide_size.w as f32;
    let h = s.course.slide_size.h as f32;
    let inside = |frame: coursecraft_course::Rect| {
        frame.x >= 0.0
            && frame.y >= 0.0
            && frame.x + frame.w <= w
            && frame.y + frame.h <= h
            && frame.w > 0.0
            && frame.h > 0.0
    };
    slide.objects.iter().all(|object| inside(object.frame))
        && slide
            .layers
            .iter()
            .flat_map(|layer| layer.objects.iter())
            .all(|object| inside(object.frame))
}

#[test]
fn every_layout_fits_three_slide_sizes_and_validates() {
    for (w, h) in [(800u32, 600u32), (1280, 720), (320, 180)] {
        for layout in [
            "blank",
            "title",
            "title_content",
            "two_columns",
            "picture_text",
            "question",
        ] {
            let mut s = Session::blank("Title");
            s.course.slide_size = coursecraft_course::Size { w, h };
            let id = ok(
                &mut s,
                "slide.add_from_layout",
                json!({ "layout": layout, "after": "slide1" }),
            )["id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(frames_inside(&s, &id), "{layout} on {w}x{h}");
            let errors: Vec<_> = s
                .course
                .validate()
                .into_iter()
                .filter(|i| i.severity == coursecraft_course::Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{layout} on {w}x{h}: {errors:#?}");
        }
    }
}

#[test]
fn layout_inherits_the_master_of_the_slide_it_follows() {
    let mut s = Session::blank("Title");
    let master = ok(&mut s, "master.add", json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["slide1"], "master": master }),
    );
    let id = ok(
        &mut s,
        "slide.add_from_layout",
        json!({ "layout": "blank", "after": "slide1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.course.slide(&id).unwrap().master.as_deref(),
        Some(master.as_str())
    );
}

#[test]
fn template_save_and_add_give_fresh_ids_retarget_own_triggers_and_undo_once() {
    let mut s = Session::blank("Title");
    let button = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Go" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "action": { "action": "jump_to_slide", "target": { "id": "slide1" } },
                "when": { "event": "click", "object": button }
            }
        }),
    );
    let template = ok(&mut s, "template.save", json!({ "slide": "slide1" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = s.course.clone();
    let new_id = ok(
        &mut s,
        "template.add_slide",
        json!({ "template": template, "after": "slide1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(new_id, template);
    let slide = s.course.slide(&new_id).unwrap();
    let new_button = slide
        .objects
        .iter()
        .find(|object| object.id != button)
        .map(|object| object.id.clone())
        .expect("copy has a fresh object id");
    assert!(slide.objects.iter().all(|object| object.id != button));
    let trigger = &slide.triggers[0];
    assert_eq!(
        trigger.when,
        coursecraft_course::Event::Click {
            object: new_button.clone()
        }
    );
    assert_eq!(
        trigger.action,
        coursecraft_course::Action::JumpToSlide {
            target: coursecraft_course::SlideTarget::Id("slide1".into())
        }
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn object_paste_onto_a_master_drops_triggers_and_refuses_media() {
    let mut s = Session::blank("Title");
    let master = ok(&mut s, "master.add", json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let object = json!({
        "id": "old",
        "kind": { "type": "text", "text": "Hello" },
        "frame": { "x": 0, "y": 0, "w": 100, "h": 40 }
    });
    let trigger = json!({
        "action": { "action": "pause_timeline" },
        "when": { "event": "timeline_starts" }
    });
    let result = ok(
        &mut s,
        "object.paste",
        json!({ "slide": master, "objects": [object], "triggers": [trigger] }),
    );
    assert_eq!(result["skipped"], 1);
    assert!(s.course.master(&master).unwrap().triggers.is_empty());

    let audio = json!({
        "id": "old",
        "kind": { "type": "audio", "src": "a.mp3" },
        "frame": { "x": 0, "y": 0, "w": 10, "h": 10 }
    });
    assert!(
        s.execute(
            "object.paste",
            &json!({ "slide": master, "objects": [audio] })
        )
        .is_err()
    );
}

#[test]
fn align_moves_a_group_as_one_rigid_unit() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 10.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "b", 100.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "c", 300.0, 100.0, 50.0, 20.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    ok(
        &mut s,
        "object.align",
        json!({ "slide": "slide1", "objects": ["a", "b", "c"], "edge": "right" }),
    );
    let a = frame_of(&s, "a");
    let b = frame_of(&s, "b");
    let c = frame_of(&s, "c");
    assert_eq!(a.x, 210.0);
    assert_eq!(b.x, 300.0);
    assert_eq!(c.x, 300.0);
}

#[test]
fn distribute_spaces_units_so_group_members_keep_their_offsets() {
    let mut s = Session::blank("Title");
    add_frame_button(&mut s, "a", 300.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "b", 400.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "d1", 0.0, 10.0, 50.0, 20.0);
    add_frame_button(&mut s, "d3", 900.0, 10.0, 50.0, 20.0);
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    ok(
        &mut s,
        "object.distribute",
        json!({ "slide": "slide1", "objects": ["d1", "a", "b", "d3"], "axis": "horizontal" }),
    );
    let a = frame_of(&s, "a");
    let b = frame_of(&s, "b");
    assert_eq!(a.x, 400.0);
    assert_eq!(b.x, 500.0);
    assert_eq!(frame_of(&s, "d1").x, 0.0);
    assert_eq!(frame_of(&s, "d3").x, 900.0);
}

#[test]
fn a_template_with_a_dangling_jump_target_is_reported_not_silently_dropped() {
    let mut s = Session::blank("Title");
    let template = ok(&mut s, "template.save", json!({ "slide": "slide1" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let template_slide = s
        .course
        .templates
        .iter_mut()
        .find(|slide| slide.id == template)
        .expect("template");
    template_slide.triggers.push(coursecraft_course::Trigger {
        action: coursecraft_course::Action::JumpToSlide {
            target: coursecraft_course::SlideTarget::Id("ghost".into()),
        },
        when: coursecraft_course::Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    let err = s
        .execute(
            "template.add_slide",
            &json!({ "template": template, "after": "slide1" }),
        )
        .expect_err("the dangling target is refused by validation");
    assert!(err.to_string().contains("ghost"), "{err}");
}

#[test]
fn template_slide_takes_the_next_slide_number_as_its_title() {
    let mut s = Session::blank("Title");
    let slide = ok(&mut s, "slide.add", json!({ "title": "Welcome" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let template = ok(&mut s, "template.save", json!({ "slide": slide }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let added = ok(
        &mut s,
        "template.add_slide",
        json!({ "template": template }),
    );
    let title = &s.course.slide(added["id"].as_str().unwrap()).unwrap().title;
    let count = s.course.slides().count();
    assert_eq!(title, &format!("Slide {count}"));
}

#[test]
fn question_delete_keeps_a_feedback_layer_the_author_changed() {
    let mut s = Session::blank("Keep edited feedback");
    ok(&mut s, "question.insert", json!({ "slide": "slide1" }));
    ok(
        &mut s,
        "question.add_feedback_layers",
        json!({ "slide": "slide1" }),
    );
    let (layer, object) = {
        let slide = s.course.slide("slide1").expect("slide");
        let layer = slide.layers.first().expect("layer");
        (layer.id.clone(), layer.objects[0].id.clone())
    };
    ok(
        &mut s,
        "text.set",
        json!({ "slide": "slide1", "object": object, "text": "Well done!" }),
    );
    ok(&mut s, "question.delete", json!({ "slide": "slide1" }));
    let slide = s.course.slide("slide1").expect("slide");
    assert!(slide.layers.iter().any(|l| l.id == layer));
}

#[test]
fn object_add_and_update_clamp_shape_adjustments() {
    use coursecraft_course::ObjectKind;

    let mut s = Session::blank("Title");
    let added = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": {
                "type": "shape",
                "shape": "rounded_rectangle",
                "fill": "#D0D7DE",
                "adjust": [9.0, 0.3]
            },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 80 }
        }),
    );
    let id = added["id"].as_str().expect("id").to_owned();
    let slide = s.course.slide("slide1").expect("slide");
    let (object, _) = slide.object(&id).expect("object");
    let ObjectKind::Shape { adjust, .. } = &object.kind else {
        panic!("shape");
    };
    assert_eq!(adjust, &vec![0.5]);

    ok(
        &mut s,
        "object.update",
        json!({
            "slide": "slide1",
            "object": id,
            "kind": {
                "type": "shape",
                "shape": "rounded_rectangle",
                "fill": "#D0D7DE",
                "adjust": [0.9, 0.2]
            }
        }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let (object, _) = slide.object(&id).expect("object");
    let ObjectKind::Shape { adjust, .. } = &object.kind else {
        panic!("shape");
    };
    assert_eq!(adjust, &vec![0.5]);
}

#[test]
fn paths_use_set_path_and_move_group_duplicate_and_paste_by_frame() {
    let mut s = Session::blank("Paths");
    let make = |s: &mut Session, x: f32| -> String {
        ok(
            s,
            "object.add",
            json!({
                "slide": "slide1",
                "kind": {
                    "type": "path",
                    "points": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                    "outline": {"color": "#1F1F1F", "width": 2}
                },
                "frame": { "x": x, "y": 0, "w": 100, "h": 100 },
                "name": "Line"
            }),
        )["id"]
            .as_str()
            .expect("id")
            .to_owned()
    };
    let first = make(&mut s, 0.0);
    let second = make(&mut s, 300.0);

    ok(
        &mut s,
        "object.set_path",
        json!({
            "slide": "slide1",
            "object": first,
            "points": [{"x": 200, "y": 100}, {"x": 500, "y": 400}]
        }),
    );
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&first))
        .map(|(object, _)| object.clone())
        .expect("path");
    assert_eq!(
        object.frame,
        coursecraft_course::Rect {
            x: 200.0,
            y: 100.0,
            w: 300.0,
            h: 300.0
        }
    );
    let coursecraft_course::ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.first().map(|p| (p.x, p.y)), Some((0.0, 0.0)));
    assert_eq!(path.points.last().map(|p| (p.x, p.y)), Some((1.0, 1.0)));

    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": first,
            "frame": { "x": 250, "y": 150, "w": 300, "h": 300 }
        }),
    );
    ok(
        &mut s,
        "object.align",
        json!({
            "slide": "slide1",
            "objects": [first, second],
            "edge": "top",
            "to": "selection"
        }),
    );
    let tops: Vec<f32> = [&first, &second]
        .iter()
        .filter_map(|id| {
            s.course
                .slide("slide1")
                .and_then(|slide| slide.object(id))
                .map(|(object, _)| object.frame.y)
        })
        .collect();
    assert_eq!(tops.len(), 2);
    assert!((tops[0] - tops[1]).abs() < 0.001);

    let group = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": [first, second] }),
    );
    assert!(group["objects"].as_u64().unwrap_or(0) >= 2);

    let duplicate = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": first }),
    );
    let duplicate_id = duplicate["id"].as_str().expect("duplicate id");
    let copy = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(duplicate_id))
        .map(|(object, _)| object.clone())
        .expect("duplicate");
    assert!(matches!(
        copy.kind,
        coursecraft_course::ObjectKind::Path { .. }
    ));

    let payload = serde_json::to_value([copy]).expect("serialise");
    let pasted = ok(
        &mut s,
        "object.paste",
        json!({ "slide": "slide1", "objects": payload, "offset": 0.0 }),
    );
    let pasted_id = pasted["id"].as_str().expect("pasted id");
    let pasted = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(pasted_id))
        .map(|(object, _)| object.clone())
        .expect("pasted");
    assert!(matches!(
        pasted.kind,
        coursecraft_course::ObjectKind::Path { .. }
    ));
    assert!(s.course.is_playable(), "{:?}", s.course.validate());
}

#[test]
fn set_lottie_changes_loop_autoplay_and_speed_in_one_undo_step() {
    let mut s = Session::blank("Lottie");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "lottie", "src": "anim.json", "loop": false, "autoplay": false },
            "frame": { "x": 10, "y": 10, "w": 200, "h": 200 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let before = s.undo_len();
    ok(
        &mut s,
        "object.set_lottie",
        json!({
            "slide": "slide1",
            "object": id.clone(),
            "loop": true,
            "autoplay": true,
            "speed": 10.0
        }),
    );
    let object = s.course.slide("slide1").unwrap().object(&id).unwrap().0;
    let coursecraft_course::ObjectKind::Lottie {
        loop_playback,
        autoplay,
        speed,
        ..
    } = object.kind
    else {
        panic!("expected a Lottie object");
    };
    assert!(loop_playback);
    assert!(autoplay);
    assert!((speed - 4.0).abs() < f32::EPSILON, "speed clamps to 4");
    assert_eq!(s.undo_len(), before + 1);

    ok(&mut s, "edit.undo", json!({}));
    let object = s.course.slide("slide1").unwrap().object(&id).unwrap().0;
    let coursecraft_course::ObjectKind::Lottie {
        loop_playback,
        autoplay,
        speed,
        ..
    } = object.kind
    else {
        panic!("expected a Lottie object");
    };
    assert!(!loop_playback);
    assert!(!autoplay);
    assert!((speed - 1.0).abs() < f32::EPSILON);
}

#[test]
fn set_web_source_changes_kind_and_undoes_in_one_step() {
    let mut s = Session::blank("Web");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "web", "source": { "type": "url", "url": "https://example.com" } },
            "frame": { "x": 10, "y": 10, "w": 320, "h": 180 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .kind,
        coursecraft_course::ObjectKind::Web {
            source: coursecraft_course::WebSource::Url {
                url: "https://example.com".into()
            }
        }
    );

    let before = s.undo_len();
    ok(
        &mut s,
        "object.set_web_source",
        json!({
            "slide": "slide1",
            "object": id.clone(),
            "source": { "type": "folder", "path": "web/site", "entry": "index.html" }
        }),
    );
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .kind,
        coursecraft_course::ObjectKind::Web {
            source: coursecraft_course::WebSource::Folder {
                path: "web/site".into(),
                entry: "index.html".into()
            }
        }
    );
    assert_eq!(s.undo_len(), before + 1);

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .kind,
        coursecraft_course::ObjectKind::Web {
            source: coursecraft_course::WebSource::Url {
                url: "https://example.com".into()
            }
        }
    );

    assert!(
        s.execute(
            "object.set_web_source",
            &json!({
                "slide": "slide1",
                "object": id,
                "source": { "type": "url", "url": "http://example.com" }
            }),
        )
        .is_err(),
        "an invalid source must be refused and leave the course unchanged"
    );
}

#[test]
fn opacity_fill_shadow_and_glow_commands_are_multi_select_and_undoable() {
    let mut s = Session::blank("Effects");
    let mut ids = Vec::new();
    for x in [10, 200] {
        let id = ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "ellipse", "fill": "#FF0000" },
                "frame": { "x": x, "y": 10, "w": 100, "h": 80 }
            }),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ids.push(id);
    }

    let before = s.undo_len();
    ok(
        &mut s,
        "object.set_opacity",
        json!({ "slide": "slide1", "objects": ids.clone(), "opacity": 0.4 }),
    );
    for id in &ids {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!((object.opacity - 0.4).abs() < 0.001);
    }
    assert_eq!(s.undo_len(), before + 1);

    ok(
        &mut s,
        "object.set_fill_style",
        json!({
            "slide": "slide1",
            "objects": ids.clone(),
            "fill_style": {
                "type": "linear",
                "angle": 45,
                "stops": [
                    { "color": "#FF0000", "position": 0 },
                    { "color": "#0000FF", "position": 1 }
                ]
            }
        }),
    );
    for id in &ids {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!(matches!(
            object.kind,
            coursecraft_course::ObjectKind::Shape {
                fill_style: Some(coursecraft_course::FillStyle::Linear { .. }),
                ..
            }
        ));
    }

    ok(
        &mut s,
        "object.set_shadow",
        json!({
            "slide": "slide1",
            "objects": ids.clone(),
            "shadow": { "color": "#00000080", "offset_x": 6, "offset_y": 4, "blur": 9 }
        }),
    );
    ok(
        &mut s,
        "object.set_glow",
        json!({
            "slide": "slide1",
            "object": ids.first().cloned().unwrap(),
            "glow": { "color": "#00FF0080", "size": 12 }
        }),
    );
    for id in &ids {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!(object.shadow.is_some());
    }
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(ids.first().unwrap())
            .unwrap()
            .0
            .glow
            .is_some()
    );

    ok(&mut s, "edit.undo", json!({}));
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(ids.first().unwrap())
            .unwrap()
            .0
            .glow
            .is_none()
    );
    ok(
        &mut s,
        "object.set_fill_style",
        json!({ "slide": "slide1", "objects": ids.clone(), "fill_style": null }),
    );
    for id in &ids {
        let object = s.course.slide("slide1").unwrap().object(id).unwrap().0;
        assert!(matches!(
            object.kind,
            coursecraft_course::ObjectKind::Shape {
                fill_style: None,
                ..
            }
        ));
    }
}

fn table_session() -> (Session, String) {
    let mut s = Session::blank("Table");
    let cells = |prefix: &str| {
        json!([
            { "text": format!("{prefix}1") },
            { "text": format!("{prefix}2") },
            { "text": format!("{prefix}3") }
        ])
    };
    let result = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": {
                "type": "table",
                "columns": [0.2, 0.3, 0.5],
                "rows": [
                    { "height": 0.2, "cells": cells("a") },
                    { "height": 0.3, "cells": cells("b") },
                    { "height": 0.5, "cells": cells("c") }
                ],
                "borders": {
                    "outer": { "color": "#1F1F1F", "width": 2 },
                    "inner": { "color": "#808080", "width": 1 }
                },
                "header_row": true
            },
            "frame": { "x": 100, "y": 100, "w": 600, "h": 300 }
        }),
    );
    let id = result["id"].as_str().unwrap_or_default().to_owned();
    (s, id)
}

fn table_object<'a>(s: &'a Session, id: &str) -> &'a coursecraft_course::Object {
    s.course
        .slide("slide1")
        .and_then(|slide| slide.object(id))
        .map(|(object, _)| object)
        .expect("table object")
}

#[test]
fn table_commands_set_cells_and_undo() {
    let (mut s, id) = table_session();
    let before = s.course.clone();
    ok(
        &mut s,
        "table.set_cell",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 1,
            "column": 2,
            "text": "Changed",
            "fill": "#FF0000",
        }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows[1].cells[2].text, "Changed");
    assert_eq!(
        rows[1].cells[2].fill.as_ref().map(|fill| fill.0.as_str()),
        Some("#FF0000")
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
    ok(&mut s, "edit.redo", json!({}));
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows[1].cells[2].text, "Changed");
}

#[test]
fn table_insert_and_delete_rows_keep_spans_and_undo() {
    let (mut s, id) = table_session();
    ok(
        &mut s,
        "table.merge",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 0,
            "column": 0,
            "rows": 2,
            "columns": 2,
        }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows[0].cells[0].row_span, 2);
    assert_eq!(rows[0].cells[0].col_span, 2);
    assert!(
        rows[1].cells[1].text.is_empty(),
        "covered cells are emptied"
    );

    ok(
        &mut s,
        "table.insert_row",
        json!({ "slide": "slide1", "object": id, "index": 1 }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[0].cells[0].row_span, 3,
        "a span crossing the insert grows"
    );
    assert!((rows.iter().map(|row| row.height).sum::<f32>() - 1.0).abs() < 1e-5);

    ok(&mut s, "edit.undo", json!({}));
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].cells[0].row_span, 2);

    ok(
        &mut s,
        "table.delete_row",
        json!({ "slide": "slide1", "object": id, "index": 1 }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].cells[0].row_span, 1,
        "the span shrinks with the deleted row"
    );
}

#[test]
fn table_insert_and_delete_columns_keep_spans_and_undo() {
    let (mut s, id) = table_session();
    ok(
        &mut s,
        "table.merge",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 0,
            "column": 0,
            "rows": 2,
            "columns": 2,
        }),
    );
    ok(
        &mut s,
        "table.insert_column",
        json!({ "slide": "slide1", "object": id, "index": 1 }),
    );
    let coursecraft_course::ObjectKind::Table { columns, rows, .. } = &table_object(&s, &id).kind
    else {
        panic!("table");
    };
    assert_eq!(columns.len(), 4);
    assert_eq!(rows[0].cells[0].col_span, 3);
    assert!((columns.iter().sum::<f32>() - 1.0).abs() < 1e-5);

    ok(&mut s, "edit.undo", json!({}));
    let coursecraft_course::ObjectKind::Table { columns, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(columns.len(), 3);

    ok(
        &mut s,
        "table.delete_column",
        json!({ "slide": "slide1", "object": id, "index": 1 }),
    );
    let coursecraft_course::ObjectKind::Table { columns, rows, .. } = &table_object(&s, &id).kind
    else {
        panic!("table");
    };
    assert_eq!(columns.len(), 2);
    assert_eq!(rows[0].cells[0].col_span, 1);
}

#[test]
fn table_split_resize_borders_and_header_row_undo() {
    let (mut s, id) = table_session();
    ok(
        &mut s,
        "table.merge",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 0,
            "column": 0,
            "rows": 2,
            "columns": 2,
        }),
    );
    ok(
        &mut s,
        "table.split",
        json!({ "slide": "slide1", "object": id, "row": 1, "column": 1 }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert_eq!(rows[0].cells[0].row_span, 1);
    assert_eq!(rows[0].cells[0].col_span, 1);

    ok(
        &mut s,
        "table.resize",
        json!({
            "slide": "slide1",
            "object": id,
            "columns": [1, 1, 2],
            "rows": [2, 2, 1],
        }),
    );
    let coursecraft_course::ObjectKind::Table { columns, rows, .. } = &table_object(&s, &id).kind
    else {
        panic!("table");
    };
    assert!((columns[0] - 0.25).abs() < 1e-5);
    assert!((rows[0].height - 0.4).abs() < 1e-5);

    ok(
        &mut s,
        "table.set_borders",
        json!({
            "slide": "slide1",
            "object": id,
            "outer": null,
            "inner": { "color": "#00FF00", "width": 3 },
        }),
    );
    let coursecraft_course::ObjectKind::Table { borders, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    assert!(borders.outer.is_none());
    let inner = borders.inner.as_ref().expect("inner border");
    assert_eq!(inner.color.0, "#00FF00");
    assert!((inner.width - 3.0).abs() < 1e-5);

    ok(
        &mut s,
        "table.set_header_row",
        json!({ "slide": "slide1", "object": id, "header_row": false }),
    );
    let coursecraft_course::ObjectKind::Table { header_row, .. } = &table_object(&s, &id).kind
    else {
        panic!("table");
    };
    assert!(!header_row);
    ok(&mut s, "edit.undo", json!({}));
    let coursecraft_course::ObjectKind::Table { header_row, .. } = &table_object(&s, &id).kind
    else {
        panic!("table");
    };
    assert!(*header_row);
}

#[test]
fn text_commands_format_a_table_cell() {
    let (mut s, id) = table_session();
    ok(
        &mut s,
        "text.style_range",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 0,
            "column": 0,
            "start": 0,
            "end": 0,
            "bold": true,
            "color": "#112233",
        }),
    );
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({
            "slide": "slide1",
            "object": id,
            "row": 0,
            "column": 0,
            "list": "bullet",
        }),
    );
    let coursecraft_course::ObjectKind::Table { rows, .. } = &table_object(&s, &id).kind else {
        panic!("table");
    };
    let cell = &rows[0].cells[0];
    assert!(cell.runs.iter().any(|run| run.bold == Some(true)));
    assert_eq!(
        cell.paragraphs[0].list,
        coursecraft_course::ListKind::Bullet
    );
}

#[test]
fn object_lock_hide_and_view_settings_survive_round_trip_and_undo() {
    let mut s = Session::blank("LockHide");
    let obj = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
            "frame": { "x": 100, "y": 100, "w": 200, "h": 120 },
        }),
    )["id"]
        .as_str()
        .expect("object id")
        .to_owned();

    // Default values are omitted from JSON.
    let initial_json = s.course.to_json().expect("json");
    assert!(!initial_json.contains("\"locked\""));
    assert!(!initial_json.contains("\"editor_hidden\""));
    assert!(!initial_json.contains("\"editor\""));

    ok(
        &mut s,
        "object.lock",
        json!({ "slide": "slide1", "object": obj, "locked": true }),
    );
    ok(
        &mut s,
        "object.hide_in_editor",
        json!({ "slide": "slide1", "object": obj, "hidden": true }),
    );
    ok(
        &mut s,
        "view.set",
        json!({ "show_rulers": true, "show_grid": true, "snap_to_grid": true }),
    );
    ok(
        &mut s,
        "view.guides.add",
        json!({ "vertical": true, "at": 320.0 }),
    );
    ok(
        &mut s,
        "view.guides.move",
        json!({ "index": 0, "at": 400.0 }),
    );

    // Save and reload (serialize round trip) preserves lock, hide, and editor settings.
    let saved = s.course.to_json().expect("save");
    let reloaded = coursecraft_course::Course::from_json(&saved).expect("reload");
    let reloaded_obj = reloaded
        .slide("slide1")
        .expect("slide")
        .object(&obj)
        .expect("obj")
        .0;
    assert!(reloaded_obj.locked);
    assert!(reloaded_obj.editor_hidden);
    assert!(reloaded.editor.show_rulers);
    assert!(reloaded.editor.show_grid);
    assert!(reloaded.editor.snap_to_grid);
    assert!(reloaded.editor.show_guides);
    assert_eq!(reloaded.editor.guides.len(), 1);
    assert!(reloaded.editor.guides[0].vertical);
    assert!((reloaded.editor.guides[0].at - 400.0).abs() < 0.01);

    // Undo restores guide move, guide add, view settings, hide, and lock in reverse order.
    ok(&mut s, "edit.undo", json!({}));
    assert!((s.course.editor.guides[0].at - 320.0).abs() < 0.01);
    ok(&mut s, "edit.undo", json!({}));
    assert!(s.course.editor.guides.is_empty());
    ok(&mut s, "edit.undo", json!({}));
    assert!(s.course.editor.is_default());
    ok(&mut s, "edit.undo", json!({}));
    assert!(
        !s.course
            .slide("slide1")
            .expect("s")
            .object(&obj)
            .expect("o")
            .0
            .editor_hidden
    );
    ok(&mut s, "edit.undo", json!({}));
    assert!(
        !s.course
            .slide("slide1")
            .expect("s")
            .object(&obj)
            .expect("o")
            .0
            .locked
    );

    // Remove guide and undo.
    ok(
        &mut s,
        "view.guides.add",
        json!({ "vertical": false, "at": 180.0 }),
    );
    ok(&mut s, "view.guides.remove", json!({ "index": 0 }));
    assert!(s.course.editor.guides.is_empty());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.editor.guides.len(), 1);
}

#[test]
fn object_set_input_edits_kinds_and_undoes() {
    let mut s = Session::blank("Inputs");
    let slide = ok(&mut s, "slide.add", json!({ "title": "Inputs" }))["id"]
        .as_str()
        .expect("slide id")
        .to_owned();
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "text_var", "value": "" }),
    );
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "number_var", "value": 0 }),
    );
    let entry = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "text_entry", "variable": "text_var", "placeholder": "Old" },
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 },
        }),
    )["id"]
        .as_str()
        .expect("entry id")
        .to_owned();
    let slider = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "slider", "variable": "number_var", "min": 0, "max": 10, "step": 1 },
            "frame": { "x": 0.0, "y": 60.0, "w": 200.0, "h": 40.0 },
        }),
    )["id"]
        .as_str()
        .expect("slider id")
        .to_owned();

    let original_slider = s
        .course
        .slide(&slide)
        .and_then(|sl| sl.object(&slider))
        .map(|(o, _)| o.kind.clone());
    ok(
        &mut s,
        "object.set_input",
        json!({
            "slide": slide,
            "object": entry,
            "patch": { "placeholder": "Type", "max_chars": 12, "multiline": true },
        }),
    );
    ok(
        &mut s,
        "object.set_input",
        json!({
            "slide": slide,
            "object": slider,
            "patch": { "min": 5, "max": 50, "step": 5, "orientation": "vertical" },
        }),
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide(&slide)
            .and_then(|sl| sl.object(&slider))
            .map(|(o, _)| o.kind.clone()),
        original_slider
    );
    ok(&mut s, "edit.undo", json!({}));
    let course = &s.course;
    let entry = course
        .slide(&slide)
        .and_then(|sl| sl.object(&entry))
        .expect("entry");
    assert!(matches!(
        &entry.0.kind,
        coursecraft_course::ObjectKind::TextEntry { placeholder, max_chars, multiline, .. }
            if placeholder == "Old" && *max_chars == 0 && !*multiline
    ));
}

#[test]
fn object_set_input_rejects_an_invalid_range() {
    let mut s = Session::blank("Inputs");
    let slide = ok(&mut s, "slide.add", json!({ "title": "Inputs" }))["id"]
        .as_str()
        .expect("slide id")
        .to_owned();
    ok(&mut s, "variable.add", json!({ "name": "n", "value": 0 }));
    let slider = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "slider", "variable": "n", "min": 0, "max": 10, "step": 1 },
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 },
        }),
    )["id"]
        .as_str()
        .expect("slider id")
        .to_owned();
    let before = s.course.clone();
    let result = s.execute(
        "object.set_input",
        &json!({
            "slide": slide,
            "object": slider,
            "patch": { "min": 20, "max": 10 },
        }),
    );
    assert!(result.is_err(), "min >= max must be rejected");
    assert_eq!(s.course, before);
}

#[test]
fn marker_and_panel_commands_undo_and_deleting_a_panel_releases_children() {
    let mut s = Session::blank("Interactions");
    let slide = ok(&mut s, "slide.add", json!({ "title": "Interactions" }))["id"]
        .as_str()
        .expect("slide id")
        .to_owned();
    let marker = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "marker", "title": "Old" },
            "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
        }),
    )["id"]
        .as_str()
        .expect("marker id")
        .to_owned();
    let panel = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "scroll_panel", "content_height": 600.0 },
            "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 200.0 },
        }),
    )["id"]
        .as_str()
        .expect("panel id")
        .to_owned();
    let child = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "button", "label": "Child" },
            "frame": { "x": 120.0, "y": 120.0, "w": 100.0, "h": 50.0 },
        }),
    )["id"]
        .as_str()
        .expect("child id")
        .to_owned();

    ok(
        &mut s,
        "object.set_marker",
        json!({
            "slide": slide,
            "object": marker,
            "patch": { "title": "New", "body": "Body", "icon": "star", "color": "#FF0000" },
        }),
    );
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&marker))
        .map(|(object, _)| object.kind.clone())
        .expect("marker");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::Marker {
            title,
            body,
            icon: coursecraft_course::MarkerIcon::Star,
            color: Some(color),
            ..
        } if title == "New" && body == "Body" && color.0 == "#FF0000"
    ));
    ok(&mut s, "edit.undo", json!({}));
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&marker))
        .map(|(object, _)| object.kind.clone())
        .expect("marker");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::Marker { title, .. } if title == "Old"
    ));

    ok(
        &mut s,
        "object.set_panel",
        json!({ "slide": slide, "object": child, "panel": panel }),
    );
    let panel_of = |s: &Session| {
        s.course
            .slide(&slide)
            .and_then(|slide| slide.object(&child))
            .and_then(|(object, _)| object.panel.clone())
    };
    assert_eq!(panel_of(&s), Some(panel.clone()));

    ok(
        &mut s,
        "object.set_scroll_panel",
        json!({
            "slide": slide,
            "object": panel,
            "patch": { "content_height": 1200.0, "bar": "hidden" },
        }),
    );
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&panel))
        .map(|(object, _)| object.kind.clone())
        .expect("panel");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::ScrollPanel {
            content_height,
            bar: coursecraft_course::ScrollBar::Hidden,
        } if (*content_height - 1200.0).abs() < 1e-6
    ));

    ok(
        &mut s,
        "object.delete",
        json!({ "slide": slide, "object": panel }),
    );
    assert_eq!(panel_of(&s), None, "deleting the panel releases its child");

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        panel_of(&s),
        Some(panel.clone()),
        "undo restores the panel and the child's membership together"
    );
}

#[test]
fn object_set_timer_and_meter_edit_and_undo() {
    let mut s = Session::blank("Timer meter");
    let slide = ok(&mut s, "slide.add", json!({ "title": "Timer meter" }))["id"]
        .as_str()
        .expect("slide id")
        .to_owned();
    ok(&mut s, "variable.add", json!({ "name": "n", "value": 0 }));
    let timer = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "timer", "duration_ms": 60000 },
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 },
        }),
    )["id"]
        .as_str()
        .expect("timer id")
        .to_owned();
    let meter = ok(
        &mut s,
        "object.add",
        json!({
            "slide": slide,
            "kind": { "type": "progress_meter", "variable": "n", "min": 0, "max": 100 },
            "frame": { "x": 0.0, "y": 80.0, "w": 200.0, "h": 40.0 },
        }),
    )["id"]
        .as_str()
        .expect("meter id")
        .to_owned();

    ok(
        &mut s,
        "object.set_timer",
        json!({
            "slide": slide,
            "object": timer,
            "patch": {
                "duration_ms": 45000,
                "format": "seconds",
                "autostart": false,
                "style": { "size": 32.0, "bold": true },
            },
        }),
    );
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&timer))
        .map(|(object, _)| object.kind.clone())
        .expect("timer");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::Timer {
            duration_ms: 45000,
            format: coursecraft_course::TimerFormat::Seconds,
            autostart: false,
            style,
            ..
        } if style.size == 32.0 && style.bold
    ));

    ok(&mut s, "edit.undo", json!({}));
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&timer))
        .map(|(object, _)| object.kind.clone())
        .expect("timer");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::Timer {
            duration_ms: 60000,
            ..
        }
    ));

    ok(
        &mut s,
        "object.set_meter",
        json!({
            "slide": slide,
            "object": meter,
            "patch": { "min": 0, "max": 10, "style": "ring", "show_value": true },
        }),
    );
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&meter))
        .map(|(object, _)| object.kind.clone())
        .expect("meter");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::ProgressMeter {
            max,
            style: coursecraft_course::MeterStyle::Ring,
            show_value: true,
            ..
        } if *max == 10.0
    ));

    ok(&mut s, "edit.undo", json!({}));
    let kind = s
        .course
        .slide(&slide)
        .and_then(|slide| slide.object(&meter))
        .map(|(object, _)| object.kind.clone())
        .expect("meter");
    assert!(matches!(
        &kind,
        coursecraft_course::ObjectKind::ProgressMeter {
            max,
            style: coursecraft_course::MeterStyle::Bar,
            show_value: false,
            ..
        } if *max == 100.0
    ));

    // A patch that would leave the course invalid is refused and changes nothing.
    let before = s.course.clone();
    let result = s.execute(
        "object.set_meter",
        &json!({
            "slide": slide,
            "object": meter,
            "patch": { "min": 50, "max": 10 },
        }),
    );
    assert!(result.is_err(), "min >= max must be rejected");
    assert_eq!(s.course, before);

    // Setting a timer on a non-timer is an error.
    let result = s.execute(
        "object.set_timer",
        &json!({
            "slide": slide,
            "object": meter,
            "patch": { "duration_ms": 1000 },
        }),
    );
    assert!(result.is_err());
}

#[test]
fn interaction_insert_tabs_is_one_undo_step() {
    let mut s = Session::blank("Tabs");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "tabs" }),
    );
    let objects = result["objects"].as_array().expect("objects");
    assert_eq!(objects.len(), 3);
    let layers = result["layers"].as_array().expect("layers");
    assert_eq!(layers.len(), 3);

    let slide = s.course.slide("slide1").expect("slide");
    assert_eq!(slide.layers.len(), 3);
    assert_eq!(
        slide.objects.first().map(|o| o.name.as_str()),
        Some("Tab 1 button")
    );
    assert!(
        slide.objects.iter().any(|o| o.name == "Tab 3 button"),
        "each tab has a readable button"
    );
    assert!(
        slide
            .layers
            .iter()
            .any(|layer| layer.objects.iter().any(|o| o.name == "Tab 2 content")),
        "each layer has readable content"
    );
    assert!(
        slide.triggers.iter().any(|t| matches!(
            &t.action,
            coursecraft_course::Action::ShowLayer { layer } if layer == "tab_layer2"
        )),
        "a click shows its tab layer"
    );
    assert!(
        slide.triggers.iter().any(|t| matches!(
            &t.action,
            coursecraft_course::Action::ShowLayer { layer } if layer == "tab_layer1"
        ) && matches!(
            &t.when,
            coursecraft_course::Event::TimelineStarts { layer: None }
        )),
        "the first tab is shown when the timeline starts"
    );

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

fn assert_unique_and_valid(s: &Session, all: &[String]) {
    let unique: std::collections::BTreeSet<&str> = all.iter().map(String::as_str).collect();
    assert_eq!(unique.len(), all.len(), "inserted ids must be unique");
    assert!(
        s.course
            .validate()
            .into_iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error),
        "the course must stay valid after an insertion"
    );
}

#[test]
fn interaction_insert_flip_cards_builds_toggleable_states() {
    let mut s = Session::blank("Flip");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "flip_cards" }),
    );
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(3));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(0));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(3));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let slide = s.course.slide("slide1").expect("slide");
    assert_eq!(slide.objects.len(), 3);
    for (i, object) in slide.objects.iter().enumerate() {
        assert_eq!(object.name, format!("Card {}", i + 1));
        assert!(
            object.states.iter().any(|state| state.name == "Back"),
            "each card has a back state"
        );
    }
    let toggles = slide
        .triggers
        .iter()
        .filter(|t| {
            matches!(
                &t.action,
                coursecraft_course::Action::SetVariable {
                    op: coursecraft_course::VarOp::Toggle,
                    ..
                }
            )
        })
        .count();
    assert_eq!(toggles, 3, "one toggle per card");
    assert_eq!(
        slide.triggers.len(),
        9,
        "toggle plus two state triggers per card"
    );
    assert_eq!(s.course.variables.len(), 3);

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn interaction_insert_hotspot_reveal_builds_markers_and_callout_layers() {
    let mut s = Session::blank("Hotspots");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "hotspot_reveal" }),
    );
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(4));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(3));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(0));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let slide = s.course.slide("slide1").expect("slide");
    assert_eq!(slide.layers.len(), 3);
    assert!(
        slide
            .objects
            .iter()
            .any(|o| o.name == "Picture placeholder")
    );
    for i in 1..=3 {
        let marker = slide
            .objects
            .iter()
            .find(|o| o.name == format!("Hotspot {i} marker"))
            .expect("marker");
        assert!(marker.states.iter().any(|st| st.name == "Visited"));
        let layer = slide
            .layers
            .iter()
            .find(|l| l.name == format!("Hotspot {i}"))
            .expect("callout layer");
        assert_eq!(layer.objects.len(), 4, "panel, title, detail and close");
        assert!(
            layer
                .objects
                .iter()
                .any(|o| o.name == format!("Close hotspot {i}"))
        );
        assert!(
            layer.triggers.iter().any(|t| matches!(
                &t.action,
                coursecraft_course::Action::HideLayer { layer: None }
            )),
            "the close button hides its own layer"
        );
        assert!(
            slide.triggers.iter().any(|t| matches!(
                &t.action,
                coursecraft_course::Action::ShowLayer { layer: target } if target == &layer.id
            )),
            "clicking the marker shows its layer"
        );
    }

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn interaction_insert_process_steps_builds_variable_driven_layers() {
    let mut s = Session::blank("Steps");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "process_steps" }),
    );
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(13));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(4));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(1));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let variable = result["variables"][0].as_str().expect("step variable name");
    assert!(variable.contains("_step"), "variable name carries _step");
    assert_eq!(
        s.course.variable(variable).map(|v| v.value.clone()),
        Some(coursecraft_course::Value::Number(1.0)),
        "the step variable starts at 1"
    );
    let slide = s.course.slide("slide1").expect("slide");
    for i in 1..=4 {
        let circle = slide
            .objects
            .iter()
            .find(|o| o.name == format!("Step {i} circle"))
            .expect("step circle");
        assert!(circle.states.iter().any(|st| st.name == "Selected"));
        assert!(slide.layers.iter().any(|l| l.name == format!("Step {i}")));
    }
    let variable_changes = slide
        .triggers
        .iter()
        .filter(|t| {
            matches!(
                &t.when,
                coursecraft_course::Event::VariableChanges { variable: v } if v == variable
            )
        })
        .count();
    assert_eq!(
        variable_changes, 20,
        "five triggers per step drive the display"
    );
    assert!(
        slide.triggers.iter().any(|t| {
            matches!(
                &t.when,
                coursecraft_course::Event::TimelineStarts { layer: None }
            ) && matches!(&t.action, coursecraft_course::Action::ShowLayer { .. })
        }),
        "step 1 is shown on slide start"
    );

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn interaction_insert_timeline_builds_dated_points_and_visited_layers() {
    let mut s = Session::blank("Timeline");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "timeline" }),
    );
    // A line, five points, five year labels and five short titles.
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(16));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(5));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(0));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let slide = s.course.slide("slide1").expect("slide");
    assert!(
        slide.objects.iter().any(|o| o.name == "Timeline line"),
        "there is a line across the slide"
    );
    for i in 1..=5 {
        let point = slide
            .objects
            .iter()
            .find(|o| o.name == format!("Point {i}"))
            .expect("point");
        assert!(
            point.states.iter().any(|st| st.name == "Visited"),
            "each point remembers a visit"
        );
        assert!(
            slide
                .objects
                .iter()
                .any(|o| o.name == format!("Point {i} year")),
            "each point has a year label"
        );
        assert!(
            slide
                .layers
                .iter()
                .any(|l| l.name == format!("Point {i}") && l.hide_other_layers),
            "each point has an exclusive detail layer"
        );
        assert!(
            slide.triggers.iter().any(|t| {
                matches!(&t.when, coursecraft_course::Event::Click { object }
                    if object == &point.id)
                    && matches!(&t.action, coursecraft_course::Action::ShowLayer { .. })
            }),
            "clicking a point shows its layer"
        );
        assert!(
            slide.triggers.iter().any(|t| {
                matches!(&t.when, coursecraft_course::Event::Click { object }
                    if object == &point.id)
                    && matches!(
                        &t.action,
                        coursecraft_course::Action::ChangeState { state, .. }
                            if state == "Visited"
                    )
            }),
            "clicking a point marks it visited"
        );
    }
    let years: Vec<&str> = slide
        .objects
        .iter()
        .filter(|o| o.name.contains(" year"))
        .filter_map(|o| match &o.kind {
            coursecraft_course::ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(years, vec!["2020", "2021", "2022", "2023", "2024"]);
    assert!(
        !slide.triggers.iter().any(|t| {
            matches!(
                &t.when,
                coursecraft_course::Event::TimelineStarts { layer: None }
            ) && matches!(&t.action, coursecraft_course::Action::ShowLayer { .. })
        }),
        "no detail layer is shown when the slide starts"
    );

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn interaction_insert_reveal_grid_builds_tiles_flag_and_counter() {
    let mut s = Session::blank("Reveal");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "reveal_grid" }),
    );
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(7));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(0));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(7));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let variables: Vec<&str> = result["variables"]
        .as_array()
        .expect("variables")
        .iter()
        .filter_map(Json::as_str)
        .collect();
    let explored = variables[0];
    assert!(explored.ends_with("_explored"), "counter variable name");
    assert_eq!(
        s.course.variable(explored).map(|v| &v.value),
        Some(&coursecraft_course::Value::Number(0.0)),
        "the counter starts at zero"
    );
    let slide = s.course.slide("slide1").expect("slide");
    for i in 1..=6 {
        let tile = slide
            .objects
            .iter()
            .find(|o| o.name == format!("Tile {i}"))
            .expect("tile");
        assert!(
            tile.states.iter().any(|st| st.name == "Revealed"),
            "each tile has a revealed state"
        );
        assert!(
            slide.triggers.iter().any(|t| {
                matches!(&t.action, coursecraft_course::Action::ChangeState { state, .. }
                    if state == "Revealed")
                    && matches!(&t.when, coursecraft_course::Event::VariableChanges { .. })
            }),
            "a flag change reveals a tile"
        );
    }
    let counter = slide
        .objects
        .iter()
        .find(|o| o.name == "Explored counter")
        .expect("counter text");
    match &counter.kind {
        coursecraft_course::ObjectKind::Text { text, .. } => {
            assert_eq!(text, &format!("%{explored}% of 6 explored"));
        }
        _ => panic!("counter must be text"),
    }
    let adds = slide
        .triggers
        .iter()
        .filter(|t| {
            matches!(
                &t.action,
                coursecraft_course::Action::SetVariable {
                    op: coursecraft_course::VarOp::Add,
                    ..
                }
            )
        })
        .count();
    assert_eq!(adds, 6, "one increment per tile");
    let resets = slide
        .triggers
        .iter()
        .filter(|t| {
            matches!(
                &t.when,
                coursecraft_course::Event::TimelineStarts { layer: None }
            )
        })
        .count();
    assert_eq!(resets, 13, "counter and every tile reset on slide start");

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn interaction_insert_scenario_choice_builds_feedback_layers_without_a_question() {
    let mut s = Session::blank("Scenario");
    let before = serde_json::to_value(&s.course).expect("course json");
    let result = ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "scenario_choice" }),
    );
    assert_eq!(result["objects"].as_array().map(Vec::len), Some(6));
    assert_eq!(result["layers"].as_array().map(Vec::len), Some(3));
    assert_eq!(result["variables"].as_array().map(Vec::len), Some(0));
    let all: Vec<String> = result["all_objects"]
        .as_array()
        .expect("all_objects")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    assert_unique_and_valid(&s, &all);

    let slide = s.course.slide("slide1").expect("slide");
    assert!(slide.question.is_none(), "this is not a graded question");
    assert!(
        slide.objects.iter().any(
            |o| matches!(&o.kind, coursecraft_course::ObjectKind::Text { text, .. }
                if text == "What would you do?")
        ),
        "the situation has the expected title"
    );
    for i in 1..=3 {
        assert!(
            slide
                .objects
                .iter()
                .any(|o| o.name == format!("Choice {i}")),
            "each choice has a button"
        );
        let layer = slide
            .layers
            .iter()
            .find(|l| l.name == format!("Choice {i}"))
            .expect("feedback layer");
        assert!(
            layer.objects.iter().any(
                |o| matches!(&o.kind, coursecraft_course::ObjectKind::Text { text, .. }
                    if text == if i == 2 { "Good choice" } else { "Think again" })
            ),
            "the feedback heading matches the choice"
        );
        assert!(
            slide.triggers.iter().any(|t| {
                matches!(&t.action, coursecraft_course::Action::ShowLayer { layer: target }
                    if target == &layer.id)
            }),
            "clicking a choice shows its layer"
        );
        if i != 2 {
            assert!(
                layer
                    .objects
                    .iter()
                    .any(|o| o.name == format!("Try again {i}")),
                "a wrong choice has a Try again button"
            );
            assert!(
                layer.triggers.iter().any(|t| matches!(
                    &t.action,
                    coursecraft_course::Action::HideLayer { layer: None }
                )),
                "Try again hides the feedback layer"
            );
        }
    }

    ok(&mut s, "edit.undo", json!({}));
    let after = serde_json::to_value(&s.course).expect("course json");
    assert_eq!(
        after, before,
        "undo restores the whole insertion in one step"
    );
}

#[test]
fn object_set_drag_sets_clears_and_undoes_in_one_step() {
    let mut s = Session::blank("Drag");
    let card = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10, "y": 20, "w": 100, "h": 80 }
        }),
    )["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let bin = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
            "frame": { "x": 300, "y": 20, "w": 120, "h": 120 }
        }),
    )["id"]
        .as_str()
        .expect("id")
        .to_owned();

    let before = s.course.clone();
    let undo_before = s.undo_len();
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [bin], "snap": false, "return_on_miss": false }
        }),
    );
    let drag = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&card))
        .and_then(|(object, _)| object.drag.clone())
        .expect("drag spec");
    assert_eq!(drag.targets, vec![bin.clone()]);
    assert!(!drag.snap);
    assert!(!drag.return_on_miss);
    assert_eq!(s.undo_len(), undo_before + 1);

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);

    // An unknown target is refused and leaves the course alone.
    let before = s.course.clone();
    assert!(
        s.execute(
            "object.set_drag",
            &json!({
                "slide": "slide1",
                "object": card,
                "drag": { "targets": ["missing"] }
            })
        )
        .is_err()
    );
    assert_eq!(s.course, before);

    // Clearing is one undo step too.
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [bin] }
        }),
    );
    ok(
        &mut s,
        "object.set_drag",
        json!({ "slide": "slide1", "object": card, "drag": null }),
    );
    assert!(
        s.course
            .slide("slide1")
            .and_then(|slide| slide.object(&card))
            .is_some_and(|(object, _)| object.drag.is_none())
    );
    ok(&mut s, "edit.undo", json!({}));
    assert!(
        s.course
            .slide("slide1")
            .and_then(|slide| slide.object(&card))
            .is_some_and(|(object, _)| object.drag.is_some())
    );
}

fn session_with_video() -> Session {
    let mut s = Session::blank("Cues");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "video", "src": "clip.mp4" },
            "frame": { "x": 40.0, "y": 40.0, "w": 480.0, "h": 270.0 },
        }),
    );
    s
}

fn cues_of(s: &Session, object: &str) -> Vec<coursecraft_course::Cue> {
    s.course
        .slide("slide1")
        .and_then(|slide| slide.object(object))
        .and_then(|(object, _)| match &object.kind {
            coursecraft_course::ObjectKind::Video { cues, .. } => Some(cues.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn video_cues_add_set_move_and_delete_with_undo() {
    let mut s = session_with_video();
    let video = s.course.scenes[0].slides[0].objects[0].id.clone();
    let before = s.course.clone();
    let undos = s.undo_len();

    let result = ok(
        &mut s,
        "video.add_cue",
        json!({ "slide": "slide1", "object": video, "at_ms": 2000, "name": "Two", "pause": true }),
    );
    let cue = result["cue"].as_str().expect("cue id").to_owned();
    assert_eq!(s.undo_len(), undos + 1);
    let cues = cues_of(&s, &video);
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].id, cue);
    assert_eq!(cues[0].name, "Two");
    assert!(cues[0].pause);

    ok(
        &mut s,
        "video.set_cue",
        json!({ "slide": "slide1", "object": video, "cue": cue, "patch": { "at_ms": 500, "name": "Moved", "pause": false } }),
    );
    let cues = cues_of(&s, &video);
    assert_eq!(cues[0].at_ms, 500.0);
    assert_eq!(cues[0].name, "Moved");
    assert!(!cues[0].pause);

    ok(
        &mut s,
        "video.remove_cue",
        json!({ "slide": "slide1", "object": video, "cue": cue }),
    );
    assert!(cues_of(&s, &video).is_empty());

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(cues_of(&s, &video).len(), 1);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(cues_of(&s, &video)[0].at_ms, 2000.0);
    ok(&mut s, "edit.undo", json!({}));
    assert!(cues_of(&s, &video).is_empty());
    assert_eq!(s.course, before);
}

#[test]
fn deleting_a_cue_removes_its_cue_reached_triggers_in_the_same_undo_step() {
    let mut s = session_with_video();
    let video = s.course.scenes[0].slides[0].objects[0].id.clone();
    let result = ok(
        &mut s,
        "video.add_cue",
        json!({ "slide": "slide1", "object": video, "at_ms": 1000, "name": "Here" }),
    );
    let cue = result["cue"].as_str().expect("cue id").to_owned();
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "cue_reached", "object": video, "cue": cue },
                "action": { "action": "pause_media", "object": video }
            }
        }),
    );
    assert_eq!(s.course.scenes[0].slides[0].triggers.len(), 1);
    let before_delete = s.course.clone();

    ok(
        &mut s,
        "video.remove_cue",
        json!({ "slide": "slide1", "object": video, "cue": cue }),
    );
    assert!(cues_of(&s, &video).is_empty());
    assert!(s.course.scenes[0].slides[0].triggers.is_empty());

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before_delete);
    assert_eq!(cues_of(&s, &video).len(), 1);
    assert_eq!(s.course.scenes[0].slides[0].triggers.len(), 1);
}

#[test]
fn add_question_cue_creates_the_layer_and_triggers_in_one_undo_step() {
    let mut s = session_with_video();
    let video = s.course.scenes[0].slides[0].objects[0].id.clone();
    let before = s.course.clone();
    let undos = s.undo_len();
    let result = ok(
        &mut s,
        "video.add_question_cue",
        json!({ "slide": "slide1", "object": video, "at_ms": 1500, "name": "Question 1" }),
    );
    assert_eq!(s.undo_len(), undos + 1);
    let cue = result["cue"].as_str().expect("cue id");
    let layer = result["layer"].as_str().expect("layer id");
    let continue_id = result["continue"].as_str().expect("continue id");

    let cues = cues_of(&s, &video);
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].id, cue);
    assert!(cues[0].pause);
    assert_eq!(cues[0].name, "Question 1");
    let slide = s.course.slide("slide1").expect("slide");
    let layer = slide.layer(layer).expect("layer");
    assert_eq!(layer.name, "Question 1");
    assert_eq!(layer.objects.len(), 5, "prompt, three choices and Continue");
    assert!(slide.triggers.iter().any(|trigger| matches!(
        &trigger.when,
        coursecraft_course::Event::CueReached { object, cue: c } if object.as_str() == video.as_str() && c.as_str() == cue
    ) && matches!(&trigger.action, coursecraft_course::Action::ShowLayer { layer: l } if l.as_str() == layer.id.as_str())));
    let continue_clicks: Vec<&coursecraft_course::Trigger> = slide
        .triggers
        .iter()
        .filter(|trigger| {
            matches!(&trigger.when, coursecraft_course::Event::Click { object } if object.as_str() == continue_id)
        })
        .collect();
    assert_eq!(continue_clicks.len(), 2);
    assert!(continue_clicks.iter().any(|trigger| matches!(
        &trigger.action,
        coursecraft_course::Action::HideLayer { layer: Some(l) } if l.as_str() == layer.id.as_str()
    )));
    assert!(continue_clicks.iter().any(|trigger| matches!(
        &trigger.action,
        coursecraft_course::Action::PlayMedia { object } if object.as_str() == video.as_str()
    )));
    assert!(
        s.course
            .validate()
            .iter()
            .all(|i| i.severity != coursecraft_course::Severity::Error)
    );

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn question_set_kind_resets_scoring_from_partial_to_all_or_nothing_and_undo_restores() {
    use coursecraft_course::{QuestionKind, Scoring};

    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "question.set",
        json!({
            "slide": "slide1",
            "question": {
                "kind": "multiple_response",
                "prompt": "Q?",
                "choices": [
                    { "id": "a", "text": "A", "correct": true },
                    { "id": "b", "text": "B", "correct": true }
                ],
                "scoring": "partial"
            }
        }),
    );
    assert_eq!(quiz(&s).scoring, Scoring::Partial);
    assert_eq!(quiz(&s).kind, QuestionKind::MultipleResponse);

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "fill_in" }),
    );
    assert_eq!(quiz(&s).scoring, Scoring::AllOrNothing);

    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(quiz(&s).scoring, Scoring::Partial);
    assert_eq!(quiz(&s).kind, QuestionKind::MultipleResponse);
}

#[test]
fn question_set_kind_likert_multiple_choice_essay_and_undo_restores_each() {
    use coursecraft_course::QuestionKind;
    let mut s = mc_session();
    let before = s.course.clone();

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "likert" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Likert);
    assert_eq!(q.choices.len(), 5);
    assert!(q.choices.iter().all(|c| !c.correct));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "likert" }),
    );
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "multiple_choice" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::MultipleChoice);
    assert_eq!(q.choices.iter().filter(|c| c.correct).count(), 1);
    ok(&mut s, "edit.undo", json!({}));
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Likert);
    assert_eq!(q.choices.len(), 5);
    assert!(q.choices.iter().all(|c| !c.correct));

    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "multiple_choice" }),
    );
    ok(
        &mut s,
        "question.set_kind",
        json!({ "slide": "slide1", "kind": "essay" }),
    );
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::Essay);
    assert!(q.choices.is_empty());
    ok(&mut s, "edit.undo", json!({}));
    let q = quiz(&s);
    assert_eq!(q.kind, QuestionKind::MultipleChoice);
    assert_eq!(q.choices.iter().filter(|c| c.correct).count(), 1);
}

// ---------------------------------------------------------------- banks and draws

fn bank_id(s: &mut Session) -> String {
    ok(s, "bank.create", json!({ "title": "Bank One" }))["id"]
        .as_str()
        .expect("bank id")
        .to_owned()
}

#[test]
fn bank_create_and_update_undo_in_one_step_each() {
    let mut s = Session::blank("Title");
    let before = s.course.clone();
    let id = bank_id(&mut s);
    assert_eq!(s.course.bank(&id).unwrap().title, "Bank One");
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);

    let id = bank_id(&mut s);
    ok(
        &mut s,
        "bank.update",
        json!({ "bank": id, "title": "Renamed" }),
    );
    assert_eq!(s.course.bank(&id).unwrap().title, "Renamed");
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.bank(&id).unwrap().title, "Bank One");
    assert!(
        s.execute("bank.update", &json!({ "bank": "ghost", "title": "X" }))
            .is_err()
    );
    assert!(
        s.execute("bank.update", &json!({ "bank": id, "title": "  " }))
            .is_err()
    );
}

#[test]
fn bank_add_question_builds_a_playable_question_that_can_be_drawn() {
    use coursecraft_course::QuestionKind;
    let mut s = Session::blank("Title");
    let bank = bank_id(&mut s);
    let before = s.course.clone();
    let q1 = ok(
        &mut s,
        "bank.add_question",
        json!({ "bank": bank, "title": "First" }),
    )["id"]
        .as_str()
        .expect("question id")
        .to_owned();
    let q2 = ok(
        &mut s,
        "bank.add_question",
        json!({ "bank": bank, "kind": "true_false", "title": "Second" }),
    )["id"]
        .as_str()
        .expect("question id")
        .to_owned();
    let bank_ref = s.course.bank(&bank).unwrap();
    assert_eq!(bank_ref.slides.len(), 2);
    assert_eq!(bank_ref.slides[0].id, q1);
    assert_eq!(bank_ref.slides[1].id, q2);
    assert!(bank_ref.slides.iter().all(|slide| slide.question.is_some()));
    assert_eq!(
        bank_ref.slides[1].question.as_ref().unwrap().kind,
        QuestionKind::TrueFalse
    );
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());

    let draw = ok(&mut s, "draw.insert", json!({ "bank": bank, "count": 2 }))["id"]
        .as_str()
        .expect("draw id")
        .to_owned();
    let draw_slide = s.course.slide(&draw).unwrap();
    assert_eq!(draw_slide.title, "Bank One");
    assert_eq!(draw_slide.draw.as_ref().unwrap().bank, bank);
    assert_eq!(draw_slide.draw.as_ref().unwrap().count, 2);
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());

    ok(&mut s, "edit.undo", json!({}));
    assert!(s.course.slide(&draw).is_none());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.bank(&bank).unwrap().slides.len(), 1);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn bank_delete_refuses_while_drawn_then_undoes() {
    let mut s = Session::blank("Title");
    let bank = bank_id(&mut s);
    ok(&mut s, "bank.add_question", json!({ "bank": bank }));
    let draw = ok(&mut s, "draw.insert", json!({ "bank": bank, "count": 1 }))["id"]
        .as_str()
        .expect("draw id")
        .to_owned();
    assert!(s.execute("bank.delete", &json!({ "bank": bank })).is_err());
    assert!(s.course.bank(&bank).is_some());
    ok(&mut s, "slide.delete", json!({ "slide": draw }));
    let before = s.course.clone();
    ok(&mut s, "bank.delete", json!({ "bank": bank }));
    assert!(s.course.bank(&bank).is_none());
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
    assert!(
        s.execute("bank.delete", &json!({ "bank": "ghost" }))
            .is_err()
    );
}

#[test]
fn view_set_last_publish_skips_undo_while_grid_settings_still_undo() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "title": "Edited" }),
    );
    let undos_after_edit = s.undo_len();

    ok(
        &mut s,
        "view.set",
        json!({ "last_publish": { "standard": "web" } }),
    );
    assert_eq!(
        s.course
            .editor
            .last_publish
            .as_ref()
            .map(|p| p.standard.as_str()),
        Some("web")
    );
    assert_eq!(
        s.undo_len(),
        undos_after_edit,
        "remembering last_publish must not add an undo step"
    );

    ok(&mut s, "view.set", json!({ "show_grid": true }));
    assert_eq!(
        s.undo_len(),
        undos_after_edit + 1,
        "grid/guide settings must remain undoable"
    );
    ok(&mut s, "edit.undo", json!({}));
    assert!(!s.course.editor.show_grid);
    assert_eq!(s.undo_len(), undos_after_edit);
}

#[test]
fn bank_move_and_remove_slide_with_draw_guard() {
    let mut s = Session::blank("Title");
    let bank = bank_id(&mut s);
    // A slide without a question cannot move into a bank.
    ok(&mut s, "slide.add", json!({ "id": "plain" }));
    assert!(
        s.execute(
            "bank.move_slide",
            &json!({ "slide": "plain", "bank": bank })
        )
        .is_err()
    );
    ok(&mut s, "question.insert", json!({ "slide": "plain" }));
    let before = s.course.clone();
    ok(
        &mut s,
        "bank.move_slide",
        json!({ "slide": "plain", "bank": bank, "index": 0 }),
    );
    assert!(s.course.slides().all(|slide| slide.id != "plain"));
    assert_eq!(s.course.bank(&bank).unwrap().slides.len(), 1);

    ok(&mut s, "bank.add_question", json!({ "bank": bank }));
    let draw = ok(&mut s, "draw.insert", json!({ "bank": bank, "count": 2 }))["id"]
        .as_str()
        .expect("draw id")
        .to_owned();
    // Removing one of two bank slides would starve the draw of 2.
    let victim = s.course.bank(&bank).unwrap().slides[0].id.clone();
    assert!(
        s.execute("bank.remove_slide", &json!({ "slide": victim }))
            .is_err()
    );
    ok(&mut s, "draw.update", json!({ "slide": draw, "count": 1 }));
    ok(&mut s, "bank.remove_slide", json!({ "slide": victim }));
    assert_eq!(s.course.bank(&bank).unwrap().slides.len(), 1);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.bank(&bank).unwrap().slides.len(), 2);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.bank(&bank).unwrap().slides.len(), 2);
    assert_eq!(
        s.course.slide(&draw).unwrap().draw.as_ref().unwrap().count,
        2
    );
    ok(&mut s, "edit.undo", json!({}));
    ok(&mut s, "edit.undo", json!({}));
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn draw_insert_and_update_error_cases() {
    let mut s = Session::blank("Title");
    let bank = bank_id(&mut s);
    ok(&mut s, "bank.add_question", json!({ "bank": bank }));
    assert!(
        s.execute("draw.insert", &json!({ "bank": "ghost", "count": 1 }))
            .is_err()
    );
    assert!(
        s.execute("draw.insert", &json!({ "bank": bank, "count": 0 }))
            .is_err()
    );
    assert!(
        s.execute("draw.insert", &json!({ "bank": bank, "count": 5 }))
            .is_err()
    );
    let draw = ok(
        &mut s,
        "draw.insert",
        json!({ "bank": bank, "count": 1, "index": 0 }),
    )["id"]
        .as_str()
        .expect("draw id")
        .to_owned();
    assert_eq!(s.course.slides().next().unwrap().id, draw);
    assert!(
        s.execute("draw.update", &json!({ "slide": draw, "count": 0 }))
            .is_err()
    );
    assert!(
        s.execute("draw.update", &json!({ "slide": draw, "bank": "ghost" }))
            .is_err()
    );
    assert!(
        s.execute("draw.update", &json!({ "slide": "slide1", "count": 1 }))
            .is_err()
    );
    assert!(
        s.execute("draw.update", &json!({ "slide": "ghost", "count": 1 }))
            .is_err()
    );
    let before = s.course.clone();
    ok(&mut s, "draw.update", json!({ "slide": draw, "count": 1 }));
    assert_eq!(s.course, before);
}

fn add_obj(s: &mut Session, kind: Json) -> String {
    ok(
        s,
        "object.add",
        json!({ "slide": "slide1", "kind": kind, "frame": { "x": 10, "y": 10, "w": 100, "h": 60 } }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn apply_style_copies_appearance_only() {
    let mut s = Session::blank("Painter");
    let a = add_obj(
        &mut s,
        json!({ "type": "shape", "shape": "ellipse", "fill": "#112233" }),
    );
    let b = add_obj(
        &mut s,
        json!({ "type": "shape", "shape": "rectangle", "fill": "#FF0000" }),
    );
    ok(
        &mut s,
        "object.set_opacity",
        json!({ "slide": "slide1", "object": a, "opacity": 0.5 }),
    );
    let before = s.undo_len();
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": "slide1", "source_slide": "slide1", "source": a, "objects": [b] }),
    );
    assert_eq!(s.undo_len(), before + 1);
    let slide = s.course.slide("slide1").unwrap();
    let ob = slide.object(&b).unwrap().0;
    assert!((ob.opacity - 0.5).abs() < 0.001);
    match &ob.kind {
        ObjectKind::Shape { shape, fill, .. } => {
            assert_eq!(fill.0, "#112233");
            assert_eq!(*shape, ShapeKind::Rectangle, "geometry kept");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(ob.frame, slide.object(&b).unwrap().0.frame);
    s.undo().unwrap();
    let ob = s.course.slide("slide1").unwrap().object(&b).unwrap().0;
    assert!((ob.opacity - 1.0).abs() < 0.001);
}

#[test]
fn apply_style_text_to_text_clears_run_overrides_and_keeps_content() {
    let mut s = Session::blank("Painter");
    let a = add_obj(
        &mut s,
        json!({ "type": "text", "text": "Source", "style": { "size": 40, "bold": true } }),
    );
    let b = add_obj(
        &mut s,
        json!({ "type": "text", "text": "Target", "runs": [
            { "text": "Tar", "size": 10.0, "italic": true }, { "text": "get" } ] }),
    );
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": "slide1", "source_slide": "slide1", "source": a, "objects": [b] }),
    );
    let ob = s.course.slide("slide1").unwrap().object(&b).unwrap().0;
    match &ob.kind {
        ObjectKind::Text {
            text, style, runs, ..
        } => {
            assert_eq!(text, "Target");
            assert!(style.bold && (style.size - 40.0).abs() < 0.001);
            assert_eq!(runs.len(), 2);
            assert!(runs.iter().all(|r| r.size.is_none() && r.italic.is_none()));
            assert_eq!(runs[0].text, "Tar");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn apply_style_button_to_shape_shares_fill_only_and_rejects_bad_ids() {
    let mut s = Session::blank("Painter");
    let a = add_obj(
        &mut s,
        json!({ "type": "button", "label": "Go", "fill": "#00AA00" }),
    );
    let b = add_obj(
        &mut s,
        json!({ "type": "shape", "shape": "ellipse", "fill": "#FF0000",
                "outline": { "color": "#000000", "width": 3.0 } }),
    );
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": "slide1", "source_slide": "slide1", "source": a, "objects": [b] }),
    );
    let ob = s.course.slide("slide1").unwrap().object(&b).unwrap().0;
    match &ob.kind {
        ObjectKind::Shape { fill, outline, .. } => {
            assert_eq!(fill.0, "#00AA00");
            assert!(outline.is_some(), "outline untouched: button has none");
        }
        other => panic!("{other:?}"),
    }
    for p in [
        json!({ "slide": "slide1", "source_slide": "slide1", "source": "nope", "objects": [b] }),
        json!({ "slide": "slide1", "source_slide": "nope", "source": a, "objects": [b] }),
        json!({ "slide": "slide1", "source_slide": "slide1", "source": a, "objects": ["nope"] }),
        json!({ "slide": "nope", "source_slide": "slide1", "source": a, "objects": [b] }),
        json!({ "slide": "slide1", "source_slide": "slide1", "source": a, "objects": [] }),
    ] {
        assert!(s.execute("object.apply_style", &p).is_err());
    }
}

fn sized_course() -> Session {
    let course = coursecraft_course::Course::from_json(
        &json!({
            "schema_version": 1, "id": "sz", "title": "Size",
            "slide_size": { "w": 1280, "h": 720 },
            "masters": [{ "id": "m1", "title": "Master", "objects": [
                { "id": "mo", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233",
                    "outline": { "color": "#000000", "width": 4.0 } },
                  "frame": { "x": 0, "y": 0, "w": 1280, "h": 100 } } ] }],
            "scenes": [{ "id": "sc", "title": "S", "slides": [{
                "id": "slide1", "title": "One",
                "objects": [
                    { "id": "t", "kind": { "type": "text", "text": "Hi", "style": { "size": 40 } },
                      "frame": { "x": 100, "y": 200, "w": 400, "h": 100 },
                      "shadow": { "color": "#000000", "offset_x": 10, "offset_y": 10, "blur": 20 },
                      "motion_paths": [{ "id": "mp", "points": [{ "x": 100, "y": 100 }, { "x": 300, "y": 100 }],
                                         "duration_ms": 1000, "relative": false }] }
                ],
                "layers": [{ "id": "l1", "name": "L", "objects": [
                    { "id": "lo", "kind": { "type": "shape", "shape": "ellipse", "fill": "#FF0000" },
                      "frame": { "x": 640, "y": 360, "w": 200, "h": 100 } } ] }]
            }]}]
        })
        .to_string(),
    )
    .expect("course");
    Session::new(course)
}

#[test]
fn set_slide_size_keep_changes_only_the_size_and_undoes() {
    let mut s = sized_course();
    let before = s.course.to_json().unwrap();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1920, "h": 1080, "mode": "keep" }),
    );
    assert_eq!((s.course.slide_size.w, s.course.slide_size.h), (1920, 1080));
    let t = s.course.slide("slide1").unwrap().object("t").unwrap().0;
    assert_eq!(t.frame.x, 100.0);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn set_slide_size_scale_uses_one_factor_and_centres_everything() {
    let mut s = sized_course();
    let before = s.course.to_json().unwrap();
    // 1280x720 -> 1000x1000: factor min(0.78125, 1.3889) = 0.78125, dy = (1000 - 562.5) / 2.
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1000, "h": 1000, "mode": "scale" }),
    );
    let f = 0.78125_f32;
    let dy = (1000.0 - 720.0 * f) / 2.0;
    let slide = s.course.slide("slide1").unwrap();
    let t = slide.object("t").unwrap().0;
    assert_eq!(t.frame.x, (100.0 * f).round());
    assert_eq!(t.frame.y, (200.0 * f + dy).round());
    assert_eq!(t.frame.w, (400.0 * f).round());
    match &t.kind {
        ObjectKind::Text { style, .. } => assert!((style.size - 40.0 * f).abs() < 0.01),
        other => panic!("{other:?}"),
    }
    let shadow = t.shadow.as_ref().unwrap();
    assert!((shadow.offset_x - 10.0 * f).abs() < 0.01 && (shadow.blur - 20.0 * f).abs() < 0.01);
    let mp = &t.motion_paths[0].points[0];
    assert!((mp.x - 100.0 * f).abs() < 0.01 && (mp.y - (100.0 * f + dy)).abs() < 0.01);
    let lo = slide.layers[0].objects[0].frame;
    assert_eq!(lo.x, (640.0 * f).round());
    let mo = s.course.master("m1").unwrap().object("mo").unwrap().0;
    assert_eq!(mo.frame.w, (1280.0 * f).round());
    match &mo.kind {
        ObjectKind::Shape { outline, .. } => {
            assert!((outline.as_ref().unwrap().width - 4.0 * f).abs() < 0.01);
        }
        other => panic!("{other:?}"),
    }
    assert_no_errors(&s);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn set_slide_size_scales_the_tile_scale_of_picture_fills() {
    // N1: a tiled picture fill follows the content when the slide size scales it.
    let mut s = sized_course();
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "tiled",
            "kind": {
                "type": "shape", "shape": "rectangle", "fill": "#224466",
                "fill_style": { "type": "picture", "src": "t.png", "mode": "tile", "tile_scale": [2.0, 0.5] }
            },
            "frame": { "x": 0, "y": 0, "w": 640, "h": 360 }
        }),
    );
    let before = s.course.to_json().unwrap();
    // 1280x720 -> 640x360 halves everything.
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 640, "h": 360, "mode": "scale" }),
    );
    let tiled = s.course.slide("slide1").unwrap().object("tiled").unwrap().0;
    match &tiled.kind {
        ObjectKind::Shape {
            fill_style: Some(coursecraft_course::FillStyle::Picture { tile_scale, .. }),
            ..
        } => assert!((tile_scale[0] - 1.0).abs() < 1e-4 && (tile_scale[1] - 0.25).abs() < 1e-4),
        other => panic!("{other:?}"),
    }
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn set_slide_size_fill_uses_max_factor_and_centres_overhang() {
    // A-01: 1280x720 -> 1000x1000 with "fill" uses factor = max(1000/1280, 1000/720) = 1000/720.
    let mut s = sized_course();
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "bg",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#224466" },
            "frame": { "x": 0, "y": 0, "w": 1280, "h": 720 }
        }),
    );
    let before = s.course.to_json().unwrap();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1000, "h": 1000, "mode": "fill" }),
    );
    let f = (1000.0_f32 / 1280.0_f32).max(1000.0_f32 / 720.0_f32);
    let dx = (1000.0 - 1280.0 * f) / 2.0;
    let dy = (1000.0 - 720.0 * f) / 2.0;
    assert!(dx < 0.0);
    assert!(dy.abs() < 0.001);

    let slide = s.course.slide("slide1").unwrap();
    let bg = slide.object("bg").unwrap().0;
    assert_eq!(bg.frame.x, dx.round());
    assert_eq!(bg.frame.y, 0.0);
    assert_eq!(bg.frame.w, (1280.0 * f).round());
    assert_eq!(bg.frame.h, 1000.0);
    let left_overhang = -bg.frame.x;
    let right_overhang = bg.frame.x + bg.frame.w - 1000.0;
    assert!((left_overhang - right_overhang).abs() <= 1.0);

    let t = slide.object("t").unwrap().0;
    assert_eq!(t.frame.x, (100.0 * f + dx).round());
    assert_eq!(t.frame.y, (200.0 * f).round());
    assert_eq!(t.frame.w, (400.0 * f).round());
    assert_no_errors(&s);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn set_slide_size_checks_bounds_mode_and_noop() {
    let mut s = sized_course();
    for p in [
        json!({ "w": 99, "h": 720, "mode": "keep" }),
        json!({ "w": 1280, "h": 99, "mode": "keep" }),
        json!({ "w": 7681, "h": 720, "mode": "keep" }),
        json!({ "w": 1280, "h": 4321, "mode": "scale" }),
        json!({ "w": 1280, "h": 720, "mode": "stretch" }),
        json!({ "w": -5, "h": 720, "mode": "keep" }),
        json!({ "w": 1280, "h": 720 }),
    ] {
        assert!(s.execute("course.set_slide_size", &p).is_err(), "{p}");
    }
    let undo = s.undo_len();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1280, "h": 720, "mode": "scale" }),
    );
    assert_eq!(s.undo_len(), undo, "unchanged size is a no-op");
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 100, "h": 100, "mode": "scale" }),
    );
    assert_no_errors(&s);
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 7680, "h": 4320, "mode": "scale" }),
    );
    assert_no_errors(&s);
}

fn build_test_state_with_all_extras() -> coursecraft_course::StateDef {
    let base_text_style = coursecraft_course::TextStyle {
        size: 20.0,
        letter_spacing: 4.0,
        insets: Some(coursecraft_course::TextInsets {
            left: 4.0,
            top: 6.0,
            right: 8.0,
            bottom: 10.0,
        }),
        ..coursecraft_course::TextStyle::default()
    };
    let base_runs = vec![coursecraft_course::TextRun {
        text: "Hi".to_owned(),
        bold: None,
        italic: None,
        underline: None,
        strikethrough: None,
        baseline: None,
        letter_spacing: Some(2.0),
        highlight: None,
        shadow: None,
        size: Some(16.0),
        color: None,
        link: None,
        font: None,
    }];
    let base_paragraphs = vec![coursecraft_course::Paragraph {
        space_before: 10.0,
        space_after: 20.0,
        bullet_size: Some(12.0),
        ..coursecraft_course::Paragraph::default()
    }];
    let base_outline = coursecraft_course::Outline {
        color: coursecraft_course::Color("#112233".to_owned()),
        width: 4.0,
        dash: coursecraft_course::DashStyle::Solid,
        dash_pattern: None,
        join: coursecraft_course::LineJoin::Miter,
        miter_limit: None,
    };
    let base_shadow = coursecraft_course::Shadow {
        color: coursecraft_course::Color("#00000080".to_owned()),
        offset_x: 6.0,
        offset_y: -8.0,
        blur: 12.0,
    };
    let base_glow = coursecraft_course::Glow {
        color: coursecraft_course::Color("#FFCC00".to_owned()),
        size: 10.0,
    };
    let base_shape_text = coursecraft_course::ShapeText {
        text: "Hi".to_owned(),
        style: base_text_style.clone(),
        runs: base_runs.clone(),
        paragraphs: base_paragraphs.clone(),
        autofit: coursecraft_course::Autofit::None,
    };
    let path_struct = coursecraft_course::Path {
        points: vec![
            coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
            coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
        ],
        style: coursecraft_course::PathStyle::Straight,
        closed: true,
        subpaths: vec![],
        fill: Some(coursecraft_course::Color("#AABBCC".to_owned())),
        fill_style: None,
        outline: base_outline.clone(),
        start_end: coursecraft_course::LineEnd::None,
        end_end: coursecraft_course::LineEnd::None,
        connector: None,
        start_glue: None,
        end_glue: None,
        text: Some(base_shape_text.clone()),
    };
    coursecraft_course::StateDef {
        name: "Hover".to_owned(),
        outline: Some(base_outline.clone()),
        text_style: Some(base_text_style.clone()),
        runs: Some(base_runs.clone()),
        paragraphs: Some(base_paragraphs.clone()),
        shadow: Some(base_shadow.clone()),
        glow: Some(base_glow.clone()),
        extras: vec![
            coursecraft_course::StateExtra {
                id: "ex_shape".to_owned(),
                name: "Badge".to_owned(),
                kind: coursecraft_course::StateExtraKind::Shape {
                    shape: coursecraft_course::ShapeKind::Ellipse,
                    fill: coursecraft_course::Color("#FF0000".to_owned()),
                    fill_style: None,
                    outline: Some(base_outline.clone()),
                    text: Some(base_shape_text),
                    adjust: vec![],
                },
                frame: coursecraft_course::Rect {
                    x: -10.0,
                    y: -20.0,
                    w: 40.0,
                    h: 30.0,
                },
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: Some(base_shadow),
                glow: Some(base_glow),
            },
            coursecraft_course::StateExtra {
                id: "ex_img".to_owned(),
                name: "Icon".to_owned(),
                kind: coursecraft_course::StateExtraKind::Image {
                    src: "media/icon.png".to_owned(),
                    alt: String::new(),
                    crop: None,
                    outline: Some(base_outline.clone()),
                    shape: None,
                },
                frame: coursecraft_course::Rect {
                    x: 10.0,
                    y: 20.0,
                    w: 24.0,
                    h: 24.0,
                },
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
            },
            coursecraft_course::StateExtra {
                id: "ex_txt".to_owned(),
                name: "Tag".to_owned(),
                kind: coursecraft_course::StateExtraKind::Text {
                    text: "Hi".to_owned(),
                    style: base_text_style,
                    runs: base_runs,
                    paragraphs: base_paragraphs,
                    autofit: coursecraft_course::Autofit::None,
                    outline: Some(base_outline),
                    point_text: false,
                },
                frame: coursecraft_course::Rect {
                    x: 0.0,
                    y: 4.0,
                    w: 60.0,
                    h: 20.0,
                },
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
            },
            coursecraft_course::StateExtra {
                id: "ex_path".to_owned(),
                name: "Arrow".to_owned(),
                kind: coursecraft_course::StateExtraKind::Path { path: path_struct },
                frame: coursecraft_course::Rect {
                    x: 8.0,
                    y: 12.0,
                    w: 50.0,
                    h: 50.0,
                },
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
            },
        ],
        ..coursecraft_course::StateDef::default()
    }
}

fn assert_scaled_state_def(st: &coursecraft_course::StateDef) {
    assert_eq!(st.outline.as_ref().unwrap().width, 6.0);
    let ts = st.text_style.as_ref().unwrap();
    assert_eq!(ts.size, 30.0);
    assert_eq!(ts.letter_spacing, 6.0);
    let ins = ts.insets.unwrap();
    assert_eq!(
        (ins.left, ins.top, ins.right, ins.bottom),
        (6.0, 9.0, 12.0, 15.0)
    );
    assert_eq!(st.runs.as_ref().unwrap()[0].size, Some(24.0));
    assert_eq!(st.runs.as_ref().unwrap()[0].letter_spacing, Some(3.0));
    assert_eq!(st.paragraphs.as_ref().unwrap()[0].space_before, 15.0);
    assert_eq!(st.paragraphs.as_ref().unwrap()[0].space_after, 30.0);
    assert_eq!(st.paragraphs.as_ref().unwrap()[0].bullet_size, Some(18.0));
    let sh = st.shadow.as_ref().unwrap();
    assert_eq!((sh.offset_x, sh.offset_y, sh.blur), (9.0, -12.0, 18.0));
    assert_eq!(st.glow.as_ref().unwrap().size, 15.0);
}

fn assert_scaled_state_extras(extras: &[coursecraft_course::StateExtra]) {
    let ex0 = &extras[0];
    assert_eq!(
        ex0.frame,
        coursecraft_course::Rect {
            x: -15.0,
            y: -30.0,
            w: 60.0,
            h: 45.0
        }
    );
    assert_eq!(ex0.shadow.as_ref().unwrap().blur, 18.0);
    assert_eq!(ex0.glow.as_ref().unwrap().size, 15.0);
    if let coursecraft_course::StateExtraKind::Shape { outline, text, .. } = &ex0.kind {
        assert_eq!(outline.as_ref().unwrap().width, 6.0);
        let t = text.as_ref().unwrap();
        assert_eq!(t.style.size, 30.0);
        assert_eq!(t.runs[0].letter_spacing, Some(3.0));
        assert_eq!(t.paragraphs[0].bullet_size, Some(18.0));
    } else {
        panic!("expected shape extra");
    }
    if let coursecraft_course::StateExtraKind::Image { outline, .. } = &extras[1].kind {
        assert_eq!(outline.as_ref().unwrap().width, 6.0);
    } else {
        panic!("expected image extra");
    }
    if let coursecraft_course::StateExtraKind::Text {
        style,
        runs,
        paragraphs,
        outline,
        ..
    } = &extras[2].kind
    {
        assert_eq!(style.size, 30.0);
        assert_eq!(style.letter_spacing, 6.0);
        assert_eq!(runs[0].size, Some(24.0));
        assert_eq!(paragraphs[0].bullet_size, Some(18.0));
        assert_eq!(outline.as_ref().unwrap().width, 6.0);
    } else {
        panic!("expected text extra");
    }
    if let coursecraft_course::StateExtraKind::Path { path } = &extras[3].kind {
        assert_eq!(path.outline.width, 6.0);
        assert_eq!(path.text.as_ref().unwrap().style.size, 30.0);
    } else {
        panic!("expected path extra");
    }
}

fn assert_scaled_state_and_extras(st: &coursecraft_course::StateDef) {
    assert_scaled_state_def(st);
    assert_scaled_state_extras(&st.extras);
}

#[test]
fn set_slide_size_scale_scales_states_and_state_extras() {
    // A-02 (scale mode): rich-states fixture + synthetic fixture covering StateDef & all 4 StateExtraKind variants.
    let rich_json = include_str!("../../../examples/rich-states/course.json");
    let rich_course = coursecraft_course::Course::from_json(rich_json).unwrap();
    let mut rich = Session::new(rich_course);
    let rich_before = rich.course.to_json().unwrap();
    ok(
        &mut rich,
        "course.set_slide_size",
        json!({ "w": 1920, "h": 1080, "mode": "scale" }),
    );
    assert_no_errors(&rich);
    rich.undo().unwrap();
    assert_eq!(rich.course.to_json().unwrap(), rich_before);

    // Non-16:9 target (1280x720 -> 1920x1620 => factor=1.5, dy=270.0) verifies parent-relative
    // StateExtra.frame scales by factor without adding +dy.
    let mut s = sized_course();
    s.course.scenes[0].slides[0].objects[0]
        .states
        .push(build_test_state_with_all_extras());
    let before = s.course.to_json().unwrap();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1920, "h": 1620, "mode": "scale" }),
    );
    let st = &s
        .course
        .slide("slide1")
        .unwrap()
        .object("t")
        .unwrap()
        .0
        .states[0];
    assert_scaled_state_and_extras(st);
    assert_no_errors(&s);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);

    // "fill" mode on 1280x720 -> 1440x1080 => factor=max(1.125, 1.5)=1.5, dx=-240.0, dy=0.0.
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1440, "h": 1080, "mode": "fill" }),
    );
    let st_fill = &s
        .course
        .slide("slide1")
        .unwrap()
        .object("t")
        .unwrap()
        .0
        .states[0];
    assert_scaled_state_and_extras(st_fill);
    assert_no_errors(&s);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

fn populate_missed_base_fields(s: &mut Session, crop: coursecraft_course::CropRect) {
    let outline = coursecraft_course::Outline {
        color: coursecraft_course::Color("#112233".to_owned()),
        width: 4.0,
        dash: coursecraft_course::DashStyle::Solid,
        dash_pattern: None,
        join: coursecraft_course::LineJoin::Miter,
        miter_limit: None,
    };
    let text_style = coursecraft_course::TextStyle {
        size: 20.0,
        letter_spacing: 4.0,
        insets: Some(coursecraft_course::TextInsets {
            left: 2.0,
            top: 4.0,
            right: 6.0,
            bottom: 8.0,
        }),
        ..coursecraft_course::TextStyle::default()
    };
    let runs = vec![coursecraft_course::TextRun {
        text: "Ab".to_owned(),
        bold: None,
        italic: None,
        underline: None,
        strikethrough: None,
        baseline: None,
        letter_spacing: Some(2.0),
        highlight: None,
        shadow: None,
        size: Some(16.0),
        color: None,
        link: None,
        font: None,
    }];
    let paragraphs = vec![coursecraft_course::Paragraph {
        space_before: 8.0,
        space_after: 12.0,
        bullet_size: Some(14.0),
        ..coursecraft_course::Paragraph::default()
    }];
    if let ObjectKind::Text {
        style,
        runs: r,
        paragraphs: p,
        outline: o,
        ..
    } = &mut s.course.scenes[0].slides[0].objects[0].kind
    {
        *style = text_style.clone();
        *r = runs.clone();
        *p = paragraphs.clone();
        *o = Some(outline.clone());
    }
    let slide = &mut s.course.scenes[0].slides[0];
    let mut base_obj = slide.objects[0].clone();
    base_obj.shadow = None;
    base_obj.motion_paths.clear();
    slide.objects.push(coursecraft_course::Object {
        id: "img1".to_owned(),
        name: "Image".to_owned(),
        kind: ObjectKind::Image {
            src: "media/a.png".to_owned(),
            alt: String::new(),
            crop: Some(crop),
            outline: Some(outline.clone()),
            shape: None,
        },
        frame: coursecraft_course::Rect {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
        },
        ..base_obj.clone()
    });
    slide.objects.push(coursecraft_course::Object {
        id: "path1".to_owned(),
        name: "Path".to_owned(),
        kind: ObjectKind::Path {
            path: coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: true,
                subpaths: vec![],
                fill: Some(coursecraft_course::Color("#112233".to_owned())),
                fill_style: None,
                outline,
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
                text: Some(coursecraft_course::ShapeText {
                    text: "Ab".to_owned(),
                    style: text_style.clone(),
                    runs: runs.clone(),
                    paragraphs: paragraphs.clone(),
                    autofit: coursecraft_course::Autofit::None,
                }),
            },
        },
        frame: coursecraft_course::Rect {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
        },
        ..base_obj.clone()
    });
    slide.objects.push(coursecraft_course::Object {
        id: "timer1".to_owned(),
        name: "Timer".to_owned(),
        kind: ObjectKind::Timer {
            duration_ms: 10_000,
            format: coursecraft_course::TimerFormat::MmSs,
            autostart: true,
            style: text_style.clone(),
        },
        frame: coursecraft_course::Rect {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 40.0,
        },
        ..base_obj.clone()
    });
    slide.objects.push(coursecraft_course::Object {
        id: "marker1".to_owned(),
        name: "Marker".to_owned(),
        kind: ObjectKind::Marker {
            title: "M".to_owned(),
            body: "Ab".to_owned(),
            runs,
            paragraphs: paragraphs.clone(),
            icon: coursecraft_course::MarkerIcon::Info,
            color: None,
        },
        frame: coursecraft_course::Rect {
            x: 10.0,
            y: 10.0,
            w: 32.0,
            h: 32.0,
        },
        ..base_obj
    });
    s.course
        .text_styles
        .push(coursecraft_course::NamedTextStyle {
            id: "body".to_owned(),
            name: "Body".to_owned(),
            style: text_style,
            paragraphs: Some(paragraphs[0].clone()),
        });
}

fn assert_scaled_missed_base_fields(s: &Session, crop: coursecraft_course::CropRect) {
    let sl = s.course.slide("slide1").unwrap();
    if let ObjectKind::Text {
        style,
        runs,
        paragraphs,
        outline,
        ..
    } = &sl.object("t").unwrap().0.kind
    {
        assert_eq!(style.letter_spacing, 6.0);
        let ins = style.insets.unwrap();
        assert_eq!(
            (ins.left, ins.top, ins.right, ins.bottom),
            (3.0, 6.0, 9.0, 12.0)
        );
        assert_eq!(runs[0].letter_spacing, Some(3.0));
        assert_eq!(paragraphs[0].bullet_size, Some(21.0));
        assert_eq!(outline.as_ref().unwrap().width, 6.0);
    } else {
        panic!("expected text");
    }
    if let ObjectKind::Image {
        crop: c, outline, ..
    } = &sl.object("img1").unwrap().0.kind
    {
        assert_eq!(c.unwrap(), crop);
        assert_eq!(outline.as_ref().unwrap().width, 6.0);
    } else {
        panic!("expected image");
    }
    assert_scaled_path_timer_marker_and_named(s, sl);
}

fn assert_scaled_path_timer_marker_and_named(s: &Session, sl: &coursecraft_course::Slide) {
    if let ObjectKind::Path { path } = &sl.object("path1").unwrap().0.kind {
        let pt = path.text.as_ref().unwrap();
        assert_eq!(pt.style.size, 30.0);
        assert_eq!(pt.style.letter_spacing, 6.0);
        assert_eq!(pt.runs[0].letter_spacing, Some(3.0));
        assert_eq!(pt.paragraphs[0].bullet_size, Some(21.0));
    } else {
        panic!("expected path");
    }
    if let ObjectKind::Timer { style, .. } = &sl.object("timer1").unwrap().0.kind {
        assert_eq!(style.size, 30.0);
        assert_eq!(style.letter_spacing, 6.0);
        assert_eq!(style.insets.unwrap().left, 3.0);
    } else {
        panic!("expected timer");
    }
    if let ObjectKind::Marker {
        runs, paragraphs, ..
    } = &sl.object("marker1").unwrap().0.kind
    {
        assert_eq!(runs[0].size, Some(24.0));
        assert_eq!(runs[0].letter_spacing, Some(3.0));
        assert_eq!(paragraphs[0].bullet_size, Some(21.0));
    } else {
        panic!("expected marker");
    }
    let named = &s.course.text_styles[0];
    assert_eq!(named.style.letter_spacing, 6.0);
    assert_eq!(named.style.insets.unwrap().bottom, 12.0);
    assert_eq!(named.paragraphs.as_ref().unwrap().bullet_size, Some(21.0));
}

#[test]
fn set_slide_size_scale_scales_previously_missed_base_fields() {
    // A-03: Text.outline, Image.outline (crop untouched), Path.text, Timer.style,
    // Marker.runs/paragraphs, TextStyle.{letter_spacing, insets}, TextRun.letter_spacing,
    // Paragraph.bullet_size, and NamedTextStyle.{letter_spacing, insets, bullet_size}.
    for mode in ["scale", "fill"] {
        let mut s = sized_course();
        let crop = coursecraft_course::CropRect {
            x: 0.1,
            y: 0.2,
            w: 0.5,
            h: 0.6,
        };
        populate_missed_base_fields(&mut s, crop);
        ok(
            &mut s,
            "course.set_slide_size",
            json!({ "w": 1920, "h": 1080, "mode": mode }),
        );
        assert_scaled_missed_base_fields(&s, crop);
        assert_no_errors(&s);
    }
}

#[test]
fn set_slide_size_extreme_aspects_and_clamps_stay_valid() {
    // A-04 (scale, fill, and keep modes + RISK-02 clamps)
    for (w, h) in [(100, 4320), (7680, 100)] {
        for mode in ["scale", "fill", "keep"] {
            let mut s = sized_course();
            let before = s.course.to_json().unwrap();
            ok(
                &mut s,
                "course.set_slide_size",
                json!({ "w": w, "h": h, "mode": mode }),
            );
            assert_no_errors(&s);
            s.undo().unwrap();
            assert_eq!(s.course.to_json().unwrap(), before);
        }
    }

    // Extreme scale factor (100x100 -> 7680x4320, factor = 43.2) clamps frame w/h, coords, motion points, and guides.
    let mut s = sized_course();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 100, "h": 100, "mode": "scale" }),
    );
    s.course.scenes[0].slides[0].objects[0].frame = coursecraft_course::Rect {
        x: 90_000.0,
        y: -90_000.0,
        w: 90_000.0,
        h: 90_000.0,
    };
    s.course.scenes[0].slides[0].objects[0].motion_paths[0].points = vec![
        coursecraft_course::Point {
            x: 90_000.0,
            y: -90_000.0,
        },
        coursecraft_course::Point {
            x: 95_000.0,
            y: 95_000.0,
        },
    ];
    s.course.editor.guides.push(coursecraft_course::Guide {
        vertical: true,
        at: 90_000.0,
    });
    let before = s.course.to_json().unwrap();
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 7680, "h": 4320, "mode": "scale" }),
    );
    assert_no_errors(&s);
    let obj = s.course.slide("slide1").unwrap().object("t").unwrap().0;
    assert_eq!(obj.frame.x, 100_000.0);
    assert_eq!(obj.frame.y, -100_000.0);
    assert_eq!(obj.frame.w, coursecraft_course::MAX_OBJECT_FRAME_SIZE);
    assert_eq!(obj.frame.h, coursecraft_course::MAX_OBJECT_FRAME_SIZE);
    assert_eq!(obj.motion_paths[0].points[0].x, 100_000.0);
    assert_eq!(obj.motion_paths[0].points[0].y, -100_000.0);
    assert_eq!(s.course.editor.guides[0].at, 100_000.0);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);

    // Extreme "fill" factor (100x100 -> 7680x4320, factor = 76.8) also clamps and round-trips.
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 7680, "h": 4320, "mode": "fill" }),
    );
    assert_no_errors(&s);
    let obj_fill = s.course.slide("slide1").unwrap().object("t").unwrap().0;
    assert_eq!(obj_fill.frame.x, 100_000.0);
    assert_eq!(obj_fill.frame.y, -100_000.0);
    assert_eq!(obj_fill.frame.w, coursecraft_course::MAX_OBJECT_FRAME_SIZE);
    assert_eq!(obj_fill.frame.h, coursecraft_course::MAX_OBJECT_FRAME_SIZE);
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn set_slide_size_scale_scales_guides_paragraph_spacing_and_text_styles() {
    let mut s = sized_course();
    s.course.editor.guides = vec![
        coursecraft_course::Guide {
            vertical: true,
            at: 200.0,
        },
        coursecraft_course::Guide {
            vertical: false,
            at: 100.0,
        },
    ];
    s.course
        .text_styles
        .push(coursecraft_course::NamedTextStyle {
            id: "h1".to_owned(),
            name: "Heading".to_owned(),
            style: coursecraft_course::TextStyle {
                size: 40.0,
                ..coursecraft_course::TextStyle::default()
            },
            paragraphs: Some(coursecraft_course::Paragraph {
                space_before: 20.0,
                space_after: 10.0,
                ..coursecraft_course::Paragraph::default()
            }),
        });
    if let ObjectKind::Text { paragraphs, .. } = &mut s.course.scenes[0].slides[0].objects[0].kind {
        paragraphs.push(coursecraft_course::Paragraph {
            space_before: 16.0,
            space_after: 8.0,
            ..coursecraft_course::Paragraph::default()
        });
    }
    ok(
        &mut s,
        "course.set_slide_size",
        json!({ "w": 1000, "h": 1000, "mode": "scale" }),
    );
    let f = 0.78125_f32;
    let dy = (1000.0 - 720.0 * f) / 2.0;
    assert_eq!(s.course.editor.guides[0].at, (200.0 * f).round());
    assert_eq!(s.course.editor.guides[1].at, (100.0 * f + dy).round());
    let named = &s.course.text_styles[0];
    assert!((named.style.size - 40.0 * f).abs() < 0.01);
    let np = named.paragraphs.as_ref().unwrap();
    assert!((np.space_before - 20.0 * f).abs() < 0.01);
    assert!((np.space_after - 10.0 * f).abs() < 0.01);
    if let ObjectKind::Text { paragraphs, .. } = &s.course.scenes[0].slides[0].objects[0].kind {
        assert!((paragraphs[0].space_before - 16.0 * f).abs() < 0.01);
        assert!((paragraphs[0].space_after - 8.0 * f).abs() < 0.01);
    } else {
        panic!("expected text object");
    }
    assert_no_errors(&s);
}

fn assert_no_errors(s: &Session) {
    let errors: Vec<_> = s
        .course
        .validate()
        .into_iter()
        .filter(|issue| issue.severity == coursecraft_course::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn text_style_range_applies_strikethrough_baseline_and_spacing_with_undo() {
    let mut s = Session::blank("Effects");
    let id = ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "text", "text": "Hello world" },
                "frame": { "x": 10, "y": 10, "w": 300, "h": 60 } }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = s.course.to_json().unwrap();
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 5,
                "strikethrough": true, "baseline": "superscript", "letter_spacing": 2.5 }),
    );
    match &s
        .course
        .slide("slide1")
        .unwrap()
        .object(&id)
        .unwrap()
        .0
        .kind
    {
        ObjectKind::Text { runs, .. } => {
            assert_eq!(runs[0].text, "Hello");
            assert_eq!(runs[0].strikethrough, Some(true));
            assert_eq!(
                runs[0].baseline,
                Some(coursecraft_course::Baseline::Superscript)
            );
            assert_eq!(runs[0].letter_spacing, Some(2.5));
            assert_eq!(runs[1].strikethrough, None);
        }
        other => panic!("{other:?}"),
    }
    assert!(s.execute(
        "text.style_range",
        &json!({ "slide": "slide1", "object": id, "start": 0, "end": 5, "baseline": "sideways" })
    )
    .is_err());
    s.undo().unwrap();
    assert_eq!(s.course.to_json().unwrap(), before);
}

#[test]
fn course_clear_slide_player_overrides_clears_one_field_or_all_with_undo() {
    use coursecraft_course::NextRequirement;
    let mut s = Session::blank("Title");
    let slide = s.course.slides().next().unwrap().id.clone();
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "next_requires": { "kind": "timeline_ends" }, "show_menu": false }),
    );
    let out = ok(
        &mut s,
        "course.clear_slide_player_overrides",
        json!({ "field": "next_requires" }),
    );
    assert_eq!(out["cleared"], 1);
    let overrides = s.course.slide(&slide).unwrap().player.clone().unwrap();
    assert_eq!(overrides.next_requires, None);
    assert_eq!(overrides.show_menu, Some(false));
    s.undo().unwrap();
    assert_eq!(
        s.course
            .slide(&slide)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .next_requires,
        Some(NextRequirement::TimelineEnds)
    );
    ok(&mut s, "course.clear_slide_player_overrides", json!({}));
    assert!(s.course.slide(&slide).unwrap().player.is_none());
    assert!(
        s.execute(
            "course.clear_slide_player_overrides",
            &json!({ "field": "logo" })
        )
        .is_err()
    );
}

#[test]
fn course_set_player_sets_navigation_and_next_rule_with_undo() {
    use coursecraft_course::{NavMode, NextRequirement};
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "done", "value": false }),
    );
    ok(
        &mut s,
        "course.set_player",
        json!({ "navigation": "restricted", "next_requires": { "kind": "timeline_ends" } }),
    );
    assert_eq!(s.course.player.navigation, NavMode::Restricted);
    assert_eq!(s.course.player.next_requires, NextRequirement::TimelineEnds);
    ok(
        &mut s,
        "course.set_player",
        json!({ "navigation": "locked", "next_requires": { "kind": "variable", "name": "done" } }),
    );
    assert_eq!(s.course.player.navigation, NavMode::Locked);
    assert_eq!(
        s.course.player.next_requires,
        NextRequirement::Variable {
            name: "done".into()
        }
    );
    s.undo().unwrap();
    assert_eq!(s.course.player.navigation, NavMode::Restricted);
    assert_eq!(s.course.player.next_requires, NextRequirement::TimelineEnds);
    assert!(
        s.execute("course.set_player", &json!({ "navigation": "sideways" }))
            .is_err()
    );
}

#[test]
fn slide_set_player_overrides_validates_variables_and_clears() {
    use coursecraft_course::NextRequirement;
    let mut s = Session::blank("Title");
    let slide = s.course.slides().next().unwrap().id.clone();
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "done", "value": false }),
    );
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "count", "value": 3 }),
    );
    for bad in [
        json!({ "kind": "variable", "name": "ghost" }),
        json!({ "kind": "variable", "name": "count" }),
    ] {
        let before = s.course.clone();
        assert!(
            s.execute(
                "slide.set_player_overrides",
                &json!({ "slide": slide, "next_requires": bad })
            )
            .is_err()
        );
        assert_eq!(s.course, before);
    }
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "next_requires": { "kind": "variable", "name": "done" } }),
    );
    let rule = s
        .course
        .slide(&slide)
        .unwrap()
        .player
        .clone()
        .unwrap()
        .next_requires;
    assert_eq!(
        rule,
        Some(NextRequirement::Variable {
            name: "done".into()
        })
    );
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "next_requires": null }),
    );
    assert!(s.course.slide(&slide).unwrap().player.is_none());
    s.undo().unwrap();
    assert!(s.course.slide(&slide).unwrap().player.is_some());
}

#[test]
fn course_set_player_toggles_speed_and_fullscreen() {
    let mut s = Session::blank("Title");
    assert!(s.course.player.show_speed && s.course.player.show_fullscreen);
    ok(
        &mut s,
        "course.set_player",
        json!({ "show_speed": false, "show_fullscreen": false }),
    );
    assert!(!s.course.player.show_speed);
    assert!(!s.course.player.show_fullscreen);
    s.undo().unwrap();
    assert!(s.course.player.show_speed && s.course.player.show_fullscreen);
}

#[test]
fn resource_commands_add_update_move_remove_with_undo() {
    use coursecraft_course::ResourceTarget;
    let mut s = Session::blank("Title");
    let a = ok(
        &mut s,
        "resource.add",
        json!({ "title": "Guide", "target": { "kind": "file", "src": "guide.pdf" } }),
    );
    let a = a["id"].as_str().unwrap().to_owned();
    let b = ok(
        &mut s,
        "resource.add",
        json!({ "title": "Site", "target": { "kind": "url", "url": "https://example.com" } }),
    );
    let b = b["id"].as_str().unwrap().to_owned();
    assert_eq!(s.course.resources.len(), 2);
    ok(
        &mut s,
        "resource.update",
        json!({ "id": b, "title": "Website", "target": { "kind": "url", "url": "mailto:a@b.co" } }),
    );
    assert_eq!(s.course.resources[1].title, "Website");
    assert_eq!(
        s.course.resources[1].target,
        ResourceTarget::Url {
            url: "mailto:a@b.co".into()
        }
    );
    ok(&mut s, "resource.move", json!({ "id": b, "to": 0 }));
    assert_eq!(s.course.resources[0].id, b);
    s.undo().unwrap();
    assert_eq!(s.course.resources[0].id, a);
    ok(&mut s, "resource.remove", json!({ "id": a }));
    assert_eq!(s.course.resources.len(), 1);
    s.undo().unwrap();
    assert_eq!(s.course.resources.len(), 2);
}

#[test]
fn resource_commands_reject_bad_input_and_leave_the_course_alone() {
    let mut s = Session::blank("Title");
    let before = s.course.clone();
    for params in [
        json!({ "title": "", "target": { "kind": "url", "url": "https://example.com" } }),
        json!({ "title": "x", "target": { "kind": "url", "url": "javascript:alert(1)" } }),
        json!({ "title": "x", "target": { "kind": "file", "src": "../up.pdf" } }),
    ] {
        assert!(s.execute("resource.add", &params).is_err(), "{params}");
        assert_eq!(s.course, before);
    }
    assert!(
        s.execute("resource.update", &json!({ "id": "ghost", "title": "x" }))
            .is_err()
    );
    assert!(
        s.execute("resource.move", &json!({ "id": "ghost", "to": 0 }))
            .is_err()
    );
}

#[test]
fn glossary_commands_add_update_move_remove_with_undo() {
    let mut s = Session::blank("Title");
    let a = ok(
        &mut s,
        "glossary.add",
        json!({ "term": "Zebra", "definition": "An animal." }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = ok(&mut s, "glossary.add", json!({ "term": "Apple" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Stored in author order, whatever the display order is.
    assert_eq!(s.course.glossary[0].term, "Zebra");
    ok(
        &mut s,
        "glossary.update",
        json!({ "id": b, "definition": "A fruit." }),
    );
    assert_eq!(s.course.glossary[1].definition, "A fruit.");
    ok(&mut s, "glossary.move", json!({ "id": b, "to": 0 }));
    assert_eq!(s.course.glossary[0].id, b);
    assert!(
        s.execute("glossary.move", &json!({ "id": b, "to": 9 }))
            .is_err()
    );
    assert!(s.execute("glossary.add", &json!({ "term": "  " })).is_err());
    assert!(
        s.execute("glossary.update", &json!({ "id": a, "term": "" }))
            .is_err()
    );
    ok(&mut s, "glossary.remove", json!({ "id": a }));
    assert_eq!(s.course.glossary.len(), 1);
    s.undo().unwrap();
    assert_eq!(s.course.glossary.len(), 2);
}

#[test]
fn glossary_sort_sorts_case_insensitively_with_undo_and_skips_when_already_sorted() {
    // C-05: glossary.sort sorts ["Zebra", "alpha", "Beta"] -> ["alpha", "Beta", "Zebra"]
    // in one undo step, and sets skip_undo = true when already sorted.
    let mut s = Session::blank("Glossary Sort");
    ok(&mut s, "glossary.add", json!({ "term": "Zebra" }));
    ok(&mut s, "glossary.add", json!({ "term": "alpha" }));
    ok(&mut s, "glossary.add", json!({ "term": "Beta" }));

    let before: Vec<String> = s.course.glossary.iter().map(|g| g.term.clone()).collect();
    assert_eq!(before, vec!["Zebra", "alpha", "Beta"]);

    ok(&mut s, "glossary.sort", json!({}));
    let sorted: Vec<String> = s.course.glossary.iter().map(|g| g.term.clone()).collect();
    assert_eq!(sorted, vec!["alpha", "Beta", "Zebra"]);

    // Sorting an already-sorted list is a no-op with skip_undo = true,
    // so edit.undo still undoes the earlier sort in one step.
    ok(&mut s, "glossary.sort", json!({}));
    assert_eq!(
        s.course
            .glossary
            .iter()
            .map(|g| g.term.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "Beta", "Zebra"]
    );

    s.undo().unwrap();
    let undone: Vec<String> = s.course.glossary.iter().map(|g| g.term.clone()).collect();
    assert_eq!(undone, vec!["Zebra", "alpha", "Beta"]);

    s.redo().unwrap();
    let redone: Vec<String> = s.course.glossary.iter().map(|g| g.term.clone()).collect();
    assert_eq!(redone, vec!["alpha", "Beta", "Zebra"]);

    // On a fresh already-sorted session, glossary.sort pushes nothing to the undo stack.
    let mut s2 = Session::blank("Already Sorted");
    ok(&mut s2, "glossary.add", json!({ "term": "Alpha" }));
    ok(&mut s2, "glossary.add", json!({ "term": "Beta" }));
    s2.execute("glossary.sort", &json!({})).unwrap();
    assert_eq!(s2.course.glossary.len(), 2);
    // Undoing now undoes the second glossary.add rather than an empty sort step.
    s2.undo().unwrap();
    assert_eq!(s2.course.glossary.len(), 1);
}

#[test]
fn course_set_player_toggles_the_transcript() {
    let mut s = Session::blank("Title");
    assert!(!s.course.player.show_transcript);
    ok(
        &mut s,
        "course.set_player",
        json!({ "show_transcript": true }),
    );
    assert!(s.course.player.show_transcript);
    s.undo().unwrap();
    assert!(!s.course.player.show_transcript);
}

#[test]
fn course_set_player_sets_title_volume_captions_and_logo_with_undo() {
    let mut s = Session::blank("Title");
    ok(
        &mut s,
        "course.set_player",
        json!({ "show_title": false, "show_volume": false, "show_captions": false, "logo": "logo.png" }),
    );
    let player = &s.course.player;
    assert!(!player.show_title && !player.show_volume && !player.show_captions);
    assert_eq!(player.logo.as_deref(), Some("logo.png"));
    ok(&mut s, "course.set_player", json!({ "logo": null }));
    assert!(s.course.player.logo.is_none());
    s.undo().unwrap();
    assert_eq!(s.course.player.logo.as_deref(), Some("logo.png"));
    s.undo().unwrap();
    assert!(s.course.player.show_title && s.course.player.logo.is_none());
}

#[test]
fn course_set_player_rejects_a_logo_that_is_not_an_image() {
    let mut s = Session::blank("Title");
    let before = s.course.clone();
    for bad in ["narration.mp3", "../logo.png", "doc.pdf", ""] {
        assert!(
            s.execute("course.set_player", &json!({ "logo": bad }))
                .is_err(),
            "{bad}"
        );
        assert_eq!(s.course, before);
    }
}

#[test]
fn slide_set_player_overrides_sets_and_clears_the_show_flags() {
    let mut s = Session::blank("Title");
    let slide = s.course.slides().next().unwrap().id.clone();
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "show_nav": false, "show_seekbar": true, "show_menu": false }),
    );
    let player = s.course.slide(&slide).unwrap().player.clone().unwrap();
    assert_eq!(
        (player.show_nav, player.show_seekbar, player.show_menu),
        (Some(false), Some(true), Some(false))
    );
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "show_nav": null }),
    );
    let player = s.course.slide(&slide).unwrap().player.clone().unwrap();
    assert_eq!(player.show_nav, None);
    assert_eq!(player.show_seekbar, Some(true));
    assert!(
        s.execute(
            "slide.set_player_overrides",
            &json!({ "slide": slide, "show_menu": "yes" })
        )
        .is_err()
    );
    s.undo().unwrap();
    assert_eq!(
        s.course
            .slide(&slide)
            .unwrap()
            .player
            .clone()
            .unwrap()
            .show_nav,
        Some(false)
    );
}

#[test]
fn course_set_player_toggles_show_bar_with_undo() {
    let mut s = Session::blank("Title");
    assert!(s.course.player.show_bar);
    ok(&mut s, "course.set_player", json!({ "show_bar": false }));
    assert!(!s.course.player.show_bar);
    s.undo().unwrap();
    assert!(s.course.player.show_bar);
}

#[test]
fn slide_set_player_overrides_sets_and_clears_show_bar_with_undo() {
    let mut s = Session::blank("Title");
    let slide = s.course.slides().next().unwrap().id.clone();
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "show_bar": false }),
    );
    assert_eq!(
        s.course
            .slide(&slide)
            .unwrap()
            .player
            .as_ref()
            .and_then(|o| o.show_bar),
        Some(false)
    );
    ok(
        &mut s,
        "slide.set_player_overrides",
        json!({ "slide": slide, "show_bar": null }),
    );
    assert!(s.course.slide(&slide).unwrap().player.is_none());
    s.undo().unwrap();
    assert_eq!(
        s.course
            .slide(&slide)
            .unwrap()
            .player
            .as_ref()
            .and_then(|o| o.show_bar),
        Some(false)
    );
}

/// Flip cards keep one text colour for both sides, so it must read on the
/// front fill and on the back state's fill (composited over the slide), for
/// the default theme and a dark-accent theme.
#[test]
fn interaction_flip_cards_text_reads_on_both_sides() {
    for accent in [None, Some("#2F6F6A"), Some("#F4C542")] {
        let mut s = Session::blank("Flip contrast");
        if let Some(accent) = accent {
            ok(&mut s, "course.set_theme", json!({ "accent": accent }));
        }
        ok(
            &mut s,
            "interaction.insert",
            json!({ "slide": "slide1", "kind": "flip_cards" }),
        );
        let backdrop =
            coursecraft_course::parse_color(&s.course.theme.background).expect("background");
        let slide = s.course.slide("slide1").expect("slide");
        for object in &slide.objects {
            let ObjectKind::Shape {
                fill,
                text: Some(text),
                ..
            } = &object.kind
            else {
                panic!("a flip card is a shape with text");
            };
            let ink = coursecraft_course::parse_color(&text.style.color).expect("text colour");
            let front = coursecraft_course::parse_color(fill)
                .expect("front")
                .over(backdrop);
            let back_fill = object
                .states
                .iter()
                .find(|st| st.name == "Back")
                .and_then(|st| st.fill.clone())
                .expect("back fill");
            let back = coursecraft_course::parse_color(&back_fill)
                .expect("back")
                .over(backdrop);
            for (side, colour) in [("front", front), ("back", back)] {
                let ratio = coursecraft_course::contrast_ratio(ink, colour);
                assert!(
                    ratio >= 4.5,
                    "{side} of {} with accent {accent:?}: contrast {ratio:.2}",
                    object.id
                );
            }
        }
    }
}

/// Process-step labels sit in boxes centred on their circles, so the text is
/// centred too.
#[test]
fn interaction_process_step_labels_are_centred() {
    let mut s = Session::blank("Steps");
    ok(
        &mut s,
        "interaction.insert",
        json!({ "slide": "slide1", "kind": "process_steps" }),
    );
    let slide = s.course.slide("slide1").expect("slide");
    let labels: Vec<_> = slide
        .objects
        .iter()
        .filter(|o| o.id.starts_with("step_label"))
        .collect();
    assert_eq!(labels.len(), 4);
    for label in labels {
        let ObjectKind::Text { style, .. } = &label.kind else {
            panic!("a label is text");
        };
        assert_eq!(
            style.align,
            coursecraft_course::Align::Center,
            "{}",
            label.id
        );
    }
}

#[test]
fn slide_update_stores_speaker_notes() {
    let mut s = Session::blank("Notes");
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "notes": "Say hello first." }),
    );
    assert_eq!(s.course.slide("slide1").unwrap().notes, "Say hello first.");
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "notes": "" }),
    );
    assert_eq!(
        s.course.slide("slide1").unwrap().notes,
        "",
        "notes can be cleared"
    );
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "title": "Intro" }),
    );
    assert_eq!(s.course.slide("slide1").unwrap().title, "Intro");
}

#[test]
fn a_command_that_changes_nothing_adds_no_undo_step() {
    let mut s = Session::blank("Same");
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "title": "Intro" }),
    );
    let revision = s.revision;
    ok(
        &mut s,
        "slide.update",
        json!({ "slide": "slide1", "title": "Intro" }),
    );
    assert_eq!(s.revision, revision, "same value: no new revision");
    ok(&mut s, "edit.undo", json!({}));
    assert_ne!(
        s.course.slide("slide1").unwrap().title,
        "Intro",
        "one Undo goes back past the real change"
    );
    let batch = s.execute_batch(&[(
        "slide.update".to_owned(),
        json!({ "slide": "slide1", "title": s.course.slide("slide1").unwrap().title.clone() }),
    )]);
    assert!(batch.is_ok());
    assert!(
        !s.can_undo(),
        "a batch that changes nothing adds no undo step"
    );
}

#[test]
fn set_scroll_panel_with_tall_frame_does_not_panic() {
    let mut s = Session::blank("Scroll");
    let added = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 20000.4 },
            "kind": { "type": "scroll_panel", "content_height": 20000.0 }
        }),
    );
    let id = added["id"].as_str().expect("id").to_owned();
    ok(
        &mut s,
        "object.set_scroll_panel",
        json!({
            "slide": "slide1",
            "object": id,
            "patch": { "content_height": 500.0 }
        }),
    );
}

#[test]
fn bank_slide_object_delete_removes_dangling_triggers() {
    let mut s = Session::blank("Bank");
    let bank = bank_id(&mut s);
    let qid = ok(
        &mut s,
        "bank.add_question",
        json!({ "bank": bank, "title": "Q1" }),
    )["id"]
        .as_str()
        .expect("question slide id")
        .to_owned();
    let submit_id = s
        .course
        .bank_slide(&qid)
        .expect("bank slide")
        .objects
        .iter()
        .find(|o| o.name == "Submit")
        .map(|o| o.id.clone())
        .expect("submit button");

    ok(
        &mut s,
        "object.delete",
        json!({ "slide": qid, "object": submit_id }),
    );
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());
}

#[test]
fn slide_duplicate_remaps_panel_and_drag_targets() {
    let mut s = Session::blank("DupSlide");
    let panel = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 200.0 },
            "kind": { "type": "scroll_panel", "content_height": 400.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let child = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 20.0, "y": 20.0, "w": 50.0, "h": 50.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_panel",
        json!({ "slide": "slide1", "object": child, "panel": panel }),
    );

    let target = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 10.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let card = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 120.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [target], "snap": true, "return_on_miss": true }
        }),
    );

    let dup_id = ok(&mut s, "slide.duplicate", json!({ "slide": "slide1" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());

    let dup = s.course.slide(&dup_id).unwrap();
    let dup_panel = dup
        .objects
        .iter()
        .find(|o| matches!(o.kind, coursecraft_course::ObjectKind::ScrollPanel { .. }))
        .unwrap();
    assert_ne!(dup_panel.id, panel);
    let dup_child = dup.objects.iter().find(|o| o.panel.is_some()).unwrap();
    assert_eq!(dup_child.panel.as_deref(), Some(dup_panel.id.as_str()));

    let dup_drag = dup.objects.iter().find(|o| o.drag.is_some()).unwrap();
    let dup_targets = &dup_drag.drag.as_ref().unwrap().targets;
    assert_eq!(dup_targets.len(), 1);
    assert_ne!(dup_targets[0], target);
    assert!(dup.object(&dup_targets[0]).is_some());
}

#[test]
fn layer_duplicate_remaps_panel_and_clears_uncopied_drag_target() {
    let mut s = Session::blank("DupLayer");
    let base_target = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 10.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let lid = ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "name": "L1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let panel = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": lid,
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 200.0 },
            "kind": { "type": "scroll_panel", "content_height": 400.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let child = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": lid,
            "frame": { "x": 20.0, "y": 20.0, "w": 50.0, "h": 50.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_panel",
        json!({ "slide": "slide1", "object": child, "panel": panel }),
    );
    let layer_target = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": lid,
            "frame": { "x": 250.0, "y": 10.0, "w": 40.0, "h": 40.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let card = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": lid,
            "frame": { "x": 250.0, "y": 100.0, "w": 40.0, "h": 40.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [base_target, layer_target], "snap": true, "return_on_miss": true }
        }),
    );

    let dup_lid = ok(
        &mut s,
        "layer.duplicate",
        json!({ "slide": "slide1", "layer": lid }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());

    let dup_layer = s.course.slide("slide1").unwrap().layer(&dup_lid).unwrap();
    let dup_panel = dup_layer
        .objects
        .iter()
        .find(|o| matches!(o.kind, coursecraft_course::ObjectKind::ScrollPanel { .. }))
        .unwrap();
    assert_ne!(dup_panel.id, panel);
    let dup_child = dup_layer
        .objects
        .iter()
        .find(|o| o.panel.is_some())
        .unwrap();
    assert_eq!(dup_child.panel.as_deref(), Some(dup_panel.id.as_str()));

    let dup_drag = dup_layer.objects.iter().find(|o| o.drag.is_some()).unwrap();
    let dup_targets = &dup_drag.drag.as_ref().unwrap().targets;
    assert_eq!(dup_targets.len(), 1);
    assert_ne!(dup_targets[0], layer_target);
    assert!(dup_layer.objects.iter().any(|o| o.id == dup_targets[0]));
}

#[test]
fn object_duplicate_remaps_or_clears_panel_and_drag_targets() {
    let mut s = Session::blank("DupObj");
    let panel = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 200.0 },
            "kind": { "type": "scroll_panel", "content_height": 400.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let child = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 20.0, "y": 20.0, "w": 50.0, "h": 50.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_panel",
        json!({ "slide": "slide1", "object": child, "panel": panel }),
    );
    let target = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 10.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let card = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 120.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [target], "snap": true, "return_on_miss": true }
        }),
    );

    // Duplicate panel + child together: child's panel points to the duplicated panel.
    let pair = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "objects": [panel, child] }),
    );
    let pair_ids = pair["ids"].as_array().unwrap();
    let dup_panel = pair_ids[0].as_str().unwrap();
    let dup_child = pair_ids[1].as_str().unwrap();
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(dup_child)
            .unwrap()
            .0
            .panel
            .as_deref(),
        Some(dup_panel)
    );

    // Duplicate child alone: panel is cleared.
    let solo_child = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": child }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(&solo_child)
            .unwrap()
            .0
            .panel,
        None
    );

    // Duplicate draggable + target together: target is remapped.
    let drag_pair = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "objects": [card, target] }),
    );
    let drag_ids = drag_pair["ids"].as_array().unwrap();
    let dup_card = drag_ids[0].as_str().unwrap();
    let dup_target = drag_ids[1].as_str().unwrap();
    assert_eq!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(dup_card)
            .unwrap()
            .0
            .drag
            .as_ref()
            .unwrap()
            .targets,
        vec![dup_target.to_owned()]
    );

    // Duplicate draggable alone: targets are cleared.
    let solo_card = ok(
        &mut s,
        "object.duplicate",
        json!({ "slide": "slide1", "object": card }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .object(&solo_card)
            .unwrap()
            .0
            .drag
            .as_ref()
            .unwrap()
            .targets
            .is_empty()
    );
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());
}

#[test]
fn object_paste_remaps_or_clears_panel_and_drag_targets() {
    let mut s = Session::blank("PasteObj");
    let slide2 = ok(&mut s, "slide.add", json!({ "scene": "scene1" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let panel = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 200.0 },
            "kind": { "type": "scroll_panel", "content_height": 400.0 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let child = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 20.0, "y": 20.0, "w": 50.0, "h": 50.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_panel",
        json!({ "slide": "slide1", "object": child, "panel": panel }),
    );
    let target = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 10.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let card = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "frame": { "x": 300.0, "y": 120.0, "w": 80.0, "h": 80.0 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#cccccc" }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.set_drag",
        json!({
            "slide": "slide1",
            "object": card,
            "drag": { "targets": [target], "snap": true, "return_on_miss": true }
        }),
    );

    let slide1 = s.course.slide("slide1").unwrap().clone();
    let panel_obj = slide1.object(&panel).unwrap().0.clone();
    let child_obj = slide1.object(&child).unwrap().0.clone();
    let target_obj = slide1.object(&target).unwrap().0.clone();
    let card_obj = slide1.object(&card).unwrap().0.clone();

    // Paste panel + child together onto slide2: panel is remapped.
    let pasted_pair = ok(
        &mut s,
        "object.paste",
        json!({ "slide": slide2, "objects": [panel_obj, child_obj], "source_slide": "slide1" }),
    );
    let pair_ids = pasted_pair["ids"].as_array().unwrap();
    assert_eq!(
        s.course
            .slide(&slide2)
            .unwrap()
            .object(pair_ids[1].as_str().unwrap())
            .unwrap()
            .0
            .panel
            .as_deref(),
        Some(pair_ids[0].as_str().unwrap())
    );

    // Paste child alone onto slide2: panel is cleared.
    let pasted_solo_child = ok(
        &mut s,
        "object.paste",
        json!({ "slide": slide2, "objects": [child_obj], "source_slide": "slide1" }),
    )["ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.course
            .slide(&slide2)
            .unwrap()
            .object(&pasted_solo_child)
            .unwrap()
            .0
            .panel,
        None
    );

    // Paste draggable + target together onto slide2: target is remapped.
    let pasted_drag_pair = ok(
        &mut s,
        "object.paste",
        json!({ "slide": slide2, "objects": [card_obj, target_obj], "source_slide": "slide1" }),
    );
    let drag_ids = pasted_drag_pair["ids"].as_array().unwrap();
    assert_eq!(
        s.course
            .slide(&slide2)
            .unwrap()
            .object(drag_ids[0].as_str().unwrap())
            .unwrap()
            .0
            .drag
            .as_ref()
            .unwrap()
            .targets,
        vec![drag_ids[1].as_str().unwrap().to_owned()]
    );

    // Paste draggable alone onto slide2: targets are cleared.
    let pasted_solo_card = ok(
        &mut s,
        "object.paste",
        json!({ "slide": slide2, "objects": [card_obj], "source_slide": "slide1" }),
    )["ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        s.course
            .slide(&slide2)
            .unwrap()
            .object(&pasted_solo_card)
            .unwrap()
            .0
            .drag
            .as_ref()
            .unwrap()
            .targets
            .is_empty()
    );
    assert!(s.course.is_playable(), "{:#?}", s.course.validate());
}

#[test]
fn execute_batch_refuses_undo_and_redo() {
    let mut s = Session::blank("BatchUndo");
    ok(&mut s, "slide.add", json!({ "scene": "scene1" }));
    let before_course = s.course.clone();
    let before_undo = s.undo_len();
    let before_redo = s.can_redo();
    let before_rev = s.revision;

    let err_undo = s
        .execute_batch(&[
            ("slide.add".to_owned(), json!({ "scene": "scene1" })),
            ("edit.undo".to_owned(), json!({})),
        ])
        .unwrap_err();
    assert!(err_undo.to_string().contains("edit.undo"), "{err_undo}");
    assert_eq!(s.course, before_course);
    assert_eq!(s.undo_len(), before_undo);
    assert_eq!(s.can_redo(), before_redo);
    assert_eq!(s.revision, before_rev);

    let err_redo = s
        .execute_batch(&[("edit.redo".to_owned(), json!({}))])
        .unwrap_err();
    assert!(err_redo.to_string().contains("edit.redo"), "{err_redo}");
    assert_eq!(s.course, before_course);
    assert_eq!(s.undo_len(), before_undo);
    assert_eq!(s.can_redo(), before_redo);
    assert_eq!(s.revision, before_rev);
}

#[test]
fn slide_set_master_rejects_bank_slide() {
    let mut s = Session::blank("BankMaster");
    let bank = bank_id(&mut s);
    let qid = ok(
        &mut s,
        "bank.add_question",
        json!({ "bank": bank, "title": "Q1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let master = ok(&mut s, "master.add", json!({ "title": "M1" }))["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let err = s
        .execute(
            "slide.set_master",
            &json!({ "slides": [qid], "master": master }),
        )
        .unwrap_err();
    assert!(err.to_string().contains(&qid), "{err}");
}

#[test]
fn is_playable_cache_avoids_revalidating_unchanged_revision() {
    let mut s = Session::blank("Cached");
    assert_eq!(s.is_playable_checks, 0);

    ok(
        &mut s,
        "slide.add",
        json!({ "scene": "scene1", "title": "S2" }),
    );
    assert_eq!(s.is_playable_checks, 1);

    // Second mutating command at the new revision uses the cached validity from keep_valid.
    ok(
        &mut s,
        "slide.add",
        json!({ "scene": "scene1", "title": "S3" }),
    );
    assert_eq!(s.is_playable_checks, 1);

    // A no-op mutating command leaves revision unchanged and still hits the cache.
    ok(
        &mut s,
        "course.replace",
        json!({ "query": "never_matches_anything", "replacement": "x" }),
    );
    assert_eq!(s.is_playable_checks, 1);

    // After undo, the cache is cleared so the next command checks once.
    s.undo().unwrap();
    ok(
        &mut s,
        "slide.add",
        json!({ "scene": "scene1", "title": "S4" }),
    );
    assert_eq!(s.is_playable_checks, 2);

    // In a batch, fixing an invalid course in step 1 must invalidate the cache so step 2
    // sees the course is now valid and refuses to break it again.
    let mut invalid = Session::blank("Valid");
    invalid.course.title.clear();
    let err = invalid
        .execute_batch(&[
            (
                "course.set_title".to_owned(),
                json!({ "title": "Fixed Title" }),
            ),
            ("course.set_title".to_owned(), json!({ "title": "" })),
        ])
        .unwrap_err();
    assert!(err.to_string().contains("step 1"), "{err}");
}

#[test]
fn object_delete_removes_many_dangling_triggers_on_base_and_layers_in_one_pass() {
    let mut s = Session::blank("DanglingTriggers");
    let b1 = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Target" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let b2 = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Keep" },
            "frame": { "x": 120, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let layer = ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "name": "L1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.add_state",
        json!({
            "slide": "slide1",
            "object": b1,
            "state": { "name": "active", "fill": "#112233" }
        }),
    );

    // Unrelated trigger on b2 (must survive).
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": b2 },
                "action": { "action": "jump_to_slide", "target": "next" }
            }
        }),
    );

    // Trigger referencing b1 in both event and action (duplicate validation error on same trigger).
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": b1 },
                "action": { "action": "change_state", "object": b1, "state": "active" }
            }
        }),
    );

    // 25 base triggers + 24 layer triggers referencing b1.
    for _ in 0..25 {
        ok(
            &mut s,
            "trigger.add",
            json!({
                "slide": "slide1",
                "trigger": {
                    "when": { "event": "click", "object": b1 },
                    "action": { "action": "jump_to_slide", "target": "next" }
                }
            }),
        );
    }
    for _ in 0..24 {
        ok(
            &mut s,
            "trigger.add",
            json!({
                "slide": "slide1",
                "layer": layer,
                "trigger": {
                    "when": { "event": "timeline_starts" },
                    "action": { "action": "change_state", "object": b1, "state": "active" }
                }
            }),
        );
    }

    let res = ok(
        &mut s,
        "object.delete",
        json!({ "slide": "slide1", "object": b1 }),
    );
    assert_eq!(res["removed_triggers"], 50);
    assert_eq!(s.course.scenes[0].slides[0].triggers.len(), 1);
    assert!(s.course.scenes[0].slides[0].layers[0].triggers.is_empty());
    assert!(s.course.is_playable());
}

fn round_trip_project(course: &coursecraft_course::Course) -> coursecraft_course::Course {
    let media: &[(String, Vec<u8>)] = &[];
    let bytes = coursecraft_format::save_project(course, media).expect("save_project");
    coursecraft_format::open_project(&bytes)
        .expect("open_project")
        .course
}

#[test]
fn object_locked_survives_save_and_open_project() {
    let mut s = Session::blank("Test");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Next" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .expect("object id")
        .to_owned();
    ok(
        &mut s,
        "object.lock",
        json!({ "slide": "slide1", "object": id, "locked": true }),
    );
    let reopened = round_trip_project(&s.course);
    let (obj, _) = reopened
        .slide("slide1")
        .and_then(|sl| sl.object(&id))
        .expect("added object");
    assert!(obj.locked);
}

#[test]
fn object_hidden_survives_save_and_open_project() {
    let mut s = Session::blank("Test");
    let id = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "kind": { "type": "button", "label": "Next" },
            "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
        }),
    )["id"]
        .as_str()
        .expect("object id")
        .to_owned();
    ok(
        &mut s,
        "object.hide_in_editor",
        json!({ "slide": "slide1", "object": id, "hidden": true }),
    );
    let reopened = round_trip_project(&s.course);
    let (obj, _) = reopened
        .slide("slide1")
        .and_then(|sl| sl.object(&id))
        .expect("added object");
    assert!(obj.editor_hidden);
}

#[test]
fn editor_rulers_setting_survives_save_and_open_project() {
    let mut s = Session::blank("Test");
    ok(&mut s, "view.set", json!({ "show_rulers": true }));
    let reopened = round_trip_project(&s.course);
    assert!(reopened.editor.show_rulers);
}

#[test]
fn editor_grid_settings_survive_save_and_open_project() {
    let mut s = Session::blank("Test");
    ok(
        &mut s,
        "view.set",
        json!({ "show_grid": true, "snap_to_grid": true }),
    );
    let reopened = round_trip_project(&s.course);
    assert!(reopened.editor.show_grid);
    assert!(reopened.editor.snap_to_grid);
}

#[test]
fn editor_guides_survive_save_and_open_project() {
    let mut s = Session::blank("Test");
    ok(
        &mut s,
        "view.guides.add",
        json!({ "vertical": true, "at": 250.0 }),
    );
    let reopened = round_trip_project(&s.course);
    assert!(reopened.editor.show_guides);
    assert_eq!(reopened.editor.guides.len(), 1);
    assert!(reopened.editor.guides[0].vertical);
    assert!((reopened.editor.guides[0].at - 250.0).abs() < 0.01);
}

#[test]
fn text_style_range_toggles_cleanly_and_undo_restores_exact_runs() {
    let mut s = Session::blank("Toggle");
    let id = ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "text", "text": "bold+ italic + underline\nsecond line" },
                "frame": { "x": 10, "y": 10, "w": 400, "h": 100 } }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 6, "italic": true }),
    );
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 12, "end": 24, "italic": true }),
    );
    let baseline_runs = match &s
        .course
        .slide("slide1")
        .unwrap()
        .object(&id)
        .unwrap()
        .0
        .kind
    {
        ObjectKind::Text { runs, .. } => runs.clone(),
        other => panic!("{other:?}"),
    };

    // Toggle underline ON then OFF on "italic" (6..12), then across two paragraphs (15..31).
    for (start, end, key, on, off) in [
        (6, 12, "underline", json!(true), json!(false)),
        (6, 12, "bold", json!(true), json!(false)),
        (6, 12, "italic", json!(true), json!(false)),
        (6, 12, "strikethrough", json!(true), json!(false)),
        (15, 31, "underline", json!(true), json!(false)),
        (15, 31, "baseline", json!("superscript"), json!("normal")),
        (15, 31, "baseline", json!("subscript"), json!("normal")),
    ] {
        let mut p_on = json!({ "slide": "slide1", "object": id, "start": start, "end": end });
        p_on[key] = on;
        let mut p_off = json!({ "slide": "slide1", "object": id, "start": start, "end": end });
        p_off[key] = off;
        ok(&mut s, "text.style_range", p_on.clone());
        ok(&mut s, "text.style_range", p_off);
        let after_off = match &s
            .course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .kind
        {
            ObjectKind::Text { runs, .. } => runs.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            after_off, baseline_runs,
            "toggling {key} on/off must merge runs back"
        );
        ok(&mut s, "text.style_range", p_on);
        s.undo().unwrap();
        let after_undo = match &s
            .course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .kind
        {
            ObjectKind::Text { runs, .. } => runs.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(after_undo, baseline_runs, "undo must restore exact runs");
    }
}

#[test]
fn course_find_and_replace_on_glossary_entries() {
    let mut s = Session::blank("Glossary Replace");
    ok(
        &mut s,
        "glossary.add",
        json!({
            "id": "g1",
            "term": "Acme Token",
            "definition": "An Acme security token."
        }),
    );

    let found = ok(&mut s, "course.find", json!({ "query": "Acme" }));
    assert_eq!(found["hits"].as_array().unwrap().len(), 2);

    let replaced = ok(
        &mut s,
        "course.replace",
        json!({ "query": "Acme", "replacement": "CourseCraft" }),
    );
    assert_eq!(replaced["replaced"], 2);
    assert_eq!(s.course.glossary[0].term, "CourseCraft Token");
    assert_eq!(
        s.course.glossary[0].definition,
        "An CourseCraft security token."
    );

    // One undo step restores both term and definition.
    s.undo().unwrap();
    assert_eq!(s.course.glossary[0].term, "Acme Token");
    assert_eq!(s.course.glossary[0].definition, "An Acme security token.");

    // RISK-09: replacing a glossary term with "" would violate validation, so it must fail
    // cleanly without mutating the course.
    let err = s
        .execute(
            "course.replace",
            &json!({ "query": "Acme Token", "replacement": "" }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("glossary term is empty"), "{err}");
    assert_eq!(s.course.glossary[0].term, "Acme Token");
    assert_eq!(s.course.glossary[0].definition, "An Acme security token.");
}

/// L-01: Every command `label` and `menu` leaf in `REGISTRY` uses sentence case,
/// with zero empty labels and the expected command count.
#[test]
fn registry_labels_and_menu_leaves_use_sentence_case() {
    const ALLOWED_CAPS: &[&str] = &[
        "CourseCraft",
        "PowerPoint",
        "PPTX",
        "Word",
        "PDF",
        "MP4",
        "CSV",
        "XLIFF",
        "SCORM",
        "LTI",
        "xAPI",
        "cmi5",
        "HTML5",
        "AI",
        "Lottie",
        "GitHub",
        "Inter",
        "OK",
        "ID",
        "A–Z",
        "Web",
    ];

    let check_phrase = |phrase: &str, context: &str| {
        assert!(!phrase.trim().is_empty(), "{context}: empty label");
        let first_char = phrase.chars().next().expect("non-empty phrase");
        assert!(
            first_char.is_uppercase(),
            "{context}: {phrase:?} must start with an uppercase letter"
        );
        let tokens: Vec<&str> = phrase
            .split(|c: char| c.is_whitespace() || c == '/')
            .filter(|t| !t.is_empty())
            .collect();
        for &token in tokens.iter().skip(1) {
            let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '–');
            if trimmed.is_empty() || ALLOWED_CAPS.contains(&trimmed) {
                continue;
            }
            assert!(
                !trimmed.chars().any(char::is_uppercase),
                "{context}: unexpected Title Case token {trimmed:?} in {phrase:?}"
            );
        }
    };

    let cmds = crate::registry();
    assert_eq!(cmds.len(), 198, "unexpected registry command count");

    for cmd in cmds {
        check_phrase(cmd.label, &format!("label for {}", cmd.id));
        if !cmd.menu.is_empty() {
            let leaf = cmd.menu.rsplit('/').next().unwrap_or(cmd.menu);
            check_phrase(leaf, &format!("menu leaf for {}", cmd.id));
        }
    }

    let course_update = cmds
        .iter()
        .find(|c| c.id == "course.update")
        .expect("course.update");
    assert_eq!(course_update.menu, "Project/Course settings…");
}

struct Df02LegacyOracle {
    course: coursecraft_course::Course,
    undo: Vec<(coursecraft_course::Course, String)>,
    redo: Vec<(coursecraft_course::Course, String)>,
    revision: u64,
    history_truncated: bool,
}

impl Df02LegacyOracle {
    fn new(course: coursecraft_course::Course) -> Self {
        Self {
            course,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            history_truncated: false,
        }
    }

    fn undo_names(&self) -> Vec<&str> {
        self.undo.iter().rev().map(|(_, n)| n.as_str()).collect()
    }

    fn redo_names(&self) -> Vec<&str> {
        self.redo.iter().rev().map(|(_, n)| n.as_str()).collect()
    }

    fn push_undo(&mut self, snapshot: coursecraft_course::Course, label: String) {
        self.undo.push((snapshot, label));
        if self.undo.len() > crate::MAX_HISTORY {
            self.undo.remove(0);
            self.history_truncated = true;
        }
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    fn execute(&mut self, id: &str, params: &Json) -> crate::Result<Json> {
        if id == "edit.undo" {
            return self.undo().map(|()| json!({}));
        }
        if id == "edit.redo" {
            return self.redo().map(|()| json!({}));
        }
        let cmd = registry()
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| EngineError::UnknownCommand(id.to_owned()))?;
        let snapshot = self.course.clone();
        let mut tmp = Session::new(self.course.clone());
        if !cmd.mutates {
            return (cmd.run)(&mut tmp, params);
        }
        tmp.skip_undo = false;
        let val = (cmd.run)(&mut tmp, params)?;
        let unchanged = std::mem::take(&mut tmp.skip_undo) || tmp.course == snapshot;
        if !unchanged {
            crate::reroute_connectors(&mut tmp.course);
            self.course = tmp.course;
            self.push_undo(snapshot, cmd.label.to_owned());
        }
        Ok(val)
    }

    fn execute_batch_with_name(
        &mut self,
        name: Option<&str>,
        steps: &[(String, Json)],
    ) -> crate::Result<Vec<Json>> {
        if steps.is_empty() {
            return Ok(Vec::new());
        }
        for (i, (id, _)) in steps.iter().enumerate() {
            if matches!(id.as_str(), "edit.undo" | "edit.redo") {
                return Err(EngineError::BatchFailed {
                    step: i,
                    command: id.clone(),
                    error: Box::new(EngineError::Invalid(format!(
                        "{id} is not allowed inside a batch"
                    ))),
                });
            }
        }
        let snapshot = self.course.clone();
        let mut tmp = Session::new(self.course.clone());
        let mut results = Vec::with_capacity(steps.len());
        let mut any_changed = false;
        let mut first_label: Option<&'static str> = None;

        for (i, (id, params)) in steps.iter().enumerate() {
            let cmd = registry()
                .iter()
                .find(|c| c.id == id.as_str())
                .ok_or_else(|| EngineError::BatchFailed {
                    step: i,
                    command: id.clone(),
                    error: Box::new(EngineError::UnknownCommand(id.clone())),
                })?;
            let step_snapshot = (name.is_none() && first_label.is_none() && cmd.mutates)
                .then(|| tmp.course.clone());
            tmp.skip_undo = false;
            let outcome = (cmd.run)(&mut tmp, params);
            if cmd.mutates {
                tmp.valid_at_revision = None;
            }
            if std::mem::take(&mut tmp.skip_undo) {
                // Nothing to record for this step.
            } else if cmd.mutates {
                any_changed = true;
                if let Some(before) = step_snapshot
                    && tmp.course != before
                {
                    first_label = Some(cmd.label);
                }
            }
            match outcome {
                Ok(res) => results.push(res),
                Err(e) => {
                    return Err(EngineError::BatchFailed {
                        step: i,
                        command: id.clone(),
                        error: Box::new(e),
                    });
                }
            }
        }

        if any_changed && tmp.course != snapshot {
            crate::reroute_connectors(&mut tmp.course);
            let label = name
                .map(str::to_owned)
                .or_else(|| first_label.map(str::to_owned))
                .unwrap_or_else(|| "Batch".to_owned());
            self.course = tmp.course;
            self.push_undo(snapshot, label);
        }
        Ok(results)
    }

    fn undo(&mut self) -> crate::Result<()> {
        let (prev, name) = self.undo.pop().ok_or(EngineError::NothingTo("undo"))?;
        let current = std::mem::replace(&mut self.course, prev);
        self.redo.push((current, name));
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn redo(&mut self) -> crate::Result<()> {
        let (next, name) = self.redo.pop().ok_or(EngineError::NothingTo("redo"))?;
        let current = std::mem::replace(&mut self.course, next);
        self.undo.push((current, name));
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn undo_steps(&mut self, n: usize) -> crate::Result<()> {
        if n > self.undo.len() {
            return Err(EngineError::NothingTo("undo"));
        }
        for _ in 0..n {
            self.undo()?;
        }
        Ok(())
    }

    fn redo_steps(&mut self, n: usize) -> crate::Result<()> {
        if n > self.redo.len() {
            return Err(EngineError::NothingTo("redo"));
        }
        for _ in 0..n {
            self.redo()?;
        }
        Ok(())
    }
}

fn df_02_assert_parity(s: &Session, o: &Df02LegacyOracle, step: usize, op: &str) {
    assert_eq!(
        s.course.to_json().expect("session json"),
        o.course.to_json().expect("oracle json"),
        "step {step} ({op}): Course::to_json() mismatch"
    );
    assert_eq!(
        s.undo_names(),
        o.undo_names(),
        "step {step} ({op}): undo_names mismatch"
    );
    assert_eq!(
        s.redo_names(),
        o.redo_names(),
        "step {step} ({op}): redo_names mismatch"
    );
    assert_eq!(
        s.revision, o.revision,
        "step {step} ({op}): revision mismatch"
    );
    assert_eq!(
        s.history_truncated(),
        o.history_truncated,
        "step {step} ({op}): history_truncated mismatch"
    );
}

fn df_02_exec_both(s: &mut Session, o: &mut Df02LegacyOracle, id: &str, p: &Json) {
    let sr = s.execute(id, p);
    let or = o.execute(id, p);
    assert!(
        sr.is_ok() && or.is_ok(),
        "expected {id} to succeed in DF-02: session={sr:?}, oracle={or:?}"
    );
}

fn df_02_phase1_single_slide(
    s: &mut Session,
    o: &mut Df02LegacyOracle,
    step: usize,
    bucket: usize,
) -> &'static str {
    if bucket < 12 {
        #[allow(clippy::cast_precision_loss)]
        let x = 10.0 + ((step * 7) % 400) as f64;
        #[allow(clippy::cast_precision_loss)]
        let y = 10.0 + ((step * 11) % 300) as f64;
        let p = json!({
            "slide": "slide1",
            "object": "perm_shape",
            "frame": { "x": x, "y": y, "w": 120.0, "h": 60.0 }
        });
        df_02_exec_both(s, o, "object.set_frame", &p);
        "object.set_frame"
    } else if bucket < 20 {
        #[allow(clippy::cast_precision_loss)]
        let x = 30.0 + ((step * 5) % 300) as f64;
        let p = json!({
            "slide": "slide1",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#557799" },
            "frame": { "x": x, "y": 160.0, "w": 50.0, "h": 50.0 }
        });
        df_02_exec_both(s, o, "object.add", &p);
        "object.add"
    } else if bucket < 28 {
        let removable = s.course.slide("slide1").and_then(|sl| {
            sl.objects
                .iter()
                .rev()
                .find(|obj| obj.id != "perm_shape" && obj.id != "perm_text")
                .map(|obj| obj.id.clone())
        });
        if let Some(obj_id) = removable {
            let p = json!({ "slide": "slide1", "object": obj_id });
            df_02_exec_both(s, o, "object.delete", &p);
            "object.delete"
        } else {
            let p = json!({
                "slide": "slide1",
                "object": "perm_text",
                "text": format!("Step {step}")
            });
            df_02_exec_both(s, o, "text.set", &p);
            "text.set"
        }
    } else {
        let p = json!({
            "slide": "slide1",
            "object": "perm_text",
            "text": format!("Copy step {step}")
        });
        df_02_exec_both(s, o, "text.set", &p);
        "text.set"
    }
}

fn df_02_phase1_step(
    s: &mut Session,
    o: &mut Df02LegacyOracle,
    step: usize,
    bucket: usize,
    counts: &mut [usize; 14],
) -> &'static str {
    if bucket < 36 {
        counts[0] += 1;
        return df_02_phase1_single_slide(s, o, step, bucket);
    }
    if bucket < 59 {
        counts[1] += 1;
        let (id, p) = if bucket < 45 {
            (
                "course.set_title",
                json!({ "title": format!("Course {step}") }),
            )
        } else if bucket < 52 {
            (
                "glossary.add",
                json!({
                    "term": format!("Term{step}"),
                    "definition": format!("Definition for step {step}")
                }),
            )
        } else {
            (
                "variable.add",
                json!({ "name": format!("v_{step}"), "value": step }),
            )
        };
        df_02_exec_both(s, o, id, &p);
        return id;
    }
    if bucket < 79 {
        counts[2] += 1;
        let (id, p) = if bucket < 67 {
            (
                "slide.add",
                json!({ "scene": "scene1", "title": format!("Slide {step}") }),
            )
        } else if bucket < 74 {
            let last_slide = s.course.scenes[0]
                .slides
                .last()
                .map(|sl| sl.id.clone())
                .expect("non-empty scene1");
            (
                "slide.move",
                json!({ "slide": last_slide, "scene": "scene1", "index": 0 }),
            )
        } else {
            ("scene.add", json!({ "title": format!("Scene {step}") }))
        };
        df_02_exec_both(s, o, id, &p);
        return id;
    }
    df_02_phase1_batch_or_edge(s, o, step, bucket, counts)
}

fn df_02_phase1_batch_or_edge(
    s: &mut Session,
    o: &mut Df02LegacyOracle,
    step: usize,
    bucket: usize,
    counts: &mut [usize; 14],
) -> &'static str {
    if bucket < 88 {
        counts[3] += 1;
        #[allow(clippy::cast_precision_loss)]
        let x = 25.0 + ((step * 13) % 350) as f64;
        let steps = vec![
            (
                "course.set_title".to_owned(),
                json!({ "title": s.course.title.clone() }),
            ),
            (
                "object.set_frame".to_owned(),
                json!({
                    "slide": "slide1",
                    "object": "perm_shape",
                    "frame": { "x": x, "y": 40.0, "w": 110.0, "h": 55.0 }
                }),
            ),
            (
                "text.set".to_owned(),
                json!({
                    "slide": "slide1",
                    "object": "perm_text",
                    "text": format!("Batch {step}")
                }),
            ),
        ];
        let sr = s.execute_batch(&steps);
        let or = o.execute_batch_with_name(None, &steps);
        assert!(
            sr.is_ok() && or.is_ok(),
            "execute_batch failed at step {step}: session={sr:?}, oracle={or:?}"
        );
        "execute_batch"
    } else if bucket < 96 {
        counts[4] += 1;
        let steps = vec![
            (
                "course.set_title".to_owned(),
                json!({ "title": format!("Named {step}") }),
            ),
            (
                "text.set".to_owned(),
                json!({
                    "slide": "slide1",
                    "object": "perm_text",
                    "text": format!("Named text {step}")
                }),
            ),
        ];
        let sr = s.execute_batch_named("Named tweak", &steps);
        let or = o.execute_batch_with_name(Some("Named tweak"), &steps);
        assert!(
            sr.is_ok() && or.is_ok(),
            "execute_batch_named failed at step {step}: session={sr:?}, oracle={or:?}"
        );
        "execute_batch_named"
    } else if bucket < 98 {
        counts[5] += 1;
        let (cmd_id, p) = if step.is_multiple_of(2) {
            (
                "course.set_title",
                json!({ "title": s.course.title.clone() }),
            )
        } else {
            (
                "course.replace",
                json!({ "query": "__no_such_text__", "replacement": "x" }),
            )
        };
        df_02_exec_both(s, o, cmd_id, &p);
        "noop"
    } else if bucket == 98 {
        counts[6] += 1;
        let p = json!({ "slide": "slide1", "object": "missing_obj" });
        let sr = s.execute("object.delete", &p);
        let or = o.execute("object.delete", &p);
        assert!(sr.is_err(), "expected failing_single to error");
        assert_eq!(sr, or);
        "failing_single"
    } else {
        counts[7] += 1;
        let steps = vec![
            (
                "course.set_title".to_owned(),
                json!({ "title": "Rolled back title" }),
            ),
            (
                "object.delete".to_owned(),
                json!({ "slide": "slide1", "object": "missing_obj" }),
            ),
        ];
        let sr = s.execute_batch(&steps);
        let or = o.execute_batch_with_name(None, &steps);
        assert!(sr.is_err(), "expected failing_batch to error");
        assert_eq!(sr, or);
        "failing_batch"
    }
}

fn df_02_phase2_step(
    s: &mut Session,
    o: &mut Df02LegacyOracle,
    step: usize,
    counts: &mut [usize; 14],
) -> &'static str {
    match step - 218 {
        0 | 1 | 2 | 12 | 25 => {
            counts[8] += 1;
            assert_eq!(s.undo(), Ok(()));
            assert_eq!(o.undo(), Ok(()));
            "undo"
        }
        3 | 13 | 26 => {
            counts[9] += 1;
            assert_eq!(s.redo(), Ok(()));
            assert_eq!(o.redo(), Ok(()));
            "redo"
        }
        4 | 14 | 20 => {
            counts[10] += 1;
            assert_eq!(s.undo_steps(5), Ok(()));
            assert_eq!(o.undo_steps(5), Ok(()));
            "undo_steps_ok"
        }
        5 | 21 => {
            counts[11] += 1;
            let n = s.undo_len() + 1;
            assert_eq!(s.undo_steps(n), Err(EngineError::NothingTo("undo")));
            assert_eq!(o.undo_steps(n), Err(EngineError::NothingTo("undo")));
            "undo_steps_oob"
        }
        6 | 15 | 22 => {
            counts[12] += 1;
            assert_eq!(s.redo_steps(3), Ok(()));
            assert_eq!(o.redo_steps(3), Ok(()));
            "redo_steps_ok"
        }
        7 | 23 => {
            counts[13] += 1;
            let n = s.redo_names().len() + 1;
            assert_eq!(s.redo_steps(n), Err(EngineError::NothingTo("redo")));
            assert_eq!(o.redo_steps(n), Err(EngineError::NothingTo("redo")));
            "redo_steps_oob"
        }
        8 | 16 => {
            counts[5] += 1;
            let p = json!({ "query": "__never_present__", "replacement": "z" });
            df_02_exec_both(s, o, "course.replace", &p);
            "noop_preserves_redo"
        }
        9 | 17 => {
            counts[6] += 1;
            let p = json!({ "slide": "slide1", "object": "missing_obj" });
            let sr = s.execute("object.delete", &p);
            let or = o.execute("object.delete", &p);
            assert!(sr.is_err(), "expected failing_single_phase2 to error");
            assert_eq!(sr, or);
            "failing_single_phase2"
        }
        10 | 18 => {
            counts[7] += 1;
            let steps = vec![
                (
                    "course.set_title".to_owned(),
                    json!({ "title": "Will fail" }),
                ),
                (
                    "object.delete".to_owned(),
                    json!({ "slide": "slide1", "object": "missing_obj" }),
                ),
            ];
            let sr = s.execute_batch(&steps);
            let or = o.execute_batch_with_name(None, &steps);
            assert!(sr.is_err(), "expected failing_batch_phase2 to error");
            assert_eq!(sr, or);
            "failing_batch_phase2"
        }
        11 | 19 | 24 => {
            counts[1] += 1;
            let p = json!({ "title": format!("Post-undo {step}") });
            df_02_exec_both(s, o, "course.set_title", &p);
            "edit_clears_redo"
        }
        _ => {
            counts[8] += 1;
            assert_eq!(s.undo(), Ok(()));
            assert_eq!(o.undo(), Ok(()));
            "undo_tail"
        }
    }
}

/// DF-02: 250-step deterministic pseudo-random mixed sequence verified step-by-step
/// against a legacy full-snapshot undo/redo oracle, including MAX_HISTORY eviction,
/// `undo_steps`/`redo_steps`, `skip_undo`, no-ops, batches, and rollbacks.
#[test]
fn df_02_differential_250_step_mixed_sequence_with_eviction() {
    let mut seed = Session::blank("DF-02 Seed");
    ok(
        &mut seed,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "perm_shape",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#224466" },
            "frame": { "x": 20.0, "y": 20.0, "w": 120.0, "h": 60.0 }
        }),
    );
    ok(
        &mut seed,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "perm_text",
            "kind": { "type": "text", "text": "Initial" },
            "frame": { "x": 20.0, "y": 100.0, "w": 240.0, "h": 40.0 }
        }),
    );
    ok(&mut seed, "slide.add", json!({ "title": "Seed Slide 2" }));

    let initial_course = seed.course;
    let mut s = Session::new(initial_course.clone());
    let mut o = Df02LegacyOracle::new(initial_course);

    let mut lcg: u64 = 0x5EED_DF02_0000_0001;
    let mut counts = [0usize; 14];

    for step in 0..250usize {
        lcg = lcg
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let bucket = ((lcg >> 33) as usize) % 100;
        let op_label = if step < 218 {
            df_02_phase1_step(&mut s, &mut o, step, bucket, &mut counts)
        } else {
            df_02_phase2_step(&mut s, &mut o, step, &mut counts)
        };
        df_02_assert_parity(&s, &o, step, op_label);
    }

    assert!(
        s.history_truncated(),
        "MAX_HISTORY ({}) eviction must occur during DF-02",
        crate::MAX_HISTORY
    );
    assert!(o.history_truncated);
    for (idx, count) in counts.iter().enumerate() {
        assert!(*count > 0, "DF-02 operation category {idx} never executed");
    }
}

#[test]
fn undo_estimated_bytes_tracks_undo_and_redo_deltas() {
    let mut s = Session::blank("Bytes");
    assert_eq!(s.undo_estimated_bytes(), 0);

    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "rect1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );
    let after_add = s.undo_estimated_bytes();
    assert!(after_add > 0);

    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": "rect1",
            "frame": { "x": 50.0, "y": 20.0, "w": 80.0, "h": 40.0 }
        }),
    );
    let after_move = s.undo_estimated_bytes();
    assert!(after_move > after_add);

    // Moving a delta from undo to redo keeps it accounted for in undo_estimated_bytes.
    s.undo().expect("undo move");
    assert_eq!(s.undo_estimated_bytes(), after_move);

    // Undoing all steps empties undo; non-zero bytes prove redo deltas are counted.
    s.undo().expect("undo add");
    assert!(!s.can_undo());
    assert!(s.can_redo());
    let after_undo_all = s.undo_estimated_bytes();
    assert!(after_undo_all > after_move);

    // A new small edit clears redo so the larger object deltas are dropped.
    ok(&mut s, "course.set_title", json!({ "title": "Renamed" }));
    assert!(!s.can_redo());
    let after_rename = s.undo_estimated_bytes();
    assert!(after_rename > 0);
    assert!(after_rename < after_undo_all);
}

#[test]
fn undo_steps_and_redo_steps_unwind_atomically_on_mid_loop_delta_error() {
    let mut s = Session::blank("Atomic Unwind");
    ok(&mut s, "course.set_title", json!({ "title": "One" }));
    ok(&mut s, "course.set_title", json!({ "title": "Two" }));
    ok(&mut s, "course.set_title", json!({ "title": "Three" }));

    // Corrupt the second step from the top (index 1) so step 0 succeeds and step 1 fails.
    let bad_delta = crate::delta::CourseDelta::SlideChange {
        loc: crate::delta::SlideLoc::SceneSlide {
            scene: 99,
            slide: 0,
        },
        change: crate::delta::SlideDelta::ObjectInserted {
            layer: None,
            index: 0,
        },
    };
    let saved_good = std::mem::replace(&mut s.undo[1].0, bad_delta.clone());

    let before_course = s.course.clone();
    let before_rev = s.revision;
    let before_undo: Vec<String> = s.undo_names().into_iter().map(str::to_owned).collect();
    let before_redo: Vec<String> = s.redo_names().into_iter().map(str::to_owned).collect();

    let err = s.undo_steps(3).expect_err("corrupted mid-loop undo fails");
    assert!(matches!(err, EngineError::Invalid(_)), "{err:?}");
    assert_eq!(s.course, before_course);
    assert_eq!(s.revision, before_rev);
    assert_eq!(s.undo_names(), before_undo);
    assert_eq!(s.redo_names(), before_redo);

    // Restore good delta, undo all 3, then test redo_steps mid-loop unwind.
    s.undo[1].0 = saved_good;
    s.undo_steps(3).expect("valid undo_steps(3)");
    s.redo[1].0 = bad_delta;

    let before_course = s.course.clone();
    let before_rev = s.revision;
    let before_undo: Vec<String> = s.undo_names().into_iter().map(str::to_owned).collect();
    let before_redo: Vec<String> = s.redo_names().into_iter().map(str::to_owned).collect();

    let err = s.redo_steps(3).expect_err("corrupted mid-loop redo fails");
    assert!(matches!(err, EngineError::Invalid(_)), "{err:?}");
    assert_eq!(s.course, before_course);
    assert_eq!(s.revision, before_rev);
    assert_eq!(s.undo_names(), before_undo);
    assert_eq!(s.redo_names(), before_redo);
}

/// SC-01: Pilot `SingleSlide` (`object.set_frame`, `object.update`, `text.set`)
/// and `CourseHeader` (`course.set_title`) commands execute and roll back with
/// zero full-course clones and byte-identical undo/redo.
#[test]
fn sc_01_pilot_scoped_commands_zero_full_clones_and_isolated() {
    let mut s = Session::blank("Pilot Scope");
    ok(&mut s, "slide.add", json!({ "title": "Slide 2" }));
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "shape1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
            "frame": { "x": 10.0, "y": 20.0, "w": 100.0, "h": 60.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "text1",
            "kind": { "type": "text", "text": "Hello" },
            "frame": { "x": 10.0, "y": 100.0, "w": 200.0, "h": 40.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide2",
            "id": "other_shape",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#993333" },
            "frame": { "x": 5.0, "y": 5.0, "w": 50.0, "h": 50.0 }
        }),
    );

    let success_cases = [
        (
            "object.set_frame",
            json!({
                "slide": "slide1",
                "object": "shape1",
                "frame": { "x": 45.0, "y": 55.0, "w": 120.0, "h": 70.0 }
            }),
        ),
        (
            "object.update",
            json!({
                "slide": "slide1",
                "object": "shape1",
                "name": "Hero Box",
                "description": "Primary callout",
                "focus_order": 2
            }),
        ),
        (
            "text.set",
            json!({
                "slide": "slide1",
                "object": "text1",
                "text": "Updated copy"
            }),
        ),
        ("course.set_title", json!({ "title": "Renamed Pilot" })),
    ];

    for (cmd, params) in &success_cases {
        let before_json = s.course.to_json().expect("before json");
        #[cfg(debug_assertions)]
        crate::test_counts::reset();

        ok(&mut s, cmd, params.clone());
        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_clones(),
            0,
            "{cmd} must not clone the full course"
        );

        let after_json = s.course.to_json().expect("after json");
        assert_ne!(after_json, before_json, "{cmd} should mutate course");

        // Re-running with identical params is a no-op with 0 full-course clones
        // and preserves any existing redo stack.
        s.undo().expect("undo");
        assert_eq!(
            s.course.to_json().expect("undone json"),
            before_json,
            "undo of {cmd} must restore byte-identical JSON"
        );
        s.redo().expect("redo");
        assert_eq!(
            s.course.to_json().expect("redone json"),
            after_json,
            "redo of {cmd} must restore byte-identical JSON"
        );

        let rev_after = s.revision;
        let undo_after = s.undo_len();
        #[cfg(debug_assertions)]
        crate::test_counts::reset();
        ok(&mut s, cmd, params.clone());
        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_clones(),
            0,
            "no-op {cmd} must not clone the full course"
        );
        assert_eq!(s.revision, rev_after);
        assert_eq!(s.undo_len(), undo_after);
    }

    // Verify no-op on both SingleSlide and CourseHeader preserves an active redo stack.
    s.undo().expect("undo to populate redo");
    assert!(s.can_redo());
    let redo_before: Vec<String> = s.redo_names().into_iter().map(str::to_owned).collect();
    let current_title = s.course.title.clone();
    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    ok(
        &mut s,
        "course.set_title",
        json!({ "title": current_title }),
    );
    ok(
        &mut s,
        "text.set",
        json!({ "slide": "slide1", "object": "text1", "text": "Updated copy" }),
    );
    #[cfg(debug_assertions)]
    assert_eq!(crate::test_counts::full_course_clones(), 0);
    assert_eq!(s.redo_names(), redo_before);
    s.redo().expect("redo still works after scoped no-ops");

    // Error rollback cases for each pilot command: 0 full-course clones and untouched course.
    let error_cases = [
        (
            "object.set_frame",
            json!({
                "slide": "slide1",
                "object": "missing_object",
                "frame": { "x": 1.0, "y": 2.0, "w": 10.0, "h": 10.0 }
            }),
        ),
        (
            "object.update",
            json!({
                "slide": "slide1",
                "object": "shape1",
                "name": "MustRollBack",
                "initial_state": "   "
            }),
        ),
        (
            "text.set",
            json!({
                "slide": "slide1",
                "object": "text1",
                "text": "x".repeat(5000)
            }),
        ),
        ("course.set_title", json!({ "title": "   " })),
    ];

    for (cmd, params) in &error_cases {
        let before_json = s.course.to_json().expect("before error json");
        let before_rev = s.revision;
        let before_undo = s.undo_len();
        #[cfg(debug_assertions)]
        crate::test_counts::reset();

        let err = s.execute(cmd, params);
        assert!(err.is_err(), "{cmd} error case must fail");
        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_clones(),
            0,
            "error rollback of {cmd} must not clone the full course"
        );
        assert_eq!(
            s.course.to_json().expect("after error json"),
            before_json,
            "error rollback of {cmd} must leave course byte-identical"
        );
        assert_eq!(s.revision, before_rev);
        assert_eq!(s.undo_len(), before_undo);
    }
}

/// SC-02: Same-slide batch of `SingleSlide` edits takes zero full-course clones,
/// commits one atomic undo step, and rolls back atomically on mid-batch error.
#[test]
fn sc_02_same_slide_batch_zero_full_clones_and_atomic_rollback() {
    let mut seed = Session::blank("Same-Slide Batch");
    ok(&mut seed, "slide.add", json!({ "title": "Slide 2" }));
    ok(
        &mut seed,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "shape1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#224466" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );
    ok(
        &mut seed,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "text1",
            "kind": { "type": "text", "text": "Start" },
            "frame": { "x": 10.0, "y": 80.0, "w": 200.0, "h": 40.0 }
        }),
    );

    let mut s = Session::new(seed.course);
    let before_json = s.course.to_json().expect("before batch json");

    let steps = vec![
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "frame": { "x": 20.0, "y": 15.0, "w": 90.0, "h": 45.0 }
            }),
        ),
        (
            "object.update".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "name": "Step Two"
            }),
        ),
        (
            "text.set".to_owned(),
            json!({
                "slide": "slide1",
                "object": "text1",
                "text": "Step Three"
            }),
        ),
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "frame": { "x": 60.0, "y": 30.0, "w": 110.0, "h": 55.0 }
            }),
        ),
        (
            "object.update".to_owned(),
            json!({
                "slide": "slide1",
                "object": "text1",
                "description": "Batch caption"
            }),
        ),
    ];

    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    let results = s.execute_batch(&steps).expect("same-slide batch succeeds");
    assert_eq!(results.len(), 5);
    #[cfg(debug_assertions)]
    assert_eq!(
        crate::test_counts::full_course_clones(),
        0,
        "same-slide batch must not clone the full course"
    );
    assert_eq!(s.undo_len(), 1);

    let after_json = s.course.to_json().expect("after batch json");
    assert_ne!(after_json, before_json);

    s.undo().expect("undo batch");
    assert_eq!(s.course.to_json().expect("undone batch json"), before_json);
    s.redo().expect("redo batch");
    assert_eq!(s.course.to_json().expect("redone batch json"), after_json);

    // Failing 4th step rolls back steps 1-3 atomically with zero full-course clones.
    let before_fail_json = s.course.to_json().expect("before fail json");
    let before_fail_rev = s.revision;
    let before_undo_names: Vec<String> = s.undo_names().into_iter().map(str::to_owned).collect();
    let before_redo_names: Vec<String> = s.redo_names().into_iter().map(str::to_owned).collect();

    let failing_steps = vec![
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "frame": { "x": 150.0, "y": 150.0, "w": 80.0, "h": 40.0 }
            }),
        ),
        (
            "object.update".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "name": "Should Not Stick"
            }),
        ),
        (
            "text.set".to_owned(),
            json!({
                "slide": "slide1",
                "object": "text1",
                "text": "Rolled back"
            }),
        ),
        (
            "object.update".to_owned(),
            json!({
                "slide": "slide1",
                "object": "shape1",
                "initial_state": "   "
            }),
        ),
    ];

    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    let err = s
        .execute_batch(&failing_steps)
        .expect_err("step 4 must fail");
    #[cfg(debug_assertions)]
    assert_eq!(
        crate::test_counts::full_course_clones(),
        0,
        "failed same-slide batch must not clone the full course"
    );
    assert!(
        matches!(err, EngineError::BatchFailed { step: 3, .. }),
        "expected BatchFailed at step 3, got {err:?}"
    );
    assert_eq!(
        s.course.to_json().expect("after fail json"),
        before_fail_json
    );
    assert_eq!(s.revision, before_fail_rev);
    assert_eq!(s.undo_names(), before_undo_names);
    assert_eq!(s.redo_names(), before_redo_names);
}

/// SC-03: Missing, non-string, or unknown `"slide"` param on a `SingleSlide`
/// command falls back safely without panicking and leaves the course untouched.
#[test]
fn sc_03_single_slide_missing_or_unknown_slide_falls_back_safely() {
    let mut s = Session::blank("SC-03 Fallback");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "shape1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );

    let before_json = s.course.to_json().expect("before json");
    let before_rev = s.revision;
    let before_undo = s.undo_len();

    let missing = s
        .execute(
            "object.set_frame",
            &json!({
                "object": "shape1",
                "frame": { "x": 50.0, "y": 50.0, "w": 80.0, "h": 40.0 }
            }),
        )
        .expect_err("missing slide must error");
    assert!(
        matches!(missing, EngineError::BadParams { .. }),
        "{missing:?}"
    );

    let non_string = s
        .execute(
            "object.set_frame",
            &json!({
                "slide": 123,
                "object": "shape1",
                "frame": { "x": 50.0, "y": 50.0, "w": 80.0, "h": 40.0 }
            }),
        )
        .expect_err("non-string slide must error");
    assert!(
        matches!(non_string, EngineError::BadParams { .. }),
        "{non_string:?}"
    );

    let unknown = s
        .execute(
            "object.set_frame",
            &json!({
                "slide": "does-not-exist",
                "object": "shape1",
                "frame": { "x": 50.0, "y": 50.0, "w": 80.0, "h": 40.0 }
            }),
        )
        .expect_err("unknown slide must error");
    assert!(matches!(unknown, EngineError::NotFound(_)), "{unknown:?}");

    assert_eq!(s.course.to_json().expect("after json"), before_json);
    assert_eq!(s.revision, before_rev);
    assert_eq!(s.undo_len(), before_undo);
}

/// SingleSlide lookup must match `Course::slide_mut` precedence (scenes -> banks
/// -> masters, never templates), even on malformed courses where a bank slide
/// shares an id with a master or template.
#[test]
fn sc_03_single_slide_matches_course_slide_mut_precedence_with_duplicate_ids() {
    let mut seed = Session::blank("Duplicate ID Precedence");
    ok(&mut seed, "master.add", json!({ "id": "dup_bank_master" }));
    let bank_id = ok(&mut seed, "bank.create", json!({ "title": "B1" }))["id"]
        .as_str()
        .expect("bank id")
        .to_owned();
    let bank_slide_id = ok(
        &mut seed,
        "bank.add_question",
        json!({ "bank": bank_id, "title": "Bank Slide" }),
    )["id"]
        .as_str()
        .expect("bank slide id")
        .to_owned();
    ok(
        &mut seed,
        "object.add",
        json!({
            "slide": bank_slide_id,
            "id": "bank_shape",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#445566" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );

    // Construct a malformed course where the bank slide has the same id as the
    // master AND a template, plus a template-only id.
    let mut course = seed.course;
    let mut tpl = course.scenes[0].slides[0].clone();
    tpl.id = "dup_bank_master".into();
    course.templates.push(tpl);
    let mut tpl_only = course.scenes[0].slides[0].clone();
    tpl_only.id = "template_only".into();
    course.templates.push(tpl_only);
    course.banks[0].slides[0].id = "dup_bank_master".into();

    let mut s = Session::new(course);
    let before_json = s.course.to_json().expect("before json");

    // SingleSlide edit must target the bank slide (matching Course::slide_mut),
    // take 0 full-course clones, pass scope isolation, and undo/redo cleanly.
    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "dup_bank_master",
            "object": "bank_shape",
            "frame": { "x": 75.0, "y": 85.0, "w": 90.0, "h": 50.0 }
        }),
    );
    #[cfg(debug_assertions)]
    assert_eq!(crate::test_counts::full_course_clones(), 0);
    assert_eq!(s.undo_len(), 1);
    let after_json = s.course.to_json().expect("after json");
    assert_ne!(after_json, before_json);

    s.undo().expect("undo bank edit");
    assert_eq!(s.course.to_json().expect("undone json"), before_json);
    s.redo().expect("redo bank edit");
    assert_eq!(s.course.to_json().expect("redone json"), after_json);

    // Partial-mutation error on the bank slide must roll back the bank slide.
    let before_err_json = s.course.to_json().expect("before err json");
    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    let err = s
        .execute(
            "object.update",
            &json!({
                "slide": "dup_bank_master",
                "object": "bank_shape",
                "name": "ShouldRollBack",
                "initial_state": "   "
            }),
        )
        .expect_err("invalid initial_state must fail");
    assert!(matches!(err, EngineError::BadParams { .. }), "{err:?}");
    #[cfg(debug_assertions)]
    assert_eq!(crate::test_counts::full_course_clones(), 0);
    assert_eq!(s.course.to_json().expect("after err json"), before_err_json);

    // Same-slide batch targeting the bank slide must also match slide_mut.
    #[cfg(debug_assertions)]
    crate::test_counts::reset();
    s.execute_batch(&[
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "dup_bank_master",
                "object": "bank_shape",
                "frame": { "x": 120.0, "y": 130.0, "w": 90.0, "h": 50.0 }
            }),
        ),
        (
            "object.update".to_owned(),
            json!({
                "slide": "dup_bank_master",
                "object": "bank_shape",
                "name": "Batch Bank Shape"
            }),
        ),
    ])
    .expect("same-slide batch on bank slide");
    #[cfg(debug_assertions)]
    assert_eq!(crate::test_counts::full_course_clones(), 0);
    s.undo().expect("undo batch");
    assert_eq!(
        s.course.to_json().expect("undone batch json"),
        before_err_json
    );

    // Template-only id is not editable via Course::slide_mut and falls back cleanly.
    let tpl_err = s
        .execute(
            "object.set_frame",
            &json!({
                "slide": "template_only",
                "object": "bank_shape",
                "frame": { "x": 1.0, "y": 2.0, "w": 10.0, "h": 10.0 }
            }),
        )
        .expect_err("template slide is not editable");
    assert!(matches!(tpl_err, EngineError::NotFound(_)), "{tpl_err:?}");
    assert_eq!(
        s.course.to_json().expect("after template err json"),
        before_err_json
    );
}

fn assert_u07_repeated_move_merge(s: &mut Session) {
    let pre_first_json = s.course.to_json().expect("pre first json");
    for step in 1..=5 {
        ok(
            s,
            "object.set_frame",
            json!({
                "slide": "slide1",
                "object": "box1",
                "frame": {
                    "x": 10.0 + (step as f64) * 20.0,
                    "y": 10.0 + (step as f64) * 10.0,
                    "w": 80.0,
                    "h": 40.0
                }
            }),
        );
    }
    assert_eq!(s.undo_len(), 5);
    let post_fifth_json = s.course.to_json().expect("post fifth json");
    assert_ne!(pre_first_json, post_fifth_json);

    let rev_before_merge = s.revision;
    for _ in 0..4 {
        assert!(s.merge_last_two_undos(Some("Move object")));
    }
    assert_eq!(
        s.revision, rev_before_merge,
        "merging must not bump revision"
    );
    assert_eq!(s.undo_len(), 1);
    assert_eq!(s.undo_names(), vec!["Move object"]);
    assert!(!s.merge_last_two_undos(Some("Too few")));

    s.undo().expect("undo merged moves");
    assert_eq!(s.course.to_json().expect("undone json"), pre_first_json);
    s.redo().expect("redo merged moves");
    assert_eq!(s.course.to_json().expect("redone json"), post_fifth_json);
}

fn assert_u07_mixed_merge_preserves_label_and_redo(s2: &mut Session) {
    let mixed_before_json = s2.course.to_json().expect("mixed before json");
    ok(
        s2,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": "box1",
            "frame": { "x": 220.0, "y": 140.0, "w": 95.0, "h": 55.0 }
        }),
    );
    let older_label = s2.undo_names()[0].to_owned();
    ok(
        s2,
        "course.set_title",
        json!({ "title": "Renamed After Move" }),
    );
    let mixed_after_json = s2.course.to_json().expect("mixed after json");
    assert_ne!(mixed_before_json, mixed_after_json);

    // Push a third edit and undo it so redo stack is non-empty when merging.
    ok(
        s2,
        "course.set_title",
        json!({ "title": "Temporary Redo Title" }),
    );
    s2.undo().expect("undo third edit to populate redo");
    let expected_redo_names: Vec<String> = s2.redo_names().into_iter().map(str::to_owned).collect();
    assert_eq!(expected_redo_names.len(), 1);
    let rev_before_mixed_merge = s2.revision;

    assert!(s2.merge_last_two_undos(None));
    assert_eq!(s2.revision, rev_before_mixed_merge);
    assert_eq!(s2.undo_len(), 1);
    assert_eq!(s2.undo_names(), vec![older_label.as_str()]);
    assert_eq!(
        s2.redo_names(),
        expected_redo_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "merging must not clear redo"
    );

    s2.undo().expect("undo merged mixed");
    assert_eq!(
        s2.course.to_json().expect("undone mixed json"),
        mixed_before_json
    );
    s2.redo().expect("redo merged mixed");
    assert_eq!(
        s2.course.to_json().expect("redone mixed json"),
        mixed_after_json
    );
}

/// U-07: `Session::merge_last_two_undos` returns `false` when `< 2` steps exist,
/// merges repeated `object.set_frame` edits into one step with custom label,
/// preserves `revision` and `redo`, and merges mixed edits keeping older label
/// when `label = None` with byte-identical `undo()` / `redo()`.
#[test]
fn u_07_session_merge_last_two_undos_roundtrip() {
    let mut s = Session::blank("U-07 Merge");
    let rev0 = s.revision;
    assert!(!s.merge_last_two_undos(Some("Nope")));
    assert_eq!(s.revision, rev0);
    assert_eq!(s.undo_len(), 0);

    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "box1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#224466" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );
    let rev1 = s.revision;
    assert!(!s.merge_last_two_undos(None));
    assert_eq!(s.revision, rev1);
    assert_eq!(s.undo_len(), 1);

    let mut s_moves = Session::new(s.course);
    assert_u07_repeated_move_merge(&mut s_moves);

    let mut s_mixed = Session::new(s_moves.course);
    assert_u07_mixed_merge_preserves_label_and_redo(&mut s_mixed);
}

/// GD-02 (`M33-08`, `M33-09`): 200 single-object edits on a 300-slide course
/// (`perf300`) perform zero full-course clones, keep all 200 undo deltas under
/// 512 KiB, and roundtrip byte-identically across `undo_steps(200)` and
/// `redo_steps(200)`.
#[test]
fn two_hundred_edits_on_300_slide_course_zero_full_clones_and_small_deltas() {
    let mut s = Session::new(crate::undo_perf_tests::perf300());
    let orig_json = s.course.to_json().expect("initial perf300 JSON");

    crate::with_scope_verifier_disabled(|| {
        #[cfg(debug_assertions)]
        crate::test_counts::reset();

        for i in 0..200 {
            ok(
                &mut s,
                "object.set_frame",
                json!({
                    "slide": "s-150",
                    "object": "h150",
                    "frame": {
                        "x": 60.0 + (i + 1) as f64,
                        "y": 520.0,
                        "w": 200.0,
                        "h": 80.0
                    }
                }),
            );
        }

        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_clones(),
            0,
            "200 single-slide edits must perform zero full-course clones"
        );
        assert_eq!(s.undo_len(), 200);
        let est_bytes = s.undo_estimated_bytes();
        assert!(
            est_bytes < 512 * 1024,
            "200 single-object deltas must use < 512 KiB, got {est_bytes} bytes"
        );

        let after_json = s.course.to_json().expect("after-200 perf300 JSON");
        assert_ne!(after_json, orig_json);

        s.undo_steps(200).expect("undo_steps(200) succeeds");
        assert_eq!(
            s.course.to_json().expect("undone perf300 JSON"),
            orig_json,
            "undo_steps(200) must restore original perf300 JSON byte-identically"
        );

        s.redo_steps(200).expect("redo_steps(200) succeeds");
        assert_eq!(
            s.course.to_json().expect("redone perf300 JSON"),
            after_json,
            "redo_steps(200) must restore post-edit perf300 JSON byte-identically"
        );

        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_clones(),
            0,
            "undo_steps(200) and redo_steps(200) must also perform zero full-course clones"
        );
    });
}

/// G-07 (`M34-07`): On a 300-slide course (`perf300`), warming playability,
/// running a real `object.set_frame` edit, and running a no-op `object.set_frame`
/// with the same frame perform zero full-course validations.
#[test]
fn g07_single_slide_frame_edit_and_noop_zero_full_validations() {
    let mut s = Session::new(crate::undo_perf_tests::perf300());
    assert!(s.is_playable(), "perf300 fixture should be playable");

    crate::with_scope_verifier_disabled(|| {
        #[cfg(debug_assertions)]
        crate::test_counts::reset();

        let moved_frame = json!({
            "slide": "s-150",
            "object": "h150",
            "frame": { "x": 75.0, "y": 530.0, "w": 200.0, "h": 80.0 }
        });

        // 1. Actual move uses scoped slide validation (0 full-course validations).
        ok(&mut s, "object.set_frame", moved_frame.clone());
        assert_eq!(s.revision, 1);
        assert_eq!(s.undo_len(), 1);
        let obj_frame = s
            .course
            .slide("s-150")
            .and_then(|slide| slide.object("h150"))
            .map(|(obj, _)| obj.frame)
            .expect("h150 exists");
        assert!((obj_frame.x - 75.0).abs() < 0.01);
        assert!((obj_frame.y - 530.0).abs() < 0.01);

        // 2. No-op with identical frame short-circuits before validation and undo.
        ok(&mut s, "object.set_frame", moved_frame);
        assert_eq!(s.revision, 1, "no-op frame edit must not bump revision");
        assert_eq!(s.undo_len(), 1, "no-op frame edit must not add undo step");

        #[cfg(debug_assertions)]
        assert_eq!(
            crate::test_counts::full_course_validations(),
            0,
            "single-slide frame move and no-op must run zero full-course validations"
        );
    });
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[test]
fn r5_01_registry_golden_lock() {
    use crate::delta::UndoScope;

    let reg = registry();
    assert_eq!(reg.len(), 198);

    const EXPECTED: &[(&str, bool, UndoScope, bool)] = &[
        ("course.info", false, UndoScope::FullCourse, false),
        ("course.map", false, UndoScope::FullCourse, false),
        ("course.validate", false, UndoScope::FullCourse, false),
        ("course.audit", false, UndoScope::FullCourse, false),
        ("course.update", true, UndoScope::CourseHeader, false),
        ("course.set_slide_size", true, UndoScope::FullCourse, false),
        ("course.set_title", true, UndoScope::CourseHeader, false),
        ("course.set_player", true, UndoScope::CourseHeader, false),
        (
            "course.clear_slide_player_overrides",
            true,
            UndoScope::FullCourse,
            false,
        ),
        ("course.set_quiz", true, UndoScope::CourseHeader, false),
        (
            "course.set_publish_settings",
            true,
            UndoScope::CourseHeader,
            false,
        ),
        ("course.set_tracking", true, UndoScope::CourseHeader, false),
        (
            "course.set_background_audio",
            true,
            UndoScope::CourseHeader,
            false,
        ),
        ("theme.set_palette", true, UndoScope::CourseHeader, false),
        (
            "theme.set_palette_color",
            true,
            UndoScope::FullCourse,
            false,
        ),
        ("course.recolor", true, UndoScope::FullCourse, false),
        ("text_style.create", true, UndoScope::FullCourse, false),
        ("text_style.update", true, UndoScope::FullCourse, false),
        ("text_style.delete", true, UndoScope::FullCourse, false),
        ("text_style.apply", true, UndoScope::SingleSlide, true),
        (
            "course.add_finish_button",
            true,
            UndoScope::FullCourse,
            false,
        ),
        ("course.set_theme", true, UndoScope::CourseHeader, false),
        ("course.add_font", true, UndoScope::CourseHeader, false),
        ("course.remove_font", true, UndoScope::CourseHeader, false),
        ("course.export_xliff", false, UndoScope::FullCourse, false),
        ("course.import_xliff", true, UndoScope::FullCourse, false),
        ("edit.undo", false, UndoScope::FullCourse, false),
        ("edit.redo", false, UndoScope::FullCourse, false),
        ("edit.history", false, UndoScope::FullCourse, false),
        ("scene.add", true, UndoScope::FullCourse, false),
        ("scene.update", true, UndoScope::FullCourse, false),
        ("scene.delete", true, UndoScope::FullCourse, false),
        ("slide.add", true, UndoScope::FullCourse, false),
        ("slide.update", true, UndoScope::SingleSlide, true),
        ("slide.delete", true, UndoScope::FullCourse, false),
        ("slide.move", true, UndoScope::FullCourse, false),
        ("slide.duplicate", true, UndoScope::FullCourse, false),
        ("slide.paste", true, UndoScope::FullCourse, false),
        ("slide.set_duration", true, UndoScope::SingleSlide, true),
        ("slide.set_background", true, UndoScope::SingleSlide, true),
        ("slide.set_transition", true, UndoScope::SingleSlide, true),
        (
            "slide.set_transition_all",
            true,
            UndoScope::FullCourse,
            false,
        ),
        (
            "slide.set_player_overrides",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        ("master.add", true, UndoScope::FullCourse, false),
        ("master.delete", true, UndoScope::FullCourse, false),
        ("slide.set_master", true, UndoScope::FullCourse, false),
        ("bank.create", true, UndoScope::FullCourse, false),
        ("bank.update", true, UndoScope::FullCourse, false),
        ("bank.delete", true, UndoScope::FullCourse, false),
        ("bank.add_question", true, UndoScope::FullCourse, false),
        ("bank.import_csv", true, UndoScope::FullCourse, false),
        ("bank.move_slide", true, UndoScope::FullCourse, false),
        ("bank.remove_slide", true, UndoScope::FullCourse, false),
        ("draw.insert", true, UndoScope::FullCourse, false),
        ("draw.update", true, UndoScope::SingleSlide, false),
        ("slide.add_from_layout", true, UndoScope::FullCourse, false),
        ("course.import_outline", true, UndoScope::FullCourse, false),
        ("template.save", true, UndoScope::FullCourse, false),
        ("template.add_slide", true, UndoScope::FullCourse, false),
        ("template.delete", true, UndoScope::FullCourse, false),
        ("layer.add", true, UndoScope::SingleSlide, true),
        ("layer.update", true, UndoScope::SingleSlide, true),
        ("layer.delete", true, UndoScope::SingleSlide, true),
        ("layer.move", true, UndoScope::SingleSlide, true),
        ("layer.duplicate", true, UndoScope::SingleSlide, true),
        ("object.add", true, UndoScope::SingleSlide, true),
        ("object.update", true, UndoScope::SingleSlide, true),
        ("object.reorder", true, UndoScope::SingleSlide, true),
        ("object.align", true, UndoScope::SingleSlide, true),
        ("object.distribute", true, UndoScope::SingleSlide, true),
        ("object.match_size", true, UndoScope::SingleSlide, true),
        ("object.group", true, UndoScope::SingleSlide, true),
        ("object.ungroup", true, UndoScope::SingleSlide, true),
        ("group.set_interactive", true, UndoScope::SingleSlide, true),
        ("group.update", true, UndoScope::SingleSlide, true),
        ("object.duplicate", true, UndoScope::SingleSlide, true),
        ("object.paste", true, UndoScope::SingleSlide, true),
        ("object.set_frame", true, UndoScope::SingleSlide, true),
        ("object.set_rotation", true, UndoScope::SingleSlide, true),
        ("object.flip", true, UndoScope::SingleSlide, true),
        ("object.lock", true, UndoScope::SingleSlide, true),
        ("object.hide_in_editor", true, UndoScope::SingleSlide, true),
        ("view.set", true, UndoScope::CourseHeader, false),
        ("view.guides.add", true, UndoScope::CourseHeader, false),
        ("view.guides.move", true, UndoScope::CourseHeader, false),
        ("view.guides.remove", true, UndoScope::CourseHeader, false),
        ("object.set_opacity", true, UndoScope::SingleSlide, true),
        (
            "object.set_outline_join",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        ("object.set_fill_style", true, UndoScope::SingleSlide, true),
        ("object.set_shadow", true, UndoScope::SingleSlide, true),
        ("object.set_glow", true, UndoScope::SingleSlide, true),
        ("object.apply_style", true, UndoScope::SingleSlide, true),
        ("object.set_drag", true, UndoScope::SingleSlide, true),
        ("object.set_lottie", true, UndoScope::SingleSlide, true),
        ("object.set_web_source", true, UndoScope::SingleSlide, true),
        ("object.set_input", true, UndoScope::SingleSlide, true),
        ("object.set_marker", true, UndoScope::SingleSlide, true),
        ("object.set_panel", true, UndoScope::SingleSlide, true),
        (
            "object.set_scroll_panel",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        ("object.set_timer", true, UndoScope::SingleSlide, true),
        ("object.set_meter", true, UndoScope::SingleSlide, true),
        ("object.set_path", true, UndoScope::SingleSlide, true),
        ("object.set_crop", true, UndoScope::SingleSlide, true),
        ("object.set_captions", true, UndoScope::SingleSlide, true),
        ("object.set_media", true, UndoScope::SingleSlide, true),
        ("video.add_cue", true, UndoScope::SingleSlide, true),
        ("video.set_cue", true, UndoScope::SingleSlide, true),
        ("video.remove_cue", true, UndoScope::SingleSlide, true),
        (
            "video.add_question_cue",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("slide.add_cue", true, UndoScope::SingleSlide, true),
        ("slide.set_cue", true, UndoScope::SingleSlide, true),
        ("slide.remove_cue", true, UndoScope::SingleSlide, true),
        ("object.add_state", true, UndoScope::SingleSlide, true),
        ("object.update_state", true, UndoScope::SingleSlide, true),
        ("object.patch_state", true, UndoScope::SingleSlide, true),
        ("object.delete_state", true, UndoScope::SingleSlide, true),
        ("object.duplicate_state", true, UndoScope::SingleSlide, true),
        ("object.rename_state", true, UndoScope::SingleSlide, true),
        ("object.reorder_state", true, UndoScope::SingleSlide, true),
        ("object.state_add_extra", true, UndoScope::SingleSlide, true),
        (
            "object.state_update_extra",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        (
            "object.state_delete_extra",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        (
            "object.state_reorder_extra",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        ("object.delete", true, UndoScope::SingleSlide, true),
        ("animation.effects", false, UndoScope::FullCourse, false),
        ("object.set_animation", true, UndoScope::SingleSlide, true),
        ("object.add_emphasis", true, UndoScope::SingleSlide, true),
        ("object.set_emphasis", true, UndoScope::SingleSlide, true),
        ("object.remove_emphasis", true, UndoScope::SingleSlide, true),
        ("object.copy_animation", true, UndoScope::SingleSlide, true),
        ("object.add_motion_path", true, UndoScope::SingleSlide, true),
        (
            "object.update_motion_path",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        (
            "object.remove_motion_path",
            true,
            UndoScope::SingleSlide,
            true,
        ),
        ("text.set", true, UndoScope::SingleSlide, true),
        ("text.style_range", true, UndoScope::SingleSlide, true),
        ("text.clear_formatting", true, UndoScope::SingleSlide, true),
        ("text.change_case", true, UndoScope::SingleSlide, true),
        ("text.set_paragraph", true, UndoScope::SingleSlide, true),
        ("text.set_paragraphs", true, UndoScope::SingleSlide, true),
        ("text.set_link", true, UndoScope::SingleSlide, true),
        ("object.set_autofit", true, UndoScope::SingleSlide, true),
        ("table.set_cell", true, UndoScope::SingleSlide, true),
        ("table.insert_row", true, UndoScope::SingleSlide, true),
        ("table.delete_row", true, UndoScope::SingleSlide, true),
        ("table.insert_column", true, UndoScope::SingleSlide, true),
        ("table.delete_column", true, UndoScope::SingleSlide, true),
        ("table.resize", true, UndoScope::SingleSlide, true),
        ("table.merge", true, UndoScope::SingleSlide, true),
        ("table.split", true, UndoScope::SingleSlide, true),
        ("table.set_borders", true, UndoScope::SingleSlide, true),
        ("table.set_header_row", true, UndoScope::SingleSlide, true),
        ("trigger.add", true, UndoScope::SingleSlide, false),
        ("trigger.update", true, UndoScope::SingleSlide, false),
        ("trigger.move", true, UndoScope::SingleSlide, false),
        ("trigger.delete", true, UndoScope::SingleSlide, false),
        ("resource.add", true, UndoScope::CourseHeader, false),
        ("resource.update", true, UndoScope::CourseHeader, false),
        ("resource.remove", true, UndoScope::CourseHeader, false),
        ("resource.move", true, UndoScope::CourseHeader, false),
        ("glossary.add", true, UndoScope::CourseHeader, false),
        ("glossary.update", true, UndoScope::CourseHeader, false),
        ("glossary.remove", true, UndoScope::CourseHeader, false),
        ("glossary.move", true, UndoScope::CourseHeader, false),
        ("glossary.sort", true, UndoScope::CourseHeader, false),
        ("variable.add", true, UndoScope::CourseHeader, false),
        ("variable.set_value", true, UndoScope::CourseHeader, false),
        ("variable.delete", true, UndoScope::FullCourse, false),
        ("interaction.insert", true, UndoScope::FullCourse, false),
        ("question.insert", true, UndoScope::SingleSlide, false),
        ("results.insert", true, UndoScope::FullCourse, false),
        ("certificate.insert", true, UndoScope::FullCourse, false),
        (
            "question.add_submit_button",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.add_feedback_layers",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.add_try_again_layer",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("question.set", true, UndoScope::SingleSlide, false),
        ("question.delete", true, UndoScope::SingleSlide, false),
        ("question.set_kind", true, UndoScope::SingleSlide, false),
        ("question.add_accepted", true, UndoScope::SingleSlide, false),
        (
            "question.remove_accepted",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.update_accepted",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.set_case_sensitive",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("question.add_pair", true, UndoScope::SingleSlide, false),
        ("question.remove_pair", true, UndoScope::SingleSlide, false),
        ("question.update_pair", true, UndoScope::SingleSlide, false),
        ("question.reorder_pair", true, UndoScope::SingleSlide, false),
        (
            "question.reorder_sequence",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.set_hotspot_image",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("question.add_region", true, UndoScope::SingleSlide, false),
        (
            "question.update_region",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.remove_region",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("question.set_markers", true, UndoScope::SingleSlide, false),
        (
            "question.add_drag_item",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.update_drag_item",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.remove_drag_item",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.set_drop_targets",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        (
            "question.set_drag_options",
            true,
            UndoScope::SingleSlide,
            false,
        ),
        ("course.find", false, UndoScope::FullCourse, false),
        ("course.replace", true, UndoScope::FullCourse, false),
    ];

    let actual: Vec<(&str, bool, UndoScope, bool)> = reg
        .iter()
        .map(|cmd| {
            (
                cmd.id,
                cmd.mutates,
                cmd.scope(),
                cmd.can_use_slide_validation(),
            )
        })
        .collect();
    assert_eq!(actual.as_slice(), EXPECTED);

    let mut table = String::new();
    for cmd in reg {
        let slide_val = cmd.can_use_slide_validation();
        table.push_str(&format!(
            "{}\t{}\t{}\t{:?}\t{}\t{}\t{:?}\t{}\n",
            cmd.id,
            cmd.label,
            cmd.menu,
            cmd.shortcut,
            cmd.params,
            cmd.mutates,
            cmd.scope(),
            slide_val
        ));
    }
    assert_eq!(table.len(), 25_847, "registry golden table byte length");
    assert_eq!(
        fnv1a_64(table.as_bytes()),
        498_912_864_581_733_648,
        "registry golden table FNV-1a hash"
    );
}

#[test]
fn r5_02_interaction_insert_golden_all_eight_kinds() {
    type Expected = (
        &'static str,
        u64,
        usize,
        u64,
        usize,
        usize,
        usize,
        usize,
        usize,
        Option<&'static str>,
        Option<&'static str>,
        Option<&'static str>,
        Option<&'static str>,
    );

    const EXPECTED: &[Expected] = &[
        (
            "tabs",
            0x8229_23c6_b2f7_4d9a,
            223,
            0xb24d_ef9c_81d3_6c76,
            9571,
            3,
            3,
            0,
            13,
            Some("tab_button1"),
            Some("tab_button3"),
            Some("tab_layer1"),
            Some("tab_layer3"),
        ),
        (
            "accordion",
            0xed37_98ae_b36e_3636,
            460,
            0x6e5c_8ecd_32d7_4669,
            16858,
            3,
            3,
            0,
            3,
            Some("accordion_header1"),
            Some("accordion_header3"),
            Some("accordion_layer11"),
            Some("accordion_layer31"),
        ),
        (
            "flip_cards",
            0x3b23_f9ad_626f_2cb2,
            172,
            0x1c45_77d7_939b_9ee0,
            8378,
            3,
            0,
            3,
            9,
            Some("flip_card1"),
            Some("flip_card3"),
            None,
            None,
        ),
        (
            "hotspot_reveal",
            0xd0fc_58aa_f9ce_d2bb,
            522,
            0xf76c_230c_92df_9d57,
            17172,
            4,
            3,
            0,
            6,
            Some("hotspot_picture1"),
            Some("hotspot_marker3"),
            Some("hotspot_layer1"),
            Some("hotspot_layer3"),
        ),
        (
            "process_steps",
            0x4f93_b7a3_a657_6f9b,
            720,
            0x1891_058d_2117_b5a0,
            34043,
            13,
            4,
            1,
            29,
            Some("step_line1"),
            Some("step_next1"),
            Some("step_layer1"),
            Some("step_layer4"),
        ),
        (
            "timeline",
            0xaf44_cbac_ec25_ccff,
            1059,
            0xddf5_f6c8_45a1_1c8d,
            27602,
            16,
            5,
            0,
            10,
            Some("timeline_line1"),
            Some("timeline_title5"),
            Some("timeline_layer1"),
            Some("timeline_layer5"),
        ),
        (
            "reveal_grid",
            0x5a9d_2d57_e41a_5620,
            410,
            0xa7a4_295f_475c_90d4,
            22524,
            7,
            0,
            7,
            31,
            Some("grid_tile_shape1"),
            Some("grid_counter1"),
            None,
            None,
        ),
        (
            "scenario_choice",
            0x6715_89b0_5895_80ad,
            605,
            0xf2b0_c8ee_d764_2f0e,
            15097,
            6,
            3,
            0,
            3,
            Some("scenario_panel1"),
            Some("scenario_choice3"),
            Some("scenario_layer1"),
            Some("scenario_layer3"),
        ),
    ];

    for &(
        kind,
        exp_ret_hash,
        exp_ret_len,
        exp_course_hash,
        exp_course_len,
        exp_objs,
        exp_layers,
        exp_vars,
        exp_triggers,
        exp_first_obj,
        exp_last_obj,
        exp_first_layer,
        exp_last_layer,
    ) in EXPECTED
    {
        let mut s = Session::blank("Interaction Golden");
        let ret = ok(
            &mut s,
            "interaction.insert",
            json!({ "slide": "slide1", "kind": kind }),
        );
        let ret_str = serde_json::to_string(&ret).expect("serialize return");
        let course_str = s.course.to_json().expect("serialize course");
        let slide = s.course.slide("slide1").expect("slide1 exists");

        assert_eq!(ret_str.len(), exp_ret_len, "{kind}: return json len");
        assert_eq!(
            fnv1a_64(ret_str.as_bytes()),
            exp_ret_hash,
            "{kind}: return json hash"
        );
        assert_eq!(course_str.len(), exp_course_len, "{kind}: course json len");
        assert_eq!(
            fnv1a_64(course_str.as_bytes()),
            exp_course_hash,
            "{kind}: course json hash"
        );
        assert_eq!(slide.objects.len(), exp_objs, "{kind}: object count");
        assert_eq!(slide.layers.len(), exp_layers, "{kind}: layer count");
        assert_eq!(s.course.variables.len(), exp_vars, "{kind}: variable count");
        assert_eq!(slide.triggers.len(), exp_triggers, "{kind}: trigger count");
        assert_eq!(
            slide.objects.first().map(|o| o.id.as_str()),
            exp_first_obj,
            "{kind}: first object id"
        );
        assert_eq!(
            slide.objects.last().map(|o| o.id.as_str()),
            exp_last_obj,
            "{kind}: last object id"
        );
        assert_eq!(
            slide.layers.first().map(|l| l.id.as_str()),
            exp_first_layer,
            "{kind}: first layer id"
        );
        assert_eq!(
            slide.layers.last().map(|l| l.id.as_str()),
            exp_last_layer,
            "{kind}: last layer id"
        );
    }
}

#[test]
fn grid_spacing_is_a_view_setting_that_undoes_and_rejects_nonsense() {
    let mut s = Session::blank("Grid");
    assert!((s.course.editor.grid_step() - 20.0).abs() < f32::EPSILON);
    ok(&mut s, "view.set", json!({ "grid_spacing": 10.0 }));
    assert_eq!(s.course.editor.grid_spacing, Some(10.0));
    assert!((s.course.editor.grid_step() - 10.0).abs() < f32::EPSILON);
    // The default spacing is stored as none, so the file stays as it was.
    ok(&mut s, "view.set", json!({ "grid_spacing": 20.0 }));
    assert_eq!(s.course.editor.grid_spacing, None);
    s.undo().expect("undo");
    assert_eq!(s.course.editor.grid_spacing, Some(10.0));
    for bad in [0.0, 1.0, 1000.0, -5.0] {
        assert!(
            s.execute("view.set", &json!({ "grid_spacing": bad }))
                .is_err(),
            "{bad} accepted"
        );
    }
    // A hand-edited file with a useless spacing draws the default grid.
    s.course.editor.grid_spacing = Some(f32::NAN);
    assert!((s.course.editor.grid_step() - 20.0).abs() < f32::EPSILON);
}

#[test]
fn transition_apply_to_all_sets_every_slide_in_one_undo_step() {
    let mut s = Session::blank("Transitions");
    ok(&mut s, "slide.add", json!({}));
    ok(&mut s, "slide.add", json!({}));
    let fade = json!({ "type": "fade", "duration_ms": 800 });
    ok(
        &mut s,
        "slide.set_transition_all",
        json!({ "transition": fade }),
    );
    let all = |s: &Session| {
        s.course
            .scenes
            .iter()
            .flat_map(|sc| sc.slides.iter())
            .map(|sl| sl.transition.as_ref().map(|t| t.duration_ms))
            .collect::<Vec<_>>()
    };
    assert!(all(&s).len() >= 3);
    assert!(all(&s).iter().all(|d| *d == Some(800)));
    s.undo().expect("undo");
    assert!(all(&s).iter().all(Option::is_none));
    assert!(
        s.execute(
            "slide.set_transition_all",
            &json!({ "transition": { "type": "fade", "duration_ms": 0 } })
        )
        .is_err()
    );
}

#[test]
fn pt_03_headless_point_text_add_and_refit() {
    use coursecraft_course::{Align, Autofit};

    let mut s = Session::blank("Point Text");
    let added = ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "lbl",
            "kind": {
                "type": "text",
                "text": "Next",
                "point_text": true,
                "autofit": "shrink",
                "style": { "wrap": true, "size": 20.0 }
            },
            "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 60.0 }
        }),
    );
    assert_eq!(added["id"], "lbl");

    let (next_frame, next_point, next_wrap, next_autofit) = {
        let slide = s.course.slide("slide1").expect("slide1");
        let (obj, _) = slide.object("lbl").expect("lbl");
        match &obj.kind {
            ObjectKind::Text {
                point_text,
                style,
                autofit,
                ..
            } => (obj.frame, *point_text, style.wrap, *autofit),
            other => panic!("expected text, got {other:?}"),
        }
    };
    assert!(next_point, "point_text must stay true on add");
    assert!(!next_wrap, "point_text must force style.wrap = false");
    assert_eq!(
        next_autofit,
        Autofit::None,
        "point_text must force autofit = None"
    );
    assert_eq!(next_frame.x, 100.0, "initial add keeps x top-left anchor");
    assert_eq!(next_frame.y, 100.0, "initial add keeps y top-left anchor");
    assert!(
        next_frame.w < 120.0 && next_frame.w >= 20.0,
        "fitted 'Next' width ({}) must be much smaller than 300",
        next_frame.w
    );
    assert!(
        next_frame.h < 50.0 && next_frame.h >= 20.0,
        "fitted 'Next' height ({}) must be smaller than 60",
        next_frame.h
    );

    // Growing text widens frame.w on one line (same height).
    ok(
        &mut s,
        "text.set",
        json!({ "slide": "slide1", "object": "lbl", "text": "Continue to Module 2" }),
    );
    let long_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    assert!(
        long_frame.w > next_frame.w * 2.0,
        "longer text ({}) must widen box beyond 'Next' ({})",
        long_frame.w,
        next_frame.w
    );
    assert_eq!(
        long_frame.h, next_frame.h,
        "single-line text.set must keep height identical"
    );

    // Shrinking back to "Go" narrows frame.w.
    ok(
        &mut s,
        "text.set",
        json!({ "slide": "slide1", "object": "lbl", "text": "Go" }),
    );
    let short_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    assert!(
        short_frame.w < next_frame.w,
        "'Go' ({}) must be narrower than 'Next' ({})",
        short_frame.w,
        next_frame.w
    );
    assert_eq!(short_frame.h, next_frame.h);

    // Align::Center preserves center X across text edits.
    ok(
        &mut s,
        "text.set_paragraph",
        json!({ "slide": "slide1", "object": "lbl", "align": "center" }),
    );
    let before_center = {
        let f = s
            .course
            .slide("slide1")
            .unwrap()
            .object("lbl")
            .unwrap()
            .0
            .frame;
        assert_eq!(
            match &s
                .course
                .slide("slide1")
                .unwrap()
                .object("lbl")
                .unwrap()
                .0
                .kind
            {
                ObjectKind::Text { style, .. } => style.align,
                _ => panic!("expected text"),
            },
            Align::Center
        );
        f.x + f.w * 0.5
    };
    ok(
        &mut s,
        "text.set",
        json!({ "slide": "slide1", "object": "lbl", "text": "Centered Heading Label" }),
    );
    let centered_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    let after_center = centered_frame.x + centered_frame.w * 0.5;
    assert!(
        (after_center - before_center).abs() <= 1.0,
        "Align::Center must preserve center X: before={before_center}, after={after_center}"
    );

    // text.style_range increasing font size grows both width and height.
    ok(
        &mut s,
        "text.style_range",
        json!({
            "slide": "slide1",
            "object": "lbl",
            "start": 0,
            "end": 8,
            "size": 44.0
        }),
    );
    let big_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    assert!(
        big_frame.w > centered_frame.w && big_frame.h > centered_frame.h,
        "text.style_range increasing size must grow frame: before={centered_frame:?}, after={big_frame:?}"
    );
}

#[test]
fn pt_03_paragraphs_apply_style_and_hostile_measured_size() {
    let mut s = Session::blank("Point Text Refit");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "lbl",
            "kind": {
                "type": "text",
                "text": "Heading Label",
                "point_text": true,
                "style": { "size": 28.0 }
            },
            "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 60.0 }
        }),
    );
    let base_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;

    // text.set_paragraphs increasing line_spacing and adding bullet indent grows frame.
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({
            "slide": "slide1",
            "object": "lbl",
            "list": "bullet",
            "line_spacing": 2.0
        }),
    );
    let para_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    assert!(
        para_frame.w > base_frame.w && para_frame.h > base_frame.h,
        "text.set_paragraphs must refit point text: before={base_frame:?}, after={para_frame:?}"
    );

    // object.apply_style copies style and refits while keeping point_text = true, wrap = false.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "src_area",
            "kind": {
                "type": "text",
                "text": "Source",
                "style": { "size": 14.0, "wrap": true }
            },
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 40.0 }
        }),
    );
    ok(
        &mut s,
        "object.apply_style",
        json!({
            "slide": "slide1",
            "source_slide": "slide1",
            "source": "src_area",
            "object": "lbl"
        }),
    );
    let (styled_frame, still_point, still_nowrap) = {
        let (obj, _) = s.course.slide("slide1").unwrap().object("lbl").unwrap();
        match &obj.kind {
            ObjectKind::Text {
                point_text, style, ..
            } => (obj.frame, *point_text, !style.wrap),
            _ => panic!("expected text"),
        }
    };
    assert!(still_point && still_nowrap);
    assert!(
        styled_frame.h < para_frame.h,
        "applying smaller text style must shrink fitted height"
    );

    // Hostile measured_size values never panic: invalid ones fall back to skrifa, huge ones clamp.
    for hostile in [
        json!({ "w": -10.0, "h": 20.0 }),
        json!({ "w": 0.0, "h": 20.0 }),
        json!({ "w": "bad", "h": 20.0 }),
        json!(null),
    ] {
        ok(
            &mut s,
            "text.set",
            json!({
                "slide": "slide1",
                "object": "lbl",
                "text": "Safe",
                "measured_size": hostile
            }),
        );
        let f = s
            .course
            .slide("slide1")
            .unwrap()
            .object("lbl")
            .unwrap()
            .0
            .frame;
        assert!(f.w.is_finite() && (20.0..=10_000.0).contains(&f.w));
        assert!(f.h.is_finite() && (1.0..=10_000.0).contains(&f.h));
    }
    ok(
        &mut s,
        "text.set",
        json!({
            "slide": "slide1",
            "object": "lbl",
            "text": "Clamped",
            "measured_size": { "w": 999_999.0, "h": 999_999.0 }
        }),
    );
    let clamped = s
        .course
        .slide("slide1")
        .unwrap()
        .object("lbl")
        .unwrap()
        .0
        .frame;
    assert_eq!(clamped.w, 10_000.0);
    assert_eq!(clamped.h, 10_000.0);
}

#[test]
fn pt_04_point_text_converts_to_area_text() {
    use coursecraft_course::Autofit;

    let mut s = Session::blank("Convert Point Text");
    for id in ["t_wrap", "t_resize", "t_shrink", "t_update"] {
        ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "id": id,
                "kind": { "type": "text", "text": "Hello", "point_text": true },
                "frame": { "x": 50.0, "y": 50.0, "w": 300.0, "h": 60.0 }
            }),
        );
    }

    // (a) text.set_paragraph { wrap: true } -> point_text = false, wrap = true, width preserved.
    let wrap_before = s
        .course
        .slide("slide1")
        .unwrap()
        .object("t_wrap")
        .unwrap()
        .0
        .frame;
    ok(
        &mut s,
        "text.set_paragraph",
        json!({ "slide": "slide1", "object": "t_wrap", "wrap": true }),
    );
    let (obj_wrap, _) = s.course.slide("slide1").unwrap().object("t_wrap").unwrap();
    assert_eq!(
        obj_wrap.frame.w, wrap_before.w,
        "wrap-on must preserve width"
    );
    match &obj_wrap.kind {
        ObjectKind::Text {
            point_text, style, ..
        } => {
            assert!(!*point_text, "wrap=true must clear point_text");
            assert!(style.wrap, "wrap must be true");
        }
        other => panic!("expected text, got {other:?}"),
    }

    // (b) object.set_frame changing w -> point_text = false, wrap = true.
    let resize_before = s
        .course
        .slide("slide1")
        .unwrap()
        .object("t_resize")
        .unwrap()
        .0
        .frame;
    let new_w = resize_before.w + 120.0;
    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": "t_resize",
            "frame": {
                "x": resize_before.x,
                "y": resize_before.y,
                "w": new_w,
                "h": resize_before.h
            }
        }),
    );
    let (obj_resize, _) = s
        .course
        .slide("slide1")
        .unwrap()
        .object("t_resize")
        .unwrap();
    assert_eq!(obj_resize.frame.w, new_w);
    match &obj_resize.kind {
        ObjectKind::Text {
            point_text, style, ..
        } => {
            assert!(!*point_text, "width resize must clear point_text");
            assert!(style.wrap, "width resize must enable wrap");
        }
        other => panic!("expected text, got {other:?}"),
    }

    // (c) object.set_autofit { autofit: "shrink" } -> point_text = false.
    ok(
        &mut s,
        "object.set_autofit",
        json!({ "slide": "slide1", "object": "t_shrink", "autofit": "shrink" }),
    );
    let (obj_shrink, _) = s
        .course
        .slide("slide1")
        .unwrap()
        .object("t_shrink")
        .unwrap();
    match &obj_shrink.kind {
        ObjectKind::Text {
            point_text,
            autofit,
            ..
        } => {
            assert!(!*point_text, "autofit=shrink must clear point_text");
            assert_eq!(*autofit, Autofit::Shrink);
        }
        other => panic!("expected text, got {other:?}"),
    }

    // (d) object.update setting wrap: true clears point_text.
    ok(
        &mut s,
        "object.update",
        json!({
            "slide": "slide1",
            "object": "t_update",
            "kind": {
                "type": "text",
                "text": "Hello",
                "point_text": true,
                "style": { "wrap": true }
            }
        }),
    );
    let (obj_upd, _) = s
        .course
        .slide("slide1")
        .unwrap()
        .object("t_update")
        .unwrap();
    match &obj_upd.kind {
        ObjectKind::Text {
            point_text, style, ..
        } => {
            assert!(
                !*point_text,
                "object.update with wrap=true must clear point_text"
            );
            assert!(style.wrap);
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn pt_05_move_align_distribute_preserve_point_text() {
    let mut s = Session::blank("Move Align Distribute");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "btn",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
            "frame": { "x": 200.0, "y": 300.0, "w": 160.0, "h": 48.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "next_lbl",
            "kind": { "type": "text", "text": "Next", "point_text": true },
            "frame": { "x": 10.0, "y": 10.0, "w": 300.0, "h": 60.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "back_lbl",
            "kind": { "type": "text", "text": "Back", "point_text": true },
            "frame": { "x": 40.0, "y": 10.0, "w": 300.0, "h": 60.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "mid_lbl",
            "kind": { "type": "text", "text": "Save", "point_text": true },
            "frame": { "x": 500.0, "y": 10.0, "w": 300.0, "h": 60.0 }
        }),
    );

    // 1. Pure move via object.set_frame preserves point_text = true and wrap = false.
    let before = s
        .course
        .slide("slide1")
        .unwrap()
        .object("next_lbl")
        .unwrap()
        .0
        .frame;
    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": "next_lbl",
            "frame": { "x": 220.0, "y": 310.0, "w": before.w, "h": before.h }
        }),
    );
    let (moved_obj, _) = s
        .course
        .slide("slide1")
        .unwrap()
        .object("next_lbl")
        .unwrap();
    assert_eq!(moved_obj.frame.x, 220.0);
    assert_eq!(moved_obj.frame.y, 310.0);
    assert!(matches!(
        &moved_obj.kind,
        ObjectKind::Text { point_text: true, style, .. } if !style.wrap
    ));

    // 2. Aligning "Next" and "Back" onto the button centers accurately and preserves point_text.
    for lbl in ["next_lbl", "back_lbl"] {
        ok(
            &mut s,
            "object.align",
            json!({ "slide": "slide1", "objects": ["btn", lbl], "edge": "center" }),
        );
        ok(
            &mut s,
            "object.align",
            json!({ "slide": "slide1", "objects": ["btn", lbl], "edge": "middle" }),
        );
        let btn_f = s
            .course
            .slide("slide1")
            .unwrap()
            .object("btn")
            .unwrap()
            .0
            .frame;
        let (lbl_obj, _) = s.course.slide("slide1").unwrap().object(lbl).unwrap();
        let btn_cx = btn_f.x + btn_f.w * 0.5;
        let btn_cy = btn_f.y + btn_f.h * 0.5;
        let lbl_cx = lbl_obj.frame.x + lbl_obj.frame.w * 0.5;
        let lbl_cy = lbl_obj.frame.y + lbl_obj.frame.h * 0.5;
        assert!(
            (lbl_cx - btn_cx).abs() < 1.0 && (lbl_cy - btn_cy).abs() < 1.0,
            "{lbl} must center on btn: btn=({btn_cx}, {btn_cy}), lbl=({lbl_cx}, {lbl_cy})"
        );
        assert!(matches!(
            &lbl_obj.kind,
            ObjectKind::Text { point_text: true, style, .. } if !style.wrap
        ));
    }

    // 3. Distributing 3 point-text objects preserves point_text and wrap = false.
    let back_f = s
        .course
        .slide("slide1")
        .unwrap()
        .object("back_lbl")
        .unwrap()
        .0
        .frame;
    ok(
        &mut s,
        "object.set_frame",
        json!({
            "slide": "slide1",
            "object": "back_lbl",
            "frame": { "x": 40.0, "y": 100.0, "w": back_f.w, "h": back_f.h }
        }),
    );
    ok(
        &mut s,
        "object.distribute",
        json!({
            "slide": "slide1",
            "objects": ["back_lbl", "next_lbl", "mid_lbl"],
            "axis": "horizontal"
        }),
    );
    for lbl in ["back_lbl", "next_lbl", "mid_lbl"] {
        let (obj, _) = s.course.slide("slide1").unwrap().object(lbl).unwrap();
        assert!(
            matches!(
                &obj.kind,
                ObjectKind::Text { point_text: true, style, .. } if !style.wrap
            ),
            "{lbl} must remain point text after distribute"
        );
    }
}

#[test]
fn pt_06_state_extra_point_text_fits_and_converts() {
    use coursecraft_course::{Autofit, StateExtraKind};

    let mut s = Session::blank("State Extra Point Text");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "card",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#EEEEEE" },
            "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 }
        }),
    );
    ok(
        &mut s,
        "object.add_state",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": { "name": "hover" }
        }),
    );

    // 1. Add point-text state extra: fits on insert and normalizes wrap=false, autofit=None.
    let res = ok(
        &mut s,
        "object.state_add_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": {
                "id": "badge_txt",
                "name": "Badge",
                "kind": {
                    "type": "text",
                    "text": "New",
                    "point_text": true,
                    "autofit": "shrink",
                    "style": { "wrap": true, "size": 18.0 }
                },
                "frame": { "x": 10.0, "y": 10.0, "w": 300.0, "h": 60.0 }
            }
        }),
    );
    assert_eq!(res["id"], "badge_txt");

    let get_extra = |s: &Session, extra_id: &str| {
        let (obj, _) = s.course.slide("slide1").unwrap().object("card").unwrap();
        let st = obj.states.iter().find(|st| st.name == "hover").unwrap();
        st.extras.iter().find(|e| e.id == extra_id).unwrap().clone()
    };

    let added_extra = get_extra(&s, "badge_txt");
    assert!(added_extra.frame.w < 120.0 && added_extra.frame.w >= 20.0);
    assert!(added_extra.frame.h < 50.0);
    match &added_extra.kind {
        StateExtraKind::Text {
            point_text,
            style,
            autofit,
            ..
        } => {
            assert!(*point_text);
            assert!(!style.wrap);
            assert_eq!(*autofit, Autofit::None);
        }
        other => panic!("expected text extra, got {other:?}"),
    }

    // 2. Update text on point-text extra refits its frame.
    ok(
        &mut s,
        "object.state_update_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": "badge_txt",
            "patch": {
                "kind": {
                    "type": "text",
                    "text": "Featured Module",
                    "point_text": true,
                    "style": { "wrap": false, "size": 18.0 }
                }
            }
        }),
    );
    let grown_extra = get_extra(&s, "badge_txt");
    assert!(
        grown_extra.frame.w > added_extra.frame.w * 2.0,
        "updating extra text must widen fitted frame"
    );
    assert!(matches!(
        &grown_extra.kind,
        StateExtraKind::Text { point_text: true, style, .. } if !style.wrap
    ));

    // 3. Turning wrap on converts extra to area text at current width.
    ok(
        &mut s,
        "object.state_update_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": "badge_txt",
            "patch": {
                "kind": {
                    "type": "text",
                    "text": "Featured Module",
                    "point_text": true,
                    "style": { "wrap": true, "size": 18.0 }
                }
            }
        }),
    );
    let wrapped_extra = get_extra(&s, "badge_txt");
    assert_eq!(wrapped_extra.frame.w, grown_extra.frame.w);
    assert!(matches!(
        &wrapped_extra.kind,
        StateExtraKind::Text { point_text: false, style, .. } if style.wrap
    ));

    // 4. Second point-text extra converts on width resize (and preserves on pure move).
    ok(
        &mut s,
        "object.state_add_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": {
                "id": "resize_txt",
                "name": "Tag",
                "kind": {
                    "type": "text",
                    "text": "Tip",
                    "point_text": true,
                    "style": { "wrap": false, "size": 18.0 }
                },
                "frame": { "x": 10.0, "y": 40.0, "w": 300.0, "h": 60.0 }
            }
        }),
    );
    let before_move = get_extra(&s, "resize_txt");
    ok(
        &mut s,
        "object.state_update_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": "resize_txt",
            "patch": {
                "frame": {
                    "x": 25.0,
                    "y": 45.0,
                    "w": before_move.frame.w,
                    "h": before_move.frame.h
                }
            }
        }),
    );
    let after_move = get_extra(&s, "resize_txt");
    assert_eq!(after_move.frame.x, 25.0);
    assert!(matches!(
        &after_move.kind,
        StateExtraKind::Text { point_text: true, style, .. } if !style.wrap
    ));

    let resized_w = after_move.frame.w + 90.0;
    ok(
        &mut s,
        "object.state_update_extra",
        json!({
            "slide": "slide1",
            "object": "card",
            "state": "hover",
            "extra": "resize_txt",
            "patch": {
                "frame": {
                    "x": after_move.frame.x,
                    "y": after_move.frame.y,
                    "w": resized_w,
                    "h": after_move.frame.h
                }
            }
        }),
    );
    let after_resize = get_extra(&s, "resize_txt");
    assert_eq!(after_resize.frame.w, resized_w);
    assert!(matches!(
        &after_resize.kind,
        StateExtraKind::Text { point_text: false, style, .. } if style.wrap
    ));
}

fn pt_11_text_info(
    s: &Session,
    slide_id: &str,
    obj_id: &str,
) -> (coursecraft_course::Rect, bool, bool, f32, Option<String>) {
    let (obj, _) = s.course.slide(slide_id).unwrap().object(obj_id).unwrap();
    match &obj.kind {
        ObjectKind::Text {
            style,
            point_text,
            text_style,
            ..
        } => (
            obj.frame,
            *point_text,
            style.wrap,
            style.size,
            text_style.clone(),
        ),
        other => panic!("expected text, got {other:?}"),
    }
}

fn pt_11_verify_apply_bigger_font(s: &mut Session) -> (String, f32, f32) {
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "pt_center",
            "kind": {
                "type": "text",
                "text": "Heading",
                "point_text": true,
                "style": { "size": 16.0, "wrap": false, "align": "center", "valign": "middle" }
            },
            "frame": { "x": 200.0, "y": 100.0, "w": 300.0, "h": 60.0 }
        }),
    );

    let (before_frame, before_pt, before_wrap, before_size, _) =
        pt_11_text_info(s, "slide1", "pt_center");
    assert!(before_pt);
    assert!(!before_wrap);
    assert_eq!(before_size, 16.0);
    let before_cx = before_frame.x + before_frame.w * 0.5;
    let before_cy = before_frame.y + before_frame.h * 0.5;

    let big_id = ok(
        s,
        "text_style.create",
        json!({
            "name": "Big Display",
            "style": {
                "size": 36.0,
                "bold": true,
                "wrap": false,
                "align": "center",
                "valign": "middle"
            }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    ok(
        s,
        "text_style.apply",
        json!({ "id": big_id, "slide": "slide1", "objects": ["pt_center"] }),
    );
    let (grown_frame, grown_pt, grown_wrap, grown_size, grown_link) =
        pt_11_text_info(s, "slide1", "pt_center");
    assert!(grown_pt);
    assert!(!grown_wrap);
    assert_eq!(grown_size, 36.0);
    assert_eq!(grown_link.as_deref(), Some(big_id.as_str()));
    assert!(grown_frame.w > before_frame.w);
    assert!(grown_frame.h > before_frame.h);
    let after_cx = grown_frame.x + grown_frame.w * 0.5;
    let after_cy = grown_frame.y + grown_frame.h * 0.5;
    assert!((after_cx - before_cx).abs() <= 1.0);
    assert!((after_cy - before_cy).abs() <= 1.0);

    // Single-step undo reverts both style and refitted frame.
    s.undo().unwrap();
    let (undone_frame, undone_pt, undone_wrap, undone_size, undone_link) =
        pt_11_text_info(s, "slide1", "pt_center");
    assert_eq!(undone_frame, before_frame);
    assert!(undone_pt);
    assert!(!undone_wrap);
    assert_eq!(undone_size, 16.0);
    assert_eq!(undone_link, None);
    s.redo().unwrap();
    assert_eq!(pt_11_text_info(s, "slide1", "pt_center").0, grown_frame);

    (big_id, before_cx, before_cy)
}

fn pt_11_verify_apply_wrap_converts(s: &mut Session) {
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "pt_wrap",
            "kind": {
                "type": "text",
                "text": "Convert Me",
                "point_text": true,
                "style": { "size": 18.0, "wrap": false }
            },
            "frame": { "x": 40.0, "y": 200.0, "w": 300.0, "h": 60.0 }
        }),
    );
    let wrap_before = pt_11_text_info(s, "slide1", "pt_wrap").0;
    let body_id = ok(
        s,
        "text_style.create",
        json!({
            "name": "Body Wrapped",
            "style": { "size": 32.0, "wrap": true }
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    ok(
        s,
        "text_style.apply",
        json!({ "id": body_id, "slide": "slide1", "objects": ["pt_wrap"] }),
    );
    let (conv_frame, conv_pt, conv_wrap, conv_size, conv_link) =
        pt_11_text_info(s, "slide1", "pt_wrap");
    assert!(!conv_pt);
    assert!(conv_wrap);
    assert_eq!(conv_size, 32.0);
    assert_eq!(conv_link.as_deref(), Some(body_id.as_str()));
    assert_eq!(conv_frame, wrap_before);

    // Single-step undo restores point text and unlinks style.
    s.undo().unwrap();
    let (u_frame, u_pt, u_wrap, u_size, u_link) = pt_11_text_info(s, "slide1", "pt_wrap");
    assert_eq!(u_frame, wrap_before);
    assert!(u_pt);
    assert!(!u_wrap);
    assert_eq!(u_size, 18.0);
    assert_eq!(u_link, None);
}

fn pt_11_setup_multi_slide_and_extra(
    s: &mut Session,
    big_id: &str,
) -> (coursecraft_course::Rect, coursecraft_course::StateExtra) {
    let area_orig = coursecraft_course::Rect {
        x: 50.0,
        y: 280.0,
        w: 220.0,
        h: 80.0,
    };
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "area_box",
            "kind": {
                "type": "text",
                "text": "Fixed Box",
                "style": { "size": 16.0, "wrap": true }
            },
            "frame": area_orig
        }),
    );
    ok(
        s,
        "text_style.apply",
        json!({ "id": big_id, "slide": "slide1", "objects": ["area_box"] }),
    );
    let (area_frame, area_pt, area_wrap, area_size, _) = pt_11_text_info(s, "slide1", "area_box");
    assert!(!area_pt);
    assert!(!area_wrap);
    assert_eq!(area_size, 36.0);
    assert_eq!(area_frame, area_orig);

    ok(
        s,
        "layer.add",
        json!({ "slide": "slide1", "id": "lay1", "name": "Overlay" }),
    );
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "lay1",
            "id": "pt_layer",
            "kind": {
                "type": "text",
                "text": "Layer Title",
                "point_text": true,
                "style": { "size": 16.0, "wrap": false }
            },
            "frame": { "x": 80.0, "y": 40.0, "w": 200.0, "h": 40.0 }
        }),
    );
    ok(
        s,
        "text_style.apply",
        json!({ "id": big_id, "slide": "slide1", "objects": ["pt_layer"] }),
    );
    ok(s, "slide.add", json!({ "id": "slide2", "title": "S2" }));
    ok(
        s,
        "object.add",
        json!({
            "slide": "slide2",
            "id": "pt_slide2",
            "kind": {
                "type": "text",
                "text": "Slide Two",
                "point_text": true,
                "style": { "size": 16.0, "wrap": false }
            },
            "frame": { "x": 120.0, "y": 60.0, "w": 200.0, "h": 40.0 }
        }),
    );
    ok(
        s,
        "text_style.apply",
        json!({ "id": big_id, "slide": "slide2", "objects": ["pt_slide2"] }),
    );

    // StateExtraKind::Text has no named-style link in the course model; attach one to
    // pt_center to confirm it stays valid and untouched during text_style.update.
    ok(
        s,
        "object.add_state",
        json!({ "slide": "slide1", "object": "pt_center", "state": { "name": "hover" } }),
    );
    ok(
        s,
        "object.state_add_extra",
        json!({
            "slide": "slide1",
            "object": "pt_center",
            "state": "hover",
            "extra": {
                "id": "badge",
                "name": "Badge",
                "kind": {
                    "type": "text",
                    "text": "New",
                    "point_text": true,
                    "style": { "size": 14.0, "wrap": false }
                },
                "frame": { "x": 0.0, "y": -20.0, "w": 120.0, "h": 30.0 }
            }
        }),
    );
    let extra_before = s
        .course
        .slide("slide1")
        .unwrap()
        .object("pt_center")
        .unwrap()
        .0
        .states
        .iter()
        .find(|st| st.name == "hover")
        .unwrap()
        .extras[0]
        .clone();
    (area_orig, extra_before)
}

fn pt_11_verify_update_wrap_converts(
    s: &mut Session,
    big_id: &str,
    expected_pt: &[(&str, &str, coursecraft_course::Rect)],
    area_orig: coursecraft_course::Rect,
) {
    ok(
        s,
        "text_style.update",
        json!({ "id": big_id, "style": { "wrap": true } }),
    );
    for &(sid, oid, expected_frame) in expected_pt {
        let (frame, pt, wrap, _, _) = pt_11_text_info(s, sid, oid);
        assert!(!pt);
        assert!(wrap);
        assert_eq!(frame, expected_frame);
    }
    let (area_frame, area_pt, area_wrap, _, _) = pt_11_text_info(s, "slide1", "area_box");
    assert!(!area_pt);
    assert!(area_wrap);
    assert_eq!(area_frame, area_orig);

    s.undo().unwrap();
    for &(sid, oid, expected_frame) in expected_pt {
        let (frame, pt, wrap, _, _) = pt_11_text_info(s, sid, oid);
        assert!(pt);
        assert!(!wrap);
        assert_eq!(frame, expected_frame);
    }
}

#[test]
fn pt_11_named_style_refits_or_converts_point_text() {
    use coursecraft_course::StateExtraKind;

    let mut s = Session::blank("Named Style Point Text");
    let (big_id, before_cx, before_cy) = pt_11_verify_apply_bigger_font(&mut s);
    pt_11_verify_apply_wrap_converts(&mut s);
    let (area_orig, extra_before) = pt_11_setup_multi_slide_and_extra(&mut s, &big_id);

    let pt_center_36 = pt_11_text_info(&s, "slide1", "pt_center").0;
    let pt_layer_36 = pt_11_text_info(&s, "slide1", "pt_layer").0;
    let pt_slide2_36 = pt_11_text_info(&s, "slide2", "pt_slide2").0;

    let upd = ok(
        &mut s,
        "text_style.update",
        json!({ "id": big_id, "style": { "size": 54.0, "wrap": false } }),
    );
    assert_eq!(upd["updated"], 4);

    let (pt_center_54, c_pt, c_wrap, c_size, _) = pt_11_text_info(&s, "slide1", "pt_center");
    let (pt_layer_54, l_pt, l_wrap, l_size, _) = pt_11_text_info(&s, "slide1", "pt_layer");
    let (pt_slide2_54, s2_pt, s2_wrap, s2_size, _) = pt_11_text_info(&s, "slide2", "pt_slide2");
    let (area_54, a_pt, _, a_size, _) = pt_11_text_info(&s, "slide1", "area_box");

    assert_eq!((c_pt, c_wrap, c_size), (true, false, 54.0));
    assert_eq!((l_pt, l_wrap, l_size), (true, false, 54.0));
    assert_eq!((s2_pt, s2_wrap, s2_size), (true, false, 54.0));
    assert!(pt_center_54.w > pt_center_36.w);
    assert!(pt_center_54.h > pt_center_36.h);
    assert!(pt_layer_54.w > pt_layer_36.w);
    assert!(pt_layer_54.h > pt_layer_36.h);
    assert!(pt_slide2_54.w > pt_slide2_36.w);
    assert!(pt_slide2_54.h > pt_slide2_36.h);
    let cx_54 = pt_center_54.x + pt_center_54.w * 0.5;
    let cy_54 = pt_center_54.y + pt_center_54.h * 0.5;
    assert!((cx_54 - before_cx).abs() <= 1.0);
    assert!((cy_54 - before_cy).abs() <= 1.0);
    assert_eq!((a_pt, a_size, area_54), (false, 54.0, area_orig));

    let extra_after = &s
        .course
        .slide("slide1")
        .unwrap()
        .object("pt_center")
        .unwrap()
        .0
        .states
        .iter()
        .find(|st| st.name == "hover")
        .unwrap()
        .extras[0];
    assert_eq!(extra_after.frame, extra_before.frame);
    assert!(matches!(
        &extra_after.kind,
        StateExtraKind::Text { point_text: true, style, .. } if !style.wrap
    ));

    // Single-step undo reverts all 4 objects and the style definition.
    s.undo().unwrap();
    assert_eq!(pt_11_text_info(&s, "slide1", "pt_center").0, pt_center_36);
    assert_eq!(pt_11_text_info(&s, "slide1", "pt_layer").0, pt_layer_36);
    assert_eq!(pt_11_text_info(&s, "slide2", "pt_slide2").0, pt_slide2_36);
    assert_eq!(pt_11_text_info(&s, "slide1", "area_box").3, 36.0);
    assert_eq!(pt_11_text_info(&s, "slide1", "area_box").0, area_orig);

    // Redo, then update style to wrap: true -> converts all linked point-text users to area text.
    s.redo().unwrap();
    pt_11_verify_update_wrap_converts(
        &mut s,
        &big_id,
        &[
            ("slide1", "pt_center", pt_center_54),
            ("slide1", "pt_layer", pt_layer_54),
            ("slide2", "pt_slide2", pt_slide2_54),
        ],
        area_orig,
    );
}

#[test]
fn gb_12_group_set_interactive_no_silent_migration_and_opt_out_guard() {
    let mut s = Session::blank("Interactive Group No Migration");
    ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "id": "l1", "name": "Overlay" }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "bg",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#1D4ED8" },
            "frame": { "x": 100.0, "y": 100.0, "w": 180.0, "h": 56.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "lbl",
            "kind": { "type": "text", "text": "  Continue  " },
            "frame": { "x": 120.0, "y": 112.0, "w": 140.0, "h": 32.0 }
        }),
    );
    let grp_res = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["bg", "lbl"] }),
    );
    let gid = grp_res["group"].as_str().unwrap().to_owned();

    // Add member triggers BEFORE making the group interactive.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": "bg" },
                "action": { "action": "show_layer", "layer": "l1" }
            }
        }),
    );
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "hover_start", "object": "lbl" },
                "action": { "action": "pause_timeline" }
            }
        }),
    );

    let plain_json = s.course.to_json().unwrap();
    let member_triggers_before = s.course.slide("slide1").unwrap().triggers.clone();

    // Turn interactive ON: must create Group record with derived name and NOT migrate member triggers.
    let res = ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": gid, "interactive": true }),
    );
    assert_eq!(res["group"].as_str(), Some(gid.as_str()));
    let slide = s.course.slide("slide1").unwrap();
    assert_eq!(slide.groups.len(), 1);
    assert_eq!(slide.groups[0].id, gid);
    assert_eq!(slide.groups[0].name, "Continue");
    assert!(slide.groups[0].interactive);
    assert_eq!(slide.groups[0].initial_state, "normal");
    assert_eq!(
        slide.triggers, member_triggers_before,
        "existing member triggers must remain completely untouched (no silent migration)"
    );

    // 1. Group Click trigger blocks opt-out without mutating the slide.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": gid },
                "action": { "action": "show_layer", "layer": "l1" }
            }
        }),
    );
    let before_blocked = s.course.clone();
    let err = s
        .execute(
            "group.set_interactive",
            &json!({ "slide": "slide1", "group": gid, "interactive": false }),
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Cannot remove button behaviour while triggers reference this group"),
        "unexpected error: {err}"
    );
    assert_eq!(s.course, before_blocked);
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "index": 2 }),
    );

    // 2. Action targeting group blocks opt-out.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "timeline_starts" },
                "action": { "action": "change_state", "object": gid, "state": "disabled" }
            }
        }),
    );
    assert!(
        s.execute(
            "group.set_interactive",
            &json!({ "slide": "slide1", "group": gid, "interactive": false }),
        )
        .is_err()
    );
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "index": 2 }),
    );

    // 3. Condition referencing group on a layer trigger blocks opt-out.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "trigger": {
                "when": { "event": "timeline_starts", "layer": "l1" },
                "condition": { "object_state": { "object": gid, "state": "normal", "is": true } },
                "action": { "action": "resume_timeline" }
            }
        }),
    );
    assert!(
        s.execute(
            "group.set_interactive",
            &json!({ "slide": "slide1", "group": gid, "interactive": false }),
        )
        .is_err()
    );
    ok(
        &mut s,
        "trigger.delete",
        json!({ "slide": "slide1", "layer": "l1", "index": 0 }),
    );

    // 4. With all group triggers deleted, opt-out succeeds and matches plain group JSON byte-for-byte.
    ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": gid, "interactive": false }),
    );
    assert!(s.course.slide("slide1").unwrap().groups.is_empty());
    assert_eq!(
        s.course.slide("slide1").unwrap().triggers,
        member_triggers_before
    );
    let after_opt_out_json = s.course.to_json().unwrap();
    assert_eq!(
        after_opt_out_json, plain_json,
        "opting out must return the course JSON to byte-identical plain group"
    );
}

#[test]
fn gb_12_group_set_interactive_collision_safe_id_and_rejections() {
    let mut s = Session::blank("Interactive Group Safety");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "box1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "box2",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#223344" },
            "frame": { "x": 20.0, "y": 15.0, "w": 20.0, "h": 20.0 }
        }),
    );
    // Simulate a legacy course whose group ID collides with an object ID (`box1`).
    {
        let slide = s.course.slide_mut("slide1").unwrap();
        slide.objects[0].group = Some("box1".to_owned());
        slide.objects[1].group = Some("box1".to_owned());
    }
    let res = ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": "box1", "interactive": true }),
    );
    let fresh_gid = res["group"].as_str().unwrap().to_owned();
    assert_ne!(fresh_gid, "box1", "colliding group id must be remapped");
    let slide = s.course.slide("slide1").unwrap();
    assert_eq!(slide.objects[0].group.as_deref(), Some(fresh_gid.as_str()));
    assert_eq!(slide.objects[1].group.as_deref(), Some(fresh_gid.as_str()));
    assert_eq!(slide.groups.len(), 1);
    assert_eq!(slide.groups[0].id, fresh_gid);
    assert_eq!(slide.groups[0].name, "Button group");
    assert!(s.course.first_error().is_none());

    // Reject < 2 members.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "solo",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
            "frame": { "x": 200.0, "y": 10.0, "w": 60.0, "h": 40.0 }
        }),
    );
    s.course.slide_mut("slide1").unwrap().objects[2].group = Some("solo_grp".to_owned());
    assert!(
        s.execute(
            "group.set_interactive",
            &json!({ "slide": "slide1", "group": "solo_grp", "interactive": true }),
        )
        .is_err()
    );

    // Reject members split across layers.
    ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "id": "layer1", "name": "L1" }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "layer1",
            "id": "layer_member",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#445566" },
            "frame": { "x": 200.0, "y": 10.0, "w": 60.0, "h": 40.0 }
        }),
    );
    s.course
        .slide_mut("slide1")
        .unwrap()
        .layer_mut("layer1")
        .unwrap()
        .objects[0]
        .group = Some("solo_grp".to_owned());
    assert!(
        s.execute(
            "group.set_interactive",
            &json!({ "slide": "slide1", "group": "solo_grp", "interactive": true }),
        )
        .is_err()
    );

    // Reject master slide.
    let mres = ok(&mut s, "master.add", json!({ "title": "Main Master" }));
    let mid = mres["id"].as_str().unwrap().to_owned();
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": mid,
            "id": "m1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 50.0, "h": 50.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": mid,
            "id": "m2",
            "kind": { "type": "text", "text": "Header" },
            "frame": { "x": 70.0, "y": 10.0, "w": 80.0, "h": 30.0 }
        }),
    );
    let mgrp = ok(
        &mut s,
        "object.group",
        json!({ "slide": mid, "objects": ["m1", "m2"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        s.execute(
            "group.set_interactive",
            &json!({ "slide": mid, "group": mgrp, "interactive": true }),
        )
        .is_err()
    );

    // Reject disallowed member kinds (TextEntry, ScrollPanel, Web).
    ok(
        &mut s,
        "variable.add",
        json!({ "name": "txt_var", "value": "" }),
    );
    let disallowed_kinds = [
        json!({ "type": "text_entry", "variable": "txt_var" }),
        json!({ "type": "scroll_panel", "content_height": 300.0 }),
        json!({ "type": "web", "source": { "type": "url", "url": "https://example.com" } }),
    ];
    for (idx, kind) in disallowed_kinds.into_iter().enumerate() {
        let base_id = format!("dis_base_{idx}");
        let bad_id = format!("dis_bad_{idx}");
        ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "id": base_id,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": 10.0, "y": 100.0, "w": 80.0, "h": 40.0 }
            }),
        );
        ok(
            &mut s,
            "object.add",
            json!({
                "slide": "slide1",
                "id": bad_id,
                "kind": kind,
                "frame": { "x": 100.0, "y": 100.0, "w": 120.0, "h": 80.0 }
            }),
        );
        let dgrp = ok(
            &mut s,
            "object.group",
            json!({ "slide": "slide1", "objects": [base_id, bad_id] }),
        )["group"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            s.execute(
                "group.set_interactive",
                &json!({ "slide": "slide1", "group": dgrp, "interactive": true }),
            )
            .is_err(),
            "disallowed kind index {idx} must be rejected"
        );
    }

    // Layer interactive group works cleanly.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "layer1",
            "id": "la",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 40.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "layer1",
            "id": "lb",
            "kind": { "type": "button", "label": "Close" },
            "frame": { "x": 15.0, "y": 15.0, "w": 90.0, "h": 30.0 }
        }),
    );
    let lgrp = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["la", "lb"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": lgrp, "interactive": true }),
    );
    let layer = s.course.slide("slide1").unwrap().layer("layer1").unwrap();
    assert_eq!(layer.groups.len(), 1);
    assert_eq!(layer.groups[0].id, lgrp);
    assert_eq!(layer.groups[0].name, "Close");
    assert!(s.course.first_error().is_none());
}

fn gb_12_setup_interactive_group_session() -> (Session, String) {
    let mut s = Session::blank("Interactive Group Update and Lifecycle");
    ok(
        &mut s,
        "slide.add",
        json!({ "id": "slide2", "title": "S2" }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "a",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#1D4ED8" },
            "frame": { "x": 40.0, "y": 40.0, "w": 160.0, "h": 50.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "b",
            "kind": { "type": "text", "text": "Next" },
            "frame": { "x": 60.0, "y": 50.0, "w": 80.0, "h": 30.0 }
        }),
    );
    ok(
        &mut s,
        "object.add_state",
        json!({
            "slide": "slide1",
            "object": "a",
            "state": { "name": "selected", "fill": "#1E40AF" }
        }),
    );
    let gid = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": gid, "interactive": true }),
    );
    (s, gid)
}

fn gb_12_assert_duplicate_and_paste(s: &mut Session, gid: &str) -> (Vec<String>, String) {
    let dup = ok(
        s,
        "object.duplicate",
        json!({ "slide": "slide1", "objects": ["a", "b"] }),
    );
    let dup_ids: Vec<String> = dup["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(dup_ids.len(), 2);
    let dup_gid = s
        .course
        .slide("slide1")
        .unwrap()
        .object(&dup_ids[0])
        .unwrap()
        .0
        .group
        .clone()
        .unwrap();
    assert_ne!(dup_gid, gid);
    let dup_group = s.course.slide("slide1").unwrap().group(&dup_gid).unwrap().0;
    assert!(dup_group.interactive);
    assert_eq!(dup_group.name, "Next slide button");
    assert_eq!(dup_group.initial_state, "selected");
    assert!(s.course.first_error().is_none());

    let paste_objs: Vec<Json> = ["a", "b"]
        .iter()
        .map(|id| {
            serde_json::to_value(s.course.slide("slide1").unwrap().object(id).unwrap().0).unwrap()
        })
        .collect();
    let paste_triggers: Vec<Json> = s
        .course
        .slide("slide1")
        .unwrap()
        .triggers
        .iter()
        .map(|t| serde_json::to_value(t).unwrap())
        .collect();
    let pasted = ok(
        s,
        "object.paste",
        json!({
            "slide": "slide2",
            "source_slide": "slide1",
            "objects": paste_objs,
            "triggers": paste_triggers
        }),
    );
    let pasted_id = pasted["id"].as_str().unwrap().to_owned();
    let slide2 = s.course.slide("slide2").unwrap();
    let pasted_gid = slide2.object(&pasted_id).unwrap().0.group.clone().unwrap();
    assert_eq!(slide2.groups.len(), 1);
    assert_eq!(slide2.groups[0].id, pasted_gid);
    assert!(slide2.groups[0].interactive);
    assert_eq!(slide2.groups[0].name, "Next slide button");
    assert_eq!(slide2.triggers.len(), 1);
    assert!(matches!(
        &slide2.triggers[0].when,
        coursecraft_course::Event::Click { object } if object == &pasted_gid
    ));
    assert!(s.course.first_error().is_none());

    ok(
        s,
        "object.ungroup",
        json!({ "slide": "slide2", "objects": [pasted_id] }),
    );
    let slide2 = s.course.slide("slide2").unwrap();
    assert!(slide2.groups.is_empty());
    assert!(slide2.triggers.is_empty());
    assert!(s.course.first_error().is_none());

    (dup_ids, dup_gid)
}

#[test]
fn gb_12_group_update_and_lifecycle_cleanup() {
    let (mut s, gid) = gb_12_setup_interactive_group_session();

    // 1. group.update updates all fields, rejects unknown state, and clears nullable fields.
    ok(
        &mut s,
        "group.update",
        json!({
            "slide": "slide1",
            "group": gid,
            "name": "Next slide button",
            "initial_state": "selected",
            "focus_order": 3,
            "description": "Advances to slide 2"
        }),
    );
    let g = &s.course.slide("slide1").unwrap().groups[0];
    assert_eq!(g.name, "Next slide button");
    assert_eq!(g.initial_state, "selected");
    assert_eq!(g.focus_order, Some(3));
    assert_eq!(g.description.as_deref(), Some("Advances to slide 2"));

    assert!(
        s.execute(
            "group.update",
            &json!({ "slide": "slide1", "group": gid, "initial_state": "nonexistent" }),
        )
        .is_err()
    );

    ok(
        &mut s,
        "group.update",
        json!({
            "slide": "slide1",
            "group": gid,
            "focus_order": null,
            "description": null
        }),
    );
    let g = &s.course.slide("slide1").unwrap().groups[0];
    assert_eq!(g.focus_order, None);
    assert_eq!(g.description, None);

    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": gid },
                "action": { "action": "jump_to_slide", "target": { "id": "slide2" } }
            }
        }),
    );

    // 2-4. object.duplicate, object.paste, and object.ungroup lifecycle checks.
    let (dup_ids, dup_gid) = gb_12_assert_duplicate_and_paste(&mut s, &gid);

    // 5. object.group merging an interactive group with another shape removes old Group + trigger.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "c",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#999999" },
            "frame": { "x": 220.0, "y": 40.0, "w": 30.0, "h": 30.0 }
        }),
    );
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["a", "c"] }),
    );
    let slide1 = s.course.slide("slide1").unwrap();
    assert!(slide1.group(&gid).is_none());
    assert!(slide1.triggers.is_empty());
    assert!(s.course.first_error().is_none());

    // 6. object.delete on duplicated interactive group: 1st member keeps Group, last removes Group + trigger.
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": dup_gid },
                "action": { "action": "pause_timeline" }
            }
        }),
    );
    ok(
        &mut s,
        "object.delete",
        json!({ "slide": "slide1", "object": dup_ids[1] }),
    );
    assert!(s.course.slide("slide1").unwrap().group(&dup_gid).is_some());
    assert_eq!(s.course.slide("slide1").unwrap().triggers.len(), 1);
    let del_res = ok(
        &mut s,
        "object.delete",
        json!({ "slide": "slide1", "object": dup_ids[0] }),
    );
    assert_eq!(del_res["removed_triggers"], 1);
    assert!(s.course.slide("slide1").unwrap().group(&dup_gid).is_none());
    assert!(s.course.slide("slide1").unwrap().triggers.is_empty());
    assert!(s.course.first_error().is_none());
}

#[test]
fn gb_12_opt_out_plain_colliding_group_and_hostile_paste_groups() {
    let mut s = Session::blank("Plain Group Colliding ID & Paste Bounds");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "box1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "box2",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#223344" },
            "frame": { "x": 20.0, "y": 15.0, "w": 20.0, "h": 20.0 }
        }),
    );
    // Legacy plain group whose group ID matches object ID "box1", while object "box1" has a trigger.
    {
        let slide = s.course.slide_mut("slide1").unwrap();
        slide.objects[0].group = Some("box1".to_owned());
        slide.objects[1].group = Some("box1".to_owned());
    }
    ok(
        &mut s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": "box1" },
                "action": { "action": "pause_timeline" }
            }
        }),
    );
    // Opting out a plain group with no Group record must be a clean no-op, not blocked by the object's trigger.
    let undo_before = s.undo_len();
    let res = ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": "box1", "interactive": false }),
    );
    assert_eq!(res["group"], "box1");
    assert_eq!(s.undo_len(), undo_before);

    // Hostile object.paste with >500 groups or oversized group name must be rejected.
    let obj_val = serde_json::to_value(&s.course.slide("slide1").unwrap().objects[0]).unwrap();
    let too_many_groups: Vec<Json> = (0..501)
        .map(|i| json!({ "id": format!("g{i}"), "interactive": true }))
        .collect();
    assert!(
        s.execute(
            "object.paste",
            &json!({
                "slide": "slide1",
                "objects": [obj_val.clone()],
                "groups": too_many_groups
            }),
        )
        .is_err()
    );
    let huge_name = "x".repeat(200_000);
    assert!(
        s.execute(
            "object.paste",
            &json!({
                "slide": "slide1",
                "objects": [obj_val],
                "groups": [{ "id": "box1", "name": huge_name, "interactive": true }]
            }),
        )
        .is_err()
    );
}

fn gb_15_setup_slide_with_interactive_groups() -> (Session, String, String, String) {
    let mut s = Session::blank("Slide Duplicate Interactive Groups");
    ok(
        &mut s,
        "layer.add",
        json!({ "slide": "slide1", "id": "l1", "name": "Overlay" }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "bg",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#1D4ED8" },
            "frame": { "x": 40.0, "y": 40.0, "w": 160.0, "h": 50.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "lbl",
            "kind": { "type": "text", "text": "Submit" },
            "frame": { "x": 60.0, "y": 50.0, "w": 100.0, "h": 30.0 }
        }),
    );
    ok(
        &mut s,
        "object.add_state",
        json!({
            "slide": "slide1",
            "object": "bg",
            "state": { "name": "selected", "fill": "#1E40AF" }
        }),
    );
    let base_gid = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["bg", "lbl"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": base_gid, "interactive": true }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "disable_btn",
            "kind": { "type": "button", "label": "Disable" },
            "frame": { "x": 240.0, "y": 40.0, "w": 120.0, "h": 44.0 }
        }),
    );
    // Coexisting plain (non-interactive) group on base slide must keep its JSON shape.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "plain_a",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#64748B" },
            "frame": { "x": 40.0, "y": 140.0, "w": 50.0, "h": 50.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "plain_b",
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#94A3B8" },
            "frame": { "x": 100.0, "y": 140.0, "w": 50.0, "h": 50.0 }
        }),
    );
    let plain_gid = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["plain_a", "plain_b"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "id": "l_bg",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#0F766E" },
            "frame": { "x": 80.0, "y": 80.0, "w": 140.0, "h": 44.0 }
        }),
    );
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "id": "l_lbl",
            "kind": { "type": "text", "text": "Close" },
            "frame": { "x": 100.0, "y": 90.0, "w": 100.0, "h": 24.0 }
        }),
    );
    let layer_gid = ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": ["l_bg", "l_lbl"] }),
    )["group"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(
        &mut s,
        "group.set_interactive",
        json!({ "slide": "slide1", "group": layer_gid, "interactive": true }),
    );
    (s, base_gid, plain_gid, layer_gid)
}

fn gb_15_add_group_triggers(s: &mut Session, base_gid: &str, layer_gid: &str) {
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": base_gid },
                "action": { "action": "show_layer", "layer": "l1" }
            }
        }),
    );
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": "disable_btn" },
                "condition": { "object_state": { "object": base_gid, "state": "normal", "is": true } },
                "action": { "action": "change_state", "object": base_gid, "state": "disabled" }
            }
        }),
    );
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "state_changes", "object": base_gid, "to": "disabled" },
                "action": { "action": "pause_timeline" }
            }
        }),
    );
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "trigger": {
                "when": { "event": "click", "object": layer_gid },
                "action": { "action": "hide_layer" }
            }
        }),
    );
    ok(
        s,
        "trigger.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "trigger": {
                "when": { "event": "timeline_starts", "layer": "l1" },
                "condition": { "object_state": { "object": layer_gid, "state": "normal", "is": true } },
                "action": { "action": "change_state", "object": base_gid, "state": "selected" }
            }
        }),
    );
}

fn gb_15_assert_copied_groups(
    copy: &coursecraft_course::Slide,
    base_gid: &str,
    plain_gid: &str,
    layer_gid: &str,
) -> (String, String) {
    assert_eq!(copy.groups.len(), 1);
    let new_base_gid = copy.groups[0].id.clone();
    assert_ne!(new_base_gid, base_gid);
    assert!(new_base_gid.starts_with("grp-"));
    assert!(copy.groups[0].interactive);
    assert_eq!(copy.groups[0].name, "Submit");
    assert_eq!(copy.group_members(&new_base_gid).len(), 2);
    assert!(
        copy.group_members(&new_base_gid)
            .iter()
            .all(|o| o.id != "bg" && o.id != "lbl")
    );
    assert!(copy.group(plain_gid).is_none());
    assert_eq!(copy.group_members(plain_gid).len(), 2);

    assert_eq!(copy.layers.len(), 1);
    let copy_layer = &copy.layers[0];
    assert_ne!(copy_layer.id, "l1");
    assert_eq!(copy_layer.groups.len(), 1);
    let new_layer_gid = copy_layer.groups[0].id.clone();
    assert_ne!(new_layer_gid, layer_gid);
    assert_ne!(new_layer_gid, new_base_gid);
    assert!(new_layer_gid.starts_with("grp-"));
    assert!(copy_layer.groups[0].interactive);
    assert_eq!(copy_layer.groups[0].name, "Close");
    assert_eq!(copy.group_members(&new_layer_gid).len(), 2);
    assert!(
        copy.group_members(&new_layer_gid)
            .iter()
            .all(|o| o.id != "l_bg" && o.id != "l_lbl")
    );
    (new_base_gid, new_layer_gid)
}

fn gb_15_assert_copied_base_triggers(
    copy: &coursecraft_course::Slide,
    orig: &coursecraft_course::Slide,
    new_base_gid: &str,
) {
    let copy_layer_id = &copy.layers[0].id;
    assert_eq!(copy.triggers.len(), orig.triggers.len());
    assert!(matches!(
        &copy.triggers[0].when,
        coursecraft_course::Event::Click { object } if object == new_base_gid
    ));
    assert!(matches!(
        &copy.triggers[0].action,
        coursecraft_course::Action::ShowLayer { layer } if layer == copy_layer_id
    ));
    let copy_disable_btn = copy
        .objects
        .iter()
        .find(|o| matches!(&o.kind, coursecraft_course::ObjectKind::Button { label, .. } if label == "Disable"))
        .expect("copied disable button")
        .id
        .clone();
    assert_ne!(copy_disable_btn, "disable_btn");
    assert!(matches!(
        &copy.triggers[1].when,
        coursecraft_course::Event::Click { object } if object == &copy_disable_btn
    ));
    assert!(matches!(
        &copy.triggers[1].condition,
        Some(coursecraft_course::Condition::ObjectState { object, state, is: true })
            if object == new_base_gid && state == "normal"
    ));
    assert!(matches!(
        &copy.triggers[1].action,
        coursecraft_course::Action::ChangeState { object, state }
            if object == new_base_gid && state == "disabled"
    ));
    assert!(matches!(
        &copy.triggers[2].when,
        coursecraft_course::Event::StateChanges { object, to }
            if object == new_base_gid && to.as_deref() == Some("disabled")
    ));
}

fn gb_15_assert_copied_layer_triggers(
    copy_layer: &coursecraft_course::Layer,
    orig_layer: &coursecraft_course::Layer,
    new_base_gid: &str,
    new_layer_gid: &str,
) {
    assert_eq!(copy_layer.triggers.len(), orig_layer.triggers.len());
    assert!(matches!(
        &copy_layer.triggers[0].when,
        coursecraft_course::Event::Click { object } if object == new_layer_gid
    ));
    assert!(matches!(
        &copy_layer.triggers[0].action,
        coursecraft_course::Action::HideLayer { layer: None }
    ));
    assert!(matches!(
        &copy_layer.triggers[1].when,
        coursecraft_course::Event::TimelineStarts { layer: Some(l) } if l == &copy_layer.id
    ));
    assert!(matches!(
        &copy_layer.triggers[1].condition,
        Some(coursecraft_course::Condition::ObjectState { object, state, is: true })
            if object == new_layer_gid && state == "normal"
    ));
    assert!(matches!(
        &copy_layer.triggers[1].action,
        coursecraft_course::Action::ChangeState { object, state }
            if object == new_base_gid && state == "selected"
    ));
}

fn gb_15_assert_no_group_slide_duplicate_identical() {
    let mut no_grp = Session::blank("No Groups");
    ok(
        &mut no_grp,
        "layer.add",
        json!({ "slide": "slide1", "id": "l1", "name": "L1" }),
    );
    ok(
        &mut no_grp,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "btn1",
            "kind": { "type": "button", "label": "Go" },
            "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 40.0 }
        }),
    );
    ok(
        &mut no_grp,
        "object.add",
        json!({
            "slide": "slide1",
            "layer": "l1",
            "id": "sh1",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 20.0, "y": 20.0, "w": 60.0, "h": 40.0 }
        }),
    );
    ok(
        &mut no_grp,
        "trigger.add",
        json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": "btn1" },
                "action": { "action": "show_layer", "layer": "l1" }
            }
        }),
    );
    let no_grp_before = no_grp.course.clone();
    let res = ok(&mut no_grp, "slide.duplicate", json!({ "slide": "slide1" }));
    assert_eq!(res["id"], "slide2");
    assert_eq!(res["skipped"], 0);
    let copied = no_grp.course.slide("slide2").unwrap();
    assert_eq!(copied.objects.len(), 1);
    assert_eq!(copied.objects[0].id, "button1");
    assert!(copied.objects[0].group.is_none());
    assert_eq!(copied.layers.len(), 1);
    assert_eq!(copied.layers[0].id, "layer1");
    assert_eq!(copied.layers[0].objects[0].id, "shape1");
    assert!(copied.groups.is_empty());
    assert!(copied.layers[0].groups.is_empty());
    assert_eq!(copied.triggers.len(), 1);
    assert!(matches!(
        &copied.triggers[0].when,
        coursecraft_course::Event::Click { object } if object == "button1"
    ));
    assert!(matches!(
        &copied.triggers[0].action,
        coursecraft_course::Action::ShowLayer { layer } if layer == "layer1"
    ));
    let serialized = no_grp.course.to_json().unwrap();
    assert!(!serialized.contains("\"groups\""));
    assert!(no_grp.course.first_error().is_none());
    ok(&mut no_grp, "edit.undo", json!({}));
    assert_eq!(no_grp.course, no_grp_before);
}

#[test]
fn gb_15_slide_duplicate_remaps_interactive_group_triggers() {
    let (mut s, base_gid, plain_gid, layer_gid) = gb_15_setup_slide_with_interactive_groups();
    gb_15_add_group_triggers(&mut s, &base_gid, &layer_gid);

    let before_course = s.course.clone();
    let before_slide1 = s.course.slide("slide1").unwrap().clone();
    let undo_before = s.undo_len();

    let dup = ok(&mut s, "slide.duplicate", json!({ "slide": "slide1" }));
    assert_eq!(dup["skipped"], 0, "no interactive group triggers dropped");
    let copy_id = dup["id"].as_str().unwrap().to_owned();
    assert_ne!(copy_id, "slide1");

    // Original slide is untouched.
    assert_eq!(s.course.slide("slide1").unwrap(), &before_slide1);

    let copy = s.course.slide(&copy_id).unwrap();
    let (new_base_gid, new_layer_gid) =
        gb_15_assert_copied_groups(copy, &base_gid, &plain_gid, &layer_gid);
    gb_15_assert_copied_base_triggers(copy, &before_slide1, &new_base_gid);
    gb_15_assert_copied_layer_triggers(
        &copy.layers[0],
        &before_slide1.layers[0],
        &new_base_gid,
        &new_layer_gid,
    );

    assert!(
        s.course.first_error().is_none(),
        "course with duplicated interactive groups must validate: {:?}",
        s.course.first_error()
    );

    // Single undo step restores the course exactly.
    assert_eq!(s.undo_len(), undo_before + 1);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before_course);

    gb_15_assert_no_group_slide_duplicate_identical();
}

#[test]
fn autofit_grow_add_text_set_paragraphs_and_undo() {
    let mut s = Session::blank("Grow Test");
    let sid = s.course.scenes[0].slides[0].id.clone();

    // 1. object.add with autofit = "grow": fits height headlessly, keeps width fixed, 1 undo step.
    let undo0 = s.undo_len();
    let res = ok(
        &mut s,
        "object.add",
        json!({
            "slide": sid,
            "id": "grow1",
            "frame": { "x": 100.0, "y": 120.0, "w": 220.0, "h": 40.0 },
            "kind": {
                "type": "text",
                "text": "One two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen",
                "autofit": "grow",
                "style": { "size": 20.0, "wrap": true }
            }
        }),
    );
    assert_eq!(res["id"], "grow1");
    assert_eq!(s.undo_len(), undo0 + 1);

    let frame_after_add = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(
        frame_after_add.w, 220.0,
        "object.add must keep Grow width fixed"
    );
    assert!(
        frame_after_add.h > 80.0,
        "object.add must refit Grow height to wrapped text, got {}",
        frame_after_add.h
    );

    // 2. text.set to short text shrinks height back (never smaller than one line); width unchanged; 1 undo step.
    let undo_before_short = s.undo_len();
    ok(
        &mut s,
        "text.set",
        json!({ "slide": sid, "object": "grow1", "text": "Hi" }),
    );
    assert_eq!(s.undo_len(), undo_before_short + 1);
    let frame_short = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(frame_short.w, 220.0, "text.set must keep Grow width fixed");
    assert!(
        frame_short.h < frame_after_add.h * 0.5 && frame_short.h >= 20.0,
        "text.set short text must shrink Grow height to ~1 line, got {}",
        frame_short.h
    );

    // Empty text stays at least one line tall.
    ok(
        &mut s,
        "text.set",
        json!({ "slide": sid, "object": "grow1", "text": "" }),
    );
    let frame_empty = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(frame_empty.w, 220.0);
    assert!(
        frame_empty.h >= 20.0,
        "empty Grow box must never shrink below one line, got {}",
        frame_empty.h
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide(&sid)
            .unwrap()
            .object("grow1")
            .unwrap()
            .0
            .frame,
        frame_short,
        "1-step undo must restore previous Grow height"
    );

    // 3. text.set_paragraphs (line_spacing + space_before + space_after) refits height; width unchanged.
    ok(
        &mut s,
        "text.set",
        json!({ "slide": sid, "object": "grow1", "text": "Line one\nLine two" }),
    );
    let frame_two = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(frame_two.w, 220.0);
    assert!(frame_two.h > frame_short.h * 1.5);

    let undo_para = s.undo_len();
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({
            "slide": sid,
            "object": "grow1",
            "line_spacing": 2.0,
            "space_before": 10.0,
            "space_after": 10.0
        }),
    );
    assert_eq!(s.undo_len(), undo_para + 1);
    let frame_spaced = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(frame_spaced.w, 220.0);
    assert!(
        frame_spaced.h > frame_two.h + 40.0,
        "text.set_paragraphs must refit Grow height: spaced={} vs two={}",
        frame_spaced.h,
        frame_two.h
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide(&sid)
            .unwrap()
            .object("grow1")
            .unwrap()
            .0
            .frame,
        frame_two
    );
}

#[test]
fn autofit_grow_apply_style_set_autofit_and_hostile() {
    use coursecraft_course::{Autofit, ObjectKind};

    let mut s = Session::blank("Grow Style Test");
    let sid = s.course.scenes[0].slides[0].id.clone();

    ok(
        &mut s,
        "object.add",
        json!({
            "slide": sid,
            "id": "grow1",
            "frame": { "x": 100.0, "y": 120.0, "w": 220.0, "h": 40.0 },
            "kind": {
                "type": "text",
                "text": "Line one\nLine two",
                "autofit": "grow",
                "style": { "size": 20.0, "wrap": true }
            }
        }),
    );
    let frame_two = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;

    // 1. object.apply_style from a larger-font source refits Grow height; width unchanged.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": sid,
            "id": "big-src",
            "frame": { "x": 400.0, "y": 100.0, "w": 300.0, "h": 80.0 },
            "kind": {
                "type": "text",
                "text": "Big",
                "style": { "size": 48.0, "bold": true }
            }
        }),
    );
    let undo_style = s.undo_len();
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": sid, "source_slide": sid, "source": "big-src", "object": "grow1" }),
    );
    assert_eq!(s.undo_len(), undo_style + 1);
    let frame_styled = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(
        frame_styled.w, 220.0,
        "object.apply_style must keep Grow width fixed"
    );
    assert!(
        frame_styled.h > frame_two.h * 1.8,
        "object.apply_style with 48pt must grow height: styled={} vs two={}",
        frame_styled.h,
        frame_two.h
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(
        s.course
            .slide(&sid)
            .unwrap()
            .object("grow1")
            .unwrap()
            .0
            .frame,
        frame_two
    );

    // 2. object.set_autofit switching a None box -> Grow refits height in 1 undo step.
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": sid,
            "id": "area-none",
            "frame": { "x": 100.0, "y": 300.0, "w": 180.0, "h": 30.0 },
            "kind": {
                "type": "text",
                "text": "A long wrapping sentence that definitely does not fit in thirty pixels of height at width 180"
            }
        }),
    );
    let undo_af = s.undo_len();
    ok(
        &mut s,
        "object.set_autofit",
        json!({ "slide": sid, "object": "area-none", "autofit": "grow" }),
    );
    assert_eq!(s.undo_len(), undo_af + 1);
    let (obj_af, _) = s.course.slide(&sid).unwrap().object("area-none").unwrap();
    assert_eq!(obj_af.frame.w, 180.0);
    assert!(obj_af.frame.h > 80.0);
    assert!(matches!(
        obj_af.kind,
        ObjectKind::Text {
            autofit: Autofit::Grow,
            point_text: false,
            ..
        }
    ));
    ok(&mut s, "edit.undo", json!({}));
    let (obj_af_undone, _) = s.course.slide(&sid).unwrap().object("area-none").unwrap();
    assert_eq!(obj_af_undone.frame.h, 30.0);
    assert!(matches!(
        obj_af_undone.kind,
        ObjectKind::Text {
            autofit: Autofit::None,
            ..
        }
    ));

    // 3. Hostile inputs (huge measured_size, long repeating text) stay finite and clamped.
    ok(
        &mut s,
        "text.set",
        json!({
            "slide": sid,
            "object": "grow1",
            "text": "Word ".repeat(600),
            "measured_size": { "w": 99999999.0, "h": 99999999.0 }
        }),
    );
    let hostile_frame = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(hostile_frame.w, 220.0);
    assert!(hostile_frame.h.is_finite() && hostile_frame.h <= 10_000.0 && hostile_frame.h >= 1.0);

    // Negative/zero measured_size falls back cleanly to headless measurement.
    ok(
        &mut s,
        "text.set",
        json!({
            "slide": sid,
            "object": "grow1",
            "text": "Fallback line",
            "measured_size": { "w": -50.0, "h": 0.0 }
        }),
    );
    let fallback_frame = s
        .course
        .slide(&sid)
        .unwrap()
        .object("grow1")
        .unwrap()
        .0
        .frame;
    assert_eq!(fallback_frame.w, 220.0);
    assert!(
        fallback_frame.h.is_finite() && fallback_frame.h >= 20.0 && fallback_frame.h < 60.0,
        "fallback headless height should be ~1 line, got {}",
        fallback_frame.h
    );
}

#[test]
fn tr_13_course_set_tracking_sets_clears_and_undoes() {
    use coursecraft_course::{QuizTracking, SlidesViewedTracking, TrackingSettings};

    let mut s = Session::blank("Tracking Test");
    assert_eq!(s.course.tracking, TrackingSettings::default());

    let res = ok(
        &mut s,
        "course.set_tracking",
        json!({ "slides_viewed": { "mode": "all" } }),
    );
    assert_eq!(
        res,
        json!({ "tracking": { "slides_viewed": { "mode": "all" } } })
    );
    assert_eq!(
        s.course.tracking.slides_viewed,
        Some(SlidesViewedTracking::All)
    );
    assert_eq!(s.course.tracking.quiz, None);

    let prev = s.course.tracking.clone();
    let undo_before = s.undo_len();
    let res = ok(
        &mut s,
        "course.set_tracking",
        json!({
            "slides_viewed": { "mode": "at_least", "count": 2 },
            "quiz": { "slide": "missing_results" }
        }),
    );
    assert_eq!(s.undo_len(), undo_before + 1);
    assert_eq!(
        res,
        json!({
            "tracking": {
                "slides_viewed": { "mode": "at_least", "count": 2 },
                "quiz": { "slide": "missing_results" }
            }
        })
    );
    assert_eq!(
        s.course.tracking,
        TrackingSettings {
            slides_viewed: Some(SlidesViewedTracking::AtLeast { count: 2 }),
            quiz: Some(QuizTracking {
                slide: Some("missing_results".into())
            }),
        }
    );

    // 1-step undo/redo restores previous TrackingSettings cleanly.
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course.tracking, prev);
    ok(&mut s, "edit.redo", json!({}));
    assert_eq!(
        s.course.tracking,
        TrackingSettings {
            slides_viewed: Some(SlidesViewedTracking::AtLeast { count: 2 }),
            quiz: Some(QuizTracking {
                slide: Some("missing_results".into())
            }),
        }
    );

    // Course quiz (`slide: None`) via `{}` and clearing both with `null`.
    let res = ok(&mut s, "course.set_tracking", json!({ "quiz": {} }));
    assert_eq!(
        res,
        json!({
            "tracking": {
                "slides_viewed": { "mode": "at_least", "count": 2 },
                "quiz": {}
            }
        })
    );
    assert_eq!(s.course.tracking.quiz, Some(QuizTracking { slide: None }));

    let res = ok(
        &mut s,
        "course.set_tracking",
        json!({ "slides_viewed": null, "quiz": null }),
    );
    assert_eq!(res, json!({ "tracking": {} }));
    assert_eq!(s.course.tracking, TrackingSettings::default());
}

#[test]
fn tr_13_course_set_tracking_rejects_invalid_params() {
    let mut s = Session::blank("Tracking Invalid Test");
    let overlong_id = "a".repeat(65);
    for bad_params in [
        json!(null),
        json!([]),
        json!({}),
        json!({ "extra": 1 }),
        json!({ "slides_viewed": { "mode": "all", "extra": 1 } }),
        json!({ "slides_viewed": { "mode": "all", "count": 3 } }),
        json!({ "slides_viewed": { "mode": "at_least", "count": 2, "extra": 1 } }),
        json!({ "slides_viewed": { "mode": "at_least", "count": 0 } }),
        json!({ "slides_viewed": { "mode": "at_least", "count": -1 } }),
        json!({ "quiz": { "slide": null, "extra": 1 } }),
        json!({ "quiz": { "slide": "" } }),
        json!({ "quiz": { "slide": "bad id" } }),
        json!({ "quiz": { "slide": overlong_id } }),
        json!({ "slides_viewed": { "mode": "all" }, "quiz": { "slide": "bad id" } }),
    ] {
        let err = s
            .execute("course.set_tracking", &bad_params)
            .expect_err("expected EngineError::BadParams");
        assert!(
            matches!(err, crate::EngineError::BadParams { .. }),
            "expected BadParams for {bad_params}, got {err:?}"
        );
        assert_eq!(
            s.course.tracking,
            coursecraft_course::TrackingSettings::default(),
            "failed command must not partially mutate tracking for {bad_params}"
        );
        assert_eq!(s.undo_len(), 0);
    }
}

#[test]
fn animation_effects_lists_the_registry_and_set_animation_checks_it() {
    let mut s = Session::blank("Test");
    let list = s.execute("animation.effects", &json!({})).unwrap();
    let ids: Vec<&str> = list["effects"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["id"].as_str())
        .collect();
    assert_eq!(ids.len(), coursecraft_course::EFFECTS.len());
    assert!(ids.contains(&"fly_in"));
    assert_eq!(list["fallback"], "fade");

    let obj_id = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 }
            }),
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let set = |s: &mut Session, effect: serde_json::Value| {
        s.execute(
            "object.set_animation",
            &json!({
                "slide": "slide1", "object": obj_id, "which": "entrance",
                "animation": { "effect": effect, "duration_ms": 500 }
            }),
        )
    };
    assert!(set(&mut s, json!({ "type": "from_the_future" })).is_err());
    assert!(set(&mut s, json!({ "type": "zoom", "from": "left" })).is_err());
    assert!(set(&mut s, json!({ "type": "float", "from": "left" })).is_ok());
}

#[test]
fn m39_set_theme_heading_font_sets_clears_and_undoes_in_one_step() {
    let mut s = Session::blank("Heading");
    let before = s.course.clone();
    let undo = s.undo_len();
    ok(
        &mut s,
        "course.set_theme",
        json!({ "font": "Roboto", "heading_font": "Lora" }),
    );
    assert_eq!(s.course.theme.font, "Roboto");
    assert_eq!(s.course.theme.heading_font.as_deref(), Some("Lora"));
    assert_eq!(s.undo_len(), undo + 1, "both fonts are one undo step");
    // A call without heading_font leaves it alone.
    ok(&mut s, "course.set_theme", json!({ "accent": "#112233" }));
    assert_eq!(s.course.theme.heading_font.as_deref(), Some("Lora"));
    ok(&mut s, "course.set_theme", json!({ "heading_font": null }));
    assert_eq!(s.course.theme.heading_font, None);
    ok(
        &mut s,
        "course.set_theme",
        json!({ "heading_font": "Lora" }),
    );
    ok(&mut s, "course.set_theme", json!({ "heading_font": "" }));
    assert_eq!(s.course.theme.heading_font, None, "an empty name clears it");
    assert!(
        s.execute("course.set_theme", &json!({ "heading_font": 3 }))
            .is_err()
    );
    for _ in 0..5 {
        ok(&mut s, "edit.undo", json!({}));
    }
    assert_eq!(s.course, before);
}

fn m39_text_fonts(s: &Session, id: &str) -> Vec<(String, String)> {
    let slide = s.course.slide(id).expect("slide");
    slide
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            ObjectKind::Text { style, .. } => Some((o.name.clone(), style.font.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn m39_title_placeholders_use_the_heading_font_and_body_keeps_the_body_font() {
    let mut s = Session::blank("Heading");
    ok(
        &mut s,
        "course.set_theme",
        json!({ "font": "Roboto", "heading_font": "Lora" }),
    );
    let id = ok(
        &mut s,
        "slide.add_from_layout",
        json!({ "layout": "title_content", "after": "slide1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fonts = m39_text_fonts(&s, &id);
    assert!(
        fonts
            .iter()
            .any(|(n, f)| n.contains("Title") && f == "Lora"),
        "{fonts:?}"
    );
    assert!(
        fonts.iter().any(|(_, f)| f == "Roboto"),
        "the body placeholder keeps the body font: {fonts:?}"
    );
    assert!(
        fonts
            .iter()
            .all(|(n, f)| n.contains("Title") == (f == "Lora")),
        "{fonts:?}"
    );
}

#[test]
fn m39_without_a_heading_font_titles_use_the_body_font() {
    let mut s = Session::blank("Heading");
    ok(&mut s, "course.set_theme", json!({ "font": "Roboto" }));
    let id = ok(
        &mut s,
        "slide.add_from_layout",
        json!({ "layout": "title", "after": "slide1" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fonts = m39_text_fonts(&s, &id);
    assert!(!fonts.is_empty());
    assert!(fonts.iter().all(|(_, f)| f == "Roboto"), "{fonts:?}");
}
