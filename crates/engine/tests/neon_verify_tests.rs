//! Regression tests for the Neon 0.6.1 bug hunt items that were verified by
//! reading code (N71, N72, N75). Each test starts from a course that
//! `Course::validate()` accepts and runs one engine command.

use coursecraft_course::{Course, Severity};
use coursecraft_engine::Session;
use serde_json::{Value, json};

fn errors_of(course: &Course) -> Vec<String> {
    course
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

fn session(mut v: Value) -> Session {
    let obj = v.as_object_mut().expect("object");
    obj.entry("schema_version").or_insert(json!(1));
    obj.entry("id").or_insert(json!("c"));
    obj.entry("title").or_insert(json!("C"));
    let course = Course::from_json(&v.to_string()).expect("course parses");
    assert!(
        errors_of(&course).is_empty(),
        "start: {:?}",
        errors_of(&course)
    );
    Session::new(course)
}

fn slide(id: &str, objects: Value, triggers: Value) -> Value {
    json!({ "id": id, "title": id, "duration_ms": 5000, "objects": objects, "triggers": triggers })
}

fn rect(id: &str, x: f64) -> Value {
    json!({ "id": id, "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
            "frame": { "x": x, "y": 10, "w": 50, "h": 40 } })
}

fn timer(id: &str) -> Value {
    json!({ "id": id, "kind": { "type": "timer", "duration_ms": 3000 },
            "frame": { "x": 10, "y": 10, "w": 120, "h": 40 } })
}

fn assert_valid(s: &Session) {
    assert!(
        errors_of(&s.course).is_empty(),
        "{:?}",
        errors_of(&s.course)
    );
}

// ---------------------------------------------------------------- N71

fn text_with_state_link(slide_target: &str) -> Value {
    json!({
        "id": "t1",
        "kind": { "type": "text", "text": "Hi" },
        "frame": { "x": 10, "y": 10, "w": 100, "h": 30 },
        "states": [{
            "name": "hover",
            "text": "Hi",
            "runs": [{ "text": "Hi", "link": { "type": "slide", "slide": slide_target } }],
            "extras": [{
                "id": "x1",
                "frame": { "x": 0, "y": 60, "w": 100, "h": 30 },
                "kind": { "type": "text", "text": "Yo",
                          "runs": [{ "text": "Yo", "link": { "type": "slide", "slide": slide_target } }] }
            }]
        }]
    })
}

#[test]
fn n71_slide_paste_strips_missing_slide_link_in_state_runs() {
    let mut s = session(json!({
        "scenes": [{ "id": "sc1", "title": "S", "slides": [slide("s1", json!([]), json!([]))] }]
    }));
    let pasted = slide(
        "other_slide",
        json!([text_with_state_link("gone")]),
        json!([]),
    );
    let res = s.execute("slide.paste", &json!({ "slide": pasted }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

#[test]
fn n71_slide_duplicate_retargets_self_link_in_state_runs() {
    let mut s = session(json!({
        "scenes": [{ "id": "sc1", "title": "S",
                     "slides": [slide("s1", json!([text_with_state_link("s1")]), json!([]))] }]
    }));
    let res = s.execute("slide.duplicate", &json!({ "slide": "s1" }));
    assert!(res.is_ok(), "{res:?}");
    let copy = &s.course.scenes[0].slides[1];
    let new_id = copy.id.clone();
    let text = serde_json::to_string(&copy.objects[0]).expect("json");
    assert!(
        text.contains(&format!("\"slide\":\"{new_id}\"")) && !text.contains("\"slide\":\"s1\""),
        "state links must point at the copy: {text}"
    );
}

// ---------------------------------------------------------------- N72

fn timer_compare(op_value: i64) -> Value {
    json!({ "compare": { "variable": "Timer.t1.Remaining", "op": "gt", "value": op_value } })
}

#[test]
fn n72_object_delete_removes_triggers_using_timer_variable() {
    let trig = json!([{
        "when": { "event": "click", "object": "b" },
        "condition": timer_compare(0),
        "action": { "action": "pause_timeline" }
    }]);
    let mut s = session(json!({
        "scenes": [{ "id": "sc1", "title": "S",
                     "slides": [slide("s1", json!([timer("t1"), rect("b", 200.0)]), trig)] }]
    }));
    let res = s.execute("object.delete", &json!({ "slide": "s1", "object": "t1" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

#[test]
fn n72_object_delete_drops_drag_target() {
    let mut drag = rect("d", 10.0);
    drag["drag"] = json!({ "targets": ["tg"] });
    let mut s = session(json!({
        "scenes": [{ "id": "sc1", "title": "S",
                     "slides": [slide("s1", json!([drag, rect("tg", 200.0)]), json!([]))] }]
    }));
    let res = s.execute("object.delete", &json!({ "slide": "s1", "object": "tg" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

fn course_with_layer() -> Value {
    json!({
        "scenes": [{ "id": "sc1", "title": "S", "slides": [{
            "id": "s1", "title": "s1", "duration_ms": 5000,
            "objects": [rect("b", 300.0)],
            "layers": [{
                "id": "l1", "name": "L1",
                "objects": [timer("t1"), rect("m1", 10.0), rect("m2", 100.0)],
                "groups": [{ "id": "g1", "interactive": true }]
            }]
        }] }]
    })
}

#[test]
fn n72_layer_delete_cleans_group_timer_and_question_references() {
    let mut v = course_with_layer();
    {
        let sl = &mut v["scenes"][0]["slides"][0];
        sl["layers"][0]["objects"][1]["group"] = json!("g1");
        sl["layers"][0]["objects"][2]["group"] = json!("g1");
        sl["triggers"] = json!([
            { "when": { "event": "click", "object": "g1" }, "action": { "action": "pause_timeline" } },
            { "when": { "event": "click", "object": "b" }, "condition": timer_compare(0),
              "action": { "action": "pause_timeline" } }
        ]);
    }
    let mut s = session(v);
    let res = s.execute("layer.delete", &json!({ "slide": "s1", "layer": "l1" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

#[test]
fn n72_layer_delete_clears_question_feedback_layer() {
    let mut v = course_with_layer();
    {
        let sl = &mut v["scenes"][0]["slides"][0];
        sl["question"] = json!({
            "kind": "freeform", "prompt": "Q", "choices": [],
            "correct_when": { "compare": { "variable": "Timer.t1.Remaining", "op": "gt", "value": 0 } },
            "correct_layer": "l1"
        });
        sl["objects"] = json!([rect("b", 300.0), timer("t1")]);
        sl["layers"][0]["objects"] = json!([rect("m1", 10.0)]);
        sl["layers"][0]["groups"] = json!([]);
    }
    let mut s = session(v);
    let res = s.execute("layer.delete", &json!({ "slide": "s1", "layer": "l1" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
    assert!(
        s.course.scenes[0].slides[0]
            .question
            .as_ref()
            .is_some_and(|q| q.correct_layer.is_none())
    );
}

#[test]
fn n72_slide_delete_cleans_jumps_start_slide_links_and_quiz_tracking() {
    let jump = json!([{
        "when": { "event": "click", "object": "b" },
        "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
    }]);
    let lightbox = json!([{
        "when": { "event": "click", "object": "b" },
        "action": { "action": "lightbox_slide", "slide": "s2" }
    }]);
    let mut link_text = text_with_state_link("s2");
    link_text["kind"]["runs"] =
        json!([{ "text": "Hi", "link": { "type": "slide", "slide": "s2" } }]);
    let mut s = session(json!({
        "start_slide": "s2",
        "tracking": { "quiz": { "slide": "s2" } },
        "scenes": [{ "id": "sc1", "title": "S", "slides": [
            slide("s1", json!([rect("b", 300.0), link_text]), jump),
            slide("s3", json!([rect("b", 300.0)]), lightbox),
            slide("s2", json!([]), json!([])),
        ] }]
    }));
    let res = s.execute("slide.delete", &json!({ "slide": "s2" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
    assert!(s.course.start_slide.is_none());
}

#[test]
fn n72_scene_delete_clears_start_slide_and_links() {
    let mut link_text = text_with_state_link("s2");
    link_text["kind"]["runs"] =
        json!([{ "text": "Hi", "link": { "type": "slide", "slide": "s2" } }]);
    let mut s = session(json!({
        "start_slide": "s2",
        "tracking": { "quiz": { "slide": "s2" } },
        "scenes": [
            { "id": "sc1", "title": "S", "slides": [slide("s1", json!([link_text]), json!([]))] },
            { "id": "sc2", "title": "S2", "slides": [slide("s2", json!([]), json!([]))] },
        ]
    }));
    let res = s.execute("scene.delete", &json!({ "scene": "sc2" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

#[test]
fn n72_clearing_background_audio_removes_its_triggers() {
    let trig = json!([{
        "when": { "event": "timeline_starts" },
        "action": { "action": "pause_media", "object": "Course.BackgroundAudio" }
    }]);
    let mut s = session(json!({
        "background_audio": { "src": "bg.mp3" },
        "scenes": [{ "id": "sc1", "title": "S", "slides": [slide("s1", json!([]), trig)] }]
    }));
    let res = s.execute("course.set_background_audio", &json!({ "src": null }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
}

// ---------------------------------------------------------------- N75

#[test]
fn n75_interaction_insert_works_on_narrow_slide() {
    for size in [1, 10, 40, 60, 99] {
        for kind in [
            "tabs",
            "accordion",
            "flip_cards",
            "hotspot_reveal",
            "process_steps",
            "timeline",
            "reveal_grid",
            "scenario_choice",
        ] {
            let mut s = session(json!({
                "slide_size": { "w": size, "h": size },
                "scenes": [{ "id": "sc1", "title": "S", "slides": [slide("s1", json!([]), json!([]))] }]
            }));
            let res = s.execute(
                "interaction.insert",
                &json!({ "slide": "s1", "kind": kind }),
            );
            assert!(res.is_ok(), "size {size} {kind}: {res:?}");
            assert_valid(&s);
        }
    }
}

#[test]
fn n72_question_delete_keeps_buttons_and_layers_other_triggers_use() {
    let button = json!({
        "id": "go", "kind": { "type": "button", "label": "Go" },
        "frame": { "x": 10, "y": 10, "w": 80, "h": 30 }
    });
    let triggers = json!([
        { "when": { "event": "click", "object": "go" }, "action": { "action": "submit_question" } },
        { "when": { "event": "key_pressed", "key": "enter", "object": "go" },
          "action": { "action": "pause_timeline" } },
        { "when": { "event": "timeline_starts", "layer": "Correct" },
          "action": { "action": "pause_timeline" } }
    ]);
    let v = json!({
        "variables": [{ "name": "v", "value": 1 }],
        "scenes": [{ "id": "sc1", "title": "S", "slides": [{
            "id": "s1", "title": "s1", "duration_ms": 5000,
            "objects": [button],
            "layers": [{ "id": "Correct", "name": "Correct",
                         "objects": [{ "id": "ct", "kind": { "type": "text", "text": "Correct!" },
                                       "frame": { "x": 10, "y": 60, "w": 100, "h": 30 } }] }],
            "triggers": triggers,
            "question": { "kind": "freeform", "prompt": "Q", "choices": [],
                          "correct_when": { "compare": { "variable": "v", "op": "eq", "value": 1 } } }
        }] }]
    });
    let mut s = session(v);
    let res = s.execute("question.delete", &json!({ "slide": "s1" }));
    assert!(res.is_ok(), "{res:?}");
    assert_valid(&s);
    assert!(s.course.scenes[0].slides[0].object("go").is_some());
    assert_eq!(s.course.scenes[0].slides[0].layers.len(), 1);
}
