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
fn deleting_a_referenced_object_is_refused() {
    let mut s = Session::blank("T");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "kind": { "type": "button", "label": "Go" }, "frame": { "x": 0, "y": 0, "w": 1, "h": 1 } }),
    );
    ok(&mut s, "slide.add", json!({}));
    ok(
        &mut s,
        "trigger.add",
        json!({ "slide": "slide1", "trigger": {
        "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "button1" } } }),
    );
    let e = s
        .execute(
            "object.delete",
            &json!({ "slide": "slide1", "object": "button1" }),
        )
        .unwrap_err();
    assert!(matches!(e, EngineError::Invalid(_)), "{e}");
    assert!(
        s.course
            .slide("slide1")
            .unwrap()
            .object("button1")
            .is_some()
    );
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
fn deleting_a_layer_used_by_a_trigger_is_refused_and_names_it() {
    let mut s = Session::blank("Title");
    add_layer_l1(&mut s);
    add_show_layer_trigger(&mut s);
    let e = s
        .execute("layer.delete", &json!({ "slide": "slide1", "layer": "l1" }))
        .unwrap_err();
    assert!(
        matches!(e, EngineError::Invalid(ref m) if m.contains("l1")),
        "{e}"
    );
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
        value: Value::Number(0.0),
    };
    assert_eq!(q.correct_when.as_ref(), Some(&default_condition));

    let mut updated = q.clone();
    updated.correct_when = Some(Condition::Compare {
        variable: "Course.ScorePercent".into(),
        op: CmpOp::Ge,
        value: Value::Number(50.0),
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

    // Removing path1 reports referencing triggers
    let remove_res = s
        .execute(
            "object.remove_motion_path",
            &json!({
                "slide": "slide1",
                "object": obj_id,
                "path": path1
            }),
        )
        .unwrap();
    let referencing = remove_res["triggers"].as_array().unwrap();
    assert_eq!(referencing.len(), 2);

    // Undo removes triggers report and restores path1
    s.undo().unwrap();
    let slide = s.course.slide("slide1").unwrap();
    let obj = slide.objects.iter().find(|o| o.id == obj_id).unwrap();
    assert!(obj.motion_paths.iter().any(|p| p.id == path1));

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
                assert_eq!(*value, Value::Bool(layer == "passed"));
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
            fill: None,
            text: None,
            src: None,
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
            fill: None,
            text: None,
            src: None,
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
