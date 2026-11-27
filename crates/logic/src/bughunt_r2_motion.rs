//! Round 2 bug-hunt tests: animations that do not fit the timeline, reduced motion, the
//! fade transition, and overshoot/elastic easing on emphasis and motion paths.

use crate::{Input, Player};
use coursecraft_course::{Course, Value};

fn one_slide(duration_ms: u32, objects: serde_json::Value) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "r2-test",
        "title": "R2 Test",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": [{ "name": "n", "value": 0.0 }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": duration_ms,
                "objects": objects,
                "triggers": [{
                    "when": { "event": "animation_completes", "object": "o" },
                    "action": { "action": "set_variable", "variable": "n", "op": "add", "value": 1.0 }
                }]
            }]
        }]
    })
    .to_string();
    Course::from_json(&json).expect("valid course")
}

fn rect_obj(extra: serde_json::Value) -> serde_json::Value {
    let mut base = serde_json::json!({
        "id": "o",
        "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
        "frame": { "x": 200.0, "y": 200.0, "w": 100.0, "h": 100.0 }
    });
    if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
    base
}

fn fade(ms: u32) -> serde_json::Value {
    serde_json::json!({ "effect": { "type": "fade" }, "duration_ms": ms, "easing": "linear" })
}

fn started(course: &Course) -> Player<'_> {
    let mut p = Player::new(course).expect("player");
    let _ = p.handle(&Input::Start);
    p
}

fn opacity(p: &Player<'_>) -> Option<f32> {
    p.sample_object("o").map(|s| s.opacity)
}

fn n<'a>(p: &'a Player<'_>) -> Option<&'a Value> {
    p.variable("n")
}

// ---- C2 --------------------------------------------------------------------

#[test]
fn c2_entrance_past_the_timeline_end_completes_at_the_end() {
    let course = one_slide(
        2000,
        serde_json::json!([rect_obj(serde_json::json!({
            "timing": { "start_ms": 1800 },
            "entrance": fade(1000)
        }))]),
    );
    let mut p = started(&course);
    let _ = p.handle(&Input::Tick { ms: 1900 });
    let mid = opacity(&p).expect("visible");
    assert!(mid > 0.0 && mid < 0.5, "mid-entrance {mid}");
    assert_eq!(n(&p), Some(&Value::Number(0.0)));
    let _ = p.handle(&Input::Tick { ms: 5000 });
    assert_eq!(opacity(&p), Some(1.0), "fully shown once the timeline ends");
    assert_eq!(n(&p), Some(&Value::Number(1.0)));
    let _ = p.handle(&Input::Tick { ms: 5000 });
    assert_eq!(n(&p), Some(&Value::Number(1.0)), "fires once");
}

#[test]
fn c2_exit_past_the_timeline_end_hides_at_the_end() {
    let course = one_slide(
        2000,
        serde_json::json!([rect_obj(serde_json::json!({
            "timing": { "start_ms": 0, "end_ms": 3000 },
            "exit": fade(1500)
        }))]),
    );
    let mut p = started(&course);
    let _ = p.handle(&Input::Tick { ms: 5000 });
    assert_eq!(opacity(&p), None, "exit completes at once at the end");
}

#[test]
fn c2_exit_never_starts_before_the_object_does() {
    let course = one_slide(
        5000,
        serde_json::json!([rect_obj(serde_json::json!({
            "timing": { "start_ms": 500, "end_ms": 800 },
            "exit": fade(1000)
        }))]),
    );
    let mut p = started(&course);
    let _ = p.handle(&Input::Tick { ms: 500 });
    let at_start = opacity(&p).expect("visible");
    assert!(at_start > 0.99, "starts at rest, got {at_start}");
    let _ = p.handle(&Input::Tick { ms: 150 });
    let mid = opacity(&p).expect("visible");
    assert!((mid - 0.5).abs() < 0.05, "half way, got {mid}");
    let _ = p.handle(&Input::Tick { ms: 150 });
    assert_eq!(opacity(&p), None);
}

// ---- C5 --------------------------------------------------------------------

fn fly(ms: u32) -> serde_json::Value {
    serde_json::json!({
        "effect": { "type": "fly_in", "from": "left" },
        "duration_ms": ms,
        "easing": "ease_out"
    })
}

#[test]
fn c5_reduced_motion_turns_entrances_and_exits_into_fades_with_the_same_timing() {
    let course = one_slide(
        5000,
        serde_json::json!([rect_obj(serde_json::json!({
            "timing": { "start_ms": 0, "end_ms": 4000 },
            "entrance": fly(1000),
            "exit": fly(1000)
        }))]),
    );
    let mut p = started(&course);
    let _ = p.handle(&Input::Tick { ms: 500 });
    assert!(
        p.sample_object("o").expect("on").offset.0.abs() > 1.0,
        "flies"
    );
    p.set_reduced_motion(true);
    let s = p.sample_object("o").expect("on");
    assert_eq!(s.offset, (0.0, 0.0), "no travel: {s:?}");
    assert!(s.opacity > 0.0 && s.opacity < 1.0, "fades: {s:?}");
    // Same timing: the completion event fires at 1000 ms, the exit runs over 3000..4000.
    let _ = p.handle(&Input::Tick { ms: 500 });
    assert_eq!(n(&p), Some(&Value::Number(1.0)));
    assert_eq!(opacity(&p), Some(1.0));
    let _ = p.handle(&Input::Tick { ms: 2500 });
    let s = p.sample_object("o").expect("on");
    assert_eq!(s.offset, (0.0, 0.0));
    assert!(s.opacity < 1.0, "exit fades: {s:?}");
    let _ = p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(opacity(&p), None);
    // Switching it off restores the authored effect.
    p.set_reduced_motion(false);
    assert!(!p.reduced_motion());
    assert!(p.sample_object("o").is_none() || opacity(&p).is_some());
}

#[test]
fn c5_reduced_motion_emphasis_plays_once_or_not_at_all() {
    let emph = |id: &str, repeat: u32| {
        serde_json::json!([rect_obj(serde_json::json!({
            "emphasis": [{
                "effect": { "type": id },
                "start_ms": 0,
                "duration_ms": 1000,
                "repeat": repeat
            }]
        }))])
    };
    // Motion effects stay at rest.
    for id in [
        "spin",
        "shake",
        "blink",
        "jump",
        "teeter",
        "pulse",
        "heartbeat",
    ] {
        let course = one_slide(5000, emph(id, 3));
        let mut p = started(&course);
        p.set_reduced_motion(true);
        for _ in 0..8 {
            let _ = p.handle(&Input::Tick { ms: 400 });
            let s = p.sample_object("o").expect("on");
            assert_eq!(s, crate::ObjectSample::IDENTITY, "{id} at rest: {s:?}");
        }
    }
    // A colour or opacity effect plays one pass and does not loop.
    let course = one_slide(5000, emph("transparency", 3));
    let mut p = started(&course);
    p.set_reduced_motion(true);
    let _ = p.handle(&Input::Tick { ms: 500 });
    assert!(opacity(&p).expect("on") < 0.7, "first pass plays");
    let _ = p.handle(&Input::Tick { ms: 1000 }); // 1500: second pass would be running
    assert_eq!(opacity(&p), Some(1.0), "no second pass");
    p.set_reduced_motion(false);
    assert!(opacity(&p).expect("on") < 0.7, "normal motion loops");
}

#[test]
fn c5_reduced_motion_slide_transitions_become_fades() {
    use coursecraft_course::{Direction, Size, Transition, TransitionEffect};
    let size = Size { w: 960, h: 540 };
    for effect in [
        TransitionEffect::Push {
            from: Direction::Left,
        },
        TransitionEffect::Zoom,
        TransitionEffect::FadeBlack,
        TransitionEffect::Fade,
    ] {
        let t = Transition {
            effect,
            duration_ms: 700,
        };
        let r = crate::animation::reduced_motion_transition(&t);
        assert_eq!(r.effect, TransitionEffect::Fade);
        assert_eq!(r.duration_ms, 700);
        let s = crate::sample_transition(&r, 0.5, size);
        assert_eq!(s.next.offset, (0.0, 0.0));
    }
}

// ---- C8 --------------------------------------------------------------------

#[test]
fn c8_fade_transition_keeps_the_old_slide_opaque() {
    use crate::sample_transition;
    use coursecraft_course::{Size, Transition, TransitionEffect};
    let t = Transition {
        effect: TransitionEffect::Fade,
        duration_ms: 500,
    };
    let size = Size { w: 960, h: 540 };
    for p in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        let s = sample_transition(&t, p, size);
        assert_eq!(s.prev.alpha, 1.0, "old slide opaque at {p}");
        assert!((s.next.alpha - p).abs() < 1e-6, "new slide fades in at {p}");
        assert!(s.backdrop.is_none());
        assert!(!s.prev_on_top);
    }
}

// ---- C9 --------------------------------------------------------------------

fn emphasis_at(effect: &str, easing: &str, ms: u32) -> crate::ObjectSample {
    let course = one_slide(
        5000,
        serde_json::json!([rect_obj(serde_json::json!({
            "emphasis": [{
                "effect": { "type": effect },
                "start_ms": 0,
                "duration_ms": 1000,
                "easing": easing
            }]
        }))]),
    );
    let mut p = started(&course);
    let _ = p.handle(&Input::Tick { ms });
    p.sample_object("o").expect("visible")
}

#[test]
fn c9_pulse_with_overshoot_does_not_snap_to_rest_while_running() {
    // Back-out easing is about 1.03 at 85 % of the pass, so the pulse is past its end.
    let s = emphasis_at("pulse", "overshoot", 850);
    assert!(s.scale.0 < 0.999, "pulse follows the eased value: {s:?}");
    assert!(s.scale.0 > 0.9 && s.scale.0.is_finite());
    // The pass still ends at rest.
    let s = emphasis_at("pulse", "overshoot", 1000);
    assert_eq!(s.scale, (1.0, 1.0));
}

#[test]
fn c9_spin_with_overshoot_and_elastic_goes_past_a_full_turn() {
    let s = emphasis_at("spin", "overshoot", 850);
    assert!(s.rotation_deg > 361.0, "overshoot: {}", s.rotation_deg);
    let s = emphasis_at("spin", "elastic", 150);
    assert!(s.rotation_deg > 361.0, "elastic: {}", s.rotation_deg);
    let s = emphasis_at("spin", "elastic", 0);
    assert_eq!(s.rotation_deg, 0.0);
}

// ---- C11 -------------------------------------------------------------------

#[test]
fn c11_motion_path_overshoot_and_elastic_run_past_the_end() {
    use crate::sample_motion_path;
    use coursecraft_course::{Easing, MotionPath, Point, Rect};
    let frame = Rect {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
    };
    let path = |easing, points: Vec<Point>| MotionPath {
        id: "p".into(),
        points,
        duration_ms: 1000,
        easing,
        relative: true,
    };
    let pt = |x, y| Point { x, y };
    let line = vec![pt(0.0, 0.0), pt(100.0, 0.0)];
    let at = |easing, ms| sample_motion_path(&path(easing, line.clone()), ms, (0.0, 0.0), frame);

    let over = at(Easing::Overshoot, 850);
    assert!(over.0 > 100.0 && over.0 < 120.0, "overshoot {over:?}");
    assert_eq!(over.1, 0.0);
    let spring = at(Easing::Elastic, 150);
    assert!(
        spring.0 > 120.0 && spring.0.is_finite(),
        "elastic {spring:?}"
    );
    // It ends exactly at the last point.
    assert_eq!(at(Easing::Overshoot, 1000), (100.0, 0.0));
    // Past the end follows the last segment's direction, skipping a zero-length tail.
    let bent = vec![pt(0.0, 0.0), pt(0.0, 100.0), pt(0.0, 100.0)];
    let b = sample_motion_path(&path(Easing::Overshoot, bent), 850, (0.0, 0.0), frame);
    assert!(b.1 > 100.0 && b.0 == 0.0 && b.1.is_finite(), "bent {b:?}");
}
