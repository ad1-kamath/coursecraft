//! Bug-hunt proof tests for `coursecraft-engine` (0.6.1 Neon review).
//!
//! These tests started as ignored proofs of bugs; each now passes because the bug is fixed.

use coursecraft_course::{Course, ObjectKind, Severity, ShapeKind};
use coursecraft_engine::Session;
use serde_json::json;

fn errors_of(course: &Course) -> Vec<String> {
    course
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

/// `text_style.create` and `text_style.update` omit `keep_valid` and accept
/// unvalidated `Paragraph` JSON, leaving the course in a state `validate()` rejects.
#[test]
fn text_style_create_leaves_invalid_course() {
    let mut s = Session::blank("Style Test");
    assert!(errors_of(&s.course).is_empty());

    let res = s.execute(
        "text_style.create",
        &json!({
            "name": "BadStyle",
            "style": {},
            "paragraphs": {
                "level": 9,
                "line_spacing": 99.0,
                "space_before": -5.0,
                "bullet_color": "not-a-color",
                "start_at": 0
            }
        }),
    );
    if res.is_ok() {
        let errs = errors_of(&s.course);
        assert!(
            errs.is_empty(),
            "text_style.create succeeded but left an invalid course: {errs:?}"
        );
    }
}

/// Post-`keep_valid` connector rerouting (`reroute_connectors_in_slide`) runs
/// after validation and can resize a connector beyond `MAX_OBJECT_FRAME_SIZE`
/// (100,000 px), leaving an invalid course while caching `is_playable() == true`.
#[test]
fn connector_reroute_after_keep_valid_leaves_invalid_course() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "conn-reroute",
          "title": "Connector Reroute",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "a",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                  "frame": { "x": 0, "y": 0, "w": 100, "h": 100 }
                },
                {
                  "id": "b",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                  "frame": { "x": 400, "y": 0, "w": 100, "h": 100 }
                },
                {
                  "id": "c1",
                  "kind": {
                    "type": "path",
                    "points": [{ "x": 0.0, "y": 0.5 }, { "x": 1.0, "y": 0.5 }],
                    "closed": false,
                    "outline": { "color": "#000000", "width": 2.0 },
                    "connector": "straight",
                    "start_glue": { "object": "a", "site": 1 },
                    "end_glue": { "object": "b", "site": 3 }
                  },
                  "frame": { "x": 100, "y": 40, "w": 300, "h": 20 }
                }
              ]
            }]
          }]
        }"##,
    )
    .expect("valid course");
    assert!(errors_of(&course).is_empty());

    let mut s = Session::new(course);
    let res = s.execute(
        "object.set_frame",
        &json!({
            "slide": "s1",
            "object": "b",
            "frame": { "x": 150000.0, "y": 0.0, "w": 100.0, "h": 100.0 }
        }),
    );
    if res.is_ok() {
        let errs = errors_of(&s.course);
        assert!(
            errs.is_empty(),
            "object.set_frame succeeded but left invalid course: {errs:?}"
        );
    }
}

/// `recolor_course` removes `theme.palette` from `source` before serializing,
/// causing `is_default_theme` to omit `"theme"` when theme colors are still
/// default, and then only runs `recolor_theme` when `palette.is_empty()`.
#[test]
fn recolor_skips_default_theme_colors_when_palette_non_empty() {
    let mut s = Session::blank("Recolor Test");
    let orig_accent = s.course.theme.accent.0.clone();
    s.execute(
        "theme.set_palette",
        &json!({
            "colors": [{ "id": "brand", "name": "Brand", "color": orig_accent }]
        }),
    )
    .expect("set_palette");

    s.execute(
        "theme.set_palette_color",
        &json!({
            "id": "brand",
            "color": "#D9383A",
            "recolor": true
        }),
    )
    .expect("set_palette_color");

    assert_eq!(
        s.course.theme.accent.0, "#D9383A",
        "theme.accent should be recolored along with the brand palette color"
    );
}

/// `EFFECTS[0]` (`"appear"`) has `default_duration_ms: 0`, which both
/// `object.set_animation` and `validate()` reject (`1..=60000`).
#[test]
fn appear_default_duration_ms_rejected_by_set_animation() {
    let mut s = Session::blank("Appear Test");
    let slide_id = s.course.scenes[0].slides[0].id.clone();
    let obj_id = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                "frame": { "x": 40, "y": 40, "w": 100, "h": 60 }
            }),
        )
        .expect("add obj")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    let appear_def = coursecraft_course::effect_def("appear").expect("appear effect exists");
    let res = s.execute(
        "object.set_animation",
        &json!({
            "slide": slide_id,
            "object": obj_id,
            "which": "entrance",
            "animation": {
                "effect": { "type": "appear" },
                "duration_ms": appear_def.default_duration_ms
            }
        }),
    );
    assert!(
        res.is_ok(),
        "applying appear with its own default_duration_ms ({}) failed: {res:?}",
        appear_def.default_duration_ms
    );
}

/// `course.replace` with 0 matches still mutates `state.text` (filling `Some(...)`
/// from `runs`) while setting `skip_undo = true`, causing an unrecorded,
/// un-undoable mutation of `Session::course`.
#[test]
fn course_replace_zero_matches_mutates_state_text_without_undo() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "replace-mutate",
          "title": "Replace Mutate",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "t1",
                "kind": { "type": "text", "text": "Base" },
                "frame": { "x": 10, "y": 10, "w": 200, "h": 40 },
                "states": [{
                  "name": "hover",
                  "runs": [{ "text": "HoverText", "bold": true }]
                }]
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let before = course.clone();
    let mut s = Session::new(course);
    let res = s
        .execute(
            "course.replace",
            &json!({ "query": "DOES_NOT_EXIST", "replacement": "X" }),
        )
        .expect("course.replace");
    assert_eq!(res["replaced"], json!(0));
    assert!(!s.can_undo(), "no undo step recorded for 0 replacements");
    assert_eq!(
        s.course, before,
        "course was silently mutated when replaced == 0 and no undo step was pushed"
    );
}

/// `table.merge` only checks the anchor cell, permitting overlapping merged
/// ranges that `validate()` also fails to detect.
#[test]
fn table_merge_allows_overlapping_merges() {
    let mut s = Session::blank("Table Overlap");
    let slide_id = s.course.scenes[0].slides[0].id.clone();
    let tbl_id = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "kind": {
                    "type": "table",
                    "columns": [0.5, 0.5],
                    "rows": [
                        { "height": 0.5, "cells": [{ "text": "A" }, { "text": "B" }] },
                        { "height": 0.5, "cells": [{ "text": "C" }, { "text": "D" }] }
                    ]
                },
                "frame": { "x": 20, "y": 20, "w": 300, "h": 200 }
            }),
        )
        .expect("add table")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    // First merge covers (0,1) and (1,1).
    s.execute(
        "table.merge",
        &json!({
            "slide": slide_id,
            "object": tbl_id,
            "row": 0,
            "column": 1,
            "rows": 2,
            "columns": 1
        }),
    )
    .expect("first merge");

    // Second merge covers (1,0) and (1,1) — overlaps (1,1)!
    let overlap = s.execute(
        "table.merge",
        &json!({
            "slide": slide_id,
            "object": tbl_id,
            "row": 1,
            "column": 0,
            "rows": 1,
            "columns": 2
        }),
    );
    assert!(
        overlap.is_err(),
        "overlapping merge into cell (1,1) already owned by (0,1) must be rejected"
    );
}

/// `layer.duplicate` remaps `object.group` on copied objects without remapping
/// `copy.groups`, causing `keep_valid` to fail on duplicating any layer with a
/// `Group` record.
#[test]
fn layer_duplicate_fails_when_layer_has_group_record() {
    let mut s = Session::blank("Layer Dup");
    let slide_id = s.course.scenes[0].slides[0].id.clone();
    let lid = s
        .execute("layer.add", &json!({ "slide": slide_id, "name": "L1" }))
        .expect("layer.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let o1 = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "layer": lid,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                "frame": { "x": 20, "y": 20, "w": 60, "h": 40 }
            }),
        )
        .expect("o1")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let o2 = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "layer": lid,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                "frame": { "x": 100, "y": 20, "w": 60, "h": 40 }
            }),
        )
        .expect("o2")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let gid = s
        .execute(
            "object.group",
            &json!({ "slide": slide_id, "objects": [o1, o2] }),
        )
        .expect("group")["group"]
        .as_str()
        .expect("gid")
        .to_owned();
    s.execute(
        "group.set_interactive",
        &json!({ "slide": slide_id, "group": gid, "interactive": true }),
    )
    .expect("set_interactive");

    let dup = s.execute(
        "layer.duplicate",
        &json!({ "slide": slide_id, "layer": lid }),
    );
    assert!(
        dup.is_ok(),
        "layer.duplicate should succeed on a valid layer with an interactive group, got: {dup:?}"
    );
}

/// `slide.duplicate` drops `Course.BackgroundAudio` triggers and fails `keep_valid`
/// on freeform questions whose `correct_when` uses `Timer.<id>.Remaining`.
#[test]
fn slide_duplicate_drops_bg_audio_and_fails_on_timer_freeform() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "dup-slide",
          "title": "Dup Slide",
          "background_audio": { "src": "bg.mp3" },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "t1",
                "kind": { "type": "timer", "duration_ms": 3000 },
                "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
              }],
              "triggers": [{
                "when": { "event": "timeline_starts" },
                "action": { "action": "pause_media", "object": "Course.BackgroundAudio" }
              }],
              "question": {
                "kind": "freeform",
                "prompt": "Finish in time",
                "choices": [],
                "correct_when": {
                  "compare": { "variable": "Timer.t1.Remaining", "op": "gt", "value": 0 }
                }
              }
            }]
          }]
        }"#,
    )
    .expect("valid course");
    assert!(errors_of(&course).is_empty());

    let mut s = Session::new(course);
    let res = s.execute("slide.duplicate", &json!({ "slide": "s1" }));
    assert!(
        res.is_ok(),
        "slide.duplicate failed on valid slide with Timer.<id>.Remaining freeform condition: {res:?}"
    );
    let copy = &s.course.scenes[0].slides[1];
    assert_eq!(
        copy.triggers.len(),
        1,
        "Course.BackgroundAudio trigger must be preserved on duplicated slide"
    );
}

/// `object.apply_style` copies `outline` onto line shapes, failing `keep_valid`
/// because line shapes cannot carry an outline.
#[test]
fn object_apply_style_fails_on_line_shape_target() {
    let mut s = Session::blank("Apply Style");
    let slide_id = s.course.scenes[0].slides[0].id.clone();
    let src = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "kind": {
                    "type": "shape",
                    "shape": "rectangle",
                    "fill": "#2266CC",
                    "outline": { "color": "#000000", "width": 2.0 }
                },
                "frame": { "x": 20, "y": 20, "w": 100, "h": 60 }
            }),
        )
        .expect("src")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let line = s
        .execute(
            "object.add",
            &json!({
                "slide": slide_id,
                "kind": {
                    "type": "shape",
                    "shape": "line_horizontal",
                    "fill": "#FF0000"
                },
                "frame": { "x": 20, "y": 120, "w": 100, "h": 4 }
            }),
        )
        .expect("line")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    let res = s.execute(
        "object.apply_style",
        &json!({
            "source_slide": slide_id,
            "source": src,
            "slide": slide_id,
            "objects": [line]
        }),
    );
    assert!(
        res.is_ok(),
        "object.apply_style onto a line shape should omit incompatible outline instead of failing: {res:?}"
    );
    // Keep unused imports clean
    let _ = (
        ObjectKind::Audio {
            src: String::new(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
        },
        ShapeKind::Rectangle,
    );
}

/// Spec (`docs/spec/course-model.md`) and `validate.rs:585-588` allow `.ttc` and
/// `.otc` font collections, but `course.add_font` rejects them.
#[test]
fn course_add_font_rejects_ttc_font_collection() {
    let mut s = Session::blank("Font TTC");
    let res = s.execute(
        "course.add_font",
        &json!({
            "family": "TestFont",
            "style": "regular",
            "src": "fonts/test.ttc"
        }),
    );
    assert!(
        res.is_ok(),
        "course.add_font should accept .ttc matching validate.rs and course-model.md: {res:?}"
    );
}
