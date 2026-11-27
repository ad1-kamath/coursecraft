//! Animation timing edits: moving an object moves its emphasis, copy-animation windows,
//! and groups of one left behind by a delete.

use coursecraft_course::Object;
use serde_json::{Value as Json, json};

use crate::Session;

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    s.execute(id, &p)
        .unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

fn add_shape(s: &mut Session, timing: Json) -> String {
    ok(
        s,
        "object.add",
        json!({ "slide": "slide1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 },
                "timing": timing }),
    )["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn get(s: &Session, id: &str) -> Object {
    s.course
        .slide("slide1")
        .and_then(|sl| sl.object(id))
        .map(|(o, _)| o.clone())
        .unwrap()
}

fn add_emphasis(s: &mut Session, id: &str, start: u32) {
    ok(
        s,
        "object.add_emphasis",
        json!({ "slide": "slide1", "object": id, "effect": "pulse", "start_ms": start, "duration_ms": 1000 }),
    );
}

#[test]
fn moving_an_object_moves_its_emphasis() {
    let mut s = Session::blank("T");
    let a = add_shape(&mut s, json!({ "start_ms": 0 }));
    add_emphasis(&mut s, &a, 0);
    add_emphasis(&mut s, &a, 400);
    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": a, "timing": { "start_ms": 3000 } }),
    );
    let o = get(&s, &a);
    assert_eq!(o.emphasis[0].start_ms, 3000);
    assert_eq!(o.emphasis[1].start_ms, 3400);
    // Moving back shifts by the same delta and one undo restores everything.
    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": a, "timing": { "start_ms": 1000 } }),
    );
    assert_eq!(get(&s, &a).emphasis[1].start_ms, 1400);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(get(&s, &a).emphasis[1].start_ms, 3400);
}

#[test]
fn changing_only_the_end_keeps_emphasis_and_huge_moves_saturate() {
    let mut s = Session::blank("T");
    let a = add_shape(&mut s, json!({ "start_ms": 2000 }));
    add_emphasis(&mut s, &a, 2500);
    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": a, "timing": { "start_ms": 2000, "end_ms": 5000 } }),
    );
    assert_eq!(get(&s, &a).emphasis[0].start_ms, 2500);
    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": a, "timing": { "start_ms": 4_000_000_000u32 } }),
    );
    assert!(get(&s, &a).emphasis[0].start_ms >= 4_000_000_000u32);
    ok(
        &mut s,
        "object.update",
        json!({ "slide": "slide1", "object": a, "timing": { "start_ms": 0 } }),
    );
    // Never below the new start, and it keeps its offset (500 ms after it, saturated).
    assert!(get(&s, &a).emphasis[0].start_ms <= 500 + 4_000_000_000u32);
}

#[test]
fn copy_animation_clamps_early_emphasis_into_the_target_window() {
    let mut s = Session::blank("T");
    let a = add_shape(&mut s, json!({ "start_ms": 2000 }));
    // An old file: emphasis before the object's start.
    add_emphasis(&mut s, &a, 500);
    let b = add_shape(&mut s, json!({ "start_ms": 1000, "end_ms": 1800 }));
    ok(
        &mut s,
        "object.copy_animation",
        json!({ "slide": "slide1", "from": a, "objects": [b] }),
    );
    // Offset clamps to 0 (target start) and the whole pass stays inside the window.
    assert_eq!(get(&s, &b).emphasis[0].start_ms, 1000);
    // A later offset is pulled back so the pass fits the target window.
    let c = add_shape(&mut s, json!({ "start_ms": 0 }));
    add_emphasis(&mut s, &c, 2000);
    let d = add_shape(&mut s, json!({ "start_ms": 100, "end_ms": 1500 }));
    ok(
        &mut s,
        "object.copy_animation",
        json!({ "slide": "slide1", "from": c, "objects": [d] }),
    );
    assert_eq!(get(&s, &d).emphasis[0].start_ms, 500);
}

#[test]
fn deleting_a_member_dissolves_a_group_of_one() {
    let mut s = Session::blank("T");
    let a = add_shape(&mut s, json!({}));
    let b = add_shape(&mut s, json!({}));
    ok(
        &mut s,
        "object.group",
        json!({ "slide": "slide1", "objects": [a, b] }),
    );
    assert!(get(&s, &a).group.is_some());
    let before = s.course.clone();
    ok(
        &mut s,
        "object.delete",
        json!({ "slide": "slide1", "object": b }),
    );
    assert_eq!(get(&s, &a).group, None);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}
