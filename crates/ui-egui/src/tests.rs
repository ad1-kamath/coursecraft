//! Headless UI tests for `coursecraft-ui-egui` (M3 Step 3b & Step 4).

use crate::*;
use coursecraft_ai::agents::{AgentId, AgentStatus};
use coursecraft_course::{
    ConnectorStyle, Course, LineEnd, Object, ObjectKind, PathStyle, Rect as CourseRect, ShapeKind,
};
use coursecraft_render::{SlideTransform, compose_authoring};
use egui::{Context, Event, Key, Modifiers, PointerButton, RawInput, Rect, pos2, vec2};

use crate::test_harness::TestHarness;
const COURSE_JSON: &str = include_str!("../../../examples/hello-course/course.json");

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn course_with_table() -> (Course, String) {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "table-ui",
        "title": "Table UI",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "slide1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [{
                    "id": "table1",
                    "kind": {
                        "type": "table",
                        "columns": [0.33333334, 0.33333334, 0.33333334],
                        "rows": [
                            { "height": 0.33333334, "cells": [{ "text": "A" }, { "text": "B" }, { "text": "C" }] },
                            { "height": 0.33333334, "cells": [{ "text": "D" }, { "text": "E" }, { "text": "F" }] },
                            { "height": 0.33333334, "cells": [{ "text": "G" }, { "text": "H" }, { "text": "I" }] }
                        ],
                        "borders": {
                            "outer": { "color": "#1F1F1F", "width": 1 },
                            "inner": { "color": "#1F1F1F", "width": 1 }
                        }
                    },
                    "frame": { "x": 300, "y": 200, "w": 600, "h": 300 }
                }]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid table course");
    (course, "table1".into())
}

fn table_canvas_transform(harness: &TestHarness) -> SlideTransform {
    SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform")
}

#[test]
fn double_click_edits_a_table_cell_and_tab_moves_to_the_next() {
    let (course, _) = course_with_table();
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "slide1".into();
    harness.idle_frame();
    let transform = table_canvas_transform(&harness);
    let cell = transform.to_screen_pos(pos2(400.0, 250.0));
    harness.click_at(cell);
    harness.click_at(cell);
    harness.idle_frame();
    let edit = harness
        .editor
        .ui
        .inline_edit
        .as_ref()
        .expect("double click edits a cell");
    assert_eq!(edit.cell, Some((0, 0)));

    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("Hello");
    harness.press(Key::Tab, Modifiers::NONE);
    harness.idle_frame();

    let text = harness
        .editor
        .session
        .course
        .slide("slide1")
        .and_then(|slide| slide.object("table1"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Table { rows, .. } => Some(rows[0].cells[0].text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(text, "Hello");
    assert_eq!(
        harness
            .editor
            .ui
            .inline_edit
            .as_ref()
            .and_then(|edit| edit.cell),
        Some((0, 1)),
        "Tab moves to the next cell"
    );

    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_none());
}

#[test]
fn dragging_a_table_border_resizes_columns_with_a_five_percent_floor() {
    let (course, _) = course_with_table();
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "slide1".into();
    harness.editor.selection.object = Some("table1".into());
    harness.editor.ui.table_cell = Some((0, 0));
    harness.idle_frame();
    let transform = table_canvas_transform(&harness);
    let boundary = transform.to_screen_pos(pos2(500.0, 350.0));
    harness.drag_by(boundary, vec2(60.0, 0.0), 6, Modifiers::NONE);
    harness.idle_frame();

    let columns = harness
        .editor
        .session
        .course
        .slide("slide1")
        .and_then(|slide| slide.object("table1"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Table { columns, .. } => Some(columns.clone()),
            _ => None,
        })
        .expect("columns");
    assert!((columns.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    assert!(
        columns[0] > 0.4 && columns[1] < 0.27,
        "dragging right must grow column 0 and shrink column 1: {columns:?}"
    );
    assert!(
        columns
            .iter()
            .all(|value| *value >= coursecraft_course::TABLE_MIN_FRACTION - 1e-5),
        "{columns:?}"
    );
}

#[test]
fn piece_1d_to_question_carries_per_answer_scoring_and_choice_points() {
    use coursecraft_course::{Choice, QuestionKind, Scoring};

    let state = QuestionState {
        kind: QuestionKind::MultipleChoice,
        prompt: "p".to_owned(),
        choices: vec![
            Choice {
                id: "a".to_owned(),
                text: "x".to_owned(),
                correct: true,
                points: 5,
            },
            Choice {
                id: "b".to_owned(),
                text: "y".to_owned(),
                correct: false,
                points: -2,
            },
        ],
        scoring: Scoring::PerAnswer,
        ..Default::default()
    };

    let q = state.to_question();
    assert_eq!(q.scoring, Scoring::PerAnswer);
    assert_eq!(q.choices[0].points, 5);
    assert_eq!(q.choices[1].points, -2);
}

#[test]
fn piece_1d_scoring_options_by_kind() {
    use coursecraft_course::{QuestionKind, Scoring};

    assert_eq!(
        crate::question::scoring_options(QuestionKind::MultipleChoice),
        &[Scoring::AllOrNothing, Scoring::PerAnswer]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::TrueFalse),
        &[Scoring::AllOrNothing, Scoring::PerAnswer]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::MultipleResponse),
        &[Scoring::AllOrNothing, Scoring::PerAnswer, Scoring::Partial]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::Sequence),
        &[Scoring::AllOrNothing, Scoring::Partial]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::Matching),
        &[Scoring::AllOrNothing, Scoring::Partial]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::Hotspot),
        &[Scoring::AllOrNothing, Scoring::Partial]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::DragDrop),
        &[Scoring::AllOrNothing, Scoring::Partial]
    );
    assert_eq!(
        crate::question::scoring_options(QuestionKind::FillIn),
        &[] as &[Scoring]
    );
}

#[test]
fn editor_new_renders_without_panic() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let _ = harness.idle_frame();
    assert_eq!(harness.editor.selection.slide, "s-intro");
    assert_eq!(harness.editor.ui.validation_errors, 0);
}

#[test]
fn editor_run_batch_success_and_failure() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut editor = Editor::new(course);

    let steps = vec![
        (
            "slide.add".to_string(),
            serde_json::json!({ "title": "Batch Slide" }),
        ),
        (
            "object.add".to_string(),
            serde_json::json!({
                "slide": "slide1",
                "kind": { "type": "button", "label": "Batch Button" },
                "frame": { "x": 0, "y": 0, "w": 100, "h": 50 }
            }),
        ),
    ];
    let res = editor.run_batch(&steps).expect("batch should succeed");
    assert_eq!(res.len(), 2);
    // Last created item is the button on slide1
    assert!(editor.selection.object.is_some());

    // Failing batch
    let bad_steps = vec![(
        "trigger.add".to_string(),
        serde_json::json!({ "slide": "slide1", "trigger": { "action": { "action": "show_layer", "layer": "ghost" }, "when": { "event": "submit" } } }),
    )];
    let bad_res = editor.run_batch(&bad_steps);
    assert!(bad_res.is_none());
    assert!(editor.ui.last_error.is_some());
    assert!(!editor.ui.error_history.is_empty());
}

#[test]
fn clicking_s_tabs_thumbnail_selects_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    assert_eq!(harness.editor.selection.slide, "s-intro");

    let panel_rect = harness.left_panel_rect();
    let thumb_rect = slide_thumbnail_rect(&harness.editor.session.course, panel_rect, "s-tabs")
        .expect("thumbnail rect for s-tabs");

    harness.click_at(thumb_rect.center());
    assert_eq!(harness.editor.selection.slide, "s-tabs");
    assert_eq!(harness.editor.selection.layer, None);
    assert_eq!(harness.editor.selection.object, None);
}

#[test]
fn clicking_tab1_on_canvas_selects_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let tab1_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-tabs",
        "tab1",
        central_rect,
    )
    .expect("screen rect for tab1");

    harness.click_at(tab1_rect.center());
    assert_eq!(harness.editor.selection.object.as_deref(), Some("tab1"));
}

#[test]
fn object_delete_and_undo_restores_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    // 1. tab1 has triggers. Delete takes them with it, says so, and one
    // Undo brings everything back.
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.object = Some("tab1".into());
    let triggers_before = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs")
        .triggers
        .len();
    harness.frame(vec![Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    assert!(
        harness.editor.ui.last_error.is_none(),
        "{:?}",
        harness.editor.ui.last_error
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs");
    assert!(slide.object("tab1").is_none(), "tab1 deleted");
    assert!(
        slide.triggers.len() < triggers_before,
        "its triggers went with it"
    );
    assert!(harness.editor.session.course.is_playable());
    let status = harness
        .editor
        .ui
        .status_message
        .as_ref()
        .map(|m| m.0.clone())
        .unwrap_or_default();
    assert!(
        status.contains("trigger"),
        "status says triggers went too: {status}"
    );
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs");
    assert!(slide.object("tab1").is_some(), "Undo restores tab1");
    assert_eq!(slide.triggers.len(), triggers_before, "and its triggers");

    // 2. An object no trigger uses: 'title' on slide 's-intro'.
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.frame(vec![Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);

    assert!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .object("title")
            .is_none(),
        "unreferenced object 'title' should be removed"
    );

    // 3. Cmd/Ctrl+Z restores it
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);

    assert!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .object("title")
            .is_some(),
        "'title' should be restored after Undo"
    );
}

#[test]
fn deleting_btn_close_on_a_layer_takes_its_trigger_and_undoes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.editor.selection.object = Some("btn-close".into());
    let has_close = |h: &TestHarness| {
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .layer("about")
            .expect("about layer")
            .objects
            .iter()
            .any(|o| o.id == "btn-close")
    };
    let layer_triggers = |h: &TestHarness| {
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .layer("about")
            .expect("about layer")
            .triggers
            .len()
    };
    let before = layer_triggers(&harness);
    harness.frame(vec![Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    assert!(
        harness.editor.ui.last_error.is_none(),
        "{:?}",
        harness.editor.ui.last_error
    );
    assert!(!has_close(&harness), "btn-close deleted");
    assert!(
        layer_triggers(&harness) < before,
        "its Close trigger went with it"
    );
    assert!(harness.editor.session.course.is_playable());
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    assert!(has_close(&harness), "Undo restores btn-close");
    assert_eq!(layer_triggers(&harness), before, "and its trigger");
}

#[test]
fn menu_adapter_for_slide_delete_produces_selected() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut editor = Editor::new(course);
    editor.selection.slide = "s-tabs".into();

    let params = adapter(&editor, "slide.delete").expect("adapter for slide.delete");
    assert_eq!(params, serde_json::json!({ "slide": "s-tabs" }));
}

#[test]
fn shortcut_parser_handles_standard_shortcuts() {
    let sc_z = parse_shortcut("Ctrl+Z").expect("parse Ctrl+Z");
    assert_eq!(sc_z.modifiers, Modifiers::COMMAND);
    assert_eq!(sc_z.logical_key, Key::Z);

    let sc_y = parse_shortcut("Ctrl+Y").expect("parse Ctrl+Y");
    assert_eq!(sc_y.modifiers, Modifiers::COMMAND);
    assert_eq!(sc_y.logical_key, Key::Y);

    let sc_m = parse_shortcut("Ctrl+M").expect("parse Ctrl+M");
    assert_eq!(sc_m.modifiers, Modifiers::COMMAND);
    assert_eq!(sc_m.logical_key, Key::M);

    let sc_del = parse_shortcut("Delete").expect("parse Delete");
    assert_eq!(sc_del.modifiers, Modifiers::NONE);
    assert_eq!(sc_del.logical_key, Key::Delete);
}

#[test]
fn selection_repair_after_deleting_selected_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut sel = Selection {
        slide: "s-tabs".into(),
        layer: Some("l1".into()),
        object: Some("tab1".into()),
        ..Default::default()
    };

    let mut modified = course;
    modified.scenes[0].slides.retain(|s| s.id != "s-tabs");

    sel.repair(&modified);
    assert_eq!(sel.slide, "s-intro");
    assert_eq!(sel.layer, None);
    assert_eq!(sel.object, None);
}

#[test]
fn click_covered_by_another_layer_does_not_select() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.idle_frame();
    let central_rect = harness.central_panel_rect();
    let pos = object_screen_rect(
        &harness.editor.session.course,
        "s-tabs",
        "tab1",
        central_rect,
    )
    .expect("tab1 rect")
    .center();

    // A click over a foreground layer covering the canvas must not reach the canvas.
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let frames = [
        vec![Event::PointerMoved(pos)],
        vec![Event::PointerMoved(pos)],
        vec![button(true)],
        vec![button(false)],
    ];
    for events in frames {
        let raw = RawInput {
            screen_rect: Some(harness.screen_rect),
            events,
            ..Default::default()
        };
        let editor = &mut harness.editor;
        let out = harness.ctx.run_ui(raw, |ui| {
            egui::Area::new(egui::Id::unique("cover"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos - vec2(50.0, 50.0))
                .show(ui.ctx(), |ui| {
                    ui.allocate_exact_size(vec2(100.0, 100.0), egui::Sense::click());
                });
            let _ = editor.show(ui);
        });
        out.drop_without_applying_deltas();
    }
    assert_eq!(harness.editor.selection.object, None);
}

#[test]
fn hidden_layer_object_is_selectable_when_layer_focused() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let id = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "layer": "about",
                "kind": { "type": "button", "label": "Ghost" },
                "frame": { "x": 100, "y": 100, "w": 120, "h": 60 }
            }),
        )
        .expect("add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    harness
        .editor
        .run(
            "object.update",
            serde_json::json!({ "slide": "s-intro", "object": id, "initial_state": "hidden" }),
        )
        .expect("update");
    harness.editor.selection.layer = Some("about".into());
    harness.idle_frame();

    let rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        &id,
        harness.central_panel_rect(),
    )
    .expect("rect");
    harness.click_at(rect.center());
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(id.as_str())
    );
}

#[test]
fn slide_add_selects_the_new_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = Some("l1".into());
    harness.editor.selection.object = Some("tab1".into());
    let params = adapter(&harness.editor, "slide.add").expect("adapter");
    let res = harness.editor.run("slide.add", params).expect("run");
    let new_id = res["id"].as_str().expect("id");
    assert_eq!(harness.editor.selection.slide, new_id);
    assert_eq!(harness.editor.selection.layer, None);
    assert_eq!(harness.editor.selection.object, None);
}

#[test]
fn object_duplicate_selects_the_copy() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    let params = adapter(&harness.editor, "object.duplicate").expect("adapter");
    let res = harness.editor.run("object.duplicate", params).expect("run");
    let new_id = res["id"].as_str().expect("id");
    assert_ne!(new_id, "title");
    assert_eq!(harness.editor.selection.object.as_deref(), Some(new_id));
}

#[test]
fn drag_btn_start_moves_frame_and_undo_restores() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("btn-start screen rect");

    let rev_before = harness.editor.session.revision;
    let frame_before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    let c_rect = canvas_rect(central_rect);
    let fit_rect = canvas_fit_rect(c_rect);
    let transform =
        SlideTransform::fit(fit_rect, harness.editor.session.course.slide_size).expect("transform");
    let scale = transform.scale();

    let expected_dx = (100.0 / scale).round();
    let expected_dy = (40.0 / scale).round();

    // Drag by (+100, +40) screen px over several frames
    harness.drag_by(btn_rect.center(), vec2(100.0, 40.0), 4, Modifiers::NONE);

    assert_eq!(
        harness.editor.session.revision,
        rev_before + 1,
        "exactly one object.set_frame command should be executed"
    );

    let frame_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    assert_eq!(frame_after.x, frame_before.x + expected_dx);
    assert_eq!(frame_after.y, frame_before.y + expected_dy);
    assert_eq!(frame_after.w, frame_before.w);
    assert_eq!(frame_after.h, frame_before.h);

    // Undo restores the frame
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);

    let frame_restored = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    assert_eq!(frame_restored, frame_before);
}

#[test]
fn escape_mid_drag_leaves_course_unchanged() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("btn-start screen rect");

    let rev_before = harness.editor.session.revision;
    let frame_before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    let start = btn_rect.center();
    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    harness.frame(vec![Event::PointerMoved(start + vec2(50.0, 20.0))]);
    // Escape mid-drag cancels
    harness.frame(vec![Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    // Pointer release
    harness.frame(vec![Event::PointerButton {
        pos: start + vec2(50.0, 20.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);

    assert_eq!(harness.editor.session.revision, rev_before);
    let frame_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;
    assert_eq!(frame_after, frame_before);
}

#[test]
fn tiny_drag_is_just_selection() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.object = None;
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("btn-start screen rect");

    let rev_before = harness.editor.session.revision;
    let start = btn_rect.center();

    // Drag by 2 px (shorter than 3 px)
    harness.drag_by(start, vec2(2.0, 0.0), 2, Modifiers::NONE);

    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some("btn-start")
    );
    assert_eq!(harness.editor.session.revision, rev_before);
}

#[test]
fn resizing_bottom_right_grows_and_clamps_at_min_size() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("btn-start screen rect");

    let rev_before = harness.editor.session.revision;
    let frame_before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    let br_handle = btn_rect.right_bottom();

    // Drag bottom-right handle outwards by (+40, +30) screen px
    harness.drag_by(br_handle, vec2(40.0, 30.0), 3, Modifiers::NONE);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let frame_grown = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    assert!(frame_grown.w > frame_before.w);
    assert!(frame_grown.h > frame_before.h);
    assert_eq!(frame_grown.x, frame_before.x);
    assert_eq!(frame_grown.y, frame_before.y);

    // Now drag bottom-right handle far up-left to test 4x4 clamping
    let new_btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("new btn-start rect");
    let new_br_handle = new_btn_rect.right_bottom();

    harness.drag_by(new_br_handle, vec2(-600.0, -400.0), 3, Modifiers::NONE);

    assert_eq!(harness.editor.session.revision, rev_before + 2);
    let frame_clamped = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    assert_eq!(frame_clamped.w, 4.0);
    assert_eq!(frame_clamped.h, 4.0);
}

#[test]
fn shift_drag_snaps_to_multiples_of_10() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let btn_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central_rect,
    )
    .expect("btn-start screen rect");

    // Drag holding Shift
    harness.drag_by(btn_rect.center(), vec2(53.0, 37.0), 3, Modifiers::SHIFT);

    let frame = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;

    assert_eq!(frame.x % 10.0, 0.0);
    assert_eq!(frame.y % 10.0, 0.0);
}

#[test]
fn button_insert_tool_places_centered_and_selects() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    harness.editor.ui.placement_tool = Some(PlacementTool::Button);
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let c_rect = canvas_rect(central_rect);
    let fit_rect = canvas_fit_rect(c_rect);
    let transform =
        SlideTransform::fit(fit_rect, harness.editor.session.course.slide_size).expect("transform");
    let click_pos = transform.slide_rect().center();

    let rev_before = harness.editor.session.revision;
    harness.click_at(click_pos);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    assert_eq!(harness.editor.ui.placement_tool, None);

    let sel_id = harness
        .editor
        .selection
        .object
        .clone()
        .expect("new object selected");
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let (obj, _) = slide.object(&sel_id).expect("new object");

    assert!(matches!(obj.kind, ObjectKind::Button { .. }));
    assert_eq!(obj.frame.w, 200.0);
    assert_eq!(obj.frame.h, 56.0);

    let slide_pos = transform.to_slide_pos(click_pos);
    assert_eq!(obj.frame.x, (slide_pos.x - 100.0).round());
    assert!((obj.frame.y - (slide_pos.y - 28.0).round()).abs() <= 1.0);
}

#[test]
fn placement_drag_sizes_new_object_in_every_direction() {
    for (dx, dy) in [
        (100.0, 60.0),
        (-100.0, 60.0),
        (100.0, -60.0),
        (-100.0, -60.0),
    ] {
        let course = Course::from_json(COURSE_JSON).expect("valid course");
        let mut harness = TestHarness::new(course);
        harness.editor.ui.snap_to_guides = false;
        harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
        harness.idle_frame();

        let transform = SlideTransform::fit(
            canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
            harness.editor.session.course.slide_size,
        )
        .expect("transform");
        let start_slide = pos2(600.0, 360.0);
        let start = transform.to_screen_pos(start_slide);
        let scale = transform.scale();
        let before = harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len();
        let revision = harness.editor.session.revision;

        harness.drag_by(start, vec2(dx * scale, dy * scale), 4, Modifiers::NONE);

        let slide = harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide");
        assert_eq!(slide.objects.len(), before + 1);
        assert_eq!(harness.editor.session.revision, revision + 1);
        let placed = slide.objects.last().expect("placed object");
        assert!(matches!(placed.kind, ObjectKind::Shape { .. }));
        assert_eq!(
            harness.editor.selection.object.as_deref(),
            Some(placed.id.as_str())
        );
        assert_eq!(harness.editor.ui.placement_tool, None);

        let expected_x = if dx > 0.0 { 600.0 } else { 500.0 };
        let expected_y = if dy > 0.0 { 360.0 } else { 300.0 };
        assert!(
            (placed.frame.x - expected_x).abs() <= 1.0,
            "x {}",
            placed.frame.x
        );
        assert!(
            (placed.frame.y - expected_y).abs() <= 1.0,
            "y {}",
            placed.frame.y
        );
        assert!(
            (placed.frame.w - 100.0).abs() <= 1.0,
            "w {}",
            placed.frame.w
        );
        assert!((placed.frame.h - 60.0).abs() <= 1.0, "h {}", placed.frame.h);
    }
}

#[test]
fn an_armed_tool_takes_the_press_even_over_an_existing_object() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let target = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start")
    .center();
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.drag_by(target, vec2(90.0, 60.0), 4, Modifiers::NONE);

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(slide.objects.len(), before + 1);
    assert!(matches!(
        slide.objects.last().expect("new object").kind,
        ObjectKind::Shape { .. }
    ));
}

#[test]
fn a_drag_starting_outside_the_slide_leaves_the_tool_armed() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = pos2(
        transform.slide_rect().max.x + 10.0,
        transform.slide_rect().center().y,
    );
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.drag_by(start, vec2(150.0, 80.0), 4, Modifiers::NONE);

    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len(),
        before
    );
    assert_eq!(
        harness.editor.ui.placement_tool,
        Some(PlacementTool::Shape(ShapeKind::Rectangle))
    );
    assert!(harness.editor.ui.placement_drag.is_none());
}

#[test]
fn placement_drag_snaps_to_edges_and_command_disables_it() {
    // The title object starts at x = 80; a drag ending at x = 82 snaps to it.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = true;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let scale = transform.scale();
    let start = transform.to_screen_pos(pos2(200.0, 200.0));
    harness.drag_by(
        start,
        vec2(-118.0 * scale, 100.0 * scale),
        4,
        Modifiers::NONE,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert!((frame.x - 80.0).abs() <= 0.5, "snapped x {}", frame.x);

    // Cmd turns snapping off, exactly as it does for a move.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = true;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let scale = transform.scale();
    let start = transform.to_screen_pos(pos2(200.0, 200.0));
    harness.drag_by(
        start,
        vec2(-118.0 * scale, 100.0 * scale),
        4,
        Modifiers::COMMAND,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert!((frame.x - 82.0).abs() <= 0.5, "unsnapped x {}", frame.x);
}

#[test]
fn placement_drag_under_the_click_slop_uses_the_default_size() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Button);
    harness.idle_frame();

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.slide_rect().center();
    harness.drag_by(start, vec2(3.0, 0.0), 2, Modifiers::NONE);

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let placed = slide.objects.last().expect("placed object");
    assert!(matches!(placed.kind, ObjectKind::Button { .. }));
    assert_eq!(placed.frame.w, 200.0);
    assert_eq!(placed.frame.h, 56.0);
}

#[test]
fn vertical_arrow_placement_defaults_to_a_tall_frame() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::ArrowUp));
    harness.idle_frame();

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.slide_rect().center();
    harness.drag_by(start, vec2(3.0, 0.0), 2, Modifiers::NONE);

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let placed = slide.objects.last().expect("placed object");
    assert_eq!(placed.frame.w, 120.0);
    assert_eq!(placed.frame.h, 200.0);
}

#[test]
fn placement_drag_clamps_to_the_slide_and_respects_the_minimum() {
    // A drag that runs off the bottom-right corner stops at the slide edge.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(1200.0, 650.0));
    let scale = transform.scale();
    harness.drag_by(
        start,
        vec2(200.0 * scale, 200.0 * scale),
        4,
        Modifiers::NONE,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert!((frame.x - 1200.0).abs() <= 1.0, "x {}", frame.x);
    assert!((frame.y - 650.0).abs() <= 1.0, "y {}", frame.y);
    assert!((frame.w - 80.0).abs() <= 1.0, "w {}", frame.w);
    assert!((frame.h - 70.0).abs() <= 1.0, "h {}", frame.h);
    assert!(frame.x + frame.w <= 1280.0 + 0.01);
    assert!(frame.y + frame.h <= 720.0 + 0.01);

    // A tiny drag at the top-left edge still makes an 8-unit frame inside it.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(4.0, 4.0));
    let scale = transform.scale();
    harness.drag_by(
        start,
        vec2(-20.0 * scale, -20.0 * scale),
        3,
        Modifiers::NONE,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert_eq!(frame.x, 0.0);
    assert_eq!(frame.y, 0.0);
    assert_eq!(frame.w, 8.0);
    assert_eq!(frame.h, 8.0);
}

#[test]
fn placement_drag_shift_and_images_keep_their_aspect() {
    // Shift makes a shape square, using the larger drag dimension.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(500.0, 300.0));
    let scale = transform.scale();
    harness.drag_by(
        start,
        vec2(100.0 * scale, 20.0 * scale),
        4,
        Modifiers::SHIFT,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert!((frame.w - 100.0).abs() <= 1.0, "w {}", frame.w);
    assert!((frame.h - 100.0).abs() <= 1.0, "h {}", frame.h);

    // An image keeps its natural ratio even without Shift.
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Image {
        src: "natural.png".into(),
        natural_size: (400.0, 200.0),
    });
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(200.0, 150.0));
    let scale = transform.scale();
    harness.drag_by(start, vec2(300.0 * scale, 50.0 * scale), 4, Modifiers::NONE);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let frame = slide.objects.last().expect("placed object").frame;
    assert!(
        (frame.w / frame.h - 2.0).abs() <= 0.02,
        "{}:{}",
        frame.w,
        frame.h
    );
    assert!(frame.w >= 300.0, "w {}", frame.w);
}

#[test]
fn placement_drag_escape_cancels_and_disarms() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(500.0, 300.0));
    let end = start + vec2(120.0, 80.0);
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerMoved(end)]);
    assert!(harness.editor.ui.placement_drag.is_some());

    harness.press(Key::Escape, Modifiers::NONE);
    assert!(harness.editor.ui.placement_drag.is_none());
    assert_eq!(harness.editor.ui.placement_tool, None);

    harness.frame(vec![Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len(),
        before
    );
}

#[test]
fn placement_drag_is_one_undo_step() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(400.0, 300.0));
    let scale = transform.scale();
    let revision = harness.editor.session.revision;
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.drag_by(start, vec2(150.0 * scale, 90.0 * scale), 4, Modifiers::NONE);
    assert_eq!(harness.editor.session.revision, revision + 1);
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len(),
        before + 1
    );

    harness.press(Key::Z, Modifiers::COMMAND);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len(),
        before
    );
}

#[test]
fn dragging_a_text_tool_still_enters_inline_edit() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::TextBox);
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(300.0, 250.0));
    let scale = transform.scale();
    harness.drag_by(start, vec2(260.0 * scale, 80.0 * scale), 4, Modifiers::NONE);
    harness.idle_frame();

    let edit = harness
        .editor
        .ui
        .inline_edit
        .as_ref()
        .expect("inline edit after dragging a text box");
    assert_eq!(edit.buffer, "Text");
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let placed = slide
        .objects
        .iter()
        .find(|object| object.id == edit.object)
        .expect("placed text");
    assert!(matches!(placed.kind, ObjectKind::Text { .. }));
    assert!(
        (placed.frame.w - 260.0).abs() <= 1.0,
        "w {}",
        placed.frame.w
    );
    assert!((placed.frame.h - 80.0).abs() <= 1.0, "h {}", placed.frame.h);
}

#[test]
fn armed_placement_shows_the_prompt_and_a_live_size_label() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Shape(ShapeKind::Rectangle));
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("Click for the default size, or drag to draw Rectangle")
            .is_some()
    );

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(400.0, 300.0));
    let end = start + vec2(100.0, 60.0);
    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerMoved(end)]);
    assert!(harness.find_text_containing(" × ").is_some());
    harness.frame(vec![Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    harness.idle_frame();
}

#[test]
fn properties_editing_label_updates_object_and_canvas() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;

    // Focus button label input
    let label_rect = harness
        .editor
        .ui
        .properties
        .btn_label_rect
        .expect("btn label rect");
    harness.click_at(label_rect.center());

    // Send select all, replacement text and Enter
    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("Get Started".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);

    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-start")
        .expect("btn-start");
    match &obj.kind {
        ObjectKind::Button { label, .. } => assert_eq!(label, "Get Started"),
        _ => panic!("expected button"),
    }

    // Verify canvas displays new label in authoring composition
    let items = compose_authoring(
        &harness.editor.session.course,
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide"),
        None,
    );
    let item = items
        .iter()
        .find(|it| it.hit == coursecraft_render::Hit::Object("btn-start".into()))
        .expect("btn-start item");
    assert!(matches!(
        &item.visual,
        coursecraft_render::Visual::Button { label, .. } if label == "Get Started"
    ));
}

#[test]
fn properties_invalid_color_sends_nothing() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;
    let color_rect = harness.editor.ui.properties.color_rect.expect("color rect");
    harness.click_at(color_rect.center());

    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("not-a-color".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);

    assert_eq!(
        harness.editor.session.revision, rev_before,
        "invalid color must not send a command"
    );
}

#[test]
fn changing_frame_x_through_drag_value_commits_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;
    let fx_rect = harness
        .editor
        .ui
        .properties
        .frame_x_rect
        .expect("frame x rect");

    // Drag DragValue
    harness.drag_by(fx_rect.center(), vec2(40.0, 0.0), 3, Modifiers::NONE);

    assert_eq!(
        harness.editor.session.revision,
        rev_before + 1,
        "DragValue interaction must commit exactly once"
    );

    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-start")
        .expect("btn-start");
    assert_ne!(obj.frame.x, 480.0);
}

#[test]
fn with_no_selection_editing_slide_title_commits() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = None;
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;
    let title_rect = harness
        .editor
        .ui
        .properties
        .slide_title_rect
        .expect("slide title rect");
    harness.click_at(title_rect.center());

    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("Introduction Overview".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(slide.title, "Introduction Overview");
}

#[test]
fn after_undo_properties_panel_shows_restored_value() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = None;
    harness.idle_frame();

    let orig_title = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .title
        .clone();
    let title_rect = harness
        .editor
        .ui
        .properties
        .slide_title_rect
        .expect("slide title rect");
    harness.click_at(title_rect.center());

    harness.frame(vec![
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("Modified Title".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);

    assert_eq!(harness.editor.ui.properties.slide_title, "Modified Title");

    // Undo
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);

    harness.idle_frame();

    assert_eq!(harness.editor.ui.properties.slide_title, orig_title);
}

#[test]
fn clicking_layer_row_selects_it_and_dims_base() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    assert_eq!(harness.editor.selection.layer, None);
    {
        let slide = harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro");
        let base_items = compose_authoring(&harness.editor.session.course, slide, None);
        assert!(base_items.iter().any(|it| !it.dimmed));
    }

    // Click "about" layer row in the Layers panel
    let row_rect = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get("about")
        .expect("about layer row rect");
    harness.click_at(row_rect.center());

    assert_eq!(harness.editor.selection.layer.as_deref(), Some("about"));
    assert!(
        !harness.editor.ui.layer_settings.is_open(),
        "selecting a layer only selects it; settings stay closed"
    );

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro");
    let layer_items = compose_authoring(&harness.editor.session.course, slide, Some("about"));
    let base_obj_item = layer_items
        .iter()
        .find(|it| it.hit == coursecraft_render::Hit::Object("btn-start".into()))
        .expect("btn-start item");
    assert!(
        base_obj_item.dimmed,
        "base object must be dimmed when a layer is selected"
    );

    // Click "Base layer" row to reset selection
    let base_row_rect = harness
        .editor
        .ui
        .layers
        .base_row_rect
        .expect("base row rect");
    harness.click_at(base_row_rect.center());
    assert_eq!(harness.editor.selection.layer, None);
}

#[test]
fn add_layer_selects_new_layer() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;
    let add_btn_rect = harness
        .editor
        .ui
        .layers
        .add_button_rect
        .expect("add layer button rect");
    harness.click_at(add_btn_rect.center());

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let selected_layer = harness
        .editor
        .selection
        .layer
        .as_ref()
        .expect("layer selected");
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.layers.iter().any(|l| &l.id == selected_layer));
}

#[test]
fn slide_layers_panel_lists_front_first_and_pins_base_last() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    assert!(harness.editor.ui.layers.header_rect.is_some());
    assert!(harness.find_text("Slide Layers").is_some());
    let rows = &harness.editor.ui.layers.layer_row_rects;
    let base = harness.editor.ui.layers.base_row_rect.expect("base row");
    let l1 = rows.get("l1").expect("l1 row").center().y;
    let l2 = rows.get("l2").expect("l2 row").center().y;
    let l3 = rows.get("l3").expect("l3 row").center().y;
    assert!(l3 < l2 && l2 < l1, "top row is the frontmost layer");
    assert!(l1 < base.center().y, "base is pinned below every layer");
}

#[test]
fn gear_opens_layer_settings_and_selection_alone_does_not() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let row = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get("about")
        .expect("about row");
    harness.click_at(row.center());
    assert_eq!(harness.editor.selection.layer.as_deref(), Some("about"));
    assert!(!harness.editor.ui.layer_settings.is_open());

    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("about")
        .expect("about gear");
    harness.click_at(gear.center());
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .ui
            .layer_settings
            .open
            .as_ref()
            .map(|target| target.layer.as_str()),
        Some("about")
    );
    assert!(
        harness.find_text("Layer settings: About").is_some(),
        "the window is titled after the layer"
    );
}

#[test]
fn double_click_renames_in_place_and_escape_cancels() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let row = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get("about")
        .expect("about row");
    let rev_before = harness.editor.session.revision;

    harness.click_at(row.center());
    harness.click_at(row.center());
    harness.idle_frame();
    assert!(harness.editor.ui.layers.rename_edit_rect.is_some());
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.layers.rename.is_none());
    assert_eq!(harness.editor.session.revision, rev_before);
    let name = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about")
        .name
        .clone();
    assert_eq!(name, "About");
}

#[test]
fn double_click_rename_commits_on_enter() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let row = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get("about")
        .expect("about row");
    let rev_before = harness.editor.session.revision;
    harness.click_at(row.center());
    harness.click_at(row.center());
    harness.idle_frame();
    let edit = harness
        .editor
        .ui
        .layers
        .rename_edit_rect
        .expect("rename field");
    harness.set_field(edit.center(), "Overview");
    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let name = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about")
        .name
        .clone();
    assert_eq!(name, "Overview");
}

#[test]
fn dragging_layer_rows_reorders_in_both_directions() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    let row = |harness: &TestHarness, id: &str| {
        *harness
            .editor
            .ui
            .layers
            .layer_row_rects
            .get(id)
            .expect("layer row")
    };
    let rev_before = harness.editor.session.revision;

    // Drag the frontmost layer down to the back.
    let top = row(&harness, "l3");
    harness.drag_by(top.center(), vec2(0.0, 120.0), 4, Modifiers::NONE);
    harness.idle_frame();
    let order: Vec<String> = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs")
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect();
    assert_eq!(order, ["l3", "l1", "l2"]);
    assert_eq!(harness.editor.session.revision, rev_before + 1);

    // Drag the back layer up to the front again.
    let back = row(&harness, "l3");
    harness.drag_by(back.center(), vec2(0.0, -120.0), 4, Modifiers::NONE);
    harness.idle_frame();
    let order: Vec<String> = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs")
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect();
    assert_eq!(order, ["l1", "l2", "l3"]);
    assert_eq!(harness.editor.session.revision, rev_before + 2);
}

#[test]
fn duplicate_layer_button_copies_in_front_and_selects_the_copy() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.idle_frame();

    let duplicate = harness
        .editor
        .ui
        .layers
        .duplicate_button_rect
        .expect("duplicate button");
    harness.click_at(duplicate.center());
    harness.idle_frame();

    let selected = harness
        .editor
        .selection
        .layer
        .clone()
        .expect("copy selected");
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro");
    assert_eq!(slide.layers.len(), 2);
    assert_eq!(slide.layers[1].id, selected);
    assert_eq!(slide.layers[1].name, "About copy");
    assert_eq!(slide.layers[0].id, "about");
}

#[test]
fn right_clicking_a_layer_row_selects_it_first() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    let row = *harness
        .editor
        .ui
        .layers
        .layer_row_rects
        .get("l2")
        .expect("l2 row");
    harness.right_click_at(row.center());
    harness.idle_frame();
    assert_eq!(harness.editor.selection.layer.as_deref(), Some("l2"));
}

#[test]
fn eye_and_dim_are_editor_only_and_leave_revision_alone() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = Some("l2".into());
    harness.idle_frame();

    let before = harness.editor.session.course.clone();
    let rev_before = harness.editor.session.revision;
    let eye = *harness
        .editor
        .ui
        .layers
        .eye_rects
        .get("l1")
        .expect("l1 eye");
    harness.click_at(eye.center());
    harness.idle_frame();

    assert!(harness.editor.ui.layers.is_shown("l1"));
    assert_eq!(harness.editor.session.revision, rev_before);
    assert_eq!(harness.editor.session.course, before);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs");
    let extra = harness.editor.ui.layers.extra_layer_ids(slide, Some("l2"));
    assert_eq!(extra, vec!["l1".to_owned()]);
    assert!(!extra.iter().any(|id| id == "l3"));

    let dim = harness.editor.ui.layers.dim_rect.expect("dim toggle");
    harness.click_at(dim.center());
    harness.idle_frame();
    assert!(!harness.editor.ui.layers.dim_other_layers);
    assert_eq!(harness.editor.session.revision, rev_before);
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn layer_settings_closes_when_its_layer_or_slide_goes_away() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.idle_frame();
    harness
        .editor
        .run(
            "layer.add",
            serde_json::json!({ "slide": "s-tabs", "id": "temp", "name": "Temp" }),
        )
        .expect("add temp");
    harness.idle_frame();

    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("temp")
        .expect("temp gear");
    harness.click_at(gear.center());
    harness.idle_frame();
    assert!(harness.editor.ui.layer_settings.is_open());
    harness
        .editor
        .run(
            "layer.delete",
            serde_json::json!({ "slide": "s-tabs", "layer": "temp" }),
        )
        .expect("delete temp");
    harness.idle_frame();
    assert!(!harness.editor.ui.layer_settings.is_open());

    // Opening on another layer and changing slide also closes it.
    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("l3")
        .expect("l3 gear");
    harness.click_at(gear.center());
    harness.idle_frame();
    assert!(harness.editor.ui.layer_settings.is_open());
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    assert!(!harness.editor.ui.layer_settings.is_open());
}

#[test]
fn slide_layers_panel_narrows_below_900_points() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut wide = TestHarness::new(course.clone());
    wide.idle_frame();
    let wide_width = wide
        .editor
        .ui
        .layers
        .panel_rect
        .expect("wide panel")
        .width();
    assert_eq!(wide_width, LAYERS_PANEL_WIDTH);

    let mut narrow = TestHarness::new_with_size(course, (800.0, 600.0));
    narrow.idle_frame();
    let narrow_width = narrow
        .editor
        .ui
        .layers
        .panel_rect
        .expect("narrow panel")
        .width();
    assert_eq!(narrow_width, LAYERS_PANEL_NARROW_WIDTH);
}

#[test]
fn slide_layers_panel_hides_in_master_view() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.enter_master_view();
    harness.idle_frame();
    assert!(harness.editor.is_master_view());
    assert!(harness.find_text("Slide Layers").is_none());
}

#[test]
fn deleting_about_layer_takes_the_triggers_that_show_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.idle_frame();
    let triggers_before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .triggers
        .len();

    let del_btn_rect = harness
        .editor
        .ui
        .layers
        .delete_button_rect
        .expect("delete layer button rect");
    harness.click_at(del_btn_rect.center());
    harness.idle_frame();

    assert!(
        harness.editor.ui.last_error.is_none(),
        "{:?}",
        harness.editor.ui.last_error
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.layer("about").is_none(), "layer deleted");
    assert!(
        slide.triggers.len() < triggers_before,
        "the Show layer trigger went with it"
    );
    assert!(harness.editor.session.course.is_playable());
    let status = harness
        .editor
        .ui
        .status_message
        .as_ref()
        .map(|m| m.0.clone())
        .unwrap_or_default();
    assert!(status.contains("trigger"), "{status}");
    harness.editor.run("edit.undo", serde_json::json!({}));
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.layer("about").is_some());
    assert_eq!(slide.triggers.len(), triggers_before);
}

#[test]
fn layer_settings_ok_commits_one_update_and_cancel_changes_nothing() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.idle_frame();

    let orig = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer")
        .clone();

    // Cancel discards every buffered change.
    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("about")
        .expect("about gear");
    harness.click_at(gear.center());
    harness.idle_frame();
    assert!(harness.editor.ui.layer_settings.is_open());
    let prevent = harness
        .editor
        .ui
        .layer_settings
        .prevent_clicks_rect
        .expect("prevent rect");
    let rev_before = harness.editor.session.revision;
    let undo_before = harness.editor.session.undo_len();
    harness.click_at(prevent.center());
    let cancel = harness
        .editor
        .ui
        .layer_settings
        .cancel_rect
        .expect("cancel rect");
    harness.click_at(cancel.center());
    harness.idle_frame();
    assert_eq!(harness.editor.session.revision, rev_before);
    assert_eq!(harness.editor.session.undo_len(), undo_before);
    assert!(!harness.editor.ui.layer_settings.is_open());
    let unchanged = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer");
    assert_eq!(unchanged.prevent_base_clicks, orig.prevent_base_clicks);

    // OK commits exactly one layer.update and one undo step.
    harness.click_at(gear.center());
    harness.idle_frame();
    let prevent = harness
        .editor
        .ui
        .layer_settings
        .prevent_clicks_rect
        .expect("prevent rect");
    harness.click_at(prevent.center());
    let ok = harness.editor.ui.layer_settings.ok_rect.expect("ok rect");
    harness.click_at(ok.center());
    harness.idle_frame();
    assert_eq!(harness.editor.session.revision, rev_before + 1);
    assert_eq!(harness.editor.session.undo_len(), undo_before + 1);
    let layer_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer");
    assert_eq!(layer_after.prevent_base_clicks, !orig.prevent_base_clicks);
    assert_eq!(layer_after.duration_ms, orig.duration_ms);
    assert_eq!(layer_after.name, orig.name);
}

#[test]
fn adding_visited_state_and_editing_fill_commits() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-about".into());
    harness.editor.ui.bottom_tab = BottomTab::States;
    harness.editor.ui.states.show_new_state_input = true;
    harness.idle_frame();

    // 1. Add "visited" state
    let rev_before = harness.editor.session.revision;
    let name_edit_rect = harness
        .editor
        .ui
        .states
        .new_state_name_rect
        .expect("state name edit rect");
    harness.click_at(name_edit_rect.center());
    harness.frame(vec![
        Event::Text("visited".into()),
        Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ]);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-about")
        .expect("btn-about");
    let visited_state = obj
        .states
        .iter()
        .find(|s| s.name == "visited")
        .expect("visited state added");
    assert_eq!(
        visited_state.fill,
        Some(coursecraft_course::Color("#5A5A5A".into()))
    );

    // 2. Edit visited state's fill
    harness.scroll_over(pos2(1100.0, 300.0), -200.0);
    harness.idle_frame();
    let rev_before_edit = harness.editor.session.revision;
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

    assert_eq!(harness.editor.session.revision, rev_before_edit + 1);
    let (obj_after, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-about")
        .expect("btn-about");
    let updated_state = obj_after
        .states
        .iter()
        .find(|s| s.name == "visited")
        .expect("visited state");
    assert_eq!(
        updated_state.fill,
        Some(coursecraft_course::Color("#2E8B57".into()))
    );
}

#[test]
fn deleting_initial_state_is_refused_and_shown() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.object = Some("btn-next".into());
    harness.editor.ui.bottom_tab = BottomTab::States;
    harness.idle_frame();

    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("slide")
        .object("btn-next")
        .expect("btn-next");
    assert_eq!(obj.initial_state, "disabled");

    let rev_before = harness.editor.session.revision;
    let card_rect = harness
        .editor
        .ui
        .states
        .state_row_rects
        .get("disabled")
        .copied()
        .expect("disabled card rect");
    let menu_pos = harness
        .find_text_in("⋯", card_rect)
        .expect("disabled card ⋯ menu");
    harness.click_at(menu_pos);
    harness.idle_frame();
    let del_pos = harness.find_text("Delete").expect("Delete menu item");
    harness.click_at(del_pos);
    harness.idle_frame();
    assert_eq!(harness.editor.session.revision, rev_before);

    let (obj_after, _) = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("slide")
        .object("btn-next")
        .expect("btn-next");
    assert!(obj_after.states.iter().any(|s| s.name == "disabled"));
}

#[test]
fn dragging_btn_start_timeline_bar_right_commits_timing_update() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let bar_rect = *harness
        .editor
        .ui
        .timeline
        .object_bar_rects
        .get("btn-start")
        .expect("btn-start timeline bar rect");

    let rev_before = harness.editor.session.revision;

    let dur = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .duration_ms as f32;
    let track_width = bar_rect.width() * dur / (dur - 1000.0);
    let delta_x = 500.0 / dur * track_width;

    harness.drag_by(bar_rect.center(), vec2(delta_x, 0.0), 3, Modifiers::NONE);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-start")
        .expect("btn-start");
    assert_eq!(obj.timing.start_ms, 1500);
}

#[test]
fn dragging_timeline_right_edge_past_duration_clamps() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let right_handle_rect = *harness
        .editor
        .ui
        .timeline
        .object_right_handle_rects
        .get("btn-start")
        .expect("btn-start right handle rect");

    let rev_before = harness.editor.session.revision;

    harness.drag_by(
        right_handle_rect.center(),
        vec2(500.0, 0.0),
        3,
        Modifiers::NONE,
    );

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("btn-start")
        .expect("btn-start");
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let end = obj.timing.end_ms.expect("has end_ms");
    assert_eq!(end, slide.duration_ms);
}

#[test]
fn editing_slide_duration_in_timeline_header_commits_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    let rev_before = harness.editor.session.revision;
    let dur_rect = harness
        .editor
        .ui
        .timeline
        .duration_edit_rect
        .expect("duration edit rect");

    harness.drag_by(dur_rect.center(), vec2(50.0, 0.0), 3, Modifiers::NONE);

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_ne!(slide.duration_ms, 5000);
}

#[test]
fn canvas_preview_state_toggle() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.object = Some("tab1".into());
    harness.editor.ui.bottom_tab = BottomTab::States;
    harness.idle_frame();

    assert_eq!(harness.editor.ui.preview_state, None);

    let card_rect = harness
        .editor
        .ui
        .states
        .state_row_rects
        .get("visited")
        .copied()
        .expect("visited card rect");
    harness.click_at(pos2(card_rect.center().x, card_rect.min.y + 30.0));

    assert_eq!(harness.editor.ui.preview_state.as_deref(), Some("visited"));
}

#[test]
fn describe_covers_all_action_and_event_variants() {
    use coursecraft_course::{
        Action, CompletionStatus, Condition, Event as CourseEvent, Object, Rect, Slide,
        SlideTarget, Trigger, Value, VarOp,
    };

    let slide = Slide {
        id: "s1".to_owned(),
        title: "Slide 1".to_owned(),
        duration_ms: 5000,
        background: None,
        transition: None,
        master: None,
        objects: vec![
            Object {
                id: "btn-start".to_owned(),
                name: String::new(),
                kind: ObjectKind::Button {
                    label: "Start".to_owned(),
                    fill: coursecraft_course::Color("#2F6FEB".to_owned()),
                    fill_style: None,
                },
                frame: Rect::default(),
                timing: Default::default(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                emphasis: Vec::new(),
                states: Vec::new(),
                initial_state: "normal".to_owned(),
                focus_order: None,
                decorative: false,
                description: None,
                group: None,
                panel: None,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
                drag: None,
                locked: false,
                editor_hidden: false,
            },
            Object {
                id: "obj2".to_owned(),
                name: "Custom Name".to_owned(),
                kind: ObjectKind::Text {
                    text: "Hello".to_owned(),
                    style: Default::default(),
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: coursecraft_course::Autofit::None,
                    text_style: None,
                    outline: None,
                    point_text: false,
                },
                frame: Rect::default(),
                timing: Default::default(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                emphasis: Vec::new(),
                states: Vec::new(),
                initial_state: "normal".to_owned(),
                focus_order: None,
                decorative: false,
                description: None,
                group: None,
                panel: None,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                opacity: 1.0,
                shadow: None,
                glow: None,
                drag: None,
                locked: false,
                editor_hidden: false,
            },
        ],
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        question: None,
        draw: None,
        notes: String::new(),
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
        cue_points: Vec::new(),
    };

    // Every Action variant
    let actions = [
        (
            Action::JumpToSlide {
                target: SlideTarget::Next,
            },
            "Jump to next slide",
        ),
        (
            Action::JumpToSlide {
                target: SlideTarget::Previous,
            },
            "Jump to previous slide",
        ),
        (
            Action::JumpToSlide {
                target: SlideTarget::Id("s2".into()),
            },
            "Jump to slide s2",
        ),
        (
            Action::ShowLayer {
                layer: "layer1".into(),
            },
            "Show layer layer1",
        ),
        (
            Action::HideLayer {
                layer: Some("layer1".into()),
            },
            "Hide layer layer1",
        ),
        (Action::HideLayer { layer: None }, "Hide this layer"),
        (
            Action::ChangeState {
                object: "btn-start".into(),
                state: "visited".into(),
            },
            "Set btn-start state to visited",
        ),
        (
            Action::ChangeState {
                object: "obj2".into(),
                state: "hidden".into(),
            },
            "Set Custom Name state to hidden",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Assign,
                value: Some(Value::Number(10.0).into()),
            },
            "Set v1 = 10",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Add,
                value: Some(Value::Number(1.0).into()),
            },
            "Set v1 + 1",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Subtract,
                value: Some(Value::Number(2.0).into()),
            },
            "Set v1 - 2",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Multiply,
                value: Some(Value::Number(3.0).into()),
            },
            "Set v1 * 3",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Divide,
                value: Some(Value::Number(4.0).into()),
            },
            "Set v1 / 4",
        ),
        (
            Action::SetVariable {
                variable: "v1".into(),
                op: VarOp::Toggle,
                value: None,
            },
            "Toggle v1",
        ),
        (
            Action::PlayMedia {
                object: "btn-start".into(),
            },
            "Play media btn-start",
        ),
        (
            Action::PauseMedia {
                object: "btn-start".into(),
            },
            "Pause media btn-start",
        ),
        (
            Action::StopMedia {
                object: "btn-start".into(),
            },
            "Stop media btn-start",
        ),
        (Action::PauseTimeline, "Pause timeline"),
        (Action::ResumeTimeline, "Resume timeline"),
        (Action::SubmitQuestion, "Submit question"),
        (Action::SubmitResults, "Submit results"),
        (
            Action::CompleteCourse {
                status: CompletionStatus::Passed,
            },
            "Complete course with status passed",
        ),
        (
            Action::CompleteCourse {
                status: CompletionStatus::Failed,
            },
            "Complete course with status failed",
        ),
        (
            Action::CompleteCourse {
                status: CompletionStatus::Completed,
            },
            "Complete course with status completed",
        ),
        (
            Action::CompleteCourse {
                status: CompletionStatus::Incomplete,
            },
            "Complete course with status incomplete",
        ),
        (Action::ExitCourse, "Exit course"),
        (
            Action::OpenUrl {
                url: "https://example.com".into(),
            },
            "Open URL https://example.com",
        ),
        (
            Action::MoveOnPath {
                object: "btn-start".into(),
                path: "path1".into(),
            },
            "Move btn-start along path1",
        ),
    ];

    for (act, expected_prefix) in actions {
        let trig = Trigger {
            action: act,
            when: CourseEvent::Click {
                object: "btn-start".into(),
            },
            condition: None,
            disabled: false,
        };
        let desc = describe(&trig, &slide);
        assert!(
            desc.starts_with(expected_prefix),
            "expected desc to start with '{expected_prefix}', got '{desc}'"
        );
    }

    // Every Event variant
    let events = [
        (
            CourseEvent::TimelineStarts { layer: None },
            "the timeline starts",
        ),
        (
            CourseEvent::TimelineStarts {
                layer: Some("layer1".into()),
            },
            "timeline starts on layer layer1",
        ),
        (
            CourseEvent::TimelineEnds { layer: None },
            "the timeline ends",
        ),
        (
            CourseEvent::TimelineEnds {
                layer: Some("layer1".into()),
            },
            "timeline ends on layer layer1",
        ),
        (
            CourseEvent::TimelineReaches {
                ms: 1200,
                layer: None,
                cue: None,
            },
            "timeline reaches 1200ms",
        ),
        (
            CourseEvent::TimelineReaches {
                ms: 1200,
                layer: Some("layer1".into()),
                cue: None,
            },
            "timeline reaches 1200ms on layer layer1",
        ),
        (
            CourseEvent::Click {
                object: "btn-start".into(),
            },
            "the user clicks btn-start",
        ),
        (
            CourseEvent::Click {
                object: "obj2".into(),
            },
            "the user clicks Custom Name",
        ),
        (
            CourseEvent::HoverStart {
                object: "btn-start".into(),
            },
            "the user hovers over btn-start",
        ),
        (
            CourseEvent::HoverEnd {
                object: "btn-start".into(),
            },
            "the user stops hovering over btn-start",
        ),
        (
            CourseEvent::VariableChanges {
                variable: "v1".into(),
            },
            "v1 changes",
        ),
        (
            CourseEvent::StateChanges {
                object: "btn-start".into(),
                to: None,
            },
            "btn-start state changes",
        ),
        (
            CourseEvent::StateChanges {
                object: "btn-start".into(),
                to: Some("visited".into()),
            },
            "btn-start state changes to visited",
        ),
        (
            CourseEvent::MediaEnds {
                object: "btn-start".into(),
            },
            "media ends on btn-start",
        ),
        (
            CourseEvent::KeyPressed {
                key: "x".into(),
                object: None,
            },
            "the user presses key 'x'",
        ),
        (CourseEvent::Submit, "the user submits"),
        (
            CourseEvent::AnimationCompletes {
                object: "btn-start".into(),
                path: None,
            },
            "btn-start's entrance finishes",
        ),
        (
            CourseEvent::AnimationCompletes {
                object: "btn-start".into(),
                path: Some("path1".into()),
            },
            "btn-start finishes moving along path1",
        ),
    ];

    for (ev, expected_when) in events {
        let trig = Trigger {
            action: Action::PauseTimeline,
            when: ev,
            condition: None,
            disabled: false,
        };
        let desc = describe(&trig, &slide);
        let expected = format!("Pause timeline when {expected_when}");
        assert_eq!(desc, expected);
    }

    // Condition variants
    let trig_cond = Trigger {
        action: Action::JumpToSlide {
            target: SlideTarget::Next,
        },
        when: CourseEvent::Click {
            object: "btn-start".into(),
        },
        condition: Some(Condition::ObjectState {
            object: "btn-start".into(),
            state: "visited".into(),
            is: false,
        }),
        disabled: false,
    };
    assert_eq!(
        describe(&trig_cond, &slide),
        "Jump to next slide when the user clicks btn-start if btn-start is not visited"
    );
}

#[test]
fn create_hello_course_about_trigger_through_form_state_and_ok() {
    use coursecraft_course::{Action, Event as CourseEvent, Trigger};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    // Remove the second trigger ("show layer about when clicking btn-about") to start
    let sid = "s-intro";
    harness
        .editor
        .run(
            "trigger.delete",
            serde_json::json!({ "slide": sid, "index": 1 }),
        )
        .expect("delete trigger 1");
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(sid)
            .expect("slide")
            .triggers
            .len(),
        1
    );

    let slide = harness
        .editor
        .session
        .course
        .slide(sid)
        .expect("slide")
        .clone();
    let mut form = TriggerFormState::new_for_owner(&slide, None, &harness.editor.session.course);
    form.action_tag = ActionTag::ShowLayer;
    form.show_layer_id = "about".to_owned();
    form.event_tag = EventKindTag::Click;
    form.event_object = "btn-about".to_owned();
    form.condition_enabled = false;

    let rev_before = harness.editor.session.revision;
    let ok = form.commit_ok(&mut harness.editor);
    assert!(ok);
    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let new_slide = harness.editor.session.course.slide(sid).expect("slide");
    assert_eq!(new_slide.triggers.len(), 2);
    let added = &new_slide.triggers[1];
    let expected = Trigger {
        action: Action::ShowLayer {
            layer: "about".to_owned(),
        },
        when: CourseEvent::Click {
            object: "btn-about".to_owned(),
        },
        condition: None,
        disabled: false,
    };
    assert_eq!(added, &expected);
}

#[test]
fn editing_trigger_target_and_ok_commits_trigger_update() {
    use coursecraft_course::{Action, SlideTarget};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    let sid = "s-intro";
    let slide = harness
        .editor
        .session
        .course
        .slide(sid)
        .expect("slide")
        .clone();
    let orig_trigger = &slide.triggers[0];
    assert_eq!(
        orig_trigger.action,
        Action::JumpToSlide {
            target: SlideTarget::Next
        }
    );

    let mut form = TriggerFormState::from_existing(
        0,
        orig_trigger,
        &slide,
        None,
        &harness.editor.session.course,
    );
    form.jump_target_tag = JumpTargetTag::Previous;

    let rev_before = harness.editor.session.revision;
    let ok = form.commit_ok(&mut harness.editor);
    assert!(ok);
    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let updated_slide = harness.editor.session.course.slide(sid).expect("slide");
    assert_eq!(
        updated_slide.triggers[0].action,
        Action::JumpToSlide {
            target: SlideTarget::Previous
        }
    );
}

#[test]
fn move_down_commits_trigger_move() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();

    let sid = "s-intro";
    let rev_before = harness.editor.session.revision;

    let slide = harness.editor.session.course.slide(sid).expect("slide");
    let t0 = slide.triggers[0].clone();
    let t1 = slide.triggers[1].clone();

    // Run trigger.move from 0 to 1
    let res = harness.editor.run(
        "trigger.move",
        serde_json::json!({
            "slide": sid,
            "from": 0,
            "to": 1,
        }),
    );
    assert!(res.is_some());
    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let updated = harness.editor.session.course.slide(sid).expect("slide");
    assert_eq!(updated.triggers[0], t1);
    assert_eq!(updated.triggers[1], t0);
}

#[test]
fn building_tab1_condition_gives_expected_condition() {
    use coursecraft_course::Condition;

    let node = ConditionNode::ObjectState {
        object: "tab1".to_owned(),
        state: "visited".to_owned(),
        is: false,
    };
    let cond = node.to_condition().expect("valid condition");
    let expected = Condition::ObjectState {
        object: "tab1".to_owned(),
        state: "visited".to_owned(),
        is: false,
    };
    assert_eq!(cond, expected);
}

#[test]
fn adding_variable_and_deleting_referenced_variable_refused() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    // 1. Add variable tabs_seen = 0 (fails if already exists, so add tabs_seen_2)
    let rev_before = harness.editor.session.revision;
    let res = harness.editor.run(
        "variable.add",
        serde_json::json!({
            "name": "tabs_seen_copy",
            "value": 0.0,
        }),
    );
    assert!(res.is_some());
    assert_eq!(harness.editor.session.revision, rev_before + 1);
    assert!(
        harness
            .editor
            .session
            .course
            .variable("tabs_seen_copy")
            .is_some()
    );

    // 2. Attempt deleting tabs_seen while referenced by s-tabs triggers
    let rev_before = harness.editor.session.revision;
    let del_res = harness.editor.run(
        "variable.delete",
        serde_json::json!({
            "name": "tabs_seen",
        }),
    );
    assert!(del_res.is_none());
    assert_eq!(harness.editor.session.revision, rev_before);
    assert!(harness.editor.ui.last_error.is_some());
    let (err_msg, _) = harness.editor.ui.last_error.as_ref().unwrap();
    assert!(
        err_msg.contains("tabs_seen"),
        "error message should mention variable name: {err_msg}"
    );
}

#[test]
fn adding_multiple_choice_question_with_choices_commits_question_set() {
    use coursecraft_course::{Choice, Layer, QuestionKind};

    let hello_course = Course::from_json(COURSE_JSON).expect("valid course");
    let expected_question = hello_course
        .slide("s-quiz")
        .expect("s-quiz")
        .question
        .clone()
        .expect("question");

    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    // Add layers "correct" and "incorrect" to s-intro so question layer references are valid
    let intro = course.slide_mut("s-intro").unwrap();
    intro.layers.push(Layer {
        id: "correct".to_owned(),
        name: "Correct".to_owned(),
        duration_ms: 1000,
        objects: Vec::new(),
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: coursecraft_course::SeekMode::Auto,
        revisit: coursecraft_course::RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    });
    intro.layers.push(Layer {
        id: "incorrect".to_owned(),
        name: "Incorrect".to_owned(),
        duration_ms: 1000,
        objects: Vec::new(),
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: coursecraft_course::SeekMode::Auto,
        revisit: coursecraft_course::RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    });

    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let sid = "s-intro";
    assert!(
        harness
            .editor
            .session
            .course
            .slide(sid)
            .unwrap()
            .question
            .is_none()
    );

    let state = QuestionState {
        last_synced_revision: 0,
        last_synced_slide: sid.to_owned(),
        kind: QuestionKind::MultipleChoice,
        prompt: "You see a spill in the corridor. What do you do first?".to_owned(),
        choices: vec![
            Choice {
                id: "a".to_owned(),
                text: "Walk around it".to_owned(),
                correct: false,
                points: 0,
            },
            Choice {
                id: "b".to_owned(),
                text: "Clean it or mark it, then report it".to_owned(),
                correct: true,
                points: 0,
            },
            Choice {
                id: "c".to_owned(),
                text: "Email facilities tomorrow".to_owned(),
                correct: false,
                points: 0,
            },
        ],
        points: 10,
        attempts: 2,
        shuffle: false,
        correct_layer: Some("correct".to_owned()),
        incorrect_layer: Some("incorrect".to_owned()),
        ..Default::default()
    };

    let q = state.to_question();
    let rev_before = harness.editor.session.revision;
    let res = harness.editor.run(
        "question.set",
        serde_json::json!({
            "slide": sid,
            "question": q,
        }),
    );
    assert!(res.is_some());
    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let actual_question = harness
        .editor
        .session
        .course
        .slide(sid)
        .unwrap()
        .question
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(actual_question, expected_question);
}

#[test]
fn course_settings_toggle_player_and_disable_quiz() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    assert!(harness.editor.session.course.player.show_menu);
    assert!(harness.editor.session.course.quiz.is_some());

    // 1. Toggle show_menu
    let res = harness.editor.run(
        "course.set_player",
        serde_json::json!({
            "show_menu": false,
        }),
    );
    assert!(res.is_some());
    assert!(!harness.editor.session.course.player.show_menu);

    // 2. Disable quiz (passing_percent: null)
    let res2 = harness.editor.run(
        "course.set_quiz",
        serde_json::json!({
            "passing_percent": serde_json::Value::Null,
        }),
    );
    assert!(res2.is_some());
    assert!(harness.editor.session.course.quiz.is_none());
}

#[test]
fn clicking_validation_issue_selects_right_slide() {
    use coursecraft_course::{Action, Event as CourseEvent, Trigger};

    let mut course = Course::from_json(COURSE_JSON).expect("valid course");

    // Introduce an invalid trigger reference on slide 1 ("s-tabs"): layer "non-existent-layer"
    let slide1 = &mut course.scenes[0].slides[1];
    slide1.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "non-existent-layer".to_owned(),
        },
        when: CourseEvent::Click {
            object: "tab1".to_owned(),
        },
        condition: None,
        disabled: false,
    });

    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    assert_eq!(harness.editor.selection.slide, "s-intro");
    assert!(harness.editor.ui.validation_errors > 0);

    // Find the issue pointing to slide 1
    let issue = harness
        .editor
        .ui
        .validation_issues
        .iter()
        .find(|i| i.path.contains("/scenes/0/slides/1"))
        .expect("issue pointing to slide 1");

    // Click/navigate to issue
    let navigated = navigate_to_issue_path(
        &harness.editor.session.course,
        &mut harness.editor.selection,
        &issue.path,
    );
    assert!(navigated);
    assert_eq!(harness.editor.selection.slide, "s-tabs");
}

#[test]
fn preview_on_intro_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let preview = Preview::new(&course, Some("s-intro"));
    assert!(preview.is_valid());
    assert!(preview.is_partial_preview());
    assert!(preview.errors.is_empty());
    assert!(preview.state.is_some());
    assert!(preview.log.iter().any(|l| l.contains("s-intro")));
}

#[test]
fn preview_driving_clicks_matches_direct_player() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");

    // Direct player execution
    let mut player = coursecraft_logic::Player::new(&course).expect("valid player");
    player.handle(&coursecraft_logic::Input::Start);
    let direct_effects = player.handle(&coursecraft_logic::Input::Click {
        object: "btn-about".into(),
    });
    assert!(
        direct_effects.contains(&coursecraft_logic::Effect::LayerShown {
            layer: "about".into(),
        })
    );

    // Preview view driving click on btn-about
    let mut preview = Preview::new(&course, Some("s-intro"));
    assert!(preview.is_valid());

    let mut view = coursecraft_render::SlideView::new();
    let p = coursecraft_logic::Player::resume(&course, preview.state.clone().expect("state"))
        .expect("resume");
    let items = coursecraft_render::compose(&p);
    let about_item = items
        .iter()
        .find(|i| i.hit == coursecraft_render::Hit::Object("btn-about".to_owned()))
        .expect("btn-about composed");

    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen_rect, course.slide_size).expect("fit");
    let btn_center = t.to_screen_rect(about_item.rect).center();

    let ctx = Context::default();
    let raw_input = RawInput {
        screen_rect: Some(screen_rect),
        events: vec![
            Event::PointerMoved(btn_center),
            Event::PointerButton {
                pos: btn_center,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: btn_center,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ],
        ..Default::default()
    };

    let mut inputs = Vec::new();
    let output = ctx.run_ui(raw_input, |ui| {
        inputs = view.show(ui, &p, None);
    });
    output.drop_without_applying_deltas();

    assert!(inputs.contains(&coursecraft_logic::Input::Click {
        object: "btn-about".into(),
    }));

    let click_input = inputs
        .into_iter()
        .find(|i| matches!(i, coursecraft_logic::Input::Click { object } if object == "btn-about"))
        .expect("click btn-about input");
    let preview_effects = preview.handle(&click_input, None);

    assert_eq!(preview_effects, direct_effects);
    assert!(
        preview
            .log
            .iter()
            .any(|l| l.contains("\"layer\":\"about\""))
    );
    assert_eq!(
        preview.state.as_ref().map(|s| &s.visible_layers),
        Some(&vec!["about".to_string()])
    );
}

#[test]
fn preview_of_invalid_course_shows_errors_and_no_player() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.start_slide = Some("non-existent-slide".into());
    let preview = Preview::new(&course, None);
    assert!(!preview.is_valid());
    assert!(!preview.errors.is_empty());
    assert!(preview.state.is_none());
}

#[test]
fn replace_course_resets_selection() {
    let course1 = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course1);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = Some("tab1-layer".into());
    harness.editor.selection.object = Some("tab1".into());

    let course2 = Session::blank("Blank Course").course;
    harness.editor.replace_course(course2);

    assert_eq!(harness.editor.selection.slide, "slide1");
    assert_eq!(harness.editor.selection.layer, None);
    assert_eq!(harness.editor.selection.object, None);
}

#[test]
fn file_ops_disabled_on_web_flag() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    assert!(harness.editor.file_ops_available);

    harness.editor.set_file_ops_available(false);
    assert!(!harness.editor.file_ops_available);
}

fn delete_key_event() -> Event {
    Event::Key {
        key: Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn editor_shortcuts_are_skipped_while_preview_has_focus() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_preview();
    harness.idle_frame();

    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());

    // Preview focused: Delete must not touch the editor's selection.
    if let Some(p) = harness.editor.preview.as_mut() {
        p.has_focus = true;
    }
    let rev = harness.editor.session.revision;
    harness.frame(vec![delete_key_event()]);
    assert_eq!(harness.editor.session.revision, rev);
    assert!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .object("title")
            .is_some()
    );

    // Preview not focused: Delete deletes as usual.
    if let Some(p) = harness.editor.preview.as_mut() {
        p.has_focus = false;
    }
    harness.frame(vec![delete_key_event()]);
    assert!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .object("title")
            .is_none()
    );
}

#[test]
fn preview_focus_follows_presses_and_hover() {
    use crate::preview::next_preview_focus;
    assert!(next_preview_focus(false, true, true));
    assert!(!next_preview_focus(true, true, false));
    assert!(next_preview_focus(true, false, false));
    assert!(next_preview_focus(false, false, true));
    assert!(!next_preview_focus(false, false, false));
}

#[test]
fn question_tab_scrolls_with_mouse_wheel_in_small_window() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 860.0));
    harness.editor.selection.slide = "s-quiz".into();
    harness.editor.ui.bottom_tab = BottomTab::Question;
    for _ in 0..3 {
        harness.idle_frame();
    }
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-quiz")
            .and_then(|s| s.question.as_ref())
            .map(|q| q.choices.len()),
        Some(3)
    );
    // Add choices so the form overflows the bounded bottom panel and must scroll.
    for _ in 0..4 {
        harness
            .editor
            .ui
            .question
            .choices
            .push(coursecraft_course::Choice {
                id: String::new(),
                text: "extra".into(),
                correct: false,
                points: 0,
            });
    }
    for _ in 0..2 {
        harness.idle_frame();
    }
    assert!(
        harness.editor.ui.question.scroll_overflow_y > 0.0,
        "scroll area must be bounded by the panel (overflow {})",
        harness.editor.ui.question.scroll_overflow_y
    );
    assert_eq!(harness.editor.ui.question.scroll_offset_y, 0.0);

    let pos = pos2(600.0, 860.0 - STATUS_BAR_HEIGHT - 60.0);
    harness.frame(vec![Event::PointerMoved(pos)]);
    harness.frame(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -80.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    }]);
    harness.idle_frame();
    assert!(
        harness.editor.ui.question.scroll_offset_y > 0.0,
        "mouse wheel should scroll the question tab"
    );
}

/// Arm the Button tool through the real menu path, then press+release once over the slide with
/// the press sharing a frame with the pointer move (as a real mouse delivers it).
fn insert_menu_then_click(idle_frames_before_press: usize) {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.idle_frame();

    let insert = harness.find_text("Insert").expect("Insert menu button");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness.find_text("Button").expect("Button menu item");
    harness.click_at(item);
    assert_eq!(
        harness.editor.ui.placement_tool,
        Some(PlacementTool::Button),
        "menu item arms the tool"
    );

    let central_rect = harness.central_panel_rect();
    let c_rect = canvas_rect(central_rect);
    let transform = SlideTransform::fit(
        canvas_fit_rect(c_rect),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    // Click near the top-left of the slide, where the open menu used to be.
    let click_pos = transform.slide_rect().min + vec2(40.0, 40.0);

    let count = |h: &TestHarness| {
        h.editor
            .session
            .course
            .slide("s-intro")
            .map_or(0, |s| s.objects.len())
    };
    let before = count(&harness);
    for _ in 0..idle_frames_before_press {
        harness.idle_frame();
    }
    harness.frame(vec![
        Event::PointerMoved(click_pos),
        Event::PointerButton {
            pos: click_pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    harness.frame(vec![Event::PointerButton {
        pos: click_pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    assert_eq!(count(&harness), before + 1, "one click places one object");
    assert_eq!(harness.editor.ui.placement_tool, None);
}

#[test]
fn insert_menu_then_single_click_places_one_object() {
    insert_menu_then_click(1);
}

/// Open a fresh trigger form on `s-tabs` (condition optionally enabled) in the editor window.
fn open_trigger_form(harness: &mut TestHarness, condition: bool) {
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = None;
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs")
        .clone();
    let mut form = TriggerFormState::new_for_owner(&slide, None, &harness.editor.session.course);
    form.condition_enabled = condition;
    harness.editor.ui.trigger_form = Some(form);
    harness.idle_frame();
    harness.idle_frame();
}

#[test]
fn condition_kind_selector_builds_compare_trigger() {
    use coursecraft_course::{Action, CmpOp, Condition, Event as CourseEvent, Trigger, Value};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_trigger_form(&mut harness, true);

    // Root starts as an object-state node; switch it to a variable comparison via the combo.
    let combo = harness.find_text("Object state").expect("kind combo");
    harness.click_at(combo);
    harness.idle_frame();
    let item = harness
        .find_text("Compare variable")
        .expect("Compare variable entry in kind popup");
    harness.click_at(item);
    harness.idle_frame();
    harness.idle_frame();

    let mut form = harness.editor.ui.trigger_form.take().expect("form open");
    match &mut form.condition_root {
        ConditionNode::Compare {
            variable,
            op,
            value,
        } => {
            assert_eq!(variable, "tabs_seen");
            assert_eq!(*op, CmpOp::Ge);
            assert_eq!(*value, Value::Number(0.0).into());
            *value = Value::Number(3.0).into();
        }
        other => panic!("expected Compare root, got {other:?}"),
    }

    form.action_tag = ActionTag::ChangeState;
    form.change_state_object = "btn-next".to_owned();
    form.change_state_state = "normal".to_owned();
    form.event_tag = EventKindTag::VariableChanges;
    form.event_var = "tabs_seen".to_owned();
    let before = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("slide")
        .triggers
        .len();
    assert!(form.commit_ok(&mut harness.editor));

    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("slide");
    assert_eq!(slide.triggers.len(), before + 1);
    let expected = Trigger {
        action: Action::ChangeState {
            object: "btn-next".to_owned(),
            state: "normal".to_owned(),
        },
        when: CourseEvent::VariableChanges {
            variable: "tabs_seen".to_owned(),
        },
        condition: Some(Condition::Compare {
            variable: "tabs_seen".to_owned(),
            op: CmpOp::Ge,
            value: Value::Number(3.0).into(),
        }),
        disabled: false,
    };
    assert_eq!(slide.triggers.last(), Some(&expected));
}

#[test]
fn condition_all_group_roundtrips_and_convert_wraps() {
    use coursecraft_course::{CmpOp, Condition, MAX_CONDITION_DEPTH, Value};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let slide = course.slide("s-tabs").expect("slide").clone();

    let cond = Condition::All(vec![
        Condition::Compare {
            variable: "tabs_seen".to_owned(),
            op: CmpOp::Ge,
            value: Value::Number(3.0).into(),
        },
        Condition::ObjectState {
            object: "tab1".to_owned(),
            state: "visited".to_owned(),
            is: true,
        },
    ]);
    let node = ConditionNode::from_condition(&cond);
    assert_eq!(node.kind(), ConditionKind::All);
    assert_eq!(node.to_condition().expect("valid"), cond);

    // Converting a leaf to a group wraps it; Any <-> All keeps the children; Not wraps.
    let leaf = ConditionNode::from_condition(&Condition::ObjectState {
        object: "tab1".to_owned(),
        state: "visited".to_owned(),
        is: false,
    });
    let all = leaf.convert_to(ConditionKind::All, &slide, &course);
    assert_eq!(all, ConditionNode::All(vec![leaf.clone()]));
    let any = node.convert_to(ConditionKind::Any, &slide, &course);
    let Condition::All(children) = cond.clone() else {
        panic!("cond is an All");
    };
    assert_eq!(any.to_condition().expect("valid"), Condition::Any(children));
    let not = leaf.convert_to(ConditionKind::Not, &slide, &course);
    assert_eq!(not, ConditionNode::Not(Box::new(leaf.clone())));
    // Same-kind conversion is the identity.
    assert_eq!(node.convert_to(ConditionKind::All, &slide, &course), node);
    // Wrapping repeatedly never silently exceeds the depth limit in the UI: depth is tracked.
    let mut deep = leaf;
    for _ in 0..MAX_CONDITION_DEPTH {
        deep = ConditionNode::Not(Box::new(deep));
    }
    assert!(deep.depth() > MAX_CONDITION_DEPTH);
}

#[test]
fn action_combo_popup_reaches_submit_results() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_trigger_form(&mut harness, false);

    let combo = harness.find_text("Jump to slide").expect("action combo");
    harness.click_at(combo);
    harness.idle_frame();
    let item = harness
        .find_text("Submit results")
        .expect("Submit results entry visible in popup");
    harness.click_at(item);
    harness.idle_frame();
    let form = harness.editor.ui.trigger_form.as_ref().expect("form open");
    assert_eq!(form.action_tag, ActionTag::SubmitResults);
}

#[test]
fn combo_popup_height_fits_every_action() {
    let ctx = Context::default();
    let mut measured = (0.0, 0.0);
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            ..Default::default()
        },
        |ui| {
            let spacing = ui.spacing();
            measured = (
                combo_popup_height(ui.ctx()),
                spacing.interact_size.y + spacing.item_spacing.y,
            );
        },
    );
    output.drop_without_applying_deltas();
    let (height, row) = measured;
    assert!(
        height >= ActionTag::ALL.len() as f32 * row,
        "{height} vs {row}"
    );
}

#[test]
fn trigger_labels_use_names_not_ids() {
    use coursecraft_course::{Action, Event as CourseEvent, Trigger};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let slide = course.slide("s-tabs").expect("slide").clone();
    // l1 is named "Slips" in the example course.
    let trig = Trigger {
        action: Action::ShowLayer { layer: "l1".into() },
        when: CourseEvent::TimelineStarts {
            layer: Some("l2".into()),
        },
        condition: None,
        disabled: false,
    };
    let text = describe(&trig, &slide);
    assert_eq!(text, "Show layer Slips when timeline starts on layer Fire");
    assert_eq!(layer_name(&slide, "l3"), "Electrical");
    assert_eq!(layer_name(&slide, "missing"), "missing");
    assert_eq!(
        object_label(&slide, "btn-next"),
        object_name(&slide, "btn-next")
    );

    // Closed combo shows the name, not the id.
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.idle_frame();
    let slide2 = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("slide")
        .clone();
    let mut form = TriggerFormState::new_for_owner(&slide2, None, &harness.editor.session.course);
    form.action_tag = ActionTag::ShowLayer;
    form.show_layer_id = "l1".to_owned();
    harness.editor.ui.trigger_form = Some(form);
    harness.idle_frame();
    harness.idle_frame();
    assert!(
        harness.find_text("Slips").is_some(),
        "closed combo shows layer name"
    );
    assert!(harness.find_text("l1").is_none(), "raw layer id not shown");
}

#[test]
fn triggers_tab_header_uses_layer_name() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-tabs".into();
    harness.editor.selection.layer = Some("l1".into());
    harness.editor.ui.bottom_tab = BottomTab::Triggers;
    harness.idle_frame();
    harness.idle_frame();
    assert!(harness.find_text("Triggers on layer Slips").is_some());
}

#[test]
fn menu_shortcut_labels_are_platform_aware() {
    let ctx = Context::default();
    coursecraft_render::install_fonts(&ctx);
    // Fonts only exist after the first pass.
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();

    ctx.set_os(egui::os::OperatingSystem::Mac);
    // macOS shows ⌘Z (or "Cmd+Z" if the font lacks the symbol), never "Ctrl+Z".
    let mac_z = menu::shortcut_label(&ctx, "Ctrl+Z");
    assert!(mac_z == "⌘Z" || mac_z == "Cmd+Z", "got {mac_z:?}");
    let mac_eq = menu::shortcut_label(&ctx, "Ctrl+=");
    assert!(mac_eq == "⌘=" || mac_eq == "Cmd+=", "got {mac_eq:?}");
    let mac_min = menu::shortcut_label(&ctx, "Ctrl+-");
    assert!(mac_min == "⌘−" || mac_min == "Cmd+−", "got {mac_min:?}");
    let mac_shift = menu::shortcut_label(&ctx, "Ctrl+Shift+Z");
    assert!(
        mac_shift == "⇧⌘Z" || mac_shift == "Shift+Cmd+Z",
        "got {mac_shift:?}"
    );
    let mac_del = menu::shortcut_label(&ctx, "Delete");
    assert!(mac_del == "⌫" || mac_del == "Delete", "got {mac_del:?}");
    assert_eq!(menu::shortcut_label(&ctx, "Escape"), "Esc");
    assert_eq!(menu::shortcut_label(&ctx, "?"), "?");

    ctx.set_os(egui::os::OperatingSystem::Windows);
    assert_eq!(menu::shortcut_label(&ctx, "Ctrl+Z"), "Ctrl+Z");
    assert_eq!(menu::shortcut_label(&ctx, "Ctrl+="), "Ctrl+=");
    assert_eq!(menu::shortcut_label(&ctx, "Ctrl+-"), "Ctrl+−");
    assert_eq!(menu::shortcut_label(&ctx, "Ctrl+Shift+Z"), "Ctrl+Shift+Z");
    assert_eq!(menu::shortcut_label(&ctx, "Delete"), "Delete");
    assert_eq!(menu::shortcut_label(&ctx, "Escape"), "Esc");
    assert_eq!(menu::shortcut_label(&ctx, "?"), "?");
    assert_eq!(menu::shortcut_label(&ctx, "Ctrl+0"), "Ctrl+0");

    // Unparseable strings pass through untouched.
    assert_eq!(menu::shortcut_label(&ctx, "Nonsense"), "Nonsense");
}

#[test]
fn test_shortcut_formatting_both_platforms_and_fallbacks() {
    // macOS with all glyphs available
    let all_glyphs = |_| true;
    assert_eq!(menu::format_shortcut("Ctrl+Z", true, all_glyphs), "⌘Z");
    assert_eq!(menu::format_shortcut("Ctrl+=", true, all_glyphs), "⌘=");
    assert_eq!(menu::format_shortcut("Ctrl+-", true, all_glyphs), "⌘−");
    assert_eq!(
        menu::format_shortcut("Ctrl+Shift+Z", true, all_glyphs),
        "⇧⌘Z"
    );
    assert_eq!(menu::format_shortcut("Delete", true, all_glyphs), "⌫");
    assert_eq!(menu::format_shortcut("Escape", true, all_glyphs), "Esc");
    assert_eq!(menu::format_shortcut("?", true, all_glyphs), "?");
    assert_eq!(menu::format_shortcut("Ctrl+0", true, all_glyphs), "⌘0");

    // macOS with missing modifier glyphs falls back to text form
    let no_glyphs = |_| false;
    assert_eq!(menu::format_shortcut("Ctrl+Z", true, no_glyphs), "Cmd+Z");
    assert_eq!(menu::format_shortcut("Ctrl+=", true, no_glyphs), "Cmd+=");
    assert_eq!(menu::format_shortcut("Ctrl+-", true, no_glyphs), "Cmd+-");
    assert_eq!(
        menu::format_shortcut("Ctrl+Shift+Z", true, no_glyphs),
        "Shift+Cmd+Z"
    );
    assert_eq!(menu::format_shortcut("Delete", true, no_glyphs), "Delete");
    assert_eq!(menu::format_shortcut("Escape", true, no_glyphs), "Esc");

    // Non-macOS with minus glyph
    assert_eq!(menu::format_shortcut("Ctrl+Z", false, all_glyphs), "Ctrl+Z");
    assert_eq!(menu::format_shortcut("Ctrl+=", false, all_glyphs), "Ctrl+=");
    assert_eq!(menu::format_shortcut("Ctrl+-", false, all_glyphs), "Ctrl+−");
    assert_eq!(
        menu::format_shortcut("Ctrl+Shift+Z", false, all_glyphs),
        "Ctrl+Shift+Z"
    );
    assert_eq!(menu::format_shortcut("Delete", false, all_glyphs), "Delete");
    assert_eq!(menu::format_shortcut("Escape", false, all_glyphs), "Esc");
    assert_eq!(menu::format_shortcut("?", false, all_glyphs), "?");

    // Non-macOS with missing minus glyph falls back to "-"
    assert_eq!(menu::format_shortcut("Ctrl+-", false, no_glyphs), "Ctrl+-");
}

#[test]
fn test_function_key_shortcut_formatting_both_platforms() {
    let all_glyphs = |_| true;
    let no_glyphs = |_| false;
    assert_eq!(menu::format_shortcut("F12", true, all_glyphs), "F12");
    assert_eq!(menu::format_shortcut("Ctrl+F12", true, all_glyphs), "⌘F12");
    assert_eq!(menu::format_shortcut("Shift+F12", true, all_glyphs), "⇧F12");
    assert_eq!(
        menu::format_shortcut("Ctrl+F12", true, no_glyphs),
        "Cmd+F12"
    );
    assert_eq!(
        menu::format_shortcut("Shift+F12", true, no_glyphs),
        "Shift+F12"
    );
    assert_eq!(
        menu::format_shortcut("Ctrl+F12", false, all_glyphs),
        "Ctrl+F12"
    );
    assert_eq!(
        menu::format_shortcut("Shift+F12", false, all_glyphs),
        "Shift+F12"
    );
}

#[test]
fn bottom_panel_is_resizable_and_bounded() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 860.0));
    for _ in 0..3 {
        harness.idle_frame();
    }
    let rect = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    assert!((rect.height() - TIMELINE_PANEL_HEIGHT).abs() < 1.5);

    // Drag the top edge upward by 120px.
    let start = pos2(600.0, rect.min.y + 2.0);
    harness.drag_by(start, vec2(0.0, -120.0), 4, Modifiers::NONE);
    harness.idle_frame();
    let grown = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    assert!(
        grown.height() > rect.height() + 100.0,
        "{grown:?} vs {rect:?}"
    );

    // Dragging far past the limit clamps at 65% of the window and keeps the canvas usable.
    harness.drag_by(
        pos2(600.0, grown.min.y),
        vec2(0.0, -600.0),
        4,
        Modifiers::NONE,
    );
    harness.idle_frame();
    let max = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    assert!(max.height() <= 860.0 * TIMELINE_PANEL_MAX_FRACTION + 1.0);
    // Drag back down past the minimum.
    harness.drag_by(
        pos2(600.0, max.min.y + 2.0),
        vec2(0.0, 900.0),
        4,
        Modifiers::NONE,
    );
    harness.idle_frame();
    let min = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    assert!(min.height() >= TIMELINE_PANEL_MIN_HEIGHT - 1.0);
}

#[test]
fn canvas_clips_objects_to_the_slide_rect() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": { "type": "text", "text": "ZZ_OUTSIDE" },
                "frame": { "x": -600.0, "y": 100.0, "w": 900.0, "h": 60.0 }
            }),
        )
        .expect("object added");
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(central)),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let slide_rect = transform.slide_rect();

    let raw_input = RawInput {
        screen_rect: Some(harness.screen_rect),
        ..Default::default()
    };
    let output = harness.ctx.run_ui(raw_input, |ui| {
        let _ = harness.editor.show(ui);
    });
    let mut found = false;
    for cs in &output.shapes {
        let mut stack = vec![&cs.shape];
        while let Some(shape) = stack.pop() {
            match shape {
                egui::epaint::Shape::Vec(v) => stack.extend(v.iter()),
                egui::epaint::Shape::Text(t) if t.galley.text() == "ZZ_OUTSIDE" => {
                    // Either the main canvas (must be clipped to the slide) or the slide
                    // thumbnail in the left panel (clipped to its own small rect).
                    if cs.clip_rect.min.x < SLIDES_PANEL_WIDTH
                        || cs.clip_rect.min.x >= central.max.x
                    {
                        continue;
                    }
                    found = true;
                    assert!(
                        slide_rect.expand(10.0).contains_rect(cs.clip_rect),
                        "object text clipped to {:?}, slide is {:?}",
                        cs.clip_rect,
                        slide_rect
                    );
                }
                _ => {}
            }
        }
    }
    output.drop_without_applying_deltas();
    assert!(found, "off-slide text should still be painted (clipped)");
}

/// Open the Question tab on the quiz slide at the given window size / pixel ratio.
fn quiz_question_harness(w: f32, h: f32, ppp: f32) -> TestHarness {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (w, h));
    harness.ctx.set_pixels_per_point(ppp);
    harness.editor.selection.slide = "s-quiz".into();
    harness.editor.ui.bottom_tab = BottomTab::Question;
    for _ in 0..4 {
        harness.idle_frame();
    }
    harness
}

/// True when "Correct layer" is painted inside the panel and not clipped away.
fn correct_layer_visible(harness: &mut TestHarness) -> bool {
    let panel = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    match harness.find_text_clipped("Correct layer") {
        Some((pos, clip)) => panel.contains(pos) && clip.contains(pos),
        None => false,
    }
}

fn check_question_tab_reachable(w: f32, h: f32, ppp: f32) {
    let mut harness = quiz_question_harness(w, h, ppp);
    let panel = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    // The scroll viewport must lie inside the visible (clipped) panel.
    let inner = harness.editor.ui.question.scroll_inner_rect;
    assert!(
        inner.max.y <= panel.max.y + 0.5 && inner.min.y >= panel.min.y - 0.5,
        "{w}x{h}@{ppp}: scroll viewport {inner:?} sticks out of panel {panel:?}"
    );
    let overflow = harness.editor.ui.question.scroll_overflow_y;
    let pos = pos2(panel.center().x, panel.max.y - 30.0);
    let mut tries = 0;
    while !correct_layer_visible(&mut harness) && tries < 6 {
        let before = harness.editor.ui.question.scroll_offset_y;
        harness.scroll_over(pos, -80.0);
        assert!(
            harness.editor.ui.question.scroll_offset_y > before || overflow <= 0.0,
            "{w}x{h}@{ppp}: wheel did not change scroll offset (overflow {overflow})"
        );
        tries += 1;
    }
    assert!(
        correct_layer_visible(&mut harness),
        "{w}x{h}@{ppp}: 'Correct layer:' never became visible inside panel {panel:?}"
    );
}

#[test]
fn question_tab_reaches_layer_pickers_800x600() {
    check_question_tab_reachable(800.0, 600.0, 1.0);
    check_question_tab_reachable(800.0, 600.0, 2.0);
}

#[test]
fn question_tab_reaches_layer_pickers_1280x860() {
    check_question_tab_reachable(1280.0, 860.0, 1.0);
    check_question_tab_reachable(1280.0, 860.0, 2.0);
}

#[test]
fn bottom_panel_top_edge_resizes_at_800x600() {
    for ppp in [1.0, 2.0] {
        let mut harness = quiz_question_harness(800.0, 600.0, ppp);
        let rect = harness.editor.ui.timeline.panel_rect.expect("panel rect");
        // Grab 6px below the top edge (just above the tab row), as a user would. The default
        // egui grab zone only reached ~4px, which is what failed in the browser.
        harness.drag_by(
            pos2(400.0, rect.min.y + 6.0),
            vec2(0.0, -60.0),
            4,
            Modifiers::NONE,
        );
        harness.idle_frame();
        let grown = harness.editor.ui.timeline.panel_rect.expect("panel rect");
        let max_h = 600.0 * TIMELINE_PANEL_MAX_FRACTION;
        assert!(
            grown.height() > rect.height() + 30.0 || (grown.height() - max_h).abs() < 1.5,
            "ppp {ppp}: {grown:?} vs {rect:?}"
        );
        assert!(grown.height() <= max_h + 1.0);
        // Shrink it back.
        let r2 = grown;
        harness.drag_by(
            pos2(400.0, r2.min.y + 2.0),
            vec2(0.0, 40.0),
            4,
            Modifiers::NONE,
        );
        harness.idle_frame();
        let shrunk = harness.editor.ui.timeline.panel_rect.expect("panel rect");
        assert!(
            shrunk.height() < r2.height() - 20.0,
            "ppp {ppp}: shrink failed: {shrunk:?} vs {r2:?}"
        );
        assert!(shrunk.height() >= TIMELINE_PANEL_MIN_HEIGHT - 1.0);
        assert!(
            shrunk.width() <= 800.5,
            "panel wider than window: {shrunk:?}"
        );
    }
}

#[test]
fn course_settings_shows_course_values_on_a_fresh_course() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    assert_eq!(harness.editor.session.revision, 0);
    harness.editor.ui.show_course_settings_window = true;
    harness.idle_frame();
    harness.idle_frame();
    let s = &harness.editor.ui.course_settings;
    assert_eq!(s.title, "Workplace Safety Basics");
    assert!(!s.id.is_empty());
    assert!(s.show_nav && s.show_menu);
    assert!(s.quiz_enabled);
    assert_eq!(s.passing_percent, 80);
    // The title is actually painted in the dialog's text field.
    assert!(harness.find_text("Workplace Safety Basics").is_some());
}

#[test]
fn course_settings_player_bar_switch_disables_bar_options_and_updates_course() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    for _ in 0..5 {
        harness.idle_frame();
    }

    assert!(harness.editor.session.course.player.show_bar);
    let bar_toggle = harness
        .find_text_last("Player bar")
        .expect("Player bar switch");
    harness.click_at(bar_toggle);
    harness.idle_frame();
    assert!(!harness.editor.session.course.player.show_bar);

    // Disabled bar options are marked disabled and show the tooltip.
    let seekbar_node = harness
        .last_widgets
        .iter()
        .find(|n| n.label.as_deref() == Some("Show seekbar"))
        .cloned()
        .expect("Show seekbar widget");
    assert!(seekbar_node.disabled, "Show seekbar must be disabled");
    harness.advance_time(1.0);
    harness.idle_frame();
    let tip = harness.probe_tooltip(seekbar_node.bounds.center());
    assert_eq!(
        tip.as_deref(),
        Some("Turn the player bar on to use this"),
        "disabled bar option must show tooltip, got {tip:?}"
    );
}

#[test]
fn b12_course_settings_seekbar_combo_disabled_with_tooltip_when_bar_or_seekbar_off() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    for _ in 0..5 {
        harness.idle_frame();
    }

    let find_seekbar_combo = |h: &mut TestHarness| {
        let row_y = h.find_text_last("Seekbar").expect("Seekbar label").y;
        let combo_pos = h
            .find_texts("Free")
            .into_iter()
            .min_by_key(|p| ((p.y - row_y).abs() * 10.0) as i32)
            .expect("Seekbar combo text");
        h.last_widgets
            .iter()
            .find(|n| n.bounds.contains(combo_pos))
            .cloned()
            .expect("Seekbar combo widget")
    };

    assert!(!find_seekbar_combo(&mut harness).disabled);

    // Turning off "Show seekbar" disables the Seekbar combo with an explanatory tooltip.
    let show_seekbar_toggle = harness
        .find_text_last("Show seekbar")
        .expect("Show seekbar toggle");
    harness.click_at(show_seekbar_toggle);
    harness.idle_frame();
    assert!(!harness.editor.session.course.player.show_seekbar);

    let combo = find_seekbar_combo(&mut harness);
    assert!(
        combo.disabled,
        "Seekbar combo must be disabled when show_seekbar is false"
    );
    harness.advance_time(1.0);
    harness.idle_frame();
    let tip = harness.probe_tooltip(combo.bounds.center());
    assert_eq!(
        tip.as_deref(),
        Some("Turn \"Show seekbar\" on to use this"),
        "disabled Seekbar combo must explain show_seekbar is off, got {tip:?}"
    );

    // Turning off "Player bar" also disables the Seekbar combo with the player bar tooltip.
    let bar_toggle = harness
        .find_text_last("Player bar")
        .expect("Player bar toggle");
    harness.click_at(bar_toggle);
    harness.idle_frame();
    assert!(!harness.editor.session.course.player.show_bar);

    let combo = find_seekbar_combo(&mut harness);
    assert!(
        combo.disabled,
        "Seekbar combo must be disabled when show_bar is false"
    );
    harness.advance_time(1.0);
    harness.idle_frame();
    let tip = harness.probe_tooltip(combo.bounds.center());
    assert_eq!(
        tip.as_deref(),
        Some("Turn the player bar on to use this"),
        "disabled Seekbar combo must explain player bar is off, got {tip:?}"
    );
}

#[test]
fn slide_properties_player_bar_override_updates_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 1100.0));
    harness.editor.selection.object = None;
    harness.editor.selection.layer = None;
    for _ in 0..5 {
        harness.idle_frame();
    }

    let slide_id = harness.editor.selection.slide.clone();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(&slide_id)
            .and_then(|s| s.player.as_ref())
            .and_then(|o| o.show_bar),
        None
    );

    // Click the "Course default" combo next to "Player bar" in Slide Properties.
    let bar_label = harness
        .find_text("Player bar")
        .expect("Player bar label in properties");
    let combo_pos = harness
        .find_texts("Course default")
        .into_iter()
        .min_by_key(|p| ((p.y - bar_label.y).abs() * 10.0) as i32)
        .expect("Course default combo for Player bar");
    harness.click_at(combo_pos);
    harness.idle_frame();

    let off_pos = harness.find_text_last("Off").expect("Off option");
    harness.click_at(off_pos);
    harness.idle_frame();

    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(&slide_id)
            .and_then(|s| s.player.as_ref())
            .and_then(|o| o.show_bar),
        Some(false)
    );
}

#[test]
fn course_settings_background_audio_choose_pick_and_remove() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    // The window grew with the player options; give it room so nothing scrolls.
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    for _ in 0..5 {
        harness.idle_frame();
    }
    if let Some(pos) = harness.find_text("Course title") {
        harness.scroll_over(pos, -300.0);
        harness.idle_frame();
    }

    assert!(harness.find_text("Background audio").is_some());
    let choose = harness.find_text("Choose…").expect("Choose button");
    let requests = click_for_requests(&mut harness, choose);
    assert!(
        requests
            .iter()
            .any(|req| matches!(req, EditorRequest::PickBackgroundAudio)),
        "{requests:?}"
    );

    let src = harness
        .editor
        .import_background_audio("theme.mp3", b"ID3 background")
        .expect("background audio imported");
    assert_eq!(src, "theme.mp3");
    assert!(
        harness
            .editor
            .media()
            .iter()
            .any(|(name, _)| name == "theme.mp3")
    );
    harness.idle_frame();
    harness.idle_frame();
    assert!(harness.find_text("theme.mp3").is_some());

    let remove = harness.find_text_last("Remove").expect("Remove button");
    harness.click_at(remove);
    harness.idle_frame();
    harness.idle_frame();
    assert!(harness.editor.session.course.background_audio.is_none());
    assert!(harness.find_text("Choose…").is_some());
}

#[test]
fn course_settings_background_volume_and_loop_commit() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.background_audio = Some(coursecraft_course::BackgroundAudio {
        src: "theme.mp3".to_owned(),
        volume: 0.5,
        looping: true,
    });
    let mut harness = TestHarness::new(course);
    harness.editor.ui.show_course_settings_window = true;
    // The Navigation row made the window taller; give it room so nothing scrolls.
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1100.0));
    for _ in 0..8 {
        harness.idle_frame();
    }
    if let Some(pos) = harness.find_text("Course title") {
        harness.scroll_over(pos, -300.0);
        harness.idle_frame();
    }

    let rect = harness
        .editor
        .ui
        .course_settings
        .background_volume_rect
        .expect("volume slider laid out");
    let value = harness
        .find_text_in("50", rect)
        .expect("the slider shows its value");
    harness.set_field(value, "25");
    harness.idle_frame();
    let audio = harness
        .editor
        .session
        .course
        .background_audio
        .clone()
        .expect("background audio");
    assert!(
        (audio.volume - 0.25).abs() < 0.001,
        "the slider writes quarter volume, got {}",
        audio.volume
    );

    let loop_pos = harness.find_text("Loop").expect("Loop checkbox");
    harness.click_at(loop_pos);
    harness.idle_frame();
    harness.idle_frame();
    let audio = harness
        .editor
        .session
        .course
        .background_audio
        .clone()
        .expect("background audio");
    assert!(!audio.looping);
}

#[test]
fn trigger_form_offers_and_round_trips_background_audio() {
    use coursecraft_course::{Action, BACKGROUND_AUDIO_ID, Event as CourseEvent};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.run(
        "course.set_background_audio",
        serde_json::json!({ "src": "theme.mp3" }),
    );
    open_trigger_form(&mut harness, false);

    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.action_tag = ActionTag::PlayMedia;
        form.media_object = "tab1".into();
        form.event_tag = EventKindTag::TimelineStarts;
    }
    harness.idle_frame();
    harness.idle_frame();
    let combo = harness.find_text_last("tab1").expect("media combo");
    harness.click_at(combo);
    harness.idle_frame();
    let background = harness
        .find_text_last("Background audio")
        .expect("background audio option");
    harness.click_at(background);
    harness.idle_frame();

    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs")
        .clone();
    let trigger = {
        let form = harness.editor.ui.trigger_form.as_ref().expect("form");
        assert_eq!(form.media_object, BACKGROUND_AUDIO_ID);
        form.build_trigger(&harness.editor.session.course)
            .expect("play background audio trigger")
    };
    assert_eq!(
        trigger.action,
        Action::PlayMedia {
            object: BACKGROUND_AUDIO_ID.to_owned()
        }
    );
    let again =
        TriggerFormState::from_existing(0, &trigger, &slide, None, &harness.editor.session.course);
    assert_eq!(again.media_object, BACKGROUND_AUDIO_ID);

    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.action_tag = ActionTag::PauseTimeline;
        form.event_tag = EventKindTag::MediaEnds;
        form.event_object = "tab1".into();
    }
    harness.idle_frame();
    harness.idle_frame();
    let combo = harness.find_text_last("tab1").expect("media ends combo");
    harness.click_at(combo);
    harness.idle_frame();
    let background = harness
        .find_text_last("Background audio")
        .expect("background audio option");
    harness.click_at(background);
    harness.idle_frame();
    let trigger = {
        let form = harness.editor.ui.trigger_form.as_ref().expect("form");
        assert_eq!(form.event_object, BACKGROUND_AUDIO_ID);
        form.build_trigger(&harness.editor.session.course)
            .expect("media ends trigger")
    };
    assert_eq!(
        trigger.when,
        CourseEvent::MediaEnds {
            object: BACKGROUND_AUDIO_ID.to_owned()
        }
    );
    let again =
        TriggerFormState::from_existing(0, &trigger, &slide, None, &harness.editor.session.course);
    assert_eq!(again.event_object, BACKGROUND_AUDIO_ID);

    assert_eq!(
        describe_action(
            &Action::PlayMedia {
                object: BACKGROUND_AUDIO_ID.to_owned()
            },
            &slide
        ),
        "Play media background audio"
    );
    assert_eq!(
        describe_event(
            &CourseEvent::MediaEnds {
                object: BACKGROUND_AUDIO_ID.to_owned()
            },
            &slide
        ),
        "media ends on background audio"
    );
}

#[test]
fn trigger_form_pause_background_audio_validates() {
    use coursecraft_course::{Action, BACKGROUND_AUDIO_ID, Event as CourseEvent, Severity};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.run(
        "course.set_background_audio",
        serde_json::json!({ "src": "theme.mp3" }),
    );
    open_trigger_form(&mut harness, false);

    let mut form = harness.editor.ui.trigger_form.take().expect("form");
    form.action_tag = ActionTag::PauseMedia;
    form.media_object = BACKGROUND_AUDIO_ID.to_owned();
    form.event_tag = EventKindTag::Click;
    form.event_object = "btn-next".to_owned();
    assert!(form.commit_ok(&mut harness.editor));

    let slide = harness
        .editor
        .session
        .course
        .slide("s-tabs")
        .expect("s-tabs");
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action
            == Action::PauseMedia {
                object: BACKGROUND_AUDIO_ID.to_owned(),
            }
            && trigger.when
                == CourseEvent::Click {
                    object: "btn-next".to_owned(),
                }
    }));
    let errors: Vec<_> = harness
        .editor
        .session
        .course
        .validate()
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn ui_states_sync_on_first_frame_and_after_replace_course() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course.clone());
    harness.editor.selection.slide = "s-quiz".into();
    harness.editor.ui.bottom_tab = BottomTab::Timeline;
    harness.idle_frame();
    harness.idle_frame();
    let quiz_ms = harness
        .editor
        .session
        .course
        .slide("s-quiz")
        .map(|s| s.duration_ms);
    assert_eq!(Some(harness.editor.ui.timeline.duration_ms), quiz_ms);

    // replace_course resets every state so the next frame resyncs.
    harness.editor.ui.show_course_settings_window = false;
    harness.editor.replace_course(course);
    harness.editor.ui.show_course_settings_window = true;
    harness.idle_frame();
    assert_eq!(
        harness.editor.ui.course_settings.title,
        "Workplace Safety Basics"
    );
}

fn preview_window_rect_is_stable(show_log: bool, w: f32, h: f32) {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (w, h));
    harness.ctx.set_pixels_per_point(2.0);
    harness.editor.open_preview();
    if let Some(p) = harness.editor.preview.as_mut() {
        p.show_log = show_log;
    }
    for _ in 0..4 {
        harness.idle_frame();
    }
    let settled = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.window_rect)
        .expect("preview window rect");
    // Click a few places inside the window, with idle frames in between.
    let targets = [
        settled.center(),
        pos2(settled.min.x + 60.0, settled.min.y + 60.0),
        pos2(settled.center().x, settled.max.y - 40.0),
    ];
    for t in targets {
        harness.click_at(t);
        for _ in 0..8 {
            harness.idle_frame();
        }
        let now = harness
            .editor
            .preview
            .as_ref()
            .and_then(|p| p.window_rect)
            .expect("preview window rect");
        assert!(
            (now.width() - settled.width()).abs() < 0.5
                && (now.height() - settled.height()).abs() < 0.5,
            "log={show_log}: preview window changed size after a click: {settled:?} -> {now:?}"
        );
    }
    for _ in 0..30 {
        harness.idle_frame();
    }
    let end = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.window_rect)
        .expect("preview window rect");
    assert_eq!(settled.size(), end.size(), "size drifted over 30 frames");
}

#[test]
fn preview_window_keeps_a_stable_size() {
    preview_window_rect_is_stable(false, 1280.0, 860.0);
    preview_window_rect_is_stable(true, 1280.0, 860.0);
    preview_window_rect_is_stable(false, 800.0, 600.0);
    preview_window_rect_is_stable(true, 800.0, 600.0);
}

fn slides_harness(ppp: f32) -> TestHarness {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (800.0, 600.0));
    harness.ctx.set_pixels_per_point(ppp);
    for _ in 0..3 {
        harness.idle_frame();
    }
    harness
}

/// Screen rect of a slide thumbnail given the slides panel's current scroll offset.
fn scrolled_thumb(harness: &TestHarness, slide: &str) -> Rect {
    slide_thumbnail_rect(
        &harness.editor.session.course,
        harness.left_panel_rect(),
        slide,
    )
    .expect("thumb")
    .translate(vec2(0.0, -harness.editor.ui.slides_scroll_y))
}

fn thumb_visible(harness: &TestHarness, slide: &str) -> bool {
    let panel = harness.left_panel_rect();
    let r = scrolled_thumb(harness, slide);
    r.min.y >= panel.min.y - 0.5 && r.max.y <= panel.max.y + 0.5
}

fn slide_ids(harness: &TestHarness) -> Vec<String> {
    harness
        .editor
        .session
        .course
        .slides()
        .map(|s| s.id.clone())
        .collect()
}

#[test]
fn slides_panel_scrolls_to_results_and_selects_it() {
    for ppp in [1.0, 2.0] {
        let mut h = slides_harness(ppp);
        assert!(
            !thumb_visible(&h, "s-results"),
            "ppp {ppp}: should overflow"
        );
        let panel = h.left_panel_rect();
        for _ in 0..6 {
            if thumb_visible(&h, "s-results") {
                break;
            }
            h.scroll_over(panel.center(), -80.0);
        }
        assert!(
            h.editor.ui.slides_scroll_y > 0.0,
            "ppp {ppp}: wheel did nothing"
        );
        assert!(
            thumb_visible(&h, "s-results"),
            "ppp {ppp}: Results not reachable"
        );
        h.click_at(scrolled_thumb(&h, "s-results").center());
        assert_eq!(h.editor.selection.slide, "s-results", "ppp {ppp}");
    }
}

#[test]
fn slides_panel_move_buttons_work_while_scrolled() {
    for ppp in [1.0, 2.0] {
        let mut h = slides_harness(ppp);
        let panel = h.left_panel_rect();
        for _ in 0..6 {
            h.scroll_over(panel.center(), -80.0);
        }
        let before = slide_ids(&h);
        assert_eq!(before.last().map(String::as_str), Some("s-results"));
        let thumb = scrolled_thumb(&h, "s-results");
        h.right_click_at(thumb.center());
        h.idle_frame();
        let up = h.find_text("Move up").expect("Move up menu item");
        h.click_at(up);
        h.idle_frame();
        let after = slide_ids(&h);
        assert_ne!(before, after, "ppp {ppp}: slide.move not applied");
        assert_eq!(after[after.len() - 2], "s-results", "ppp {ppp}");
    }
}

#[test]
fn slides_panel_drag_moves_a_thumbnail_down_one() {
    let mut h = slides_harness(1.0);
    let before = slide_ids(&h);
    assert_eq!(before.first().map(String::as_str), Some("s-intro"));

    let start = scrolled_thumb(&h, "s-intro");
    let target = scrolled_thumb(&h, "s-tabs");
    let revision = h.editor.session.revision;
    h.drag_by(
        start.center(),
        vec2(0.0, target.center().y - start.center().y),
        4,
        Modifiers::NONE,
    );

    let after = slide_ids(&h);
    assert_eq!(
        after,
        vec!["s-tabs", "s-intro", "s-quiz", "s-results"],
        "dragging a thumbnail down must move the slide"
    );
    assert_eq!(h.editor.session.revision, revision + 1);
}

#[test]
fn slides_panel_scrolls_new_slide_into_view() {
    for ppp in [1.0, 2.0] {
        let mut h = slides_harness(ppp);
        let n = slide_ids(&h).len();
        h.editor.selection.slide = "s-results".into();
        h.editor.run(
            "slide.add",
            serde_json::json!({"scene": "main", "after": "s-results"}),
        );
        // Scrolling to a target is animated; give it time to settle.
        for _ in 0..90 {
            h.idle_frame();
        }
        let ids = slide_ids(&h);
        assert_eq!(ids.len(), n + 1, "ppp {ppp}");
        let new_id = ids.last().cloned().expect("new slide");
        assert_eq!(h.editor.selection.slide, new_id, "ppp {ppp}: not selected");
        assert!(h.editor.ui.slides_scroll_y > 0.0);
        assert!(
            thumb_visible(&h, &new_id),
            "ppp {ppp}: new slide not in view"
        );
    }
}

fn test_png_bytes() -> Vec<u8> {
    use coursecraft_media::image::{ImageBuffer, ImageFormat, Rgba};
    use std::io::Cursor;
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(2, 2);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
    img.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
    img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("PNG encoding failed");
    bytes
}

#[test]
fn image_import_validates_and_stores_and_arms_tool() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    let src = harness.editor.import_image("my picture.png", &png);
    assert_eq!(src, Some("my-picture.png".into()));
    assert_eq!(harness.editor.media().len(), 1);
    assert_eq!(harness.editor.media()[0].0, "my-picture.png");

    match &harness.editor.ui.placement_tool {
        Some(PlacementTool::Image { src, natural_size }) => {
            assert_eq!(src, "my-picture.png");
            assert_eq!(*natural_size, (2.0, 2.0));
        }
        other => panic!("expected PlacementTool::Image, got {:?}", other),
    }

    // Corrupt bytes rejected
    let bad_src = harness.editor.import_image("corrupt.png", b"not a png");
    assert_eq!(bad_src, None);
    assert_eq!(harness.editor.media().len(), 1);
    assert!(harness.editor.ui.last_error.is_some());
}

#[test]
fn svg_import_uses_its_natural_size() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32" viewBox="0 0 64 32"><rect width="64" height="32" fill="#00aa00"/></svg>"##;
    let src = harness.editor.import_image("vector logo.svg", svg);
    assert_eq!(src, Some("vector-logo.svg".into()));
    match &harness.editor.ui.placement_tool {
        Some(PlacementTool::Image { src, natural_size }) => {
            assert_eq!(src, "vector-logo.svg");
            assert_eq!(*natural_size, (64.0, 32.0));
        }
        other => panic!("expected PlacementTool::Image, got {other:?}"),
    }
    assert_eq!(harness.editor.media().len(), 1);
}

#[test]
fn image_placement_on_canvas_and_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    harness.editor.import_image("logo.png", &png);

    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());

    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let placed_obj = slide
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Image { src, .. } if src == "logo.png"))
        .expect("placed image object");
    assert_eq!(placed_obj.frame.w, 2.0);
    assert_eq!(placed_obj.frame.h, 2.0);

    // One undo step removes the object
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    let slide_after = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    assert!(
        !slide_after
            .objects
            .iter()
            .any(|o| matches!(&o.kind, ObjectKind::Image { src, .. } if src == "logo.png"))
    );
}

fn solid_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let pixels = vec![0u8; width as usize * height as usize * 4];
    coursecraft_media::encode_png_rgba(width, height, &pixels).expect("PNG encoding failed")
}

fn pasted_image_object(harness: &TestHarness) -> &Object {
    harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .and_then(|slide| {
            slide.objects.iter().find(|object| {
                matches!(&object.kind, ObjectKind::Image { src, .. } if src == "pasted-image.png")
            })
        })
        .expect("pasted image object")
}

#[test]
fn pasted_image_is_centred_and_selected() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let before = harness.editor.undo_len_for_test();

    // 2x2 PNG on a 1280x720 slide: kept at natural size, centred.
    assert!(harness.editor.paste_image_png(&test_png_bytes()));

    let object = pasted_image_object(&harness);
    assert_eq!(object.frame.w, 2.0);
    assert_eq!(object.frame.h, 2.0);
    assert_eq!(object.frame.x, 639.0);
    assert_eq!(object.frame.y, 359.0);
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(object.id.as_str())
    );
    assert_eq!(harness.editor.undo_len_for_test(), before + 1);
}

#[test]
fn pasted_image_larger_than_slide_is_scaled_to_fit() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    // A small slide keeps this test's PNG tiny while still being twice its size.
    harness.editor.session.course.slide_size = coursecraft_course::Size { w: 100, h: 50 };

    assert!(harness.editor.paste_image_png(&solid_rgba_png(200, 100)));

    let object = pasted_image_object(&harness);
    assert_eq!(object.frame.w, 100.0);
    assert_eq!(object.frame.h, 50.0);
    assert_eq!(object.frame.x, 0.0);
    assert_eq!(object.frame.y, 0.0);
    // 200x100 is 2:1; the fitted 100x50 keeps the same aspect ratio.
    assert!((object.frame.w / object.frame.h - 2.0).abs() < f32::EPSILON);
}

#[test]
fn one_undo_removes_the_pasted_image() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let before = harness.editor.undo_len_for_test();

    assert!(harness.editor.paste_image_png(&test_png_bytes()));
    assert_eq!(harness.editor.undo_len_for_test(), before + 1);
    harness.editor.session.undo().expect("undo");

    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    assert!(!slide.objects.iter().any(|object| {
        matches!(&object.kind, ObjectKind::Image { src, .. } if src == "pasted-image.png")
    }));
}

#[test]
fn image_paste_is_refused_in_master_view() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let before = harness.editor.undo_len_for_test();
    harness.editor.ui.master_view = true;

    assert!(!harness.editor.paste_image_png(&test_png_bytes()));
    assert_eq!(harness.editor.undo_len_for_test(), before);
    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    assert!(!slide.objects.iter().any(|object| {
        matches!(&object.kind, ObjectKind::Image { src, .. } if src == "pasted-image.png")
    }));
}

#[test]
fn save_writes_only_referenced_media() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    let src1 = harness.editor.import_image("used.png", &png).unwrap();
    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());

    let _src2 = harness.editor.import_image("unused.png", &png).unwrap();
    assert_eq!(harness.editor.media().len(), 2);

    let used = harness.editor.used_media();
    assert_eq!(used.len(), 1);
    assert_eq!(used[0].0, src1);
    // In-memory bytes are retained.
    assert_eq!(harness.editor.media().len(), 2);
}

#[test]
fn save_keeps_every_file_in_a_web_folder() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.set_media(vec![
        (
            "web/site/index.html".to_owned(),
            std::sync::Arc::<[u8]>::from(b"<html></html>".as_slice()),
        ),
        (
            "web/site/style.css".to_owned(),
            std::sync::Arc::<[u8]>::from(b"body{}".as_slice()),
        ),
        (
            "web/site/assets/app.js".to_owned(),
            std::sync::Arc::<[u8]>::from(b"run()".as_slice()),
        ),
        (
            "unused.png".to_owned(),
            std::sync::Arc::<[u8]>::from(b"unused".as_slice()),
        ),
    ]);

    let id = crate::insert_web_object(
        &mut harness.editor,
        coursecraft_course::WebSource::Folder {
            path: "web/site".into(),
            entry: "index.html".into(),
        },
    )
    .expect("insert web object");
    assert!(!id.is_empty());
    assert!(
        harness.editor.missing_media().is_empty(),
        "{:?}",
        harness.editor.missing_media()
    );

    let used = harness.editor.used_media();
    let names: Vec<&str> = used.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "web/site/index.html",
            "web/site/style.css",
            "web/site/assets/app.js"
        ],
        "the whole web folder is kept and unused media is dropped"
    );

    let saved = coursecraft_format::save_project(&harness.editor.session.course, &used)
        .expect("save project");
    let reopened = coursecraft_format::open_project(&saved).expect("reopen project");
    assert_eq!(reopened.media.len(), 3);
    assert!(
        reopened
            .media
            .iter()
            .any(|(name, _)| name == "web/site/assets/app.js")
    );
}

#[test]
fn used_media_shares_arc_bytes_without_copying() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    let src = harness.editor.import_image("shared.png", &png).unwrap();
    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());

    let used = harness.editor.used_media();
    assert_eq!(used.len(), 1);
    assert_eq!(used[0].0, src);
    let stored = harness
        .editor
        .media()
        .iter()
        .find(|(name, _)| name == &src)
        .expect("stored media");
    assert!(
        std::sync::Arc::ptr_eq(&used[0].1, &stored.1),
        "used_media should clone the Arc handle rather than copying media bytes"
    );
}

#[test]
fn undo_after_delete_and_save_keeps_image_bytes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    let src = harness.editor.import_image("logo.png", &png).unwrap();
    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());
    assert_eq!(harness.editor.used_media().len(), 1);

    // Delete the placed image, then "save": the written set no longer has it.
    harness.frame(vec![delete_key_event()]);
    assert!(harness.editor.used_media().is_empty());

    // Bytes stay in memory, so undo brings back a renderable, saveable image.
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    assert!(harness.editor.media().iter().any(|(s, _)| *s == src));
    let used = harness.editor.used_media();
    assert_eq!(used.len(), 1);
    assert_eq!(used[0].0, src);
    assert!(!used[0].1.is_empty());
}

#[test]
fn image_replace_and_reset_to_natural_size() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let png = test_png_bytes();
    let src1 = harness.editor.import_image("first.png", &png).unwrap();
    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());

    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let obj = slide
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Image { src, .. } if src == &src1))
        .expect("placed image")
        .clone();

    harness.editor.selection.object = Some(obj.id.clone());
    harness.idle_frame();

    harness.editor.run(
        "object.set_frame",
        serde_json::json!({
            "slide": harness.editor.selection.slide,
            "object": obj.id,
            "frame": { "x": obj.frame.x, "y": obj.frame.y, "w": 100.0, "h": 100.0 }
        }),
    );

    let natural_size = harness.editor.natural_image_size(&src1);
    assert_eq!(natural_size, Some((2.0, 2.0)));
    harness.editor.run(
        "object.set_frame",
        serde_json::json!({
            "slide": harness.editor.selection.slide,
            "object": obj.id,
            "frame": { "x": obj.frame.x, "y": obj.frame.y, "w": natural_size.unwrap().0, "h": natural_size.unwrap().1 }
        }),
    );
    let slide_after = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let obj_after = slide_after.object(&obj.id).unwrap().0;
    assert_eq!(obj_after.frame.w, 2.0);
    assert_eq!(obj_after.frame.h, 2.0);

    harness.editor.pending_image_pick = Some(PendingImagePick::ReplaceObject(obj.id.clone()));
    let src2 = harness.editor.import_image("second.png", &png).unwrap();
    assert_eq!(src2, "second.png");
    let slide_replaced = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let obj_replaced = slide_replaced.object(&obj.id).unwrap().0;
    match &obj_replaced.kind {
        ObjectKind::Image { src, .. } => assert_eq!(src, "second.png"),
        other => panic!("expected Image kind, got {:?}", other),
    }
}

#[test]
fn audio_and_video_import_and_placement_on_canvas() {
    use coursecraft_playback::tiny_wav_bytes;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    // 1. Import audio file
    let wav = tiny_wav_bytes();
    let src = harness.editor.import_audio("narrator voice.wav", &wav);
    assert_eq!(src, Some("narrator-voice.wav".into()));
    assert_eq!(harness.editor.media().len(), 1);
    assert_eq!(harness.editor.media()[0].0, "narrator-voice.wav");

    match &harness.editor.ui.placement_tool {
        Some(PlacementTool::Audio { src }) => {
            assert_eq!(src, "narrator-voice.wav");
        }
        other => panic!("expected PlacementTool::Audio, got {:?}", other),
    }

    // Place audio on canvas: creates an audio object of size 48x48
    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());

    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let placed_audio = slide
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Audio { src, .. } if src == "narrator-voice.wav"))
        .expect("placed audio object");
    assert_eq!(placed_audio.frame.w, 48.0);
    assert_eq!(placed_audio.frame.h, 48.0);

    // 2. Import video file
    let video_bytes = vec![0u8; 1024];
    let v_src = harness.editor.import_video("intro.mp4", &video_bytes);
    assert_eq!(v_src, Some("intro.mp4".into()));
    assert_eq!(harness.editor.media().len(), 2);

    match &harness.editor.ui.placement_tool {
        Some(PlacementTool::Video { src }) => {
            assert_eq!(src, "intro.mp4");
        }
        other => panic!("expected PlacementTool::Video, got {:?}", other),
    }

    // Place video on canvas: creates a video object of size 640x360
    harness.click_at(canvas_rect.center());
    let slide2 = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let placed_video = slide2
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Video { src, .. } if src == "intro.mp4"))
        .expect("placed video object");
    assert_eq!(placed_video.frame.w, 640.0);
    assert_eq!(placed_video.frame.h, 360.0);
}

#[test]
fn lottie_import_and_placement_on_canvas() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let json = r#"{
      "v":"5.5.7","fr":30,"ip":0,"op":30,"w":100,"h":100,"nm":"tiny","ddd":0,
      "assets":[],
      "layers":[{
        "ddd":0,"ind":1,"ty":4,"nm":"Rect","sr":1,
        "ks":{
          "a":{"a":0,"k":[0,0]},
          "p":{"a":0,"k":[50,50]},
          "s":{"a":0,"k":[100,100]},
          "r":{"a":0,"k":0},
          "o":{"a":0,"k":100}
        },
        "ao":0,
        "shapes":[{"ty":"gr","it":[
          {"ty":"rc","d":1,"s":{"a":0,"k":[40,40]},
           "p":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},
          {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1},
          {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},
           "s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
        ]}],
        "ip":0,"op":30,"st":0,"bm":0
      }]
    }"#;
    let src = harness.editor.import_lottie("motion.json", json.as_bytes());
    assert_eq!(src, Some("motion.json".into()));
    assert_eq!(harness.editor.media().len(), 1);
    match &harness.editor.ui.placement_tool {
        Some(PlacementTool::Lottie { src, natural_size }) => {
            assert_eq!(src, "motion.json");
            assert_eq!(*natural_size, (100.0, 100.0));
        }
        other => panic!("expected PlacementTool::Lottie, got {other:?}"),
    }

    let canvas_rect = harness.central_panel_rect();
    harness.click_at(canvas_rect.center());
    let slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("slide");
    let placed = slide
        .objects
        .iter()
        .find(|o| matches!(&o.kind, ObjectKind::Lottie { src, .. } if src == "motion.json"))
        .expect("placed Lottie object");
    assert_eq!(placed.frame.w, 100.0);
    assert_eq!(placed.frame.h, 100.0);

    let bad = harness.editor.import_lottie("broken.json", b"not json");
    assert_eq!(bad, None);
    assert!(harness.editor.ui.last_error.is_some());
}

#[test]
fn media_import_size_limit_rejected() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let oversized = vec![0u8; Editor::MAX_MEDIA_IMPORT_BYTES + 1];
    let res = harness.editor.import_audio("huge.wav", &oversized);
    assert_eq!(res, None);
    assert!(harness.editor.ui.last_error.is_some());
    assert!(
        harness
            .editor
            .ui
            .last_error
            .as_ref()
            .unwrap()
            .0
            .contains("200 MiB")
    );
}

#[test]
fn preview_backend_stop_all_on_close() {
    use coursecraft_playback::NullBackend;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    harness.editor.open_preview();
    let null_backend = NullBackend::new();
    let preview = harness.editor.preview.take().expect("preview opened");
    harness.editor.preview = Some(preview.with_backend(Box::new(null_backend.clone())));

    // Run frame with preview open
    harness.idle_frame();

    // Closing the preview drops/clears it
    harness.editor.preview = None;

    // Backend must have received stop_all
    assert!(
        null_backend
            .recorded_calls()
            .contains(&"stop_all".to_string())
    );
}

#[test]
fn desktop_preview_speed_menu_changes_player_clock_rate() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.player.show_speed = true;
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    harness.editor.open_preview();
    harness.idle_frame();
    harness.idle_frame();

    let speed_btn = harness
        .find_text("1\u{00D7}")
        .expect("speed button visible in desktop preview");
    harness.click_at(speed_btn);
    harness.idle_frame();

    let choice_15 = harness
        .find_text("1.5\u{00D7}")
        .expect("1.5× speed option in popup");
    harness.click_at(choice_15);
    harness.idle_frame();

    let preview = harness.editor.preview.as_ref().expect("preview open");
    let state = preview.state.as_ref().expect("preview player state");
    assert_eq!(
        state.speed_quarters, 6,
        "choosing 1.5× must set player clock rate to 6 quarters (1.5×)"
    );
    assert!(
        (preview.media.rate() - 1.5).abs() < f32::EPSILON,
        "media driver rate must also update to 1.5"
    );
}

#[test]
fn font_picker_lists_inter_and_a_course_family() {
    const INTER_REGULAR: &[u8] = coursecraft_media::text_metrics::INTER_REGULAR;

    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.fonts.push(coursecraft_course::FontFace {
        family: "Lato".into(),
        style: coursecraft_course::FontStyle::Regular,
        src: "fonts/lato.ttf".into(),
    });
    let mut harness = TestHarness::new(course);
    harness.editor.set_media(vec![(
        "fonts/lato.ttf".into(),
        std::sync::Arc::from(INTER_REGULAR),
    )]);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();

    let picker = harness.find_text("Inter").expect("font picker");
    harness.click_at(picker);
    harness.idle_frame();

    assert!(
        harness.find_text("Lato").is_some(),
        "the picker must list the course family"
    );
    assert!(
        harness.find_text("Add font…").is_some(),
        "the picker must offer Add font…"
    );
}

#[test]
fn font_picker_lists_installed_family_and_adds_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.editor.ui.system_fonts = Some(vec![crate::SystemFontChoice {
        name: "Georgia".into(),
        embedding_restricted: true,
    }]);
    harness.idle_frame();

    let picker = harness.find_text("Inter").expect("font picker");
    let opening = click_for_requests(&mut harness, picker);
    assert!(
        !opening
            .iter()
            .any(|req| matches!(req, EditorRequest::ListSystemFonts)),
        "the list is already known, so it must not be requested again: {opening:?}"
    );

    assert!(
        harness.find_text("On this computer").is_some(),
        "the picker must have an On this computer section"
    );
    let family = harness.find_text("Georgia").expect("installed family");
    let requests = click_for_requests(&mut harness, family);
    let add = requests
        .iter()
        .find_map(|req| match req {
            EditorRequest::AddSystemFont { family, apply } => Some((family.clone(), apply.clone())),
            _ => None,
        })
        .expect("AddSystemFont request");
    assert_eq!(add.0, "Georgia");
    match add.1 {
        Some((cmd, params)) => {
            assert_eq!(cmd, "text.style_range");
            assert_eq!(params["slide"], "s-intro");
            assert_eq!(params["object"], "title");
            assert_eq!(params["font"], "Georgia");
        }
        None => panic!("the installed font click must carry apply params"),
    }
}

#[test]
fn font_picker_requests_installed_list_only_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();

    let picker = harness.find_text("Inter").expect("font picker");
    let mut requests = click_for_requests(&mut harness, picker);
    requests.extend(harness.frame(vec![]));
    assert!(
        harness.find_text("Loading fonts…").is_some(),
        "the picker must show a loading message while the list is unknown"
    );
    requests.extend(harness.frame(vec![]));
    assert_eq!(
        requests
            .iter()
            .filter(|req| matches!(req, EditorRequest::ListSystemFonts))
            .count(),
        1,
        "the list must be requested exactly once: {requests:?}"
    );
    assert!(harness.editor.ui.properties.system_fonts_list_requested);
}

#[test]
fn properties_italic_checkbox_toggles_italic_style() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();

    assert!(harness.editor.ui.properties.text_bold);
    assert!(!harness.editor.ui.properties.text_italic);

    // Toggle italic toggle button
    let rev_before = harness.editor.session.revision;
    let italic_btn_pos = harness.find_text("I").expect("italic toggle button");
    harness.click_at(italic_btn_pos);
    harness.idle_frame();

    assert_eq!(harness.editor.session.revision, rev_before + 1);

    let (obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("title")
        .expect("title");
    match &obj.kind {
        ObjectKind::Text { runs, style, .. } => {
            assert!(style.bold);
            let eff_italic = runs.first().and_then(|r| r.italic).unwrap_or(style.italic);
            assert!(eff_italic);
        }
        _ => panic!("expected text object"),
    }
}

#[test]
fn compare_condition_picker_lists_builtins_and_set_var_does_not() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_trigger_form(&mut harness, true);

    // Switch root to compare
    let combo = harness.find_text("Object state").expect("kind combo");
    harness.click_at(combo);
    harness.idle_frame();
    let item = harness
        .find_text("Compare variable")
        .expect("Compare variable item");
    harness.click_at(item);
    harness.idle_frame();

    // Click variable picker combo
    let var_combo = harness.find_text("tabs_seen").expect("var picker combo");
    harness.click_at(var_combo);
    harness.idle_frame();

    // Course.Progress should be listed in the compare variable popup
    assert!(
        harness.find_text("Course.Progress").is_some(),
        "expected Course.Progress in compare variable list"
    );

    // Switch action to SetVariable
    let action_combo = harness.find_text("Jump to slide").expect("action combo");
    harness.click_at(action_combo);
    harness.idle_frame();
    let set_var_item = harness
        .find_text("Set variable")
        .expect("Set variable item");
    harness.click_at(set_var_item);
    harness.idle_frame();

    // Click action set variable combo
    let set_var_combo = harness
        .find_text("tabs_seen")
        .expect("set var picker combo");
    harness.click_at(set_var_combo);
    harness.idle_frame();

    // Course.Progress should NOT be listed in set_variable picker
    assert!(
        harness.find_text("Course.Progress").is_none(),
        "Course.Progress must not be in set_variable picker"
    );
}

#[test]
fn text_formatting_toolbar_selection_and_bold_toggle() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-results".into();
    harness.editor.selection.object = Some("results-text".into());
    harness.idle_frame();
    harness.idle_frame();

    // Select characters 0..6 in the body TextEdit
    let text_edit_id = egui::Id::unique("prop_text_body");
    let mut state = egui::TextEdit::load_state(&harness.ctx, text_edit_id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text_selection::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(6),
        )));
    state.store(&harness.ctx, text_edit_id);

    // Find and click the "B" button in the formatting row
    let b_pos = harness.find_text("B").expect("B formatting button");
    harness.click_at(b_pos);
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-results").unwrap();
    let (obj, _) = slide.object("results-text").unwrap();
    if let ObjectKind::Text { runs, .. } = &obj.kind {
        assert!(!runs.is_empty(), "expected styled runs");
        assert_eq!(runs[0].bold, Some(true));
    } else {
        panic!("results-text is not text");
    }
}

#[test]
fn text_toolbar_applies_bullets_then_a_link() {
    use coursecraft_course::{Link, ListKind, ObjectKind};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-results".into();
    harness.editor.selection.object = Some("results-text".into());
    harness.idle_frame();
    harness.idle_frame();

    let text_len = harness
        .editor
        .session
        .course
        .slide("s-results")
        .and_then(|slide| slide.object("results-text"))
        .map(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => text.chars().count(),
            _ => 0,
        })
        .unwrap_or(0);
    let text_edit_id = egui::Id::unique("prop_text_body");
    let mut state = egui::TextEdit::load_state(&harness.ctx, text_edit_id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text_selection::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(text_len),
        )));
    state.store(&harness.ctx, text_edit_id);
    harness.idle_frame();

    // Paragraph controls are at the bottom of the object inspector.
    let panel = harness.right_panel_rect();
    for _ in 0..8 {
        harness.scroll_over(panel.center(), -120.0);
        harness.idle_frame();
    }

    let bullets = harness.find_text("Bullets").expect("Bullets button");
    harness.click_at(bullets);
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-results").unwrap();
    let (object, _) = slide.object("results-text").unwrap();
    let ObjectKind::Text { paragraphs, .. } = &object.kind else {
        panic!("results-text is not text");
    };
    assert_eq!(
        paragraphs.first().map(|p| p.list),
        Some(ListKind::Bullet),
        "the selected paragraph becomes a bullet"
    );

    for _ in 0..2 {
        harness.scroll_over(panel.center(), -120.0);
        harness.idle_frame();
    }
    let link_button = harness.find_text("Link…").expect("Link button");
    harness.click_at(link_button);
    harness.idle_frame();
    harness.editor.ui.properties.link_url = "https://example.com".into();
    let apply = harness.find_text_last("Apply").expect("Apply button");
    harness.click_at(apply);
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-results").unwrap();
    let (object, _) = slide.object("results-text").unwrap();
    let ObjectKind::Text { runs, .. } = &object.kind else {
        panic!("results-text is not text");
    };
    assert!(
        runs.iter().any(|run| matches!(
            run.link,
            Some(Link::Url { ref url }) if url == "https://example.com"
        )),
        "the run carries the URL link: {runs:?}"
    );
}

#[test]
fn typing_in_styled_text_preserves_runs() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-results".into();
    harness.editor.selection.object = Some("results-text".into());
    harness.idle_frame();

    // First style a range via text.style_range
    harness
        .editor
        .run(
            "text.style_range",
            serde_json::json!({
                "slide": "s-results",
                "object": "results-text",
                "start": 0,
                "end": 6,
                "bold": true,
            }),
        )
        .expect("style range");
    harness.idle_frame();

    // Now edit text_body and blur/change
    let mut text_edit_pos = None;
    for _ in 0..20 {
        let panel = harness.right_panel_rect();
        if let Some(pos) = harness.find_text_in("Text content", panel) {
            text_edit_pos = Some(pos + vec2(110.0, 0.0));
            break;
        }
        harness.scroll_over(panel.center(), -120.0);
    }
    let text_edit_pos = text_edit_pos.expect("Text content label");
    harness.editor.ui.properties.text_body = "Thanks a lot for completing the course.".to_owned();
    // Simulate commit via blur
    harness.click_at(text_edit_pos);
    let mut frame_pos = None;
    for _ in 0..20 {
        let panel = harness.right_panel_rect();
        if let Some(pos) = harness.find_text_in("Font", panel) {
            frame_pos = Some(pos);
            break;
        }
        harness.scroll_over(panel.center(), 120.0);
    }
    let frame_pos = frame_pos.expect("Font label to blur");
    harness.click_at(frame_pos);
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-results").unwrap();
    let (obj, _) = slide.object("results-text").unwrap();
    if let ObjectKind::Text { text, runs, .. } = &obj.kind {
        assert_eq!(text, "Thanks a lot for completing the course.");
        assert!(!runs.is_empty(), "runs must survive text editing");
        assert_eq!(runs[0].bold, Some(true));
    } else {
        panic!("results-text is not text");
    }
}

#[test]
fn theme_dialog_picks_course_font_and_resets_to_defaults() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.fonts.push(coursecraft_course::FontFace {
        family: "Lato".into(),
        style: coursecraft_course::FontStyle::Regular,
        src: "fonts/lato.ttf".into(),
    });
    let mut harness = TestHarness::new(course);
    harness.editor.set_media(vec![(
        "fonts/lato.ttf".into(),
        std::sync::Arc::from(coursecraft_media::text_metrics::INTER_REGULAR),
    )]);
    harness.editor.ui.show_theme_window = true;
    harness.idle_frame();
    harness.idle_frame();

    let def = coursecraft_course::Theme::default();
    let picker = harness
        .find_text_last(&def.font)
        .expect("theme font picker");
    harness.click_at(picker);
    let course_font = harness.find_text("Lato").expect("course font");
    let undo_len = harness.editor.undo_len_for_test();
    harness.click_at(course_font);
    assert_eq!(harness.editor.session.course.theme.font, "Lato");
    assert_eq!(harness.editor.ui.theme_dialog.font, "Lato");
    assert_eq!(harness.editor.undo_len_for_test(), undo_len + 1);

    let reset_btn = harness
        .find_text("Reset to defaults")
        .expect("Reset to defaults button");
    harness.click_at(reset_btn);
    harness.idle_frame();

    assert_eq!(harness.editor.ui.theme_dialog.font, def.font);
    assert_eq!(harness.editor.session.course.theme, def);
    harness.editor.run("edit.undo", serde_json::json!({}));
    harness.idle_frame();
    assert_eq!(harness.editor.session.course.theme.font, "Lato");
    assert_eq!(harness.editor.ui.theme_dialog.font, "Lato");
}

#[test]
fn theme_dialog_requests_embedding_and_add_font() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let default_font = course.theme.font.clone();
    let mut harness = TestHarness::new(course);
    harness.editor.ui.system_fonts = Some(vec![crate::SystemFontChoice {
        name: "Georgia".into(),
        embedding_restricted: false,
    }]);
    harness.editor.ui.show_theme_window = true;
    harness.idle_frame();
    harness.idle_frame();

    let picker = harness
        .find_text_last(&default_font)
        .expect("theme font picker");
    harness.click_at(picker);
    let system_font = harness.find_text("Georgia").expect("installed font");
    let requests = click_for_requests(&mut harness, system_font);
    let (command, params) = requests
        .iter()
        .find_map(|req| match req {
            EditorRequest::AddSystemFont { family, apply } if family == "Georgia" => apply.as_ref(),
            _ => None,
        })
        .expect("apply theme font after embedding");
    assert_eq!(command, "course.set_theme");
    assert_eq!(params, &serde_json::json!({ "font": "Georgia" }));
    assert_eq!(harness.editor.session.course.theme.font, default_font);
    assert_eq!(harness.editor.ui.theme_dialog.font, default_font);

    let picker = harness
        .find_text_last(&default_font)
        .expect("theme font picker");
    harness.click_at(picker);
    let add_font = harness.find_text("Add font…").expect("Add font button");
    let requests = click_for_requests(&mut harness, add_font);
    assert!(
        requests
            .iter()
            .any(|req| matches!(req, EditorRequest::PickFonts))
    );
}

#[test]
fn inserting_button_after_changing_accent_uses_new_accent() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.run(
        "course.set_theme",
        serde_json::json!({
            "accent": "#E63946",
        }),
    );
    assert_eq!(harness.editor.session.course.theme.accent.0, "#E63946");

    // Insert button via PlacementTool::Button on canvas
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.ui.placement_tool = Some(PlacementTool::Button);
    // Canvas click
    let canvas_center = pos2(500.0, 400.0);
    harness.click_at(canvas_center);
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-intro").unwrap();
    let btn_obj = slide.objects.last().expect("newly inserted button");
    if let ObjectKind::Button { fill, .. } = &btn_obj.kind {
        assert_eq!(fill.0, "#E63946", "new button must use theme accent color");
    } else {
        panic!("expected button kind, got {:?}", btn_obj.kind);
    }
}

#[test]
fn canvas_motion_path_handle_dragging_updates_points() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());

    // Add a motion path to btn-start
    harness.editor.run(
        "object.add_motion_path",
        serde_json::json!({
            "slide": "s-intro",
            "object": "btn-start",
            "path": {
                "id": "mp1",
                "name": "Line 1",
                "points": [{"x": 0.0, "y": 0.0}, {"x": 100.0, "y": 50.0}],
                "duration_ms": 1000,
                "easing": "ease_out",
                "relative": true
            }
        }),
    );
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-intro").unwrap();
    let (btn, _) = slide.object("btn-start").unwrap();
    assert_eq!(btn.motion_paths.len(), 1);

    // Compute the screen position of handle index 1 (x: 100, y: 50 relative to btn frame)
    let central = harness.central_panel_rect();
    let c_rect = canvas_rect(central);
    let transform = SlideTransform::fit(
        canvas_fit_rect(c_rect),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let handle_screen = transform.to_screen_pos(pos2(btn.frame.x + 100.0, btn.frame.y + 50.0));

    // Drag handle by (+20, +20)
    harness.drag_by(handle_screen, vec2(20.0, 20.0), 3, Modifiers::NONE);
    harness.idle_frame();

    let slide_after = harness.editor.session.course.slide("s-intro").unwrap();
    let (btn_after, _) = slide_after.object("btn-start").unwrap();
    let updated_pt = btn_after.motion_paths[0].points[1];
    assert!(
        (updated_pt.x - 100.0).abs() > 1.0 || (updated_pt.y - 50.0).abs() > 1.0,
        "motion path point should have moved from (100, 50), got ({}, {})",
        updated_pt.x,
        updated_pt.y
    );
}

fn shape_adjust(harness: &TestHarness, id: &str) -> Vec<f32> {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Shape { adjust, .. } => Some(adjust.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn dragging_a_shape_adjust_handle_is_one_undo_and_escape_cancels() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    let id = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": {
                    "type": "shape",
                    "shape": "rounded_rectangle",
                    "fill": "#D0D7DE",
                    "adjust": [0.12]
                },
                "frame": { "x": 300, "y": 200, "w": 200, "h": 120 }
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("shape id");
    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let c_rect = canvas_rect(central);
    let transform = SlideTransform::fit(
        canvas_fit_rect(c_rect),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let frame = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.frame)
        .expect("shape frame");
    let handles = coursecraft_render::adjust_handles(
        ShapeKind::RoundedRectangle,
        frame.w,
        frame.h,
        &shape_adjust(&harness, &id),
    );
    let Some(&(hx, hy)) = handles.first() else {
        panic!("rounded rectangle has an adjustment handle");
    };
    let handle_screen = transform.to_screen_pos(pos2(frame.x + hx, frame.y + hy));

    let revision = harness.editor.session.revision;
    let undo = harness.editor.session.undo_len();
    let before = shape_adjust(&harness, &id);
    harness.drag_by(handle_screen, vec2(30.0, 8.0), 3, Modifiers::NONE);
    harness.idle_frame();

    let after = shape_adjust(&harness, &id);
    assert_ne!(after, before, "the drag changes the adjustment");
    assert_eq!(harness.editor.session.revision, revision + 1);
    assert_eq!(harness.editor.session.undo_len(), undo + 1);

    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(shape_adjust(&harness, &id), before, "one undo restores it");

    // Escape mid-drag sends nothing and leaves the course alone.
    let revision = harness.editor.session.revision;
    let adjusted_handles =
        coursecraft_render::adjust_handles(ShapeKind::RoundedRectangle, frame.w, frame.h, &before);
    let Some(&(hx, hy)) = adjusted_handles.first() else {
        panic!("rounded rectangle has an adjustment handle");
    };
    let start = transform.to_screen_pos(pos2(frame.x + hx, frame.y + hy));
    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerMoved(start + vec2(25.0, 5.0))]);
    harness.frame(vec![Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerButton {
        pos: start + vec2(25.0, 5.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    assert_eq!(shape_adjust(&harness, &id), before);
    assert_eq!(harness.editor.session.revision, revision);
}

#[test]
fn reset_shape_clears_the_adjustment_in_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    let id = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": {
                    "type": "shape",
                    "shape": "rounded_rectangle",
                    "fill": "#D0D7DE",
                    "adjust": [0.2]
                },
                "frame": { "x": 300, "y": 200, "w": 200, "h": 120 }
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("shape id");
    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();

    let revision = harness.editor.session.revision;
    let reset = harness
        .find_text("Reset shape")
        .expect("Reset shape button");
    harness.click_at(reset);
    harness.idle_frame();
    assert!(shape_adjust(&harness, &id).is_empty());
    assert_eq!(harness.editor.session.revision, revision + 1);

    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(shape_adjust(&harness, &id), vec![0.2]);
}

#[test]
fn trigger_form_move_on_path_and_animation_completes_builds_valid_trigger() {
    use coursecraft_course::Action;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();

    // Add motion path to btn-start
    harness.editor.run(
        "object.add_motion_path",
        serde_json::json!({
            "slide": "s-intro",
            "object": "btn-start",
            "path": {
                "id": "mp1",
                "name": "Line 1",
                "points": [{"x": 0.0, "y": 0.0}, {"x": 100.0, "y": 50.0}],
                "duration_ms": 1000,
                "easing": "ease_out",
                "relative": true
            }
        }),
    );
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-intro").unwrap();
    let mut form = TriggerFormState::new_for_owner(slide, None, &harness.editor.session.course);
    form.action_tag = ActionTag::MoveOnPath;
    form.move_path_object = "btn-start".into();
    form.move_path_id = "mp1".into();

    form.event_tag = EventKindTag::AnimationCompletes;
    form.anim_complete_object = "btn-start".into();
    form.anim_complete_which = "mp1".into();

    let built = form
        .build_trigger(&harness.editor.session.course)
        .expect("builds valid trigger");
    assert_eq!(
        built.action,
        Action::MoveOnPath {
            object: "btn-start".into(),
            path: "mp1".into(),
        }
    );
    assert_eq!(
        built.when,
        coursecraft_course::Event::AnimationCompletes {
            object: "btn-start".into(),
            path: Some("mp1".into()),
        }
    );
}

#[test]
fn trigger_form_run_javascript_shows_a_code_box_and_builds_the_action() {
    use coursecraft_course::Action;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_trigger_form(&mut harness, false);
    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.action_tag = ActionTag::RunJavaScript;
        form.javascript_code = "CourseCraft.setVar('tabs_seen', 3);".to_owned();
    }
    harness.idle_frame();
    harness.idle_frame();
    assert!(
        harness
            .find_text("Runs only in the published course")
            .is_some(),
        "the code box and its note are shown"
    );

    let form = harness.editor.ui.trigger_form.as_ref().expect("form");
    let trigger = form
        .build_trigger(&harness.editor.session.course)
        .expect("builds a Run JavaScript trigger");
    assert_eq!(
        trigger.action,
        Action::RunJavaScript {
            code: "CourseCraft.setVar('tabs_seen', 3);".into()
        }
    );

    let mut too_big = form.clone();
    too_big.javascript_code = "x".repeat(coursecraft_course::MAX_JAVASCRIPT_BYTES + 1);
    assert!(
        too_big
            .build_trigger(&harness.editor.session.course)
            .is_err(),
        "code over the cap is refused"
    );
}

#[test]
fn trigger_form_lightbox_actions_round_trip() {
    use coursecraft_course::Action;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_trigger_form(&mut harness, false);
    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.action_tag = ActionTag::LightboxSlide;
        form.lightbox_slide_id = "s-quiz".into();
    }
    harness.idle_frame();
    harness.idle_frame();
    let form = harness.editor.ui.trigger_form.as_ref().expect("form");
    let trigger = form
        .build_trigger(&harness.editor.session.course)
        .expect("builds a lightbox trigger");
    assert_eq!(
        trigger.action,
        Action::LightboxSlide {
            slide: "s-quiz".into()
        }
    );

    let mut again = TriggerFormState::from_existing(
        0,
        &trigger,
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide"),
        None,
        &harness.editor.session.course,
    );
    assert_eq!(again.action_tag, ActionTag::LightboxSlide);
    assert_eq!(again.lightbox_slide_id, "s-quiz");

    again.action_tag = ActionTag::CloseLightbox;
    let close = again
        .build_trigger(&harness.editor.session.course)
        .expect("builds a close trigger");
    assert_eq!(close.action, Action::CloseLightbox);

    // The form refuses a lightbox on its own slide.
    let mut self_box = again;
    self_box.action_tag = ActionTag::LightboxSlide;
    self_box.lightbox_slide_id = self_box.owner_slide.clone();
    assert!(
        self_box
            .build_trigger(&harness.editor.session.course)
            .is_err(),
        "a lightbox cannot open its own slide"
    );
}

#[test]
fn trigger_form_round_trips_the_quiz_actions() {
    use coursecraft_course::Action;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("slide");
    for (tag, action) in [
        (ActionTag::RetryQuiz, Action::RetryQuiz),
        (ActionTag::ReviewQuiz, Action::ReviewQuiz),
        (ActionTag::ExitReview, Action::ExitReview),
    ] {
        let mut form = TriggerFormState::new_for_owner(slide, None, &course);
        form.action_tag = tag;
        let trigger = form
            .build_trigger(&course)
            .expect("builds a quiz action trigger");
        assert_eq!(trigger.action, action);
        let again = TriggerFormState::from_existing(0, &trigger, slide, None, &course);
        assert_eq!(again.action_tag, tag);
    }
}

#[test]
fn trigger_form_round_trips_print_slide() {
    use coursecraft_course::Action;

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("slide");
    let mut form = TriggerFormState::new_for_owner(slide, None, &course);
    form.action_tag = ActionTag::PrintSlide;
    let trigger = form
        .build_trigger(&course)
        .expect("builds a print slide trigger");
    assert_eq!(trigger.action, Action::PrintSlide);
    let again = TriggerFormState::from_existing(0, &trigger, slide, None, &course);
    assert_eq!(again.action_tag, ActionTag::PrintSlide);
}

#[test]
fn preview_print_queues_a_png_save_request() {
    use coursecraft_course::{Action, Trigger};

    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course
        .slide_mut("s-intro")
        .expect("intro slide")
        .triggers
        .push(Trigger {
            action: Action::PrintSlide,
            when: coursecraft_course::Event::Click {
                object: "btn-about".into(),
            },
            condition: None,
            disabled: false,
        });

    let mut preview = Preview::new(&course, Some("s-intro"));
    assert!(preview.is_valid());
    let effects = preview.handle(
        &coursecraft_logic::Input::Click {
            object: "btn-about".into(),
        },
        None,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, coursecraft_logic::Effect::Print { .. })),
        "the click fires the print effect"
    );
    let request = preview
        .requests
        .iter()
        .find_map(|request| match request {
            EditorRequest::SaveImage {
                png,
                suggested_name,
            } => Some((png.clone(), suggested_name.clone())),
            _ => None,
        })
        .expect("SaveImage request");
    assert!(
        request.0.starts_with(b"\x89PNG"),
        "the queued bytes are a PNG"
    );
    assert!(request.1.ends_with(".png"));
}

#[test]
fn insert_menu_creates_a_results_slide_and_selects_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-quiz".into();
    harness.idle_frame();
    harness.idle_frame();

    let insert = harness.find_text("Insert").expect("Insert menu button");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness
        .find_text("Results slide")
        .expect("Results slide menu item");
    harness.click_at(item);
    harness.idle_frame();

    let id = harness.editor.selection.slide.clone();
    assert_ne!(id, "s-quiz");
    let slide = harness
        .editor
        .session
        .course
        .slide(&id)
        .expect("new results slide");
    assert_eq!(slide.title, "Results");
    assert_eq!(
        harness.editor.session.course.slide_index(&id),
        harness
            .editor
            .session
            .course
            .slide_index("s-quiz")
            .map(|index| index + 1)
    );
}

#[test]
fn insert_menu_creates_a_certificate_slide_and_selects_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-quiz".into();
    harness.idle_frame();
    harness.idle_frame();

    let insert = harness.find_text("Insert").expect("Insert menu button");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness
        .find_text("Certificate slide")
        .expect("Certificate slide menu item");
    harness.click_at(item);
    harness.idle_frame();

    let id = harness.editor.selection.slide.clone();
    assert_ne!(id, "s-quiz");
    let slide = harness
        .editor
        .session
        .course
        .slide(&id)
        .expect("new certificate slide");
    assert_eq!(slide.title, "Certificate");
    assert_eq!(
        harness.editor.session.course.slide_index(&id),
        harness
            .editor
            .session
            .course
            .slide_index("s-quiz")
            .map(|index| index + 1)
    );
}

#[test]
fn slide_transition_properties_editing() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = None;
    harness.idle_frame();

    // Set transition directly via engine command
    harness.editor.run(
        "slide.set_transition",
        serde_json::json!({
            "slide": "s-intro",
            "transition": {
                "type": "fade",
                "duration_ms": 750
            }
        }),
    );
    harness.idle_frame();

    let slide = harness.editor.session.course.slide("s-intro").unwrap();
    assert_eq!(
        slide.transition,
        Some(coursecraft_course::Transition {
            effect: coursecraft_course::TransitionEffect::Fade,
            duration_ms: 750,
        })
    );
    assert_eq!(harness.editor.ui.properties.slide_transition_effect, "fade");
    assert_eq!(
        harness.editor.ui.properties.slide_transition_duration_ms,
        750
    );
}

fn quiz_question(harness: &TestHarness) -> coursecraft_course::Question {
    harness
        .editor
        .session
        .course
        .slide("s-quiz")
        .and_then(|slide| slide.question.clone())
        .expect("quiz question")
}

#[test]
fn switching_question_kind_to_fill_in_shows_the_fill_in_form() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    assert!(harness.find_text("+ Add choice").is_some());
    let prompt = quiz_question(&harness).prompt;
    harness.pick_combo("Multiple choice", "Fill in");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::FillIn);
    assert!(q.choices.is_empty(), "fill-in keeps no choices");
    assert_eq!(q.accepted, vec!["answer".to_owned()]);
    assert!(!q.case_sensitive);
    assert_eq!(q.prompt, prompt);
    assert_eq!(q.points, 10);
    assert_eq!(q.attempts, 2);
    assert!(harness.find_text("Case sensitive").is_some());
    assert!(
        harness.find_text("+ Add choice").is_none(),
        "fill-in does not offer choice rows"
    );
}

#[test]
fn freeform_question_shows_the_condition_builder_and_applies_a_change() {
    use coursecraft_course::{CmpOp, Condition, QuestionKind, Value};

    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    harness.pick_combo("Multiple choice", "Custom question (graded by a condition)");
    harness.idle_frame();
    harness.idle_frame();

    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Freeform);
    assert!(q.choices.is_empty(), "freeform keeps no choices");
    assert_eq!(
        q.correct_when,
        Some(Condition::Compare {
            variable: "tabs_seen".into(),
            op: CmpOp::Ge,
            value: Value::Number(0.0).into(),
        })
    );
    assert!(harness.find_text("Correct when").is_some());
    assert!(
        harness
            .find_text("Design the slide yourself. Submit grades the condition.")
            .is_some()
    );
    assert!(
        harness.find_text("Scoring").is_none(),
        "freeform has no scoring picker"
    );
    assert!(harness.find_text("Points").is_some());
    assert!(harness.find_text("+ Add choice").is_none());

    let panel = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    let scroll_pos = egui::pos2(panel.center().x, panel.max.y - 30.0);
    harness.scroll_over(scroll_pos, -160.0);

    harness.pick_combo("Compare variable", "Not");
    harness.idle_frame();
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(
        q.correct_when,
        Some(Condition::Not(Box::new(Condition::Compare {
            variable: "tabs_seen".into(),
            op: CmpOp::Ge,
            value: Value::Number(0.0).into(),
        }))),
        "the form writes the changed condition through question.set"
    );
}

#[test]
fn likert_choices_never_mark_correct_and_add_hides_at_eleven() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    harness.pick_combo("Multiple choice", "Likert scale");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Likert);
    assert_eq!(q.choices.len(), 5);
    assert!(q.choices.iter().all(|c| !c.correct));

    let add = harness.find_text("+ Add choice").expect("add choice");
    harness.click_at(add);
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.choices.len(), 6);
    assert!(q.choices.iter().all(|c| !c.correct));

    let panel = harness.editor.ui.timeline.panel_rect.expect("panel");
    let trash_glyph = coursecraft_render::Icon::Trash.codepoint().to_string();
    let trash = harness
        .find_text_in(&trash_glyph, panel)
        .expect("remove choice");
    harness.click_at(trash);
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.choices.len(), 5);
    assert!(q.choices.iter().all(|c| !c.correct));

    while quiz_question(&harness).choices.len() < 11 {
        let add = harness.find_text("+ Add choice").expect("add choice");
        harness.click_at(add);
        harness.idle_frame();
    }
    let q = quiz_question(&harness);
    assert_eq!(q.choices.len(), 11);
    assert!(q.choices.iter().all(|c| !c.correct));
    assert!(
        harness.find_text("+ Add choice").is_none(),
        "add choice is hidden at 11 choices"
    );
}

#[test]
fn survey_question_shows_not_graded_and_no_points() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    harness.pick_combo("Multiple choice", "Likert scale");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Likert);
    assert!(harness.find_text("Not graded").is_some());
    assert!(harness.find_text("Points").is_none());
}

#[test]
fn adding_a_matching_pair_appends_one() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    harness.pick_combo("Multiple choice", "Matching");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Matching);
    assert!(q.choices.is_empty());
    assert_eq!(q.pairs.len(), 2);
    assert!(harness.find_text("Shuffled each attempt").is_some());
    let add = harness.find_text("+ Add pair").expect("add pair");
    harness.click_at(add);
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.pairs.len(), 3);
    assert!(
        q.pairs
            .iter()
            .all(|p| !p.left.is_empty() && !p.right.is_empty())
    );
}

#[test]
fn reordering_a_sequence_item_moves_it_down() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);
    harness.pick_combo("Multiple choice", "Sequence");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Sequence);
    assert!(q.choices.iter().all(|c| !c.correct));
    assert_eq!(
        q.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    let panel = harness.editor.ui.timeline.panel_rect.expect("panel");
    // Icon buttons render the arrow glyph, not a text label; find the first one in the panel.
    let down_glyph = coursecraft_render::Icon::ArrowDown.codepoint().to_string();
    let down = harness
        .find_text_in(&down_glyph, panel)
        .expect("first sequence down");
    harness.click_at(down);
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(
        q.choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["b", "a", "c"]
    );
}

#[test]
fn switching_question_kind_to_hotspot_shows_the_hotspot_form_and_adds_region() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);

    // Switching to Hotspot before an image exists on the slide fails
    harness.pick_combo("Multiple choice", "Hotspot");
    harness.idle_frame();
    assert!(harness.editor.ui.last_error.is_some());
    assert_ne!(quiz_question(&harness).kind, QuestionKind::Hotspot);

    // Add an image object to s-quiz
    harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-quiz",
            "kind": { "type": "image", "src": "diagram.png" },
            "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 300.0 }
        }),
    );
    harness.idle_frame();

    // Now switch to Hotspot succeeds
    harness.pick_combo("Multiple choice", "Hotspot");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Hotspot);
    assert!(q.hotspot_image.is_some());
    assert_eq!(q.regions.len(), 1);
    assert_eq!(q.markers, 1);
    assert!(harness.find_text("+ Add region").is_some());

    // Click "+ Add region"
    let add = harness.find_text("+ Add region").expect("add region");
    harness.click_at(add);
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.regions.len(), 2);
}

#[test]
fn canvas_hotspot_region_dragging_updates_position_and_undo() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);

    // Add an image object to s-quiz
    harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-quiz",
            "kind": { "type": "image", "src": "diagram.png" },
            "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 300.0 }
        }),
    );
    harness.idle_frame();

    harness.pick_combo("Multiple choice", "Hotspot");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::Hotspot);
    let reg_id = q.regions[0].id.clone();
    let initial_x = q.regions[0].x;
    let initial_y = q.regions[0].y;

    // Select the region in the editor
    harness.editor.ui.question.selected_region = Some(reg_id);

    // Find the region center on canvas
    let central = harness.central_panel_rect();
    let c_rect = canvas_rect(central);
    let transform = SlideTransform::fit(
        canvas_fit_rect(c_rect),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let reg_center = transform.to_screen_pos(pos2(
        initial_x + q.regions[0].w / 2.0,
        initial_y + q.regions[0].h / 2.0,
    ));

    // Drag the region by (+40, +30)
    harness.drag_by(reg_center, vec2(40.0, 30.0), 3, Modifiers::NONE);
    harness.idle_frame();

    let q_after = quiz_question(&harness);
    assert!(
        (q_after.regions[0].x - initial_x).abs() > 1.0,
        "region x should have moved from {}, got {}",
        initial_x,
        q_after.regions[0].x
    );
    assert!(
        (q_after.regions[0].y - initial_y).abs() > 1.0,
        "region y should have moved from {}, got {}",
        initial_y,
        q_after.regions[0].y
    );

    // Undo restores the region's position
    harness.frame(vec![Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    harness.idle_frame();

    let q_restored = quiz_question(&harness);
    assert!(
        (q_restored.regions[0].x - initial_x).abs() < 1e-3,
        "undo should restore region x"
    );
}

#[test]
fn switching_question_kind_to_drag_drop_shows_form_and_updates_target() {
    use coursecraft_course::QuestionKind;
    let mut harness = quiz_question_harness(1280.0, 900.0, 1.0);

    // Add two objects to s-quiz (named Target A and Target B)
    harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-quiz",
            "name": "Target A",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 100.0, "y": 100.0, "w": 100.0, "h": 100.0 }
        }),
    );
    harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-quiz",
            "name": "Target B",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#445566" },
            "frame": { "x": 300.0, "y": 100.0, "w": 100.0, "h": 100.0 }
        }),
    );
    harness.idle_frame();

    // Switch to Drag and drop
    harness.pick_combo("Multiple choice", "Drag and drop");
    harness.idle_frame();
    let q = quiz_question(&harness);
    assert_eq!(q.kind, QuestionKind::DragDrop);
    assert_eq!(q.drag_items.len(), 1);
    assert!(q.snap_to_center);
    assert!(harness.find_text("Snap to center").is_some());
    assert!(harness.find_text("Return if wrong").is_some());
    assert!(harness.find_text("One per target").is_some());

    // Initially the drag item's target is Target A
    let first_target = q.drag_items[0].target.clone();
    let target_a_obj = harness
        .editor
        .session
        .course
        .slide("s-quiz")
        .and_then(|s| s.object(&first_target))
        .map(|(obj, _)| obj.clone())
        .expect("target a obj");
    assert_eq!(target_a_obj.name, "Target A");

    // Open the target combo for this drag item and pick Target B
    let combo_pos = harness.find_text("Target A").expect("target A combo");
    harness.click_at(combo_pos);
    harness.idle_frame();
    let item_pos = harness
        .find_text_last("Target B")
        .expect("target B in popup");
    harness.click_at(item_pos);
    harness.idle_frame();

    let q = quiz_question(&harness);
    let new_target = q.drag_items[0].target.clone();
    let target_b_obj = harness
        .editor
        .session
        .course
        .slide("s-quiz")
        .and_then(|s| s.object(&new_target))
        .map(|(obj, _)| obj.clone())
        .expect("target b obj");
    assert_eq!(target_b_obj.name, "Target B");
}

#[test]
fn inspector_widget_rects_within_panel_minus_margin() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);

    // Test both a selected text object ("title") and a selected button ("btn-start")
    for obj_id in ["title", "btn-start"] {
        harness.editor.selection.slide = "s-intro".into();
        harness.editor.selection.object = Some(obj_id.into());
        harness.idle_frame();

        let panel_rect = harness.right_panel_rect();
        let max_x_allowed = panel_rect.max.x - coursecraft_render::PANEL_MARGIN;
        let min_x_allowed = panel_rect.min.x + coursecraft_render::PANEL_MARGIN;

        let mut checked_count = 0;
        harness.ctx.viewport(|vp| {
            for (_, widgets) in vp.prev_pass.widgets.layers() {
                for w in widgets {
                    // Check interactive widgets (controls: text inputs, combos, buttons, drag values)
                    // located within the right inspector panel (excluding the panel's outer scrollbar)
                    let is_scrollbar = format!("{:?}", w.id).contains("scroll");
                    if w.sense.interactive()
                        && !is_scrollbar
                        && w.rect.center().y >= panel_rect.min.y
                        && w.rect.center().y <= panel_rect.max.y
                        && w.rect.min.x >= panel_rect.min.x - 1.0
                        && w.rect.max.x <= panel_rect.max.x + 5.0
                    {
                        // Skip zero-sized or unallocated widgets
                        if w.rect.width() <= 0.0 || w.rect.height() <= 0.0 {
                            continue;
                        }
                        checked_count += 1;
                        assert!(
                            w.rect.max.x <= max_x_allowed + 1.0,
                            "Widget {:?} on object '{}' extends past right inner edge: max.x={} > allowed={}",
                            w.id,
                            obj_id,
                            w.rect.max.x,
                            max_x_allowed
                        );
                        assert!(
                            w.rect.min.x >= min_x_allowed - 1.0,
                            "Widget on object '{}' extends past left inner edge: min.x={} < allowed={}",
                            obj_id,
                            w.rect.min.x,
                            min_x_allowed
                        );
                    }
                }
            }
        });
        assert!(
            checked_count > 5,
            "Expected several inspector widgets to be verified for object '{}'",
            obj_id
        );
    }
}

#[test]
fn canvas_zoom_transform_hit_test_and_drag() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let central_rect = harness.central_panel_rect();
    let c_rect = canvas_rect(central_rect);

    // 1. At Fit, clicking an object's centre selects it
    harness.editor.ui.zoom = ZoomLevel::Fit;
    harness.editor.selection.object = None;
    harness.idle_frame();

    let fit_rect = canvas_fit_rect(c_rect);
    let fit_t = SlideTransform::fit(fit_rect, harness.editor.session.course.slide_size)
        .expect("fit transform");

    let (title_obj, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("title")
        .expect("title object");
    let title_slide_rect = Rect::from_min_size(
        pos2(title_obj.frame.x, title_obj.frame.y),
        vec2(title_obj.frame.w, title_obj.frame.h),
    );
    let title_screen_center = fit_t.to_screen_rect(title_slide_rect).center();

    harness.click_at(title_screen_center);
    harness.idle_frame();
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some("title"),
        "clicking object center at Fit zoom should select it"
    );

    // The zoomed view stays on the slide centre, so move the title there to keep it in view.
    harness
        .editor
        .run(
            "object.set_frame",
            serde_json::json!({ "slide": "s-intro", "object": "title", "frame": { "x": 560.0, "y": 310.0, "w": 160.0, "h": 100.0 } }),
        )
        .expect("move the title");
    let title_slide_rect = Rect::from_min_size(pos2(560.0, 310.0), vec2(160.0, 100.0));
    // 2. At 200%, dragging an object by 20 screen px moves it 10 slide px
    harness.editor.ui.zoom = ZoomLevel::Percent200;
    harness.idle_frame();

    let (title_before, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("title")
        .expect("title object");
    let orig_slide_x = title_before.frame.x;

    let slide_size = harness.editor.session.course.slide_size;
    let (sw, sh) = (slide_size.w as f32, slide_size.h as f32);
    let scale_200 = 2.0;
    let slide_w = sw * scale_200;
    let slide_h = sh * scale_200;
    // Zooming keeps the view centre, so the slide is where the canvas drew it,
    // not at the top-left of the scrollable area.
    harness.idle_frame();
    let slide_rect_200 = harness.editor.ui.last_slide_rect.expect("slide drawn");
    assert!(
        (slide_rect_200.width() - slide_w).abs() < 1.0
            && (slide_rect_200.height() - slide_h).abs() < 1.0,
        "200% slide rect {slide_rect_200:?}"
    );
    let t_200 = SlideTransform::fit(slide_rect_200, slide_size).expect("200% transform");

    let title_200_rect = t_200.to_screen_rect(title_slide_rect);
    // The title sits at the slide centre, which the zoom keeps at the view centre.
    let start_pos = title_200_rect.center();

    // Drag by 20 screen px to the right
    harness.drag_by(start_pos, vec2(20.0, 0.0), 4, Modifiers::NONE);
    harness.idle_frame();

    let (title_after, _) = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .object("title")
        .expect("title object");
    assert_eq!(
        title_after.frame.x,
        orig_slide_x + 10.0,
        "dragging 20 screen px at 200% zoom should move 10 slide px"
    );
}

#[test]
fn toolbar_normal_width_labels_and_actions() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1280.0, 800.0));
    harness.idle_frame();

    // At >= 1100 px width, toolbar labels are laid out
    assert!(
        harness.find_text("New slide").is_some(),
        "New slide button label"
    );
    assert!(
        harness.find_text("Preview").is_some(),
        "Preview button label"
    );
    assert!(harness.find_text("Text").is_some(), "Text button label");
    assert!(harness.find_text("Shape").is_some(), "Shape button label");

    // Clicking Text arms TextBox placement tool
    let text_pos = harness.find_text("Text").expect("Text button");
    harness.click_at(text_pos);
    harness.idle_frame();
    assert_eq!(
        harness.editor.ui.placement_tool,
        Some(crate::PlacementTool::TextBox)
    );

    // Clicking Text again disarms it
    harness.click_at(text_pos);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.placement_tool, None);

    // Clicking Inspector toggles switches tab
    let format_pos = harness.find_text("Format").expect("Format button");
    harness.click_at(format_pos);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.inspector_tab, crate::InspectorTab::Format);

    let course_pos = harness.find_text("Course").expect("Course button");
    harness.click_at(course_pos);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.inspector_tab, crate::InspectorTab::Course);

    // Clicking Preview dispatches preview request (find_text_last skips the top-level Preview menu)
    let preview_pos = harness.find_text_last("Preview").expect("Preview button");
    let reqs = harness.frame(vec![
        egui::Event::PointerMoved(preview_pos),
        egui::Event::PointerButton {
            pos: preview_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        egui::Event::PointerButton {
            pos: preview_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ]);
    assert!(
        reqs.iter()
            .any(|r| matches!(r, crate::EditorRequest::Preview { .. })),
        "Preview toolbar button must dispatch Preview request"
    );
}

#[test]
fn toolbar_narrow_width_drops_labels_icon_only() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1024.0, 768.0));
    harness.idle_frame();

    // Below 1100 px, toolbar labels are dropped to prevent clipping
    assert!(
        harness.find_text("New slide").is_none(),
        "New slide label should be dropped at 1024 px width"
    );

    // But icons are still rendered and interactive
    let plus_icon = coursecraft_render::Icon::Plus;
    let plus_pos = harness
        .find_icon(plus_icon)
        .expect("Plus icon rendered at 1024 px width");
    assert!(plus_pos.y <= crate::toolbar::TOOLBAR_HEIGHT + crate::TOP_BAR_HEIGHT);
}

#[test]
fn test_fit_zoom_does_not_intersect_zoom_hud_at_1440x900_and_1024x768() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");

    // 1. At 1440x900
    let mut harness_1440 = TestHarness::new_with_size(course.clone(), (1440.0, 900.0));
    harness_1440.editor.ui.zoom = ZoomLevel::Fit;
    harness_1440.idle_frame();
    let slide_rect_1440 = harness_1440
        .editor
        .ui
        .last_slide_rect
        .expect("slide rect at 1440x900");
    let zoom_rect_1440 = harness_1440
        .editor
        .ui
        .last_zoom_hud_rect
        .expect("zoom hud rect at 1440x900");
    assert!(
        !slide_rect_1440.intersects(zoom_rect_1440),
        "slide rect {slide_rect_1440:?} must not intersect zoom control {zoom_rect_1440:?} at 1440x900"
    );

    // 2. At 1024x768
    let mut harness_1024 = TestHarness::new_with_size(course, (1024.0, 768.0));
    harness_1024.editor.ui.zoom = ZoomLevel::Fit;
    harness_1024.idle_frame();
    let slide_rect_1024 = harness_1024
        .editor
        .ui
        .last_slide_rect
        .expect("slide rect at 1024x768");
    let zoom_rect_1024 = harness_1024
        .editor
        .ui
        .last_zoom_hud_rect
        .expect("zoom hud rect at 1024x768");
    assert!(
        !slide_rect_1024.intersects(zoom_rect_1024),
        "slide rect {slide_rect_1024:?} must not intersect zoom control {zoom_rect_1024:?} at 1024x768"
    );
}

#[test]
fn test_welcome_mode_shows_card_and_actions() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.show_welcome = true;
    harness.editor.ui.has_loaded_course = false;
    harness.idle_frame();

    assert!(harness.find_text("CourseCraft").is_some());
    assert!(harness.find_text("New blank course").is_some());
    assert!(harness.find_text("Explore an example").is_some());
}

#[test]
fn test_welcome_explore_example_feature_tour_loads_with_media() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut editor = Editor::new(course);
    editor.ui.show_welcome = true;

    editor.open_example(crate::welcome::ExampleId::FeatureShowcase);

    assert!(!editor.ui.show_welcome);
    assert!(editor.ui.has_loaded_course);
    assert_eq!(editor.session.course.id, "coursecraft-feature-tour");
    assert_eq!(editor.media.len(), 10);
    assert!(editor.media.iter().any(|(src, _)| src == "done.jpg"));
    assert!(editor.media.iter().any(|(src, _)| src == "intro.mp4"));
}

#[test]
fn test_welcome_explore_example_safety_basics_loads_with_media() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut editor = Editor::new(course);
    editor.open_example(crate::welcome::ExampleId::HelloCourse);
    assert_eq!(editor.session.course.id, "workplace-safety-basics");
    assert_eq!(editor.media.len(), 5);
    assert!(editor.media.iter().any(|(src, _)| src == "workshop.jpg"));
}

#[test]
fn test_empty_slide_hint_and_buttons() {
    let course = Session::blank("Blank course").course;
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    assert!(
        harness.find_text("Add something to this slide").is_some(),
        "Empty slide must display hint"
    );
    assert!(harness.find_text("Text").is_some());
    assert!(harness.find_text("Shape").is_some());
    assert!(harness.find_text("Image").is_some());
}

#[test]
fn test_keyboard_shortcuts_dialog_opens_and_closes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    assert!(!harness.editor.ui.show_shortcuts_dialog);

    // Press ? (Shift+/)
    harness.press(Key::Slash, Modifiers::SHIFT);
    assert!(harness.editor.ui.show_shortcuts_dialog);
    assert!(harness.find_text("Keyboard shortcuts").is_some());

    // Press Escape to close
    harness.press(Key::Escape, Modifiers::NONE);
    assert!(!harness.editor.ui.show_shortcuts_dialog);
}

#[test]
fn test_coach_marks_progression_and_skip() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_coach_marks_done(false);
    harness.editor.ui.current_coach_mark = Some(1);
    harness.idle_frame();

    assert!(harness.find_text("1 of 3").is_some());
    assert!(
        harness
            .find_text("Your slides live here. Right-click a slide for more options.")
            .is_some()
    );

    // Advance to step 2
    let next_pos = harness
        .find_text_last("Next")
        .expect("Next button on coach mark 1");
    harness.click_at(next_pos);
    assert_eq!(harness.editor.ui.current_coach_mark, Some(2));
    assert!(harness.find_text("2 of 3").is_some());
    assert!(
        harness
            .find_text("Add text, shapes, media, buttons and questions.")
            .is_some()
    );

    // Skip
    let skip_pos = harness
        .find_text("Skip")
        .expect("Skip button on coach mark 2");
    harness.click_at(skip_pos);
    assert!(harness.editor.coach_marks_done());
    assert_eq!(harness.editor.ui.current_coach_mark, None);
}

#[test]
fn test_empty_states_single_primary_action() {
    fn count_exact_text(harness: &mut TestHarness, target: &str) -> usize {
        fn walk(shape: &egui::epaint::Shape, target: &str) -> usize {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == target => 1,
                egui::epaint::Shape::Vec(v) => v.iter().map(|s| walk(s, target)).sum(),
                _ => 0,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(harness.screen_rect),
            ..Default::default()
        };
        let output = harness.ctx.run_ui(raw_input, |ui| {
            let _ = harness.editor.show(ui);
        });
        let count = output.shapes.iter().map(|cs| walk(&cs.shape, target)).sum();
        output.drop_without_applying_deltas();
        count
    }

    // 1. Blank course: slide has no layers
    let blank_course = coursecraft_engine::Session::blank("Blank").course;
    let mut harness = TestHarness::new(blank_course);
    harness.editor.ui.inspector_tab = InspectorTab::Slide;
    harness.idle_frame();
    // Exactly one "+ New layer" primary action, beside the empty-state sentence.
    assert_eq!(count_exact_text(&mut harness, "+ New layer"), 1);

    // 2. Triggers panel on blank course (no triggers)
    harness.editor.ui.bottom_tab = BottomTab::Triggers;
    harness.idle_frame();
    // Exactly one "+ Add trigger" primary action
    assert_eq!(count_exact_text(&mut harness, "+ Add trigger"), 1);

    // 3. Variables panel on blank course (no variables)
    harness.editor.ui.bottom_tab = BottomTab::Variables;
    harness.idle_frame();
    // Exactly one "+ Add variable" primary action (and header "+ Add" is hidden)
    assert_eq!(count_exact_text(&mut harness, "+ Add variable"), 1);
    assert_eq!(count_exact_text(&mut harness, "+ Add"), 0);
}

#[test]
fn timeline_ruler_labels_stay_inside_the_panel() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.idle_frame();

    let panel = harness
        .editor
        .ui
        .timeline
        .panel_rect
        .expect("timeline panel");
    let labels = &harness.editor.ui.timeline.ruler_label_rects;
    assert!(!labels.is_empty(), "the time ruler must paint labels");
    for label in labels {
        assert!(
            panel.contains_rect(*label),
            "ruler label {label:?} must stay inside the timeline panel {panel:?}"
        );
    }
}

#[test]
fn shortcut_groups_never_show_empty_headings() {
    let groups = crate::shortcuts::build_shortcut_groups();
    assert!(!groups.is_empty(), "the shortcut sheet must list something");
    for group in &groups {
        assert!(
            !group.items.is_empty(),
            "group '{}' must not be shown with no rows",
            group.category
        );
    }
    assert!(
        crate::shortcuts::dialog_height(&groups, 900.0) <= 900.0 * 0.8 + 0.5,
        "the shortcut dialog is capped at 80% of the window height"
    );
}

#[test]
fn shortcut_dialog_shows_every_row_without_scrolling_at_1440x900() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1440.0, 900.0));
    harness.editor.ui.show_shortcuts_dialog = true;
    harness.idle_frame();
    harness.idle_frame();

    let groups = crate::shortcuts::build_shortcut_groups();
    let dialog_h = crate::shortcuts::dialog_height(&groups, 900.0);
    assert!(
        dialog_h <= 900.0 * 0.8 + 0.5,
        "dialog height {dialog_h} must respect the 80% cap"
    );

    for group in &groups {
        for row in &group.items {
            let (center, clip) = harness
                .find_text_clipped(row.action)
                .unwrap_or_else(|| panic!("shortcut row '{}' must be painted", row.action));
            assert!(
                clip.contains(center),
                "shortcut row '{}' must be fully visible without scrolling (center {center:?}, clip {clip:?})",
                row.action
            );
        }
    }
}

#[test]
fn question_prompt_overlap_warning_tracks_visible_text() {
    const SHOWCASE_JSON: &str = include_str!("../../../examples/showcase/course.json");
    let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-q-fi".to_owned();
    harness.editor.ui.bottom_tab = BottomTab::Question;
    harness.idle_frame();

    assert!(
        harness
            .find_text_containing("The question prompt overlaps")
            .is_none(),
        "no warning without overlapping text"
    );

    harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-q-fi",
                "name": "Overlapping Title",
                "kind": {
                    "type": "text",
                    "text": "A title in the prompt area",
                    "style": { "size": 28 }
                },
                "frame": { "x": 80, "y": 60, "w": 600, "h": 60 }
            }),
        )
        .expect("add overlapping text");
    harness.idle_frame();

    assert!(
        harness
            .find_text_containing("The question prompt overlaps \"Overlapping Title\"")
            .is_some(),
        "an overlapping visible title must show the amber warning"
    );
}

fn example_media_library_harness() -> TestHarness {
    let course = Course::from_json(include_str!("../../../examples/feature-tour/course.json"))
        .expect("valid feature tour course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .open_example(crate::welcome::ExampleId::FeatureShowcase);
    harness.editor.ui.show_media_library = true;
    harness.idle_frame();
    harness.idle_frame();
    harness
}

#[test]
fn media_library_lists_example_files_and_usage_counts() {
    let mut harness = example_media_library_harness();
    assert!(harness.find_text("Media library").is_some());
    for name in ["campus-map.jpg", "chime.vtt", "chime.wav", "done.jpg"] {
        assert!(
            harness.find_text(name).is_some(),
            "the library must list {name}"
        );
    }
    assert!(harness.find_text("Used by 1").is_some());
    assert!(harness.find_text("10 files").is_some());
}

#[test]
fn media_library_filter_narrows_the_list() {
    let mut harness = example_media_library_harness();
    harness.editor.ui.media_library.filter = "chime".to_owned();
    harness.idle_frame();
    assert!(harness.find_text("chime.wav").is_some());
    assert!(harness.find_text("done.jpg").is_none());
}

#[test]
fn media_library_selecting_a_used_file_lists_places_and_opens_one() {
    let mut harness = example_media_library_harness();
    let banner = harness.find_text("done.jpg").expect("done.jpg row");
    harness.click_at(banner);
    harness.idle_frame();
    assert_eq!(
        harness.editor.ui.media_library.selected.as_deref(),
        Some("done.jpg")
    );
    let place = harness
        .find_text_containing("Trophy picture")
        .expect("a place that uses the trophy picture");
    harness.click_at(place);
    assert_eq!(harness.editor.selection.slide, "results");
}

#[test]
fn media_library_delete_is_disabled_for_a_used_file() {
    let mut harness = example_media_library_harness();
    let banner = harness.find_text("done.jpg").expect("done.jpg row");
    harness.click_at(banner);
    harness.idle_frame();
    let delete = harness.find_text("Delete").expect("Delete button");
    harness.click_at(delete);
    harness.idle_frame();
    assert!(harness.editor.ui.media_library.confirm.is_none());
    assert!(
        harness
            .editor
            .media()
            .iter()
            .any(|(src, _)| src == "done.jpg")
    );
}

#[test]
fn media_library_delete_unused_removes_only_the_unused_file_after_confirm() {
    let mut harness = example_media_library_harness();
    let before = harness.editor.session.course.clone();
    let bytes = include_bytes!("../../../examples/feature-tour/media/hero.jpg");
    assert!(
        harness
            .editor
            .add_media_bytes("unused.png", bytes)
            .is_some()
    );
    harness.idle_frame();

    let delete_unused = harness
        .find_text("Delete unused")
        .expect("Delete unused button");
    harness.click_at(delete_unused);
    harness.idle_frame();
    assert!(
        harness.editor.ui.media_library.confirm.is_some(),
        "Delete unused must open the confirmation"
    );
    assert!(
        harness
            .find_text("Delete 1 file? This cannot be undone.")
            .is_some(),
        "the confirmation text is painted"
    );
    let confirm = harness.find_text_last("Delete").expect("confirm button");
    harness.click_at(confirm);
    harness.idle_frame();

    assert!(
        harness
            .editor
            .media()
            .iter()
            .all(|(src, _)| src != "unused.png"),
        "the unused file is removed"
    );
    assert!(
        harness
            .editor
            .media()
            .iter()
            .any(|(src, _)| src == "done.jpg"),
        "used files stay"
    );
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn media_library_insert_on_slide_adds_the_picked_image() {
    let mut harness = example_media_library_harness();
    let floorplan = harness.find_text("campus-map.jpg").expect("campus map row");
    harness.click_at(floorplan);
    harness.idle_frame();
    let insert = harness
        .find_text("Insert on slide")
        .expect("Insert on slide button");
    harness.click_at(insert);
    harness.idle_frame();

    let slide_id = harness.editor.selection.slide.clone();
    let slide = harness
        .editor
        .session
        .course
        .slide(&slide_id)
        .expect("selected slide");
    assert!(slide.objects.iter().any(|object| matches!(
        &object.kind,
        ObjectKind::Image { src, .. } if src == "campus-map.jpg"
    )));
}

#[test]
fn media_library_replace_swaps_bytes_and_keeps_the_name() {
    let mut harness = example_media_library_harness();
    let bytes = include_bytes!("../../../examples/feature-tour/media/hero.jpg");
    let src = harness
        .editor
        .add_media_bytes("replace-me.png", bytes)
        .expect("add media");
    assert!(harness.editor.replace_media_bytes(&src, b"new bytes"));
    let (name, stored) = harness
        .editor
        .media()
        .iter()
        .find(|(name, _)| name == &src)
        .expect("entry survives");
    assert_eq!(name, &src);
    assert_eq!(stored.as_ref(), b"new bytes");
}

#[test]
fn view_menu_toggles_the_media_library_window() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let view = harness.find_text("View").expect("View menu");
    harness.click_at(view);
    harness.idle_frame();
    let media_library = harness
        .find_text("Media library")
        .expect("Media library item");
    harness.click_at(media_library);
    assert!(harness.editor.ui.show_media_library);
}

#[test]
fn referenced_media_includes_caption_files() {
    let course = Course::from_json(include_str!("../../../examples/showcase/course.json"))
        .expect("valid showcase course");
    let referenced = referenced_media_srcs(&course);
    assert!(referenced.contains("intro.vtt"), "{referenced:?}");
    assert!(referenced.contains("chime.vtt"), "{referenced:?}");
}

#[test]
fn background_music_and_bank_question_media_are_kept_on_save() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course.background_audio = Some(coursecraft_course::BackgroundAudio {
        src: "music.mp3".into(),
        volume: 0.2,
        looping: true,
    });
    let mut bank_slide = course.slides().next().expect("a slide").clone();
    bank_slide.id = "bank_q1".into();
    bank_slide.objects = vec![
        serde_json::from_value(serde_json::json!({
            "id": "bank_pic", "name": "Picture",
            "kind": {"type": "image", "src": "bank-only.png", "alt": "A picture"},
            "frame": {"x": 0, "y": 0, "w": 10, "h": 10}
        }))
        .expect("image object"),
    ];
    course.banks = vec![coursecraft_course::QuestionBank {
        id: "b".into(),
        title: "Bank".into(),
        slides: vec![bank_slide],
    }];
    let referenced = referenced_media_srcs(&course);
    assert!(referenced.contains("music.mp3"), "{referenced:?}");
    assert!(referenced.contains("bank-only.png"), "{referenced:?}");
}

#[test]
fn referenced_media_and_rewrite_walk_state_fill_style_path_fill_style_and_video_poster() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    let shp: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "shp-s",
        "name": "Shape",
        "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
        "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 },
        "states": [{
            "name": "hover",
            "fill_style": { "type": "picture", "src": "state-bg.png" }
        }]
    }))
    .expect("shape");
    let path_obj: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "path-s",
        "name": "Path",
        "kind": {
            "type": "path",
            "points": [{ "x": 0.0, "y": 0.0 }, { "x": 10.0, "y": 0.0 }, { "x": 10.0, "y": 10.0 }],
            "closed": true,
            "outline": { "color": "#000000", "width": 1.0 },
            "fill_style": { "type": "picture", "src": "path-bg.png" }
        },
        "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 }
    }))
    .expect("path");
    let vid: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "vid-s",
        "name": "Video",
        "kind": { "type": "video", "src": "clip.mp4", "poster": "poster-s.png" },
        "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 }
    }))
    .expect("video");
    course.scenes[0].slides[0]
        .objects
        .extend([shp, path_obj, vid]);

    let referenced = referenced_media_srcs(&course);
    assert!(referenced.contains("state-bg.png"), "{referenced:?}");
    assert!(referenced.contains("path-bg.png"), "{referenced:?}");
    assert!(referenced.contains("poster-s.png"), "{referenced:?}");

    let mut renames = std::collections::HashMap::new();
    renames.insert("state-bg.png".to_string(), "state-bg-2.png".to_string());
    renames.insert("path-bg.png".to_string(), "path-bg-2.png".to_string());
    renames.insert("poster-s.png".to_string(), "poster-s-2.png".to_string());
    rewrite_slide_media_srcs(&mut course.scenes[0].slides[0], &renames);

    let after = referenced_media_srcs(&course);
    assert!(after.contains("state-bg-2.png"), "{after:?}");
    assert!(after.contains("path-bg-2.png"), "{after:?}");
    assert!(after.contains("poster-s-2.png"), "{after:?}");
}

#[test]
fn object_uses_font_checks_state_text_style_and_runs() {
    let obj_style: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "btn-1",
        "name": "Btn",
        "kind": { "type": "button", "label": "Go" },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 },
        "states": [{
            "name": "hover",
            "text_style": { "font": "Lora", "size": 16.0, "color": "#111111" }
        }]
    }))
    .expect("obj_style");
    let obj_run: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "btn-2",
        "name": "Btn 2",
        "kind": { "type": "button", "label": "Go" },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 },
        "states": [{
            "name": "hover",
            "runs": [{ "text": "Hi", "font": "Fira Code" }]
        }]
    }))
    .expect("obj_run");

    assert!(object_uses_font(&obj_style, &|f| f == "Lora"));
    assert!(object_uses_font(&obj_run, &|f| f == "Fira Code"));
    assert!(!object_uses_font(&obj_style, &|f| f == "Other"));
}

#[test]
fn referenced_media_and_rewrite_walk_state_extra_image_shape_and_path() {
    let mut course = coursecraft_course::Course::from_json(include_str!(
        "../../../examples/hello-course/course.json"
    ))
    .expect("valid course");
    let obj: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "btn-extra-media",
        "name": "Btn",
        "kind": { "type": "button", "label": "Go" },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 },
        "states": [{
            "name": "hover",
            "extras": [
                {
                    "id": "ex-img",
                    "kind": { "type": "image", "src": "ex-img.png" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 20.0, "h": 20.0 }
                },
                {
                    "id": "ex-shp",
                    "kind": {
                        "type": "shape",
                        "shape": "rectangle",
                        "fill": "#112233",
                        "fill_style": { "type": "picture", "src": "ex-shp.png" }
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 20.0, "h": 20.0 }
                },
                {
                    "id": "ex-path",
                    "kind": {
                        "type": "path",
                        "points": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 1.0, "y": 1.0 }],
                        "closed": true,
                        "outline": { "color": "#000000", "width": 1.0 },
                        "fill_style": { "type": "picture", "src": "ex-path.png" }
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 20.0, "h": 20.0 }
                }
            ]
        }]
    }))
    .expect("btn");
    course.scenes[0].slides[0].objects.push(obj);

    let referenced = referenced_media_srcs(&course);
    assert!(referenced.contains("ex-img.png"), "{referenced:?}");
    assert!(referenced.contains("ex-shp.png"), "{referenced:?}");
    assert!(referenced.contains("ex-path.png"), "{referenced:?}");

    let mut renames = std::collections::HashMap::new();
    renames.insert("ex-img.png".to_string(), "ex-img-2.png".to_string());
    renames.insert("ex-shp.png".to_string(), "ex-shp-2.png".to_string());
    renames.insert("ex-path.png".to_string(), "ex-path-2.png".to_string());
    rewrite_slide_media_srcs(&mut course.scenes[0].slides[0], &renames);

    let after = referenced_media_srcs(&course);
    assert!(after.contains("ex-img-2.png"), "{after:?}");
    assert!(after.contains("ex-shp-2.png"), "{after:?}");
    assert!(after.contains("ex-path-2.png"), "{after:?}");
}

#[test]
fn object_uses_font_checks_state_extra_text_and_shape_runs() {
    let obj: coursecraft_course::Object = serde_json::from_value(serde_json::json!({
        "id": "btn-extra-font",
        "name": "Btn",
        "kind": { "type": "button", "label": "Go" },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 },
        "states": [{
            "name": "hover",
            "extras": [
                {
                    "id": "ex-txt",
                    "kind": {
                        "type": "text",
                        "text": "Hi",
                        "style": { "font": "Merriweather", "size": 14.0, "color": "#111111" },
                        "runs": [{ "text": "Hi", "font": "IBM Plex Mono" }]
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 20.0 }
                },
                {
                    "id": "ex-shp",
                    "kind": {
                        "type": "shape",
                        "shape": "rectangle",
                        "fill": "#112233",
                        "text": {
                            "text": "Yo",
                            "style": { "font": "Rubik", "size": 14.0, "color": "#111111" }
                        }
                    },
                    "frame": { "x": 0.0, "y": 24.0, "w": 40.0, "h": 20.0 }
                }
            ]
        }]
    }))
    .expect("btn");

    assert!(object_uses_font(&obj, &|f| f == "Merriweather"));
    assert!(object_uses_font(&obj, &|f| f == "IBM Plex Mono"));
    assert!(object_uses_font(&obj, &|f| f == "Rubik"));
    assert!(!object_uses_font(&obj, &|f| f == "Other"));
}

fn click_for_requests(harness: &mut TestHarness, pos: egui::Pos2) -> Vec<EditorRequest> {
    harness.frame(vec![Event::PointerMoved(pos)]);
    harness.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    harness.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }])
}

fn open_publish_from_file_menu(harness: &mut TestHarness) {
    let file = harness.find_text("File").expect("File menu");
    harness.click_at(file);
    harness.idle_frame();
    let publish = harness
        .find_text_last("Publish…")
        .expect("Publish menu item");
    harness.click_at(publish);
    harness.idle_frame();
}

#[test]
fn publish_dialog_opens_from_the_file_menu() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    open_publish_from_file_menu(&mut harness);
    assert!(harness.editor.ui.publish.open, "Publish dialog opened");
    assert!(
        harness.find_text("Publish course").is_some(),
        "dialog title is painted"
    );
}

#[test]
fn publish_dialog_defaults_to_scorm12() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();
    assert_eq!(harness.editor.ui.publish.standard, PublishStandard::Scorm12);
    assert!(harness.find_text("SCORM 1.2").is_some());
    assert!(
        harness
            .find_text_containing("Works in almost every LMS")
            .is_some()
    );
}

#[test]
fn publish_validation_error_disables_publish_and_shows_count() {
    use coursecraft_course::{Action, Event as CourseEvent, Trigger};
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    let slide = &mut course.scenes[0].slides[1];
    slide.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "non-existent-layer".to_owned(),
        },
        when: CourseEvent::Click {
            object: "tab1".to_owned(),
        },
        condition: None,
        disabled: false,
    });
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    assert!(harness.editor.ui.validation_errors > 0);

    harness.editor.open_publish_dialog();
    harness.idle_frame();
    let count = harness.editor.ui.validation_errors;
    assert!(
        harness
            .find_text_containing(&format!("This course has {count} problem(s)"))
            .is_some(),
        "the blocking count is painted"
    );
    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, EditorRequest::Publish { .. })),
        "Publish must be disabled while validation errors exist"
    );
}

#[test]
fn publish_click_emits_the_chosen_standard() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();

    let cmi5 = harness.find_text_last("cmi5").expect("cmi5 radio label");
    harness.click_at(cmi5);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.publish.standard, PublishStandard::Cmi5);

    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::Publish {
                standard: PublishStandard::Cmi5
            }
        )),
        "the chosen standard is sent with the request: {requests:?}"
    );
}

#[test]
fn publish_dialog_offers_lti_and_requests_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();

    let lti = harness
        .find_text_last("LTI 1.3 (for the CourseCraft LTI server)")
        .expect("LTI radio label");
    harness.click_at(lti);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.publish.standard, PublishStandard::Lti);

    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::Publish {
                standard: PublishStandard::Lti
            }
        )),
        "the LTI standard is sent with the request: {requests:?}"
    );
}

#[test]
fn publish_dialog_offers_web_and_requests_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();

    assert!(
        harness
            .find_text_containing(
                "Upload the folder to any web server. Results are not reported to an LMS."
            )
            .is_some(),
        "the Web help text is painted"
    );

    let web = harness
        .find_text_last("Web (any website, no LMS)")
        .expect("Web radio label");
    harness.click_at(web);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.publish.standard, PublishStandard::Web);

    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::Publish {
                standard: PublishStandard::Web
            }
        )),
        "the Web standard is sent with the request: {requests:?}"
    );
}

#[test]
fn publish_unavailable_disables_publish() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_publish_available(false);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("doesn't include the player")
            .is_some()
    );
    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, EditorRequest::Publish { .. }))
    );
}

#[test]
fn publish_busy_hides_close_and_shows_spinner_text() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.editor.set_publish_busy();
    harness.idle_frame();
    assert!(harness.find_text("Publishing…").is_some());
    assert!(
        harness.find_icon(coursecraft_render::Icon::X).is_none(),
        "the busy dialog has no close button"
    );
    assert!(harness.editor.ui.publish.open);
}

#[test]
fn publish_success_shows_the_file_name_and_size() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness
        .editor
        .set_publish_success("My Course-scorm12.zip", 1_500_000);
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("Published to My Course-scorm12.zip")
            .is_some()
    );
    assert!(harness.find_text("1.4 MB").is_some());
    assert!(harness.find_text("Show in folder").is_some());
    assert!(harness.find_text("Done").is_some());
    assert!(
        harness
            .find_text_containing("Upload this zip to your LMS as a new course.")
            .is_some()
    );
}

#[test]
fn missing_media_is_empty_when_every_referenced_file_is_loaded() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course
        .slide_mut("s-intro")
        .expect("s-intro")
        .objects
        .push(image_object("photo.png"));
    let mut harness = TestHarness::new(course);
    harness.editor.set_media(vec![(
        "photo.png".to_owned(),
        std::sync::Arc::<[u8]>::from(vec![1_u8, 2, 3]),
    )]);
    assert!(harness.editor.missing_media().is_empty());
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();
    assert!(
        harness.find_text_containing("Missing media:").is_none(),
        "no false alarm when the media store has the file"
    );
}

#[test]
fn missing_media_blocks_publishing_and_is_listed_in_danger_colour() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course
        .slide_mut("s-intro")
        .expect("s-intro")
        .objects
        .push(image_object("placeholder.png"));
    let mut harness = TestHarness::new(course);
    assert_eq!(harness.editor.missing_media(), vec!["placeholder.png"]);
    harness.idle_frame();
    harness.editor.open_publish_dialog();
    harness.idle_frame();
    assert!(
        harness
            .find_text("Missing media: placeholder.png")
            .is_some(),
        "the missing file is listed"
    );
    let publish = harness.find_text_last("Publish").expect("Publish button");
    let requests = click_for_requests(&mut harness, publish);
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request, EditorRequest::Publish { .. })),
        "Publish must stay disabled while media is missing"
    );
}

fn image_object(src: &str) -> Object {
    Object {
        id: format!("image-{}", src.replace('.', "-")),
        name: String::new(),
        kind: ObjectKind::Image {
            src: src.to_owned(),
            alt: "Picture".to_owned(),
            crop: None,
            outline: None,
            shape: None,
        },
        frame: CourseRect {
            x: 600.0,
            y: 10.0,
            w: 120.0,
            h: 120.0,
        },
        timing: Default::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    }
}

#[test]
fn accessibility_section_renders_and_a_click_selects_the_object() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    course
        .slide_mut("s-intro")
        .expect("s-intro")
        .objects
        .push(Object {
            id: "audit-photo".to_owned(),
            name: String::new(),
            kind: ObjectKind::Image {
                src: "photo.png".to_owned(),
                alt: String::new(),
                crop: None,
                outline: None,
                shape: None,
            },
            frame: CourseRect {
                x: 600.0,
                y: 10.0,
                w: 120.0,
                h: 120.0,
            },
            timing: Default::default(),
            states: Vec::new(),
            initial_state: "normal".to_owned(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            emphasis: Vec::new(),
            focus_order: None,
            decorative: false,
            description: None,
            group: None,
            panel: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            opacity: 1.0,
            shadow: None,
            glow: None,
            drag: None,
            locked: false,
            editor_hidden: false,
        });

    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.ui.show_validation_window = true;
    harness.idle_frame();

    assert!(
        harness
            .editor
            .ui
            .audit_findings
            .iter()
            .any(|f| f.check == "alt-text" && f.object.as_deref() == Some("audit-photo")),
        "the missing alt text finding is loaded"
    );
    assert_eq!(harness.editor.ui.validation_errors, 0);

    harness.idle_frame();
    assert!(
        harness.find_text("Accessibility").is_some(),
        "the accessibility section header is painted"
    );
    let position = harness
        .find_text_containing("has no alt text")
        .expect("finding button is painted");

    harness.click_at(position);
    assert_eq!(harness.editor.selection.slide, "s-intro");
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some("audit-photo"),
        "clicking the finding selects its object"
    );
}

fn course_with_loaded_image(src: &str) -> (TestHarness, String) {
    course_with_loaded_image_bytes(src, test_png_bytes())
}

fn course_with_loaded_image_bytes(src: &str, bytes: Vec<u8>) -> (TestHarness, String) {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    let object = image_object(src);
    let id = object.id.clone();
    course
        .slide_mut("s-intro")
        .expect("s-intro")
        .objects
        .push(object);
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .set_media(vec![(src.to_owned(), std::sync::Arc::<[u8]>::from(bytes))]);
    harness.editor.selection.slide = "s-intro".into();
    // Two frames: the first paints and asks for the picture, the second finds
    // the uploaded texture.
    harness.idle_frame();
    harness.idle_frame();
    (harness, id)
}

/// A 4x2 (2:1) picture, so a square frame letterboxes it when uncropped.
fn test_wide_png_bytes() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(4 * 2 * 4);
    for i in 0..8u8 {
        pixels.extend_from_slice(&[i * 30, 255 - i * 30, 128, 255]);
    }
    coursecraft_media::encode_png_rgba(4, 2, &pixels).expect("PNG encoding failed")
}

fn stored_image_frame(harness: &TestHarness, id: &str) -> coursecraft_course::Rect {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .expect("image")
        .0
        .frame
}

fn image_center(harness: &TestHarness, id: &str) -> egui::Pos2 {
    object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        id,
        harness.central_panel_rect(),
    )
    .expect("image rect")
    .center()
}

#[test]
fn double_clicking_an_image_enters_crop_mode() {
    let (mut harness, id) = course_with_loaded_image("photo.png");
    assert!(harness.editor.loaded_image_size("photo.png").is_some());
    let center = image_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    let session = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .expect("double click enters crop mode");
    assert_eq!(session.object, id);
    assert_eq!(session.slide, "s-intro");
}

#[test]
fn a_missing_image_never_enters_crop_mode() {
    let (mut harness, id) = course_with_loaded_image("photo.png");
    harness.editor.set_media(Vec::new());
    harness.editor.media_cache = coursecraft_render::MediaCache::new();
    harness.idle_frame();
    let center = image_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.crop_edit.is_none());
}

#[test]
fn crop_mode_enter_applies_one_undo_step() {
    let (mut harness, id) = course_with_loaded_image("photo.png");
    let center = image_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    let before = harness.editor.session.course.clone();
    let revision = harness.editor.session.revision;

    let edit = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .expect("crop mode")
        .edit;
    harness
        .editor
        .ui
        .crop_edit
        .as_mut()
        .expect("crop mode")
        .edit = edit.drag_handle(
        crate::crop::CropHandle::BottomRight,
        vec2(-20.0, -20.0),
        false,
    );
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();

    assert!(harness.editor.ui.crop_edit.is_none(), "Enter applies");
    assert_eq!(
        harness.editor.session.revision,
        revision + 1,
        "apply is one undo step"
    );
    let cropped = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image");
    match &cropped.0.kind {
        ObjectKind::Image {
            crop: Some(crop), ..
        } => {
            assert!(crop.w < 1.0 && crop.h < 1.0, "{crop:?}");
        }
        other => panic!("expected a crop, got {other:?}"),
    }

    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn entering_crop_mode_on_a_letterboxed_image_uses_the_fitted_frame() {
    let (mut harness, id) = course_with_loaded_image_bytes("wide.png", test_wide_png_bytes());
    assert_eq!(
        harness.editor.loaded_image_size("wide.png"),
        Some((4.0, 2.0))
    );
    let stored = stored_image_frame(&harness, &id);

    assert!(harness.editor.start_crop(&id));
    let session = harness.editor.ui.crop_edit.as_ref().expect("crop mode");
    // The 4x2 picture is drawn 120x60, centred in the 120x120 object frame.
    assert!((session.edit.frame.width() - stored.w).abs() < 1e-3);
    assert!((session.edit.frame.height() - stored.w * 0.5).abs() < 1e-3);
    assert!((session.edit.frame.center().x - (stored.x + stored.w * 0.5)).abs() < 1e-3);
    assert!((session.edit.frame.center().y - (stored.y + stored.h * 0.5)).abs() < 1e-3);
    assert_eq!(session.edit.crop, crate::crop::CropEdit::full_crop());
}

#[test]
fn entering_then_enter_on_a_letterboxed_image_writes_nothing() {
    let (mut harness, id) = course_with_loaded_image_bytes("wide.png", test_wide_png_bytes());
    let before_course = harness.editor.session.course.clone();
    let before_frame = stored_image_frame(&harness, &id);
    let revision = harness.editor.session.revision;

    assert!(harness.editor.start_crop(&id));
    assert!(harness.editor.ui.crop_edit.is_some());
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();

    assert!(
        harness.editor.ui.crop_edit.is_none(),
        "Enter applies and leaves crop mode"
    );
    assert_eq!(
        harness.editor.session.revision, revision,
        "an untouched crop adds no undo step"
    );
    assert_eq!(harness.editor.session.course, before_course);
    assert_eq!(
        stored_image_frame(&harness, &id),
        before_frame,
        "the fitted rectangle is not written back as the stored frame"
    );
}

#[test]
fn undo_after_applying_a_letterboxed_crop_restores_the_crop_and_frame() {
    let (mut harness, id) = course_with_loaded_image_bytes("wide.png", test_wide_png_bytes());
    let before = harness.editor.session.course.clone();
    let stored = stored_image_frame(&harness, &id);

    assert!(harness.editor.start_crop(&id));
    let edit = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .expect("crop mode")
        .edit;
    harness
        .editor
        .ui
        .crop_edit
        .as_mut()
        .expect("crop mode")
        .edit = edit.drag_handle(crate::crop::CropHandle::Right, vec2(-30.0, 0.0), false);
    let working = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .expect("crop mode")
        .edit;
    let working_crop = working.crop_model();
    let working_frame = working.frame;
    let revision = harness.editor.session.revision;

    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();

    assert_eq!(harness.editor.session.revision, revision + 1);
    let after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image")
        .0
        .clone();
    match &after.kind {
        ObjectKind::Image {
            crop: Some(crop), ..
        } => {
            assert!((crop.x - working_crop.x).abs() < 1e-5, "{crop:?}");
            assert!((crop.y - working_crop.y).abs() < 1e-5, "{crop:?}");
            assert!((crop.w - working_crop.w).abs() < 1e-5, "{crop:?}");
            assert!((crop.h - working_crop.h).abs() < 1e-5, "{crop:?}");
        }
        other => panic!("expected a crop, got {other:?}"),
    }
    assert!((after.frame.x - working_frame.min.x).abs() < 1e-4);
    assert!((after.frame.y - working_frame.min.y).abs() < 1e-4);
    assert!((after.frame.w - working_frame.width()).abs() < 1e-4);
    assert!((after.frame.h - working_frame.height()).abs() < 1e-4);

    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(harness.editor.session.course, before);
    assert_eq!(stored_image_frame(&harness, &id), stored);
}

#[test]
fn crop_mode_escape_cancels_without_a_change() {
    let (mut harness, id) = course_with_loaded_image("photo.png");
    let center = image_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    let before = harness.editor.session.course.clone();
    let revision = harness.editor.session.revision;

    let edit = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .expect("crop mode")
        .edit;
    harness
        .editor
        .ui
        .crop_edit
        .as_mut()
        .expect("crop mode")
        .edit = edit.drag_handle(crate::crop::CropHandle::Left, vec2(20.0, 10.0), false);
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();

    assert!(harness.editor.ui.crop_edit.is_none());
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(id.as_str()),
        "Escape keeps the object selected"
    );
    assert_eq!(harness.editor.session.revision, revision);
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn nudges_do_not_move_the_object_while_cropping() {
    let (mut harness, id) = course_with_loaded_image("photo.png");
    let center = image_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image")
        .0
        .frame;

    harness.press(Key::ArrowRight, Modifiers::NONE);
    harness.press(Key::ArrowDown, Modifiers::SHIFT);
    harness.idle_frame();

    assert!(harness.editor.ui.crop_edit.is_some());
    let after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image")
        .0
        .frame;
    assert_eq!(after, before, "canvas nudges are off while cropping");
}

#[test]
fn inspector_reset_clears_the_crop_and_restores_the_scale() {
    let mut course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut object = image_object("photo.png");
    object.frame = CourseRect {
        x: 600.0,
        y: 10.0,
        w: 60.0,
        h: 120.0,
    };
    object.kind = ObjectKind::Image {
        src: "photo.png".to_owned(),
        alt: "Picture".to_owned(),
        crop: Some(coursecraft_course::CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        }),
        outline: None,
        shape: None,
    };
    let id = object.id.clone();
    course
        .slide_mut("s-intro")
        .expect("s-intro")
        .objects
        .push(object);
    let mut harness = TestHarness::new(course);
    harness.editor.set_media(vec![(
        "photo.png".to_owned(),
        std::sync::Arc::<[u8]>::from(test_png_bytes()),
    )]);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();
    harness.idle_frame();

    let reset = harness
        .find_text_in("Reset", harness.right_panel_rect())
        .expect("Reset button in the image inspector");
    harness.click_at(reset);
    harness.idle_frame();

    let after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image");
    match &after.0.kind {
        ObjectKind::Image { crop, .. } => assert_eq!(*crop, None),
        other => panic!("expected an image, got {other:?}"),
    }
    assert!(
        (after.0.frame.w - 120.0).abs() < 1e-3 && (after.0.frame.h - 120.0).abs() < 1e-3,
        "the frame grows to show the whole source at the same scale: {:?}",
        after.0.frame
    );

    harness.editor.run("edit.undo", serde_json::json!({}));
    let restored = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .expect("image");
    assert!(matches!(
        &restored.0.kind,
        ObjectKind::Image { crop: Some(_), .. }
    ));
}

#[test]
fn toolbar_question_popup_inserts_and_opens_question_tab() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let toolbar_rect = Rect::from_min_max(
        pos2(0.0, TOP_BAR_HEIGHT),
        pos2(
            harness.screen_rect.max.x,
            TOP_BAR_HEIGHT + crate::toolbar::TOOLBAR_HEIGHT,
        ),
    );
    let question_btn = harness
        .find_text_in("Question", toolbar_rect)
        .expect("Question toolbar button");
    harness.click_at(question_btn);
    harness.idle_frame();
    for name in [
        "Multiple choice",
        "Multiple response",
        "True / false",
        "Fill in",
        "Sequence",
        "Matching",
        "Hotspot",
        "Drag and drop",
    ] {
        assert!(
            harness.find_text(name).is_some(),
            "question popup is missing {name}"
        );
    }
    let kind = harness
        .find_text("Multiple choice")
        .expect("Multiple choice popup item");
    harness.click_at(kind);
    harness.idle_frame();

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.question.is_some());
    assert_eq!(harness.editor.ui.bottom_tab, BottomTab::Question);
    assert!(slide.objects.iter().any(|object| {
        matches!(&object.kind, ObjectKind::Button { label, .. } if label == "Submit")
    }));
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action == coursecraft_course::Action::SubmitQuestion
            && matches!(trigger.when, coursecraft_course::Event::Click { .. })
    }));

    // A slide that already has a question just opens the panel.
    harness.editor.ui.bottom_tab = BottomTab::Timeline;
    let question_btn = harness
        .find_text_in("Question", toolbar_rect)
        .expect("Question toolbar button");
    harness.click_at(question_btn);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.bottom_tab, BottomTab::Question);
}

#[test]
fn placing_a_text_box_enters_inline_edit_and_selects_the_placeholder() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.ui.placement_tool = Some(PlacementTool::TextBox);
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    harness.click_at(transform.slide_rect().center());
    harness.idle_frame();
    let edit = harness
        .editor
        .ui
        .inline_edit
        .as_ref()
        .expect("inline edit after placing text");
    assert_eq!(edit.buffer, "Text");
    harness.type_text("Typed");
    harness.press(Key::Enter, Modifiers::COMMAND);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_none());
    let texts: Vec<String> = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .map(|slide| {
            slide
                .objects
                .iter()
                .filter_map(|object| match &object.kind {
                    ObjectKind::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        texts.iter().any(|text| text == "Typed"),
        "text objects: {texts:?}"
    );
}

#[test]
fn inline_edit_commits_with_command_enter() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    let original = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("title"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_owned();

    let center = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "title",
        harness.central_panel_rect(),
    )
    .expect("title rect")
    .center();
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(
        harness.editor.ui.inline_edit.is_some(),
        "double click enters inline edit"
    );
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("Edited title");
    harness.press(Key::Enter, Modifiers::COMMAND);
    harness.idle_frame();
    let edited = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("title"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_owned();
    assert_eq!(edited, "Edited title");
    assert!(harness.editor.ui.inline_edit.is_none());

    harness.editor.run("edit.undo", serde_json::json!({}));
    let restored = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("title"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_owned();
    assert_eq!(restored, original);
}

#[test]
fn double_click_edits_shape_text_and_escape_cancels_with_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    let result = harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-intro",
            "kind": {
                "type": "shape",
                "shape": "rectangle",
                "fill": "#D0D7DE"
            },
            "frame": { "x": 300, "y": 300, "w": 220, "h": 120 }
        }),
    );
    let id = result
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
        .expect("shape id")
        .to_owned();
    harness.idle_frame();
    let center = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        &id,
        harness.central_panel_rect(),
    )
    .expect("shape rect")
    .center();

    let revision = harness.editor.session.revision;

    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(
        harness.editor.ui.inline_edit.is_some(),
        "double click enters shape text editing"
    );
    harness.type_text("Inside the shape");
    harness.press(Key::Enter, Modifiers::COMMAND);
    harness.idle_frame();

    let text = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Shape { text, .. } => text.as_ref().map(|text| text.text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(text, "Inside the shape");
    assert!(harness.editor.ui.inline_edit.is_none());

    harness.editor.run("edit.undo", serde_json::json!({}));
    let undone = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Shape { text, .. } => text.as_ref().map(|text| text.text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(undone.is_empty(), "one undo removes the whole text edit");
    assert!(harness.editor.session.revision > revision);
    let after_undo = harness.editor.session.course.clone();
    let after_undo_revision = harness.editor.session.revision;

    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.ui.inline_edit.is_some(),
        "Enter edits the selected shape"
    );
    harness.type_text("Never committed");
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_none());
    assert_eq!(harness.editor.session.course, after_undo);
    assert_eq!(harness.editor.session.revision, after_undo_revision);
}

#[test]
fn inline_edit_frame_is_transparent_with_a_thin_accent_outline() {
    let accent = egui::Color32::from_rgb(0x0A, 0x6C, 0xFF);
    let frame = crate::canvas::inline_edit_frame(accent);
    assert_eq!(frame.fill, egui::Color32::TRANSPARENT);
    assert_eq!(frame.stroke.color, accent);
    assert!(frame.stroke.width > 0.0 && frame.stroke.width <= 2.0);
}

#[test]
fn inline_edit_caret_and_selection_use_the_object_text_colour() {
    let accent = egui::Color32::from_rgb(0x0A, 0x6C, 0xFF);
    let text = egui::Color32::from_rgb(0x1F, 0x1F, 0x1F);
    let visuals = crate::canvas::inline_edit_visuals(egui::Visuals::default(), text, accent);
    assert_eq!(visuals.text_cursor.stroke.color, text);
    assert_eq!(visuals.selection.stroke.color, text);
    assert_eq!(visuals.selection.bg_fill.a(), 140);
}

#[test]
fn inline_edit_paints_no_dark_input_fill_behind_the_text() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    let center = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "title",
        harness.central_panel_rect(),
    )
    .expect("title rect")
    .center();
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_some());
    // The editor still shows the text; the frame is transparent, so no dark
    // input-coloured rectangle is painted at its centre.
    let input = coursecraft_render::palette(&harness.ctx).input;
    let raw_input = RawInput {
        screen_rect: Some(harness.screen_rect),
        ..Default::default()
    };
    let output = harness.ctx.run_ui(raw_input, |ui| {
        let _ = harness.editor.show(ui);
    });
    fn walk(shape: &egui::epaint::Shape, center: egui::Pos2, input: egui::Color32) -> bool {
        match shape {
            egui::epaint::Shape::Rect(rect) => rect.rect.contains(center) && rect.fill == input,
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|s| walk(s, center, input)),
            _ => false,
        }
    }
    assert!(
        !output
            .shapes
            .iter()
            .any(|shape| walk(&shape.shape, center, input)),
        "the in-place editor must not paint the dark input fill"
    );
    output.drop_without_applying_deltas();
}

#[test]
fn inline_edit_cancels_with_escape() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    let original = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("title"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_owned();
    let center = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "title",
        harness.central_panel_rect(),
    )
    .expect("title rect")
    .center();
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_some());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("Discarded");
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_none());
    let after_cancel = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("title"))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or("")
        .to_owned();
    assert_eq!(after_cancel, original);
}

#[test]
fn question_panel_add_submit_button_replaces_the_dead_trigger() {
    use coursecraft_course::{Action, Event};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness
        .editor
        .run(
            "question.set",
            serde_json::json!({
                "slide": "s-intro",
                "question": {
                    "kind": "multiple_choice",
                    "prompt": "Q?",
                    "choices": [
                        { "id": "a", "text": "A", "correct": true },
                        { "id": "b", "text": "B" }
                    ]
                }
            }),
        )
        .expect("question.set");
    harness.editor.ui.bottom_tab = BottomTab::Question;
    harness.idle_frame();
    harness.idle_frame();
    let add = harness
        .find_text("Add Submit button")
        .expect("Add Submit button");
    harness.click_at(add);
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(!slide.triggers.iter().any(|trigger| {
        trigger.action == Action::SubmitQuestion && trigger.when == Event::Submit
    }));
    assert!(slide.triggers.iter().any(|trigger| {
        trigger.action == Action::SubmitQuestion && matches!(trigger.when, Event::Click { .. })
    }));
}

#[test]
fn hotspot_panel_asks_for_an_image_that_is_not_in_media_and_opens_the_picker() {
    use coursecraft_engine::Session;

    let mut session = Session::blank("Hotspot");
    session
        .execute(
            "question.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "hotspot" }),
        )
        .expect("question.insert");
    let image = session
        .course
        .slide("slide1")
        .and_then(|slide| slide.question.as_ref())
        .and_then(|question| question.hotspot_image.clone())
        .expect("hotspot image");
    let mut harness = TestHarness::new(session.course);
    harness.editor.ui.bottom_tab = BottomTab::Question;
    for _ in 0..3 {
        harness.idle_frame();
    }
    assert!(
        harness
            .find_text_containing("Choose the image learners will click on.")
            .is_some(),
        "the panel must ask for the hotspot image"
    );
    let choose = harness
        .find_text("Choose image…")
        .expect("Choose image button");
    let requests = click_for_requests(&mut harness, choose);
    assert_eq!(
        harness.editor.pending_image_pick,
        Some(PendingImagePick::ReplaceObject(image.clone()))
    );
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::PickImage)),
        "{requests:?}"
    );

    harness.editor.set_media(vec![(
        "placeholder.png".to_owned(),
        std::sync::Arc::<[u8]>::from(Vec::<u8>::new()),
    )]);
    for _ in 0..3 {
        harness.idle_frame();
    }
    assert!(
        harness
            .find_text_containing("Choose the image learners will click on.")
            .is_none(),
        "a file present in the media store must clear the warning"
    );
}

#[test]
fn course_settings_offers_to_report_the_score_when_missing() {
    use coursecraft_course::Action;

    let mut session = coursecraft_engine::Session::blank("Quiz");
    session
        .execute("question.insert", &serde_json::json!({ "slide": "slide1" }))
        .expect("question.insert");
    let mut harness = TestHarness::new(session.course);
    // The window grew with the player options; give it room so nothing scrolls.
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    for _ in 0..5 {
        harness.idle_frame();
    }
    let button = harness
        .find_text("Add Finish button")
        .expect("Add Finish button");
    harness.click_at(button);
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("slide1")
        .expect("slide1");
    assert!(slide.objects.iter().any(|object| object.name == "Finish"));
    assert!(
        slide
            .triggers
            .iter()
            .any(|trigger| trigger.action == Action::SubmitResults)
    );
    assert!(
        slide
            .triggers
            .iter()
            .any(|trigger| trigger.action == Action::ExitCourse)
    );
}

#[test]
fn finish_button_reports_the_score_after_the_answer_then_exits() {
    use coursecraft_course::CompletionStatus;
    use coursecraft_logic::{Effect, Input, Player};

    let mut session = coursecraft_engine::Session::blank("Quiz");
    let submit = session
        .execute("question.insert", &serde_json::json!({ "slide": "slide1" }))
        .expect("question.insert")["button"]
        .as_str()
        .expect("submit button")
        .to_owned();
    let finish = session
        .execute("course.add_finish_button", &serde_json::json!({}))
        .expect("course.add_finish_button")["button"]
        .as_str()
        .expect("finish button")
        .to_owned();

    let mut player = Player::new(&session.course).expect("valid player");
    player.handle(&Input::Start);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    let answer = player.handle(&Input::Click { object: submit });
    assert!(
        answer
            .iter()
            .any(|effect| matches!(effect, Effect::QuestionAnswered { correct: true, .. })),
        "{answer:?}"
    );

    let effects = player.handle(&Input::Click { object: finish });
    let score = effects
        .iter()
        .position(|effect| matches!(effect, Effect::Score { percent: 100, .. }))
        .expect("score effect");
    let completion = effects
        .iter()
        .position(|effect| {
            matches!(
                effect,
                Effect::Completion {
                    status: CompletionStatus::Completed
                }
            )
        })
        .expect("completion effect");
    let exit = effects
        .iter()
        .position(|effect| matches!(effect, Effect::Exit))
        .expect("exit effect");
    assert!(score < exit && completion < exit, "{effects:?}");
}

#[test]
fn finish_button_completes_a_course_without_questions() {
    use coursecraft_course::CompletionStatus;
    use coursecraft_logic::{Effect, Input, Player};

    let mut session = coursecraft_engine::Session::blank("No quiz");
    let finish = session
        .execute("course.add_finish_button", &serde_json::json!({}))
        .expect("course.add_finish_button")["button"]
        .as_str()
        .expect("finish button")
        .to_owned();
    let mut player = Player::new(&session.course).expect("valid player");
    player.handle(&Input::Start);
    let effects = player.handle(&Input::Click { object: finish });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Completion {
                status: CompletionStatus::Completed
            }
        )),
        "{effects:?}"
    );
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Exit)));
}

#[test]
fn default_drag_drop_question_grades_a_drop_on_its_target() {
    use coursecraft_logic::{Effect, Input, Player};

    let mut session = coursecraft_engine::Session::blank("Drag");
    let submit = session
        .execute(
            "question.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "drag_drop" }),
        )
        .expect("question.insert")["button"]
        .as_str()
        .expect("submit button")
        .to_owned();
    let slide = session.course.slide("slide1").expect("slide");
    let question = slide.question.as_ref().expect("question");
    let item = question.drag_items.first().expect("drag item").item.clone();
    let target = question.drop_targets.first().expect("drop target").clone();
    let (item_object, _) = slide.object(&item).expect("item object");
    let x = item_object.frame.x;
    let y = item_object.frame.y;

    let mut player = Player::new(&session.course).expect("valid player");
    player.handle(&Input::Start);
    let dropped = player.handle(&Input::DragDropped {
        item,
        target: Some(target),
        x,
        y,
    });
    assert!(
        dropped
            .iter()
            .any(|effect| matches!(effect, Effect::ItemDropped { .. })),
        "{dropped:?}"
    );
    let effects = player.handle(&Input::Click { object: submit });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::QuestionAnswered { correct: true, .. })),
        "{effects:?}"
    );
}

#[test]
fn new_slide_from_the_ui_is_numbered_after_the_course_count() {
    let mut session = coursecraft_engine::Session::blank("Slides");
    session
        .execute(
            "slide.add",
            &serde_json::json!({ "scene": "scene1", "title": "Second" }),
        )
        .expect("second slide");
    let mut harness = TestHarness::new(session.course);
    harness.editor.selection.slide = "slide1".into();
    harness.idle_frame();
    crate::toolbar::add_new_slide(&mut harness.editor);
    let titles: Vec<String> = harness
        .editor
        .session
        .course
        .slides()
        .map(|slide| slide.title.clone())
        .collect();
    assert!(
        titles.iter().any(|title| title == "Slide 3"),
        "new slide title: {titles:?}"
    );
}

#[test]
fn scene_header_renames_and_delete_confirms_slides() {
    let mut session = coursecraft_engine::Session::blank("Scenes");
    let second = session
        .execute("scene.add", &serde_json::json!({ "title": "Second" }))
        .expect("scene.add")["id"]
        .as_str()
        .expect("scene id")
        .to_owned();
    session
        .execute(
            "slide.add",
            &serde_json::json!({ "scene": second, "title": "Second slide" }),
        )
        .expect("slide.add");
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();

    let left = harness.left_panel_rect();
    let header = harness
        .find_text_in("Scene 1", left)
        .expect("first scene header");
    harness.click_at(header);
    harness.click_at(header);
    harness.idle_frame();
    assert!(harness.editor.ui.scene_rename.is_some());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("Renamed");
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .scenes
            .first()
            .map(|s| s.title.as_str()),
        Some("Renamed")
    );

    let left = harness.left_panel_rect();
    let header = harness
        .find_text_in("Renamed", left)
        .expect("renamed scene header");
    harness.right_click_at(header);
    harness.idle_frame();
    let delete = harness
        .find_text("Delete scene")
        .expect("Delete scene menu item");
    harness.click_at(delete);
    harness.idle_frame();
    assert!(harness.editor.ui.pending_delete_scene.is_some());
    let confirm = harness
        .find_text_last("Delete scene")
        .expect("confirmation button");
    harness.click_at(confirm);
    harness.idle_frame();
    assert_eq!(harness.editor.session.course.scenes.len(), 1);
    assert_eq!(
        harness
            .editor
            .session
            .course
            .scenes
            .first()
            .map(|s| s.id.as_str()),
        Some(second.as_str())
    );
}

#[test]
fn scene_header_menu_adds_a_slide_to_that_scene() {
    let mut session = coursecraft_engine::Session::blank("Scenes");
    session
        .execute("scene.add", &serde_json::json!({ "title": "Second" }))
        .expect("scene.add");
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();

    let left = harness.left_panel_rect();
    let header = harness
        .find_text_in("Second", left)
        .expect("second scene header");
    let before = harness.editor.session.course.slides().count();
    harness.right_click_at(header);
    harness.idle_frame();
    let new_slide = harness
        .find_text_last("New slide")
        .expect("New slide menu item");
    harness.click_at(new_slide);
    harness.idle_frame();

    assert_eq!(
        harness.editor.session.course.slides().count(),
        before + 1,
        "the scene menu must add a slide"
    );
    assert_eq!(
        harness
            .editor
            .session
            .course
            .scenes
            .last()
            .map(|scene| scene.slides.len()),
        Some(1),
        "the new slide belongs to the right-clicked scene"
    );
}

#[test]
fn slides_list_empty_space_menu_adds_a_slide() {
    let mut harness = TestHarness::new(coursecraft_engine::Session::blank("Empty").course);
    harness.idle_frame();
    let left = harness.left_panel_rect();
    let before = harness.editor.session.course.slides().count();
    harness.right_click_at(pos2(left.center().x, left.max.y - 60.0));
    harness.idle_frame();
    let new_slide = harness
        .find_text_last("New slide")
        .expect("New slide menu item");
    harness.click_at(new_slide);
    harness.idle_frame();
    assert_eq!(harness.editor.session.course.slides().count(), before + 1);
}

#[test]
fn course_inspector_title_commits_through_course_update() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.inspector_tab = InspectorTab::Course;
    harness.idle_frame();
    let title_pos = harness
        .find_text_in("Title", harness.right_panel_rect())
        .expect("Title label")
        + vec2(130.0, 0.0);
    harness.set_field(title_pos, "Edited Course");
    assert_eq!(harness.editor.session.course.title, "Edited Course");
}

#[test]
fn escape_closes_preview_unless_the_drawer_is_open() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    harness.editor.open_preview();
    harness.idle_frame();
    assert!(harness.editor.preview.is_some());
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_none());

    harness.editor.open_preview();
    if let Some(preview) = harness.editor.preview.as_mut() {
        preview.frame.open_drawer();
    }
    harness.idle_frame();
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "the outline drawer takes the first Escape"
    );
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|preview| !preview.frame.drawer_open())
    );
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_none());
}

#[test]
fn bottom_panel_max_leaves_200px_of_canvas_at_1024x768() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1024.0, 768.0));
    for _ in 0..3 {
        harness.idle_frame();
    }
    let rect = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    harness.drag_by(
        pos2(500.0, rect.min.y + 2.0),
        vec2(0.0, -900.0),
        4,
        Modifiers::NONE,
    );
    harness.idle_frame();
    let grown = harness.editor.ui.timeline.panel_rect.expect("panel rect");
    let canvas_height = harness.bottom_panel_top() - harness.top_bars_height();
    assert!(
        canvas_height >= 200.0 - 1.0,
        "canvas is {canvas_height} px tall, panel {grown:?}"
    );
    assert!((harness.editor.ui.timeline.panel_height - grown.height()).abs() < 0.5);
}

#[test]
fn every_menu_command_has_a_route() {
    for command in coursecraft_engine::registry() {
        if !command.menu.is_empty() {
            assert!(
                menu_route(command.id).is_some(),
                "{} ({}) has no menu route",
                command.id,
                command.menu
            );
        }
    }
    let delete_variable = coursecraft_engine::registry()
        .iter()
        .find(|command| command.id == "variable.delete")
        .expect("variable.delete");
    assert!(
        delete_variable.menu.is_empty(),
        "Delete Variable lives in the Variables tab, not a permanently greyed menu item"
    );
}

#[test]
fn menu_routes_open_the_variables_tab_and_course_settings() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let project = harness.find_text("Project").expect("Project menu");
    harness.click_at(project);
    harness.idle_frame();
    let variables = harness
        .find_text_last("Variables")
        .expect("Variables menu item");
    harness.click_at(variables);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.bottom_tab, BottomTab::Variables);

    let project = harness.find_text("Project").expect("Project menu");
    harness.click_at(project);
    harness.idle_frame();
    let properties = harness
        .find_text_last("Course settings…")
        .expect("Course settings menu item");
    harness.click_at(properties);
    harness.idle_frame();
    assert!(harness.editor.ui.show_course_settings_window);
}

#[test]
fn preview_from_start_reaches_the_course_complete_end_card() {
    let mut session = coursecraft_engine::Session::blank("Ends");
    session
        .execute(
            "trigger.add",
            &serde_json::json!({
                "slide": "slide1",
                "trigger": {
                    "action": { "action": "exit_course" },
                    "when": { "event": "timeline_starts" }
                }
            }),
        )
        .expect("exit trigger");
    let mut harness = TestHarness::new(session.course);
    harness.editor.open_preview_at(None);
    harness.idle_frame();
    let preview = harness.editor.preview.as_ref().expect("preview");
    assert!(preview.course.start_slide.is_none());
    assert!(preview.ended, "the course exits on its first slide");
    assert!(preview.frame.ended);
    assert!(preview.frame.ended_card_rect().is_some());
}

#[test]
fn edit_menu_delete_scene_opens_the_confirmation() {
    let mut session = coursecraft_engine::Session::blank("Scenes");
    session
        .execute("scene.add", &serde_json::json!({ "title": "Second" }))
        .expect("scene.add");
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    let edit = harness.find_text("Edit").expect("Edit menu");
    harness.click_at(edit);
    harness.idle_frame();
    let delete = harness
        .find_text_last("Delete scene")
        .expect("Delete scene menu item");
    harness.click_at(delete);
    harness.idle_frame();
    assert!(harness.editor.ui.pending_delete_scene.is_some());
}

fn object_frame(harness: &TestHarness, slide: &str, object: &str) -> CourseRect {
    harness
        .editor
        .session
        .course
        .slide(slide)
        .and_then(|slide| slide.object(object))
        .map(|(object, _)| object.frame)
        .expect("object frame")
}

#[test]
fn shift_click_toggles_objects_on_the_canvas() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let title = object_screen_rect(&harness.editor.session.course, "s-intro", "title", central)
        .expect("title")
        .center();
    let start = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start")
    .center();

    harness.click_at(title);
    assert_eq!(harness.editor.selection.ids(), vec!["title"]);
    harness.click_at_with(start, Modifiers::SHIFT);
    assert_eq!(harness.editor.selection.ids(), vec!["title", "btn-start"]);
    harness.click_at_with(title, Modifiers::SHIFT);
    assert_eq!(harness.editor.selection.ids(), vec!["btn-start"]);
    harness.click_at_with(start, Modifiers::COMMAND);
    assert!(harness.editor.selection.ids().is_empty());
}

#[test]
fn marquee_selects_objects_it_touches() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let start_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start");
    let about_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-about",
        central,
    )
    .expect("btn-about");
    let union = start_rect.union(about_rect);
    let start = pos2(union.left() - 12.0, union.top() - 12.0);
    let delta = union.size() + vec2(24.0, 24.0);
    harness.drag_by(start, delta, 4, Modifiers::NONE);

    let ids = harness.editor.selection.ids();
    assert!(ids.contains(&"btn-start".to_owned()), "ids: {ids:?}");
    assert!(ids.contains(&"btn-about".to_owned()), "ids: {ids:?}");
    assert!(!ids.contains(&"title".to_owned()), "ids: {ids:?}");
}

#[test]
fn moving_several_objects_is_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .selection
        .set_many(["title".to_owned(), "btn-start".to_owned()]);
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let start = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start")
    .center();
    let title_before = object_frame(&harness, "s-intro", "title");
    let start_before = object_frame(&harness, "s-intro", "btn-start");
    let revision = harness.editor.session.revision;
    harness.drag_by(start, vec2(40.0, 20.0), 4, Modifiers::NONE);
    assert_eq!(harness.editor.session.revision, revision + 1);
    let title_after = object_frame(&harness, "s-intro", "title");
    let start_after = object_frame(&harness, "s-intro", "btn-start");
    assert_ne!(title_after, title_before);
    assert_eq!(
        title_after.x - title_before.x,
        start_after.x - start_before.x
    );
    assert_eq!(
        title_after.y - title_before.y,
        start_after.y - start_before.y
    );
    harness.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(object_frame(&harness, "s-intro", "title"), title_before);
    assert_eq!(object_frame(&harness, "s-intro", "btn-start"), start_before);
}

#[test]
fn resizing_a_multi_selection_scales_every_frame() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .selection
        .set_many(["btn-start".to_owned(), "btn-about".to_owned()]);
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let start_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start");
    let about_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-about",
        central,
    )
    .expect("btn-about");
    let union = start_rect.union(about_rect);
    let start_before = object_frame(&harness, "s-intro", "btn-start");
    let about_before = object_frame(&harness, "s-intro", "btn-about");
    let revision = harness.editor.session.revision;
    harness.drag_by(union.right_bottom(), vec2(40.0, 20.0), 4, Modifiers::NONE);
    assert_eq!(harness.editor.session.revision, revision + 1);
    let start_after = object_frame(&harness, "s-intro", "btn-start");
    let about_after = object_frame(&harness, "s-intro", "btn-about");
    assert!(start_after.w > start_before.w);
    assert!(start_after.h > start_before.h);
    assert!(about_after.w > about_before.w);
    assert!(about_after.h > about_before.h);
}

#[test]
fn deleting_several_objects_is_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let extra = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": { "type": "text", "text": "Extra" },
                "frame": { "x": 40, "y": 40, "w": 100, "h": 40 }
            }),
        )
        .and_then(|result| {
            result
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
        })
        .expect("extra object");
    harness
        .editor
        .selection
        .set_many(["title".to_owned(), extra.clone()]);
    harness.idle_frame();
    let revision = harness.editor.session.revision;
    harness.press(Key::Delete, Modifiers::NONE);
    assert_eq!(harness.editor.session.revision, revision + 1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.object("title").is_none());
    assert!(slide.object(&extra).is_none());
    harness.press(Key::Z, Modifiers::COMMAND);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert!(slide.object("title").is_some());
    assert!(slide.object(&extra).is_some());
}

#[test]
fn copy_and_paste_objects_through_events() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["btn-start".to_owned()]);
    harness.idle_frame();
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.frame(vec![Event::Copy]);
    assert!(harness.editor.ui.clipboard.is_some());
    let text = harness
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("clipboard text");
    harness.frame(vec![Event::Paste(text)]);

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(slide.objects.len(), before + 1);
    let selected = harness
        .editor
        .selection
        .object
        .clone()
        .expect("pasted object selected");
    assert_ne!(selected, "btn-start");
    let (pasted, _) = slide.object(&selected).expect("pasted object");
    assert!(matches!(
        &pasted.kind,
        ObjectKind::Button { label, .. } if label == "Start"
    ));
}

#[test]
fn plain_text_paste_makes_a_text_box() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();
    harness.frame(vec![Event::Paste("Pasted plain text".to_owned())]);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(slide.objects.len(), before + 1);
    let selected = harness
        .editor
        .selection
        .object
        .clone()
        .expect("text box selected");
    let (object, _) = slide.object(&selected).expect("text box");
    assert!(matches!(
        &object.kind,
        ObjectKind::Text { text, .. } if text == "Pasted plain text"
    ));
}

#[test]
fn clipboard_events_are_ignored_while_a_text_field_has_focus() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    let center = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "title",
        harness.central_panel_rect(),
    )
    .expect("title")
    .center();
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.inline_edit.is_some());
    harness.frame(vec![Event::Copy]);
    assert!(harness.editor.ui.clipboard.is_none());
    harness.press(Key::Escape, Modifiers::NONE);
}

#[test]
fn multi_inspector_sets_x_on_every_object_in_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .selection
        .set_many(["btn-start".to_owned(), "btn-about".to_owned()]);
    harness.idle_frame();
    let field = harness
        .editor
        .ui
        .properties
        .multi_x_rect
        .expect("multi X field");
    let revision = harness.editor.session.revision;
    harness.set_field(field.center(), "123");
    assert_eq!(harness.editor.session.revision, revision + 1);
    assert_eq!(object_frame(&harness, "s-intro", "btn-start").x, 123.0);
    assert_eq!(object_frame(&harness, "s-intro", "btn-about").x, 123.0);
}

#[test]
fn command_a_selects_all_objects_on_the_focused_layer() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.press(Key::A, Modifiers::COMMAND);
    let ids = harness.editor.selection.ids();
    assert_eq!(ids, vec!["title", "btn-start", "btn-about"]);
}

#[test]
fn slide_copy_and_paste_from_the_navigator() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let panel = harness.left_panel_rect();
    let thumb = slide_thumbnail_rect(&harness.editor.session.course, panel, "s-tabs")
        .expect("s-tabs thumbnail");
    harness.click_at(thumb.center());
    assert_eq!(harness.editor.selection.slide, "s-tabs");
    let before = harness.editor.session.course.slides().count();

    harness.frame(vec![Event::Copy]);
    assert!(harness.editor.ui.clipboard.is_some());
    let text = harness
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("clipboard text");
    harness.frame(vec![Event::Paste(text)]);

    assert_eq!(harness.editor.session.course.slides().count(), before + 1);
    let selected = harness.editor.selection.slide.clone();
    assert_ne!(selected, "s-tabs");
    let slide = harness
        .editor
        .session
        .course
        .slide(&selected)
        .expect("pasted slide");
    assert!(slide.title.ends_with(" copy"));
}

#[test]
fn timeline_shift_click_extends_and_command_click_toggles() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    harness.idle_frame();

    let row = |harness: &TestHarness, id: &str| {
        let rect = harness
            .editor
            .ui
            .timeline
            .object_row_rects
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("timeline row {id}"));
        pos2(rect.left() + 10.0, rect.center().y)
    };
    let about = row(&harness, "btn-about");
    harness.click_at_with(about, Modifiers::SHIFT);
    assert_eq!(
        harness.editor.selection.ids(),
        vec!["title", "btn-start", "btn-about"]
    );

    let start = row(&harness, "btn-start");
    harness.click_at_with(start, Modifiers::COMMAND);
    assert_eq!(harness.editor.selection.ids(), vec!["title", "btn-about"]);
}

fn timeline_row_point(harness: &TestHarness, id: &str) -> egui::Pos2 {
    let rect = harness
        .editor
        .ui
        .timeline
        .object_row_rects
        .get(id)
        .copied()
        .unwrap_or_else(|| panic!("timeline row {id}"));
    pos2(rect.left() + 10.0, rect.center().y)
}

fn timeline_stack_ids(harness: &TestHarness, slide: &str, layer: Option<&str>) -> Vec<String> {
    let slide = harness
        .editor
        .session
        .course
        .slide(slide)
        .unwrap_or_else(|| panic!("slide {slide}"));
    match layer {
        Some(layer) => slide
            .layer(layer)
            .unwrap_or_else(|| panic!("layer {layer}"))
            .objects
            .iter()
            .map(|object| object.id.clone())
            .collect(),
        None => slide
            .objects
            .iter()
            .map(|object| object.id.clone())
            .collect(),
    }
}

fn add_timeline_object(session: &mut Session, id: &str, layer: Option<&str>, x: f32) {
    let mut params = serde_json::json!({
        "slide": "slide1",
        "id": id,
        "kind": { "type": "button", "label": id },
        "frame": { "x": x, "y": 100, "w": 160, "h": 60 },
    });
    if let Some(layer) = layer {
        params["layer"] = serde_json::json!(layer);
    }
    session.execute("object.add", &params).expect("add object");
}

#[test]
fn timeline_drag_down_moves_the_top_row_to_the_back_in_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let start = timeline_row_point(&harness, "btn-about");
    let bottom = timeline_row_point(&harness, "title");
    let revision = harness.editor.session.revision;
    harness.drag_by(
        start,
        vec2(0.0, bottom.y - start.y + 8.0),
        4,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None),
        vec!["btn-about", "title", "btn-start"],
        "dragging the frontmost row down must put it at the back"
    );
    assert_eq!(harness.editor.session.revision, revision + 1);

    harness
        .editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None),
        vec!["title", "btn-start", "btn-about"]
    );
}

#[test]
fn timeline_vertical_pull_on_a_bar_reorders_without_moving_timing() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let bar = harness
        .editor
        .ui
        .timeline
        .object_bar_rects
        .get("btn-about")
        .copied()
        .expect("bar for the top row");
    let bottom = timeline_row_point(&harness, "title");
    let revision = harness.editor.session.revision;
    harness.drag_by(
        bar.center(),
        vec2(0.0, bottom.y - bar.center().y + 8.0),
        4,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None),
        vec!["btn-about", "title", "btn-start"],
        "pulling a bar straight down must reorder the row"
    );
    assert_eq!(
        harness.editor.session.revision,
        revision + 1,
        "the pull must be one reorder and no timing edit"
    );
}

#[test]
fn timeline_drag_works_when_the_rows_overflow_the_panel() {
    let mut session = Session::blank("Overflow");
    for index in 0..12 {
        add_timeline_object(&mut session, &format!("o{index}"), None, 20.0);
    }
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.idle_frame();

    let panel = harness.editor.ui.timeline.panel_rect.expect("panel");
    let rects = harness.editor.ui.timeline.object_row_rects.clone();
    assert!(
        rects.values().any(|rect| rect.bottom() > panel.bottom()),
        "the rows must overflow the panel or this test proves nothing"
    );
    let mut visible: Vec<(String, f32)> = rects
        .iter()
        .filter(|(_, rect)| rect.bottom() < panel.bottom() - 40.0)
        .map(|(id, rect)| (id.clone(), rect.center().y))
        .collect();
    visible.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (top_id, top_y) = visible.first().cloned().expect("top row");
    let (low_id, low_y) = visible.last().cloned().expect("low row");
    assert_ne!(top_id, low_id);
    let x = timeline_row_point(&harness, &low_id).x;

    harness.drag_by(
        pos2(x, low_y),
        vec2(0.0, top_y - 4.0 - low_y),
        6,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, "slide1", None).last(),
        Some(&low_id),
        "the row pulled to the top must become the frontmost"
    );
}

#[test]
fn timeline_drag_up_moves_the_bottom_row_to_the_front() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let start = timeline_row_point(&harness, "title");
    let top = timeline_row_point(&harness, "btn-about");
    harness.drag_by(start, vec2(0.0, top.y - start.y - 8.0), 4, Modifiers::NONE);

    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None),
        vec!["btn-start", "btn-about", "title"],
        "dragging the backmost row up must make it the frontmost object"
    );
}

#[test]
fn timeline_click_without_dragging_selects_and_does_not_reorder() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let point = timeline_row_point(&harness, "btn-start");
    let revision = harness.editor.session.revision;
    let before = timeline_stack_ids(&harness, "s-intro", None);
    harness.click_at(point);

    assert_eq!(harness.editor.selection.ids(), vec!["btn-start"]);
    assert_eq!(timeline_stack_ids(&harness, "s-intro", None), before);
    assert_eq!(harness.editor.session.revision, revision);
}

#[test]
fn timeline_escape_cancels_a_row_drag() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let start = timeline_row_point(&harness, "btn-about");
    let bottom = timeline_row_point(&harness, "title");
    let before = timeline_stack_ids(&harness, "s-intro", None);
    let revision = harness.editor.session.revision;
    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerMoved(pos2(start.x, bottom.y + 8.0))]);
    harness.press(Key::Escape, Modifiers::NONE);
    harness.frame(vec![Event::PointerButton {
        pos: pos2(start.x, bottom.y + 8.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    harness.idle_frame();

    assert_eq!(timeline_stack_ids(&harness, "s-intro", None), before);
    assert_eq!(harness.editor.session.revision, revision);
}

#[test]
fn timeline_row_menu_shows_every_item_and_disables_forward_at_the_front() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    harness.right_click_at(timeline_row_point(&harness, "btn-about"));
    harness.idle_frame();
    for label in [
        "Cut",
        "Copy",
        "Paste",
        "Duplicate",
        "Delete",
        "Group",
        "Ungroup",
        "Order",
        "Align",
        "Select all",
    ] {
        assert!(
            harness.find_text(label).is_some(),
            "row menu must show {label}"
        );
    }
    let order = harness.find_text("Order").expect("Order submenu");
    harness.click_at(order);
    harness.idle_frame();
    for label in [
        "Bring to front",
        "Bring forward",
        "Send backward",
        "Send to back",
    ] {
        assert!(
            harness.find_text(label).is_some(),
            "Order submenu must show {label}"
        );
    }
    let revision = harness.editor.session.revision;
    let before = timeline_stack_ids(&harness, "s-intro", None);
    let front = harness.find_text("Bring to front").expect("Bring to front");
    harness.click_at(front);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.revision, revision,
        "a disabled Bring to front must not run a command"
    );
    assert_eq!(timeline_stack_ids(&harness, "s-intro", None), before);
}

#[test]
fn timeline_row_menu_disables_backward_at_the_back() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    harness.right_click_at(timeline_row_point(&harness, "title"));
    harness.idle_frame();
    let order = harness.find_text("Order").expect("Order submenu");
    harness.click_at(order);
    harness.idle_frame();
    let revision = harness.editor.session.revision;
    let before = timeline_stack_ids(&harness, "s-intro", None);
    let back = harness.find_text("Send to back").expect("Send to back");
    harness.click_at(back);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.revision, revision,
        "a disabled Send to back must not run a command"
    );
    assert_eq!(timeline_stack_ids(&harness, "s-intro", None), before);
}

#[test]
fn timeline_row_menu_brings_the_middle_row_forward() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    harness.right_click_at(timeline_row_point(&harness, "btn-start"));
    harness.idle_frame();
    let order = harness.find_text("Order").expect("Order submenu");
    harness.click_at(order);
    harness.idle_frame();
    let forward = harness.find_text("Bring forward").expect("Bring forward");
    harness.click_at(forward);
    harness.idle_frame();
    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None),
        vec!["title", "btn-about", "btn-start"]
    );
}

#[test]
fn timeline_row_menu_duplicates_the_selected_row() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness.idle_frame();

    let before = timeline_stack_ids(&harness, "s-intro", None).len();
    harness.right_click_at(timeline_row_point(&harness, "btn-start"));
    harness.idle_frame();
    let duplicate = harness.find_text("Duplicate").expect("Duplicate");
    harness.click_at(duplicate);
    harness.idle_frame();

    assert_eq!(
        timeline_stack_ids(&harness, "s-intro", None).len(),
        before + 1
    );
}

#[test]
fn canvas_right_click_selects_the_topmost_unselected_object() {
    let mut session = Session::blank("Right-click");
    for (id, label) in [("under", "Under"), ("over", "Over")] {
        session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": "slide1",
                    "id": id,
                    "kind": { "type": "button", "label": label },
                    "frame": { "x": 300, "y": 200, "w": 200, "h": 80 }
                }),
            )
            .expect("object.add");
    }
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.editor.selection.clear_objects();
    harness.idle_frame();

    let over = object_screen_rect(
        &harness.editor.session.course,
        "slide1",
        "over",
        harness.central_panel_rect(),
    )
    .expect("over screen rect");
    harness.right_click_at(over.center());
    harness.idle_frame();

    assert_eq!(
        harness.editor.selection.ids(),
        vec!["over"],
        "right-click must select the topmost object hit by the pointer"
    );
    assert!(harness.find_text("Duplicate").is_some());
    harness.press(Key::Escape, Modifiers::NONE);
}

#[test]
fn canvas_right_click_inside_a_multi_selection_keeps_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .selection
        .set_many(["title".to_owned(), "btn-start".to_owned()]);
    harness.idle_frame();

    let btn = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        harness.central_panel_rect(),
    )
    .expect("btn-start screen rect");
    harness.right_click_at(btn.center());
    harness.idle_frame();

    let ids = harness.editor.selection.ids();
    assert!(
        ids.contains(&"title".to_owned()) && ids.contains(&"btn-start".to_owned()),
        "right-clicking a selected object must keep the multi-selection: {ids:?}"
    );
    harness.press(Key::Escape, Modifiers::NONE);
}

#[test]
fn canvas_menu_cut_and_copy_match_the_keyboard_and_undo_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");

    // Copy from the menu writes the same clipboard text as the keyboard event.
    let mut menu_copy = TestHarness::new(course.clone());
    menu_copy
        .editor
        .selection
        .set_many(["btn-start".to_owned()]);
    menu_copy.idle_frame();
    let btn = object_screen_rect(
        &menu_copy.editor.session.course,
        "s-intro",
        "btn-start",
        menu_copy.central_panel_rect(),
    )
    .expect("btn-start screen rect");
    menu_copy.right_click_at(btn.center());
    menu_copy.idle_frame();
    let copy = menu_copy.find_text("Copy").expect("Copy menu item");
    menu_copy.click_at(copy);
    menu_copy.idle_frame();
    let menu_text = menu_copy
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("menu copy clipboard text");

    let mut keyboard_copy = TestHarness::new(course.clone());
    keyboard_copy
        .editor
        .selection
        .set_many(["btn-start".to_owned()]);
    keyboard_copy.idle_frame();
    keyboard_copy.frame(vec![Event::Copy]);
    assert_eq!(
        keyboard_copy.editor.ui.clipboard_text.as_deref(),
        Some(menu_text.as_str()),
        "menu Copy must produce the same payload as the keyboard"
    );

    // Cut from the menu removes the object and is one undo step.
    let mut menu_cut = TestHarness::new(course.clone());
    menu_cut.editor.selection.set_many(["title".to_owned()]);
    menu_cut.idle_frame();
    let btn = object_screen_rect(
        &menu_cut.editor.session.course,
        "s-intro",
        "title",
        menu_cut.central_panel_rect(),
    )
    .expect("title screen rect");
    let revision = menu_cut.editor.session.revision;
    let undo_len = menu_cut.editor.session.undo_len();
    menu_cut.right_click_at(btn.center());
    menu_cut.idle_frame();
    assert!(
        menu_cut.editor.selection.contains("title"),
        "right-click should keep the selected object: {:?} (active area {:?})",
        menu_cut.editor.selection.ids(),
        menu_cut.editor.ui.last_active_area
    );
    let cut = menu_cut.find_text("Cut").expect("Cut menu item");
    menu_cut.click_at(cut);
    menu_cut.idle_frame();
    assert!(
        menu_cut.editor.ui.clipboard.is_some(),
        "menu Cut must fill the clipboard (active area {:?}, error {:?})",
        menu_cut.editor.ui.last_active_area,
        menu_cut.editor.ui.last_error
    );

    assert!(
        menu_cut
            .editor
            .session
            .course
            .slide("s-intro")
            .is_some_and(|slide| slide.object("title").is_none()),
        "menu Cut must remove the object (error {:?}, undo {:?})",
        menu_cut.editor.ui.last_error,
        menu_cut.editor.session.undo_len()
    );
    assert_eq!(menu_cut.editor.session.revision, revision + 1);
    assert_eq!(menu_cut.editor.session.undo_len(), undo_len + 1);
    let menu_cut_text = menu_cut
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("menu cut clipboard text");

    let mut keyboard_cut = TestHarness::new(course);
    keyboard_cut.editor.selection.set_many(["title".to_owned()]);
    keyboard_cut.idle_frame();
    keyboard_cut.frame(vec![Event::Cut]);
    assert_eq!(
        keyboard_cut.editor.ui.clipboard_text.as_deref(),
        Some(menu_cut_text.as_str()),
        "menu Cut must produce the same payload as the keyboard"
    );
    assert!(
        keyboard_cut
            .editor
            .session
            .course
            .slide("s-intro")
            .is_some_and(|slide| slide.object("title").is_none()),
        "the keyboard cut must remove the same object"
    );

    menu_cut
        .editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert!(
        menu_cut
            .editor
            .session
            .course
            .slide("s-intro")
            .is_some_and(|slide| slide.object("title").is_some()),
        "one undo must restore a cut object"
    );
}

#[test]
fn canvas_menu_paste_matches_the_keyboard_and_undoes_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");

    let mut menu = TestHarness::new(course.clone());
    menu.editor.selection.set_many(["btn-start".to_owned()]);
    menu.idle_frame();
    menu.frame(vec![Event::Copy]);
    let clipboard_text = menu
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("clipboard text");
    let btn = object_screen_rect(
        &menu.editor.session.course,
        "s-intro",
        "btn-start",
        menu.central_panel_rect(),
    )
    .expect("btn-start screen rect");
    let revision = menu.editor.session.revision;
    let undo_len = menu.editor.session.undo_len();
    menu.right_click_at(btn.center());
    menu.idle_frame();
    let paste = menu.find_text("Paste").expect("Paste menu item");
    menu.click_at(paste);
    menu.idle_frame();
    let menu_id = menu
        .editor
        .selection
        .object
        .clone()
        .expect("pasted object selected");
    let menu_frame = menu
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&menu_id))
        .map(|(object, _)| object.frame)
        .expect("pasted object");
    assert_eq!(menu.editor.session.revision, revision + 1);
    assert_eq!(menu.editor.session.undo_len(), undo_len + 1);

    let mut keyboard = TestHarness::new(course);
    keyboard.editor.selection.set_many(["btn-start".to_owned()]);
    keyboard.idle_frame();
    keyboard.frame(vec![Event::Paste(clipboard_text)]);
    let key_id = keyboard
        .editor
        .selection
        .object
        .clone()
        .expect("keyboard pasted object selected");
    let key_frame = keyboard
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&key_id))
        .map(|(object, _)| object.frame)
        .expect("keyboard pasted object");
    assert_eq!(
        menu_frame, key_frame,
        "menu Paste must place the copy exactly like the keyboard"
    );

    menu.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert!(
        menu.editor
            .session
            .course
            .slide("s-intro")
            .is_some_and(|slide| slide.object(&menu_id).is_none()),
        "one undo must remove a pasted object"
    );
}

#[test]
fn canvas_background_menu_paste_is_disabled_when_nothing_was_copied() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let t = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");

    harness.right_click_at(t.to_screen_pos(pos2(640.0, 300.0)));
    harness.idle_frame();
    assert!(harness.editor.selection.object.is_none());
    assert!(
        !crate::menu::paste_enabled(&harness.editor),
        "Paste is disabled while the private clipboard is empty"
    );
    let paste = harness.find_text("Paste").expect("Paste menu item");
    assert!(harness.find_text("Select all").is_some());
    assert!(
        harness.find_text("Cut").is_none() && harness.find_text("Duplicate").is_none(),
        "the empty-canvas menu only has Paste and Select all"
    );

    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .map(|slide| slide.objects.len())
        .unwrap_or_default();
    let revision = harness.editor.session.revision;
    harness.click_at(paste);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .map(|slide| slide.objects.len())
            .unwrap_or_default(),
        before
    );
    assert_eq!(harness.editor.session.revision, revision);
    assert!(
        !crate::menu::paste_enabled(&harness.editor),
        "nothing was copied, so Paste stays disabled"
    );
}

#[test]
fn canvas_background_paste_lands_centred_and_clamped_at_the_pointer() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["btn-start".to_owned()]);
    harness.idle_frame();
    harness.frame(vec![Event::Copy]);
    let before: Vec<String> = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .map(|slide| slide.objects.iter().map(|o| o.id.clone()).collect())
        .unwrap_or_default();
    let t = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");

    let target = pos2(640.0, 300.0);
    harness.right_click_at(t.to_screen_pos(target));
    harness.idle_frame();
    let paste = harness.find_text("Paste").expect("Paste menu item");
    harness.click_at(paste);
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let pasted = slide
        .objects
        .iter()
        .find(|object| !before.contains(&object.id))
        .expect("pasted object");
    let centre = pos2(
        pasted.frame.x + pasted.frame.w * 0.5,
        pasted.frame.y + pasted.frame.h * 0.5,
    );
    assert!(
        (centre.x - target.x).abs() <= 1.0 && (centre.y - target.y).abs() <= 1.0,
        "pasted joint centre {centre:?} should be at {target:?}"
    );

    let revision = harness.editor.session.revision;
    let undo_len = harness.editor.session.undo_len();
    let before_ids: Vec<String> = slide.objects.iter().map(|o| o.id.clone()).collect();
    harness.right_click_at(t.to_screen_pos(pos2(10.0, 10.0)));
    harness.idle_frame();
    let paste = harness.find_text("Paste").expect("Paste menu item");
    harness.click_at(paste);
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let clamped = slide
        .objects
        .iter()
        .find(|object| !before_ids.contains(&object.id))
        .expect("clamped pasted object");
    assert_eq!(clamped.frame.x, 0.0);
    assert_eq!(clamped.frame.y, 0.0);
    assert_eq!(harness.editor.session.revision, revision + 1);
    assert_eq!(harness.editor.session.undo_len(), undo_len + 1);
}

#[test]
fn timeline_drag_tracks_rows_after_scrolling() {
    let mut session = Session::blank("Scrolling");
    for index in 0..12 {
        add_timeline_object(&mut session, &format!("o{index}"), None, 20.0);
    }
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.idle_frame();

    let panel = harness.editor.ui.timeline.panel_rect.expect("panel");
    let before_scroll = harness.editor.ui.timeline.object_row_rects["o0"].center().y;
    let scroll_point = pos2(panel.center().x, panel.min.y + 120.0);
    for _ in 0..3 {
        harness.scroll_over(scroll_point, -80.0);
    }
    let after_scroll = harness.editor.ui.timeline.object_row_rects["o0"].center().y;
    assert!(
        after_scroll < before_scroll - 1.0,
        "the timeline rows must scroll"
    );

    let mut visible: Vec<(String, f32)> = harness
        .editor
        .ui
        .timeline
        .object_row_rects
        .iter()
        .filter(|(_, rect)| rect.center().y >= panel.min.y + 90.0 && rect.center().y <= panel.max.y)
        .map(|(id, rect)| (id.clone(), rect.center().y))
        .collect();
    visible.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert!(visible.len() >= 4, "at least four rows must be visible");
    let (top_id, top_y) = visible[2].clone();
    let (_below_id, below_y) = visible[3].clone();

    let before = timeline_stack_ids(&harness, "slide1", None);
    let start_x = timeline_row_point(&harness, &top_id).x;
    harness.drag_by(
        pos2(start_x, top_y),
        vec2(0.0, below_y - top_y + 8.0),
        4,
        Modifiers::NONE,
    );
    let after = timeline_stack_ids(&harness, "slide1", None);

    let mut expected = before.clone();
    let from = expected
        .iter()
        .position(|id| id == &top_id)
        .expect("top row id");
    let to = expected
        .iter()
        .position(|id| id == &visible[3].0)
        .expect("row below id");
    expected.swap(from, to);
    assert_eq!(
        after, expected,
        "a drag after scrolling must use the rows under the pointer"
    );
}

#[test]
fn timeline_drag_moves_a_group_as_one_contiguous_block() {
    let mut session = Session::blank("Groups");
    add_timeline_object(&mut session, "a", None, 20.0);
    add_timeline_object(&mut session, "b", None, 200.0);
    add_timeline_object(&mut session, "c", None, 380.0);
    add_timeline_object(&mut session, "d", None, 560.0);
    session
        .execute(
            "object.group",
            &serde_json::json!({ "slide": "slide1", "objects": ["b", "d"] }),
        )
        .expect("group");
    let before = session
        .course
        .slide("slide1")
        .expect("slide")
        .objects
        .iter()
        .map(|object| object.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(before, vec!["a", "c", "b", "d"]);

    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.idle_frame();
    let start = timeline_row_point(&harness, "d");
    let bottom = timeline_row_point(&harness, "a");
    let revision = harness.editor.session.revision;
    harness.drag_by(
        start,
        vec2(0.0, bottom.y - start.y + 8.0),
        4,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, "slide1", None),
        vec!["b", "d", "a", "c"],
        "the group must stay contiguous, keep its order and move as one"
    );
    assert_eq!(harness.editor.session.revision, revision + 1);
    harness
        .editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert_eq!(
        timeline_stack_ids(&harness, "slide1", None),
        vec!["a", "c", "b", "d"]
    );
}

#[test]
fn timeline_drag_reorders_only_the_selected_layer() {
    let mut session = Session::blank("Layers");
    add_timeline_object(&mut session, "base1", None, 20.0);
    add_timeline_object(&mut session, "base2", None, 220.0);
    let layer = session
        .execute("layer.add", &serde_json::json!({ "slide": "slide1" }))
        .expect("layer")["id"]
        .as_str()
        .expect("layer id")
        .to_owned();
    add_timeline_object(&mut session, "la", Some(&layer), 20.0);
    add_timeline_object(&mut session, "lb", Some(&layer), 220.0);
    add_timeline_object(&mut session, "lc", Some(&layer), 420.0);

    let mut harness = TestHarness::new(session.course);
    harness.editor.selection.slide = "slide1".into();
    harness.editor.selection.layer = Some(layer.clone());
    harness.idle_frame();
    harness.idle_frame();

    let start = timeline_row_point(&harness, "lc");
    let bottom = timeline_row_point(&harness, "la");
    harness.drag_by(
        start,
        vec2(0.0, bottom.y - start.y + 8.0),
        4,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, "slide1", Some(&layer)),
        vec!["lc", "la", "lb"]
    );
    assert_eq!(
        timeline_stack_ids(&harness, "slide1", None),
        vec!["base1", "base2"],
        "rows on one layer must not reorder the base"
    );
}

#[test]
fn timeline_drag_reorders_objects_on_a_master() {
    let mut session = Session::blank("Masters");
    let master = session
        .execute("master.add", &serde_json::json!({ "title": "Brand" }))
        .expect("master")["id"]
        .as_str()
        .expect("master id")
        .to_owned();
    for (index, id) in ["ma", "mb", "mc"].iter().enumerate() {
        session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": master,
                    "id": id,
                    "kind": { "type": "button", "label": id },
                    "frame": { "x": 20.0 + index as f32 * 180.0, "y": 100, "w": 160, "h": 60 },
                }),
            )
            .expect("master object");
    }

    let mut harness = TestHarness::new(session.course);
    harness.editor.selection.slide = master.clone();
    harness.idle_frame();
    harness.idle_frame();

    let start = timeline_row_point(&harness, "mc");
    let bottom = timeline_row_point(&harness, "ma");
    harness.drag_by(
        start,
        vec2(0.0, bottom.y - start.y + 8.0),
        4,
        Modifiers::NONE,
    );

    assert_eq!(
        timeline_stack_ids(&harness, &master, None),
        vec!["mc", "ma", "mb"]
    );
}

#[test]
fn pasted_graded_question_can_be_answered_through_logic() {
    let mut session = coursecraft_engine::Session::blank("Question copy");
    session
        .execute(
            "question.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "multiple_choice" }),
        )
        .expect("question");
    let original_correct = session
        .course
        .slide("slide1")
        .and_then(|slide| slide.question.as_ref())
        .and_then(|question| question.correct_layer.clone())
        .expect("correct layer");
    let result = session
        .execute("slide.duplicate", &serde_json::json!({ "slide": "slide1" }))
        .expect("duplicate");
    let new_slide = result["id"].as_str().expect("new id").to_owned();
    session.course.start_slide = Some(new_slide.clone());
    let copied_correct = session
        .course
        .slide(&new_slide)
        .and_then(|slide| slide.question.as_ref())
        .and_then(|question| question.correct_layer.clone())
        .expect("copied correct layer");
    assert_ne!(copied_correct, original_correct);
    let submit_button = session
        .course
        .slide(&new_slide)
        .and_then(|slide| slide.objects.iter().find(|object| object.name == "Submit"))
        .map(|object| object.id.clone())
        .expect("copied submit button");

    let mut player = coursecraft_logic::Player::new(&session.course).expect("valid course");
    let _ = player.handle(&coursecraft_logic::Input::Start);
    let _ = player.handle(&coursecraft_logic::Input::SelectChoice {
        choice: "a".to_owned(),
    });
    let effects = player.handle(&coursecraft_logic::Input::Click {
        object: submit_button,
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            coursecraft_logic::Effect::LayerShown { layer } if layer == &copied_correct
        )),
        "the copied feedback layer must show: {effects:?}"
    );
}

#[test]
fn paste_then_save_and_reopen_round_trips() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["btn-start".to_owned()]);
    harness.idle_frame();
    harness.frame(vec![Event::Copy]);
    let text = harness
        .editor
        .ui
        .clipboard_text
        .clone()
        .expect("clipboard text");
    harness.frame(vec![Event::Paste(text)]);
    let json = harness
        .editor
        .session
        .course
        .to_json()
        .expect("course json");
    let reopened = Course::from_json(&json).expect("reopened course");
    assert_eq!(reopened, harness.editor.session.course);
}

#[test]
fn course_settings_language_field_commits() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    // The window grew with the player options; give it room so nothing scrolls.
    harness.screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    for _ in 0..5 {
        harness.idle_frame();
    }
    let language_pos = harness.find_text("Language").expect("Language label") + vec2(120.0, 0.0);
    harness.set_field(language_pos, "pt-BR");
    assert_eq!(
        harness.editor.session.course.language.as_deref(),
        Some("pt-BR")
    );
}

fn group_two(harness: &mut TestHarness, ids: [&str; 2]) {
    harness.editor.selection.set_many(ids);
    harness
        .editor
        .run(
            "object.group",
            serde_json::json!({
                "slide": "s-intro",
                "objects": ids,
            }),
        )
        .expect("group");
}

fn group_id(harness: &TestHarness, id: &str) -> Option<String> {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .and_then(|(object, _)| object.group.clone())
}

#[test]
fn clicking_a_group_member_selects_the_whole_group() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    group_two(&mut harness, ["title", "btn-start"]);
    harness.editor.selection.clear_objects();
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let title = object_screen_rect(&harness.editor.session.course, "s-intro", "title", central)
        .expect("title")
        .center();
    harness.click_at(title);

    let ids = harness.editor.selection.ids();
    assert_eq!(ids, vec!["title", "btn-start"]);
}

#[test]
fn double_clicking_a_group_member_focuses_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    group_two(&mut harness, ["title", "btn-start"]);
    harness.editor.selection.clear_objects();
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let title = object_screen_rect(&harness.editor.session.course, "s-intro", "title", central)
        .expect("title")
        .center();
    harness.click_at(title);
    harness.click_at(title);

    assert_eq!(harness.editor.selection.ids(), vec!["title"]);
    assert_eq!(harness.editor.ui.group_focus.as_deref(), Some("title"));
}

#[test]
fn dragging_a_group_moves_every_member() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    group_two(&mut harness, ["title", "btn-start"]);
    harness.editor.ui.snap_to_guides = false;
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let title = object_screen_rect(&harness.editor.session.course, "s-intro", "title", central)
        .expect("title")
        .center();
    harness.click_at(title);
    let title_before = object_frame(&harness, "s-intro", "title");
    let start_before = object_frame(&harness, "s-intro", "btn-start");
    harness.drag_by(title, vec2(40.0, 20.0), 4, Modifiers::NONE);
    let title_after = object_frame(&harness, "s-intro", "title");
    let start_after = object_frame(&harness, "s-intro", "btn-start");
    assert_eq!(
        title_after.x - title_before.x,
        start_after.x - start_before.x
    );
    assert_eq!(
        title_after.y - title_before.y,
        start_after.y - start_before.y
    );
    assert_ne!(title_after, title_before);
}

#[test]
fn command_g_and_shift_command_g_group_and_ungroup() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["title", "btn-start"]);
    harness.idle_frame();
    harness.press(Key::G, Modifiers::COMMAND);
    let group = group_id(&harness, "title").expect("grouped");
    assert_eq!(
        group_id(&harness, "btn-start").as_deref(),
        Some(group.as_str())
    );

    harness.press(Key::G, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(group_id(&harness, "title"), None);
    assert_eq!(group_id(&harness, "btn-start"), None);
}

#[test]
fn arrange_actions_are_enabled_only_for_valid_selections() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["title"]);
    assert!(crate::menu::align_params(&harness.editor, "left").is_some());
    assert!(crate::menu::distribute_params(&harness.editor, "horizontal").is_none());
    assert!(crate::menu::match_size_params(&harness.editor, "width").is_none());
    assert!(crate::menu::adapter(&harness.editor, "object.group").is_none());

    harness.editor.selection.set_many(["title", "btn-start"]);
    assert!(crate::menu::match_size_params(&harness.editor, "width").is_some());
    assert!(crate::menu::distribute_params(&harness.editor, "horizontal").is_none());
    assert!(crate::menu::adapter(&harness.editor, "object.group").is_some());
    assert!(crate::menu::adapter(&harness.editor, "object.ungroup").is_none());

    harness
        .editor
        .selection
        .set_many(["title", "btn-start", "btn-about"]);
    assert!(crate::menu::distribute_params(&harness.editor, "horizontal").is_some());

    group_two(&mut harness, ["title", "btn-start"]);
    harness.editor.selection.set_many(["title"]);
    assert!(crate::menu::adapter(&harness.editor, "object.ungroup").is_some());
}

#[test]
fn dragging_snaps_to_another_objects_left_edge_and_command_disables_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "id": "snap-target",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#D0D7DE" },
                "frame": { "x": 400, "y": 520, "w": 10, "h": 60 }
            }),
        )
        .expect("add target");
    harness.editor.selection.set_many(["btn-start"]);
    harness.editor.ui.snap_to_guides = true;
    harness.idle_frame();

    let central = harness.central_panel_rect();
    let start_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start");
    let target_rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "snap-target",
        central,
    )
    .expect("snap-target");
    let delta = vec2(target_rect.left() - 3.0 - start_rect.left(), 0.0);
    harness.drag_by(start_rect.center(), delta, 4, Modifiers::NONE);
    assert_eq!(object_frame(&harness, "s-intro", "btn-start").x, 400.0);

    harness.press(Key::Z, Modifiers::COMMAND);
    assert_eq!(object_frame(&harness, "s-intro", "btn-start").x, 80.0);
    harness.drag_by(start_rect.center(), delta, 4, Modifiers::COMMAND);
    let scale = SlideTransform::fit(
        canvas_fit_rect(central),
        harness.editor.session.course.slide_size,
    )
    .expect("transform")
    .scale();
    let expected = 80.0 + delta.x / scale;
    let x = object_frame(&harness, "s-intro", "btn-start").x;
    assert!((x - expected).abs() < 0.6, "x {x} expected {expected}");
    assert_ne!(x, 400.0);
}

#[test]
fn duplicating_a_group_gives_the_copies_a_fresh_shared_group() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    group_two(&mut harness, ["title", "btn-start"]);
    let original = group_id(&harness, "title").expect("group");
    harness.editor.selection.set_many(["title", "btn-start"]);
    harness.editor.duplicate_selection();

    let ids = harness.editor.selection.ids();
    assert_eq!(ids.len(), 2);
    let copied = group_id(&harness, &ids[0]).expect("copy group");
    assert_eq!(
        group_id(&harness, &ids[1]).as_deref(),
        Some(copied.as_str())
    );
    assert_ne!(copied, original);
}

// ---------------------------------------------------------------- masters

fn add_master(harness: &mut TestHarness, title: &str) -> String {
    harness
        .editor
        .run("master.add", serde_json::json!({ "title": title }))
        .and_then(|value| {
            value
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_owned)
        })
        .expect("master added")
}

#[test]
fn entering_and_leaving_master_view_restores_the_slide_and_hides_tabs() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    let master = add_master(&mut harness, "Brand");
    harness.idle_frame();

    harness.editor.enter_master_view();
    harness.idle_frame();
    assert!(harness.editor.ui.master_view);
    assert_eq!(harness.editor.selection.slide, master);
    assert_eq!(harness.editor.ui.inspector_tab, InspectorTab::Slide);
    assert_eq!(harness.editor.ui.bottom_tab, BottomTab::Timeline);
    assert!(
        !harness
            .editor
            .ui
            .timeline
            .tab_rects
            .contains_key("Triggers")
    );
    assert!(
        !harness
            .editor
            .ui
            .timeline
            .tab_rects
            .contains_key("Variables")
    );
    assert!(
        !harness
            .editor
            .ui
            .timeline
            .tab_rects
            .contains_key("Question")
    );
    assert!(harness.find_text("Close slide masters").is_some());

    harness.editor.leave_master_view();
    harness.idle_frame();
    assert!(!harness.editor.ui.master_view);
    assert_eq!(harness.editor.selection.slide, "s-intro");
}

#[test]
fn an_object_placed_in_master_view_lands_in_the_master() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let master = add_master(&mut harness, "Brand");
    harness.editor.enter_master_view();
    harness.editor.selection.slide = master.clone();
    harness.idle_frame();

    harness.editor.ui.placement_tool = Some(PlacementTool::Button);
    harness.idle_frame();
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    harness.click_at(transform.slide_rect().center());
    harness.idle_frame();

    let master_slide = harness
        .editor
        .session
        .course
        .master(&master)
        .expect("master");
    assert_eq!(master_slide.objects.len(), 1);
    assert!(
        harness
            .editor
            .session
            .course
            .play_slide("s-intro")
            .is_some_and(|slide| slide.objects.iter().all(|object| object.id != "button1"))
    );
}

#[test]
fn the_master_dropdown_applies_a_master_to_the_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.clear_objects();
    harness.editor.ui.inspector_tab = InspectorTab::Slide;
    let master = add_master(&mut harness, "Brand");
    harness.idle_frame();

    harness.pick_combo("None", "Brand");
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .and_then(|slide| slide.master.clone())
            .as_deref(),
        Some(master.as_str())
    );
}

#[test]
fn the_layout_popup_lists_layouts_and_adds_the_chosen_one() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let before = harness.editor.session.course.slides().count();

    let caret = harness
        .find_icon(coursecraft_render::Icon::CaretDown)
        .expect("toolbar caret");
    harness.click_at(caret);
    harness.idle_frame();
    assert!(harness.find_text("Title and content").is_some());
    let item = harness.find_text("Title and content").expect("layout item");
    harness.click_at(item);
    harness.idle_frame();

    assert_eq!(harness.editor.session.course.slides().count(), before + 1);
    let new_slide = harness
        .editor
        .session
        .course
        .slide(&harness.editor.selection.slide)
        .expect("new slide selected");
    assert!(new_slide
        .objects
        .iter()
        .any(|object| matches!(&object.kind, ObjectKind::Text { text, .. } if text == "Click to add a title")));
}

#[test]
fn the_layout_popup_lists_templates_and_can_remove_one() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness
        .editor
        .run(
            "template.save",
            serde_json::json!({ "slide": "s-intro", "title": "Intro template" }),
        )
        .expect("template saved");
    harness.idle_frame();

    let caret = harness
        .find_icon(coursecraft_render::Icon::CaretDown)
        .expect("toolbar caret");
    harness.click_at(caret);
    harness.idle_frame();
    assert!(harness.find_text("Your templates").is_some());
    assert!(harness.find_text("Intro template").is_some());
    assert_eq!(harness.editor.session.course.templates.len(), 1);
}

#[test]
fn deleting_the_selected_master_in_master_view_falls_back_to_an_existing_one() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let first = add_master(&mut harness, "First");
    let second = add_master(&mut harness, "Second");
    harness.editor.enter_master_view();
    harness.editor.selection.slide = second.clone();
    harness.idle_frame();
    harness
        .editor
        .run("master.delete", serde_json::json!({ "master": second }))
        .expect("master deleted");
    harness.idle_frame();
    assert!(harness.editor.ui.master_view);
    assert_eq!(harness.editor.selection.slide, first);
}

// --- M9 step 2: AI settings -------------------------------------------------

fn ai_config() -> coursecraft_ai::AiConfig {
    coursecraft_ai::AiConfig {
        base_url: "http://localhost:1234/v1".to_string(),
        model: "test-model".to_string(),
        api_key: String::new(),
        speech_to_text_model: "whisper-1".to_string(),
        text_to_speech_model: "tts-1".to_string(),
        voice: "alloy".to_string(),
    }
}

#[test]
fn project_menu_opens_ai_settings() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    let project = harness.find_text("Project").expect("Project menu");
    harness.click_at(project);
    harness.idle_frame();
    let item = harness
        .find_text_last("AI settings…")
        .expect("AI settings menu item");
    harness.click_at(item);
    harness.idle_frame();

    assert!(harness.editor.ui.ai.open);
    assert_eq!(harness.editor.ui.ai.tab, AiSettingsTab::Features);
    assert!(harness.find_text("Test connection").is_some());
}

#[test]
fn help_connect_agent_opens_the_agents_tab_with_five_rows() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_automation_available(true);
    harness
        .editor
        .set_agent_exe_path(Some("/Applications/CourseCraft".to_string()));
    harness.idle_frame();

    let help = harness.find_text("Help").expect("Help menu");
    harness.click_at(help);
    harness.idle_frame();
    let item = harness
        .find_text_last("Connect an AI agent…")
        .expect("Connect an AI agent menu item");
    let requests = click_for_requests(&mut harness, item);

    assert!(harness.editor.ui.ai.open);
    assert_eq!(harness.editor.ui.ai.tab, AiSettingsTab::Agents);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::AgentDetect)),
        "opening the tab asks the app to detect agents"
    );

    harness.editor.set_agent_statuses(vec![
        (AgentId::ClaudeCode, AgentStatus::Found),
        (AgentId::Codex, AgentStatus::Connected),
        (AgentId::ClaudeDesktop, AgentStatus::NotInstalled),
        (AgentId::Cursor, AgentStatus::Found),
        (AgentId::GeminiCli, AgentStatus::NotInstalled),
    ]);
    harness.idle_frame();

    assert!(harness.find_text("Agents").is_some());
    assert!(harness.find_text("AI features").is_some());
    for name in [
        "Claude Code",
        "Codex",
        "Claude Desktop",
        "Cursor",
        "Gemini CLI",
    ] {
        assert!(
            harness.find_text(name).is_some(),
            "{name} row should be visible"
        );
    }
    assert!(harness.find_text("Set up by hand").is_some());
    assert!(harness.find_text("Installed, not connected").is_some());
    assert!(harness.find_text("Connected").is_some());
    assert!(harness.find_text("Not installed").is_some());
}

#[test]
fn the_web_agents_tab_shows_the_setup_lines_expanded() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_automation_available(false);
    harness
        .editor
        .set_agent_exe_path(Some("/Applications/CourseCraft".to_string()));
    harness.editor.open_agent_settings();
    harness.idle_frame();

    assert_eq!(harness.editor.ui.ai.tab, AiSettingsTab::Agents);
    assert!(harness.find_text("Set up by hand").is_some());
    assert!(harness.find_text("Check the CLI").is_some());
    assert!(harness.find_text("Tell any agent").is_some());
    assert!(
        harness.find_text("Installed, not connected").is_none(),
        "no rows on the web"
    );
}

#[test]
fn choosing_a_provider_fills_only_the_base_url() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.ai.config = coursecraft_ai::AiConfig::default();
    harness.editor.ui.ai.saved = coursecraft_ai::AiConfig::default();
    harness.editor.open_ai_settings();
    harness.idle_frame();
    harness.idle_frame();

    harness.pick_combo("Other", "Ollama (on this computer)");
    assert_eq!(
        harness.editor.ui.ai.config.base_url,
        "http://localhost:11434/v1"
    );
    assert_eq!(harness.editor.ui.ai.config.model, "");
    assert_eq!(harness.editor.ui.ai.provider, AiProvider::Ollama);
}

#[test]
fn test_connection_emits_a_small_chat_request() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();
    harness.idle_frame();

    let button = harness
        .find_text("Test connection")
        .expect("Test connection button");
    let requests = click_for_requests(&mut harness, button);
    let request = requests
        .iter()
        .find_map(|request| match request {
            EditorRequest::AiChat { purpose, request } => Some((purpose.clone(), request)),
            _ => None,
        })
        .expect("AI chat request");
    assert_eq!(request.0, AiPurpose::TestConnection);
    assert_eq!(request.1.max_tokens, Some(8));
    assert!(matches!(
        harness.editor.ui.ai.test,
        AiTestState::Running { .. }
    ));
}

#[test]
fn choosing_claude_code_shows_its_status_and_test_connection() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();
    harness.editor.set_agent_statuses(vec![
        (AgentId::ClaudeCode, AgentStatus::Found),
        (AgentId::Codex, AgentStatus::NotInstalled),
        (AgentId::ClaudeDesktop, AgentStatus::NotInstalled),
        (AgentId::Cursor, AgentStatus::NotInstalled),
        (AgentId::GeminiCli, AgentStatus::NotInstalled),
    ]);
    harness.idle_frame();

    let option = harness
        .find_text("Claude Code (your Claude plan)")
        .expect("Claude Code choice");
    click_for_requests(&mut harness, option);
    harness.idle_frame();

    assert_eq!(
        harness.editor.ui.ai.source,
        coursecraft_ai::AiSource::ClaudeCode
    );
    assert!(
        harness.find_text("Installed, not connected").is_some(),
        "the chosen agent's status is shown"
    );
    assert!(harness.find_text("Test connection").is_some());
    assert!(
        harness.find_text("Base URL").is_none(),
        "the endpoint fields are hidden for an agent"
    );
}

#[test]
fn a_test_result_shows_the_host_or_the_error() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();

    harness.editor.ui.ai.test = AiTestState::Running {
        host: Some("localhost".to_string()),
    };
    harness.editor.set_ai_result(
        AiPurpose::TestConnection,
        Ok(coursecraft_ai::ChatReply {
            text: "OK".to_string(),
            truncated: false,
        }),
    );
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("Connected to localhost.")
            .is_some()
    );

    harness.editor.ui.ai.test = AiTestState::Running {
        host: Some("localhost".to_string()),
    };
    harness.editor.set_ai_result(
        AiPurpose::TestConnection,
        Err("The endpoint rejected the key.".to_string()),
    );
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("The endpoint rejected the key.")
            .is_some()
    );
}

#[test]
fn save_stores_the_settings_and_remove_clears_them() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();
    harness.idle_frame();

    let save = harness.find_text("Save").expect("Save button");
    let requests = click_for_requests(&mut harness, save);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::SaveAiConfig(Some(_))))
    );
    assert!(!harness.editor.ui.ai.open);
    assert_eq!(
        harness.editor.ui.ai.saved.base_url,
        "http://localhost:1234/v1"
    );

    harness.editor.open_ai_settings();
    harness.idle_frame();
    harness.idle_frame();
    let remove = harness.find_text("Remove").expect("Remove button");
    let requests = click_for_requests(&mut harness, remove);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::SaveAiConfig(None)))
    );
    assert_eq!(
        harness.editor.ui.ai.config,
        coursecraft_ai::AiConfig::default()
    );
    assert_eq!(
        harness.editor.ui.ai.saved,
        coursecraft_ai::AiConfig::default()
    );
}

#[test]
fn escape_closes_ai_settings_and_discards_edits() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();

    harness.editor.ui.ai.config.model = "unsaved".to_string();
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();

    assert!(!harness.editor.ui.ai.open);
    assert_eq!(harness.editor.ui.ai.config.model, "test-model");
}

#[test]
fn ai_settings_fields_take_the_keyboard_away_from_the_canvas() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ai_config()));
    harness.editor.open_ai_settings();
    harness.idle_frame();

    let objects_before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .map_or(0, |slide| slide.objects.len());
    harness.press(Key::Delete, Modifiers::NONE);
    harness.idle_frame();
    let objects_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .map_or(0, |slide| slide.objects.len());
    assert_eq!(objects_before, objects_after);
    assert!(harness.editor.ui.ai.open);
}

fn restart_preview_course(slides: usize) -> coursecraft_engine::Session {
    let mut session = coursecraft_engine::Session::blank("Restart preview");
    session
        .execute(
            "variable.add",
            &serde_json::json!({ "name": "score", "value": 0 }),
        )
        .expect("variable");
    for index in 2..=slides {
        session
            .execute(
                "slide.add",
                &serde_json::json!({ "id": format!("slide{index}"), "title": format!("Slide {index}") }),
            )
            .expect("slide");
    }
    let add_trigger =
        |session: &mut coursecraft_engine::Session, slide: &str, trigger: serde_json::Value| {
            session
                .execute(
                    "trigger.add",
                    &serde_json::json!({ "slide": slide, "trigger": trigger }),
                )
                .expect("trigger");
        };
    let advance = serde_json::json!({
        "action": { "action": "jump_to_slide", "target": "next" },
        "when": { "event": "timeline_reaches", "ms": 100 }
    });
    add_trigger(&mut session, "slide1", advance.clone());
    if slides >= 3 {
        add_trigger(
            &mut session,
            "slide2",
            serde_json::json!({
                "action": { "action": "set_variable", "variable": "score", "op": "assign", "value": 7 },
                "when": { "event": "timeline_starts" }
            }),
        );
        add_trigger(&mut session, "slide2", advance.clone());
        for index in 3..slides {
            add_trigger(&mut session, &format!("slide{index}"), advance.clone());
        }
    }
    add_trigger(
        &mut session,
        &format!("slide{slides}"),
        serde_json::json!({
            "action": { "action": "exit_course" },
            "when": { "event": "timeline_starts" }
        }),
    );
    session
}

#[test]
fn restart_preview_button_resets_slide_clock_variables_and_ended_card() {
    let session = restart_preview_course(3);
    let mut harness = TestHarness::new(session.course);
    harness.editor.open_preview_at(None);
    if let Some(preview) = harness.editor.preview.as_mut() {
        preview.frame.open_drawer();
    }

    for _ in 0..400 {
        if harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|preview| preview.ended)
        {
            break;
        }
        harness.idle_frame();
    }

    let preview = harness.editor.preview.as_ref().expect("preview");
    assert!(preview.ended, "the course should reach its end");
    assert_eq!(preview.state.as_ref().expect("state").slide, 2);
    assert_eq!(
        preview
            .state
            .as_ref()
            .and_then(|state| state.variables.get("score")),
        Some(&coursecraft_course::Value::Number(7.0)),
        "the middle slide changes the score"
    );
    assert_eq!(preview.state.as_ref().expect("state").visited.len(), 3);
    assert!(preview.frame.ended_card_rect().is_some());
    assert!(preview.frame.drawer_open());

    let restart = harness
        .find_icon_last(coursecraft_render::Icon::ArrowClockwise)
        .expect("Restart preview button");
    harness.click_at(restart);

    let preview = harness.editor.preview.as_ref().expect("preview");
    let state = preview.state.as_ref().expect("state");
    assert_eq!(state.slide, 0, "restart goes back to the start slide");
    assert_eq!(state.base.ms, 0, "restart plays from 0 ms");
    assert_eq!(
        state.variables.get("score"),
        Some(&coursecraft_course::Value::Number(0.0)),
        "restart resets variables"
    );
    assert_eq!(state.visited.len(), 1, "restart resets visited slides");
    assert!(state.answers.is_empty(), "restart resets quiz answers");
    assert!(!preview.ended, "the ended flag is cleared");
    assert!(!preview.frame.ended, "the frame leaves the ended state");
    assert!(
        preview.frame.ended_card_rect().is_none(),
        "the ended card has gone"
    );
    assert!(
        !preview.frame.drawer_open(),
        "restart closes the outline drawer"
    );
}

#[test]
fn restart_preview_button_works_after_ending_on_the_final_slide() {
    let session = restart_preview_course(2);
    let mut harness = TestHarness::new(session.course);
    harness.editor.open_preview_at(None);

    for _ in 0..400 {
        if harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|preview| preview.ended)
        {
            break;
        }
        harness.idle_frame();
    }
    let preview = harness.editor.preview.as_ref().expect("preview");
    assert!(preview.ended);
    assert_eq!(
        preview.state.as_ref().expect("state").slide,
        1,
        "the course ends on its last slide, which has no next"
    );

    let restart = harness
        .find_icon_last(coursecraft_render::Icon::ArrowClockwise)
        .expect("Restart preview button");
    harness.click_at(restart);

    let preview = harness.editor.preview.as_ref().expect("preview");
    assert_eq!(preview.state.as_ref().expect("state").slide, 0);
    assert!(!preview.ended);
    assert!(preview.frame.ended_card_rect().is_none());
}

#[test]
fn remove_question_button_deletes_the_question_and_undo_restores_it() {
    let mut session = coursecraft_engine::Session::blank("Question");
    session
        .execute(
            "question.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "multiple_choice" }),
        )
        .expect("question.insert");
    session
        .execute(
            "question.add_feedback_layers",
            &serde_json::json!({ "slide": "slide1" }),
        )
        .expect("feedback layers");
    session
        .execute(
            "question.add_submit_button",
            &serde_json::json!({ "slide": "slide1" }),
        )
        .expect("submit button");

    let mut harness = TestHarness::new(session.course);
    harness.editor.ui.bottom_tab = BottomTab::Question;
    harness.idle_frame();
    let before = harness.editor.session.course.clone();
    let remove = harness
        .find_text("Remove question")
        .expect("Remove question button");
    harness.click_at(remove);
    harness.idle_frame();

    assert!(
        harness.editor.ui.last_error.is_none(),
        "removing a question should not show an error: {:?}",
        harness.editor.ui.last_error
    );
    assert!(
        harness
            .editor
            .session
            .course
            .slide("slide1")
            .is_some_and(|slide| slide.question.is_none())
    );

    harness
        .editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn ungroup_is_disabled_for_an_ungrouped_object_in_the_canvas_menu() {
    let mut session = coursecraft_engine::Session::blank("Ungroup");
    session
        .execute(
            "object.add",
            &serde_json::json!({
                "slide": "slide1",
                "id": "lone",
                "kind": { "type": "button", "label": "Lone" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 80 }
            }),
        )
        .expect("lone object");
    session
        .execute(
            "object.add",
            &serde_json::json!({
                "slide": "slide1",
                "id": "other",
                "kind": { "type": "button", "label": "Other" },
                "frame": { "x": 400, "y": 100, "w": 200, "h": 80 }
            }),
        )
        .expect("other object");

    let mut harness = TestHarness::new(session.course);
    harness.editor.selection.set_many(["lone"]);
    harness.idle_frame();

    assert!(
        crate::menu::adapter(&harness.editor, "object.ungroup").is_none(),
        "a single ungrouped object cannot be ungrouped"
    );
    assert_eq!(
        crate::menu::adapter_reason(&harness.editor, "object.ungroup"),
        "This object isn't in a group"
    );

    let lone = object_screen_rect(
        &harness.editor.session.course,
        "slide1",
        "lone",
        harness.central_panel_rect(),
    )
    .expect("lone screen rect");
    let revision = harness.editor.session.revision;
    harness.right_click_at(lone.center());
    harness.idle_frame();
    let ungroup = harness.find_text("Ungroup").expect("Ungroup menu item");
    harness.click_at(ungroup);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.revision, revision,
        "a disabled Ungroup menu item must not run a command"
    );
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();

    harness
        .editor
        .run(
            "object.group",
            serde_json::json!({ "slide": "slide1", "objects": ["lone", "other"] }),
        )
        .expect("group");
    harness.editor.selection.set_many(["lone"]);
    assert!(
        crate::menu::adapter(&harness.editor, "object.ungroup").is_some(),
        "a grouped object can be ungrouped"
    );
    assert_eq!(
        crate::menu::adapter_reason(&harness.editor, "object.ungroup"),
        ""
    );
}

#[test]
fn restart_preview_opened_on_a_later_slide_restarts_that_slide_subset() {
    let mut session = coursecraft_engine::Session::blank("Restart from the start");
    let scene = session.course.scenes[0].id.clone();
    session
        .execute(
            "slide.add",
            &serde_json::json!({ "scene": scene, "title": "Thank you" }),
        )
        .expect("add slide");
    let last = session.course.scenes[0].slides[1].id.clone();

    let mut preview = Preview::new(&session.course, Some(&last));
    assert!(preview.is_partial_preview());
    assert_eq!(preview.state.as_ref().map(|s| s.slide), Some(0));
    assert!(preview.log.iter().any(|l| l.contains(&last)));
    preview.restart();
    assert_eq!(preview.state.as_ref().map(|s| s.slide), Some(0));
    assert!(preview.log.iter().any(|l| l.contains(&last)));
}

#[test]
fn timeline_rows_sit_below_the_ruler_with_the_front_object_first() {
    let mut session = coursecraft_engine::Session::blank("Timeline rows");
    let slide = session.course.scenes[0].slides[0].id.clone();
    for name in ["Back shape", "Front shape"] {
        let added = session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": slide,
                    "kind": { "type": "text", "text": name },
                    "frame": { "x": 10, "y": 10, "w": 100, "h": 40 },
                }),
            )
            .expect("add");
        let id = added["id"].as_str().expect("id").to_owned();
        session
            .execute(
                "object.update",
                &serde_json::json!({ "slide": slide, "object": id, "name": name }),
            )
            .expect("name");
    }
    let mut harness = TestHarness::new(session.course);
    for _ in 0..3 {
        harness.idle_frame();
    }
    let ruler = harness.find_text("0s").expect("ruler label");
    let front = harness
        .find_text("Front shape")
        .expect("front row is visible");
    let back = harness
        .find_text("Back shape")
        .expect("back row is visible");
    assert!(front.y > ruler.y + 8.0, "front {front:?} ruler {ruler:?}");
    assert!(back.y > front.y, "front {front:?} back {back:?}");
}

#[test]
fn pausing_the_preview_holds_the_clock_across_frames() {
    let session = coursecraft_engine::Session::blank("Pause preview");
    let mut preview = Preview::new(&session.course, None);
    preview.handle(&coursecraft_logic::Input::Tick { ms: 100 }, None);
    preview.handle(&coursecraft_logic::Input::PlayerPause, None);
    let before = preview.state.as_ref().map(|s| s.base.ms);
    // `handle` rebuilds the player from its state, as each drawn frame does.
    preview.handle(&coursecraft_logic::Input::Tick { ms: 500 }, None);
    assert_eq!(preview.state.as_ref().map(|s| s.base.ms), before);
    preview.handle(&coursecraft_logic::Input::PlayerResume, None);
    preview.handle(&coursecraft_logic::Input::Tick { ms: 500 }, None);
    assert_ne!(preview.state.as_ref().map(|s| s.base.ms), before);
}

// ---------------------------------------------------------------- shape gallery

fn open_toolbar_shape_gallery(harness: &mut TestHarness) {
    harness.idle_frame();
    let shape = harness
        .find_icon(coursecraft_render::Icon::Shapes)
        .expect("toolbar Shape button");
    harness.click_at(shape);
    harness.idle_frame();
}

fn gallery_rect(harness: &TestHarness, kind: ShapeKind) -> Rect {
    harness
        .editor
        .ui
        .shape_gallery_rects
        .iter()
        .find_map(|(candidate, rect)| (*candidate == kind).then_some(*rect))
        .unwrap_or_else(|| panic!("{kind:?} is not in the gallery"))
}

#[test]
fn gallery_pick_arms_the_tool_and_a_drag_places_that_kind() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    open_toolbar_shape_gallery(&mut harness);
    // The full catalog pushes Block Arrows below the popup's first viewport,
    // so search for the kind before clicking it.
    let search = harness
        .editor
        .ui
        .shape_gallery_search_rect
        .expect("search field");
    harness.set_field(search.center(), "right arrow");
    harness.idle_frame();
    let icon = gallery_rect(&harness, ShapeKind::ArrowRight);
    harness.click_at(icon.center());
    harness.idle_frame();
    assert_eq!(
        harness.editor.ui.placement_tool,
        Some(PlacementTool::Shape(ShapeKind::ArrowRight))
    );

    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform");
    let start = transform.to_screen_pos(pos2(500.0, 300.0));
    let scale = transform.scale();
    harness.drag_by(
        start,
        vec2(180.0 * scale, 100.0 * scale),
        4,
        Modifiers::NONE,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let placed = slide.objects.last().expect("placed object");
    match &placed.kind {
        ObjectKind::Shape { shape, .. } => assert_eq!(*shape, ShapeKind::ArrowRight),
        other => panic!("expected a shape, got {other:?}"),
    }
    assert_eq!(placed.name, "Right arrow 1");
    assert_eq!(harness.editor.ui.placement_tool, None);
}

#[test]
fn gallery_search_matches_labels_and_says_when_nothing_matches() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_toolbar_shape_gallery(&mut harness);
    let search = harness
        .editor
        .ui
        .shape_gallery_search_rect
        .expect("search field");
    harness.set_field(search.center(), "chevron");
    assert!(
        harness
            .editor
            .ui
            .shape_gallery_rects
            .iter()
            .any(|(kind, _)| *kind == ShapeKind::ArrowChevron)
    );
    assert!(
        harness
            .editor
            .ui
            .shape_gallery_rects
            .iter()
            .all(|(kind, _)| *kind != ShapeKind::ArrowRight)
    );
    harness.set_field(search.center(), "zzzz");
    assert!(harness.editor.ui.shape_gallery_rects.is_empty());
    assert!(harness.find_text("No shapes match").is_some());
}

#[test]
fn inspector_shape_swap_is_one_undo_step() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.set_many(["about-panel"]);
    harness.idle_frame();
    let before = harness.editor.session.revision;
    let picker = harness
        .find_text_in("Rounded rectangle", harness.right_panel_rect())
        .expect("shape picker in the inspector");
    harness.click_at(picker);
    harness.idle_frame();
    let search = harness
        .editor
        .ui
        .properties
        .shape_gallery_search_rect
        .expect("shape search field");
    harness.set_field(search.center(), "left arrow");
    harness.idle_frame();
    let icon = harness
        .editor
        .ui
        .properties
        .shape_gallery_rects
        .iter()
        .find_map(|(kind, rect)| (*kind == ShapeKind::ArrowLeft).then_some(*rect))
        .expect("Left arrow gallery icon");
    harness.click_at(icon.center());
    harness.idle_frame();
    let shape_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| {
            slide
                .object("about-panel")
                .map(|(object, _)| object.kind.clone())
        })
        .expect("shape object");
    assert!(matches!(
        shape_after,
        ObjectKind::Shape {
            shape: ShapeKind::ArrowLeft,
            ..
        }
    ));
    assert_eq!(harness.editor.session.revision, before + 1);
    harness.editor.run("edit.undo", serde_json::json!({}));
    let restored = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| {
            slide
                .object("about-panel")
                .map(|(object, _)| object.kind.clone())
        })
        .expect("shape object");
    assert!(matches!(
        restored,
        ObjectKind::Shape {
            shape: ShapeKind::RoundedRectangle,
            ..
        }
    ));
}

#[test]
fn inspector_switches_a_shape_to_a_linear_gradient_and_adds_a_stop() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    // The panel lives on the About layer: select it the way a click there does.
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.editor.selection.set_many(["about-panel"]);
    harness.idle_frame();

    let fill_type = harness
        .find_text_in("Solid", harness.right_panel_rect())
        .expect("fill type combo in the inspector");
    harness.click_at(fill_type);
    harness.idle_frame();
    let linear = harness
        .find_text_last("Linear gradient")
        .expect("linear gradient option");
    harness.click_at(linear);
    harness.idle_frame();

    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| {
            slide
                .object("about-panel")
                .map(|(object, _)| object.kind.clone())
        })
        .expect("shape object");
    assert!(
        matches!(
            kind,
            ObjectKind::Shape {
                fill_style: Some(coursecraft_course::FillStyle::Linear { .. }),
                ..
            }
        ),
        "shape now has a linear fill: {kind:?}"
    );

    // The click on the menu item is not also a click on what lies under the menu.
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some("about-panel")
    );
    assert_eq!(harness.editor.ui.inspector_tab, InspectorTab::Format);

    let add_stop = {
        let mut found = None;
        for _ in 0..40 {
            let panel = harness.right_panel_rect();
            if let Some(position) = harness.find_text_in("Add colour", panel) {
                found = Some(position);
                break;
            }
            harness.scroll_over(panel.center(), -120.0);
        }
        found.expect("visible Add colour button")
    };
    harness.click_at(add_stop);
    harness.idle_frame();
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| {
            slide
                .object("about-panel")
                .map(|(object, _)| object.kind.clone())
        })
        .expect("shape object");
    let ObjectKind::Shape {
        fill_style: Some(coursecraft_course::FillStyle::Linear { stops, .. }),
        ..
    } = kind
    else {
        panic!("expected a linear fill");
    };
    assert_eq!(stops.len(), 3);
}

#[test]
fn layer_settings_new_options_dispatch_once_and_cancel_changes_nothing() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = Some("about".into());
    harness.idle_frame();

    let orig = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer")
        .clone();
    let gear = *harness
        .editor
        .ui
        .layers
        .gear_rects
        .get("about")
        .expect("about gear");

    // Cancel discards checkbox and dropdown changes.
    harness.click_at(gear.center());
    harness.idle_frame();
    let rev_before = harness.editor.session.revision;
    let undo_before = harness.editor.session.undo_len();
    let hide_base = harness
        .editor
        .ui
        .layer_settings
        .hide_base_rect
        .expect("hide base rect");
    harness.click_at(hide_base.center());
    let allow = harness
        .editor
        .ui
        .layer_settings
        .allow_seek_rect
        .expect("allow seek rect");
    harness.click_at(allow.center());
    harness.idle_frame();
    let no = harness.find_text_last("No").expect("No option");
    harness.click_at(no);
    let cancel = harness
        .editor
        .ui
        .layer_settings
        .cancel_rect
        .expect("cancel rect");
    harness.click_at(cancel.center());
    harness.idle_frame();
    assert_eq!(harness.editor.session.revision, rev_before);
    assert_eq!(harness.editor.session.undo_len(), undo_before);
    let unchanged = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer");
    assert_eq!(unchanged.hide_base_objects, orig.hide_base_objects);
    assert_eq!(unchanged.allow_seek, orig.allow_seek);

    // OK sends every changed option in one layer.update and one undo step.
    harness.click_at(gear.center());
    harness.idle_frame();
    let hide_base = harness
        .editor
        .ui
        .layer_settings
        .hide_base_rect
        .expect("hide base rect");
    harness.click_at(hide_base.center());
    let ends = harness
        .editor
        .ui
        .layer_settings
        .hide_when_ends_rect
        .expect("timeline ends rect");
    harness.click_at(ends.center());
    let allow = harness
        .editor
        .ui
        .layer_settings
        .allow_seek_rect
        .expect("allow seek rect");
    harness.click_at(allow.center());
    harness.idle_frame();
    let no = harness.find_text_last("No").expect("No option");
    harness.click_at(no);
    let revisit = harness
        .editor
        .ui
        .layer_settings
        .revisit_rect
        .expect("revisit rect");
    harness.click_at(revisit.center());
    harness.idle_frame();
    let resume = harness
        .find_text_last("Resume where it left off")
        .expect("resume option");
    harness.click_at(resume);
    let ok = harness.editor.ui.layer_settings.ok_rect.expect("ok rect");
    harness.click_at(ok.center());
    harness.idle_frame();

    assert_eq!(harness.editor.session.revision, rev_before + 1);
    assert_eq!(harness.editor.session.undo_len(), undo_before + 1);
    let layer_after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer");
    assert!(layer_after.hide_base_objects);
    assert!(layer_after.hide_when_timeline_ends);
    assert_eq!(layer_after.allow_seek, coursecraft_course::SeekMode::No);
    assert_eq!(layer_after.revisit, coursecraft_course::RevisitMode::Resume);

    // One undo restores every option.
    harness.editor.session.undo().expect("undo");
    let layer_undone = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.layer("about"))
        .expect("about layer");
    assert_eq!(layer_undone.hide_base_objects, orig.hide_base_objects);
    assert_eq!(
        layer_undone.hide_when_timeline_ends,
        orig.hide_when_timeline_ends
    );
    assert_eq!(layer_undone.allow_seek, orig.allow_seek);
    assert_eq!(layer_undone.revisit, orig.revisit);
}

#[test]
fn slide_revisit_dropdown_sends_slide_update() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.editor.selection.object = None;
    harness.editor.ui.inspector_tab = InspectorTab::Slide;
    harness.idle_frame();

    let automatic = harness.find_text("Automatic").expect("revisit dropdown");
    harness.click_at(automatic);
    harness.idle_frame();
    let start_over = harness
        .find_text_last("Start over")
        .expect("Start over option");
    harness.click_at(start_over);
    harness.idle_frame();

    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("s-intro")
            .revisit,
        coursecraft_course::RevisitMode::Reset
    );
}

fn path_kind(
    points: &[coursecraft_course::Point],
    closed: bool,
) -> (serde_json::Value, serde_json::Value) {
    let (frame, fractions) = coursecraft_course::path_frame_and_points(points).expect("path frame");
    (
        serde_json::json!({
            "type": "path",
            "points": fractions,
            "closed": closed,
            "fill": if closed { serde_json::json!("#2F6FEB") } else { serde_json::Value::Null },
            "outline": { "color": "#1F1F1F", "width": 2 },
        }),
        serde_json::json!({ "x": frame.x, "y": frame.y, "w": frame.w, "h": frame.h }),
    )
}

fn add_path_for_test(
    harness: &mut TestHarness,
    points: &[coursecraft_course::Point],
    closed: bool,
) -> String {
    let (kind, frame) = path_kind(points, closed);
    let result = harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s-intro",
            "kind": kind,
            "frame": frame,
            "name": "Test path",
        }),
    );
    result
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
        .expect("path id")
        .to_owned()
}

fn path_screen_point(harness: &TestHarness, id: &str, index: usize) -> Option<egui::Pos2> {
    let transform = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )?;
    let slide = harness.editor.session.course.slide("s-intro")?;
    let (object, _) = slide.object(id)?;
    let ObjectKind::Path { path } = &object.kind else {
        return None;
    };
    let point = path.points.get(index)?;
    Some(transform.to_screen_pos(pos2(
        object.frame.x + point.x * object.frame.w,
        object.frame.y + point.y * object.frame.h,
    )))
}

fn draw_transform(harness: &TestHarness) -> SlideTransform {
    SlideTransform::fit(
        canvas_fit_rect(canvas_rect(harness.central_panel_rect())),
        harness.editor.session.course.slide_size,
    )
    .expect("transform")
}

fn add_shape_for_test(harness: &mut TestHarness, x: f32, y: f32, w: f32, h: f32) -> String {
    harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": {
                    "type": "shape",
                    "shape": "rectangle",
                    "fill": "#D9D9D9"
                },
                "frame": { "x": x, "y": y, "w": w, "h": h }
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("shape id")
}

fn add_connector_for_test(
    harness: &mut TestHarness,
    start: coursecraft_course::Point,
    end: coursecraft_course::Point,
) -> String {
    let (mut kind, frame) = path_kind(&[start, end], false);
    kind["connector"] = serde_json::json!("straight");
    harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": kind,
                "frame": frame,
                "name": "Test connector",
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("connector id")
}

fn shape_site_screen(harness: &TestHarness, id: &str, site: u8) -> egui::Pos2 {
    let frame = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .map(|(object, _)| object.frame)
        .expect("shape frame");
    let point = match site % 4 {
        0 => pos2(frame.x + frame.w * 0.5, frame.y),
        1 => pos2(frame.x + frame.w, frame.y + frame.h * 0.5),
        2 => pos2(frame.x + frame.w * 0.5, frame.y + frame.h),
        _ => pos2(frame.x, frame.y + frame.h * 0.5),
    };
    draw_transform(harness).to_screen_pos(point)
}

fn connector_glues(harness: &TestHarness, id: &str) -> (Option<String>, Option<String>) {
    let object = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .map(|(object, _)| object)
        .expect("connector");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    (
        path.start_glue.as_ref().map(|glue| glue.object.clone()),
        path.end_glue.as_ref().map(|glue| glue.object.clone()),
    )
}

#[test]
fn line_tool_drags_commits_one_path_and_click_places_the_default_line() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Line));
    harness.idle_frame();
    let transform = draw_transform(&harness);
    let start = transform.to_screen_pos(pos2(400.0, 300.0));
    let scale = transform.scale();
    let revision = harness.editor.session.revision;

    harness.drag_by(
        start,
        vec2(200.0 * scale, 100.0 * scale),
        4,
        Modifiers::NONE,
    );

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let object = slide.objects.last().expect("new path");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.len(), 2);
    assert!(!path.closed);
    assert_eq!(path.connector, None);
    assert_eq!(path.outline.width, 2.0);
    assert_eq!(object.name, "Line 1");
    assert_eq!(harness.editor.session.revision, revision + 1);
    assert_eq!(harness.editor.ui.placement_tool, None);
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(object.id.as_str())
    );
    assert!(
        (object.frame.w - 200.0).abs() <= 1.0,
        "w {}",
        object.frame.w
    );
    assert!(
        (object.frame.h - 100.0).abs() <= 1.0,
        "h {}",
        object.frame.h
    );

    // A plain click places a 160 px horizontal line.
    let before = slide.objects.len();
    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Line));
    harness.idle_frame();
    harness.click_at(transform.to_screen_pos(pos2(500.0, 300.0)));
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(slide.objects.len(), before + 1);
    let object = slide.objects.last().expect("click path");
    assert!(
        (object.frame.w - 160.0).abs() <= 1.0,
        "w {}",
        object.frame.w
    );
    assert!(object.frame.h >= 1.0);
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.len(), 2);
    assert!((path.points[0].y - path.points[1].y).abs() < 0.001);
}

#[test]
fn draw_tools_snap_shift_and_set_arrow_and_connector_fields() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let transform = draw_transform(&harness);
    let start = transform.to_screen_pos(pos2(300.0, 250.0));
    let scale = transform.scale();

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Line));
    harness.idle_frame();
    harness.drag_by(
        start,
        vec2(200.0 * scale, 100.0 * scale),
        4,
        Modifiers::SHIFT,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("snapped line").kind else {
        panic!("expected a path");
    };
    let first = path.points[0];
    let last = path.points[1];
    assert!(
        ((last.x - first.x).abs() - (last.y - first.y).abs()).abs() < 0.01,
        "shift should snap to 45 degrees"
    );

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Arrow));
    harness.idle_frame();
    harness.drag_by(start, vec2(150.0 * scale, 80.0 * scale), 4, Modifiers::NONE);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("arrow").kind else {
        panic!("expected a path");
    };
    assert_eq!(path.end_end, LineEnd::Arrow);
    assert_eq!(path.connector, None);

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::ElbowConnector));
    harness.idle_frame();
    harness.drag_by(start, vec2(150.0 * scale, 80.0 * scale), 4, Modifiers::ALT);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("connector").kind else {
        panic!("expected a path");
    };
    assert_eq!(path.connector, Some(ConnectorStyle::Elbow));
    assert_eq!(path.points.len(), 2);

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::CurvedConnector));
    harness.idle_frame();
    harness.drag_by(start, vec2(150.0 * scale, 80.0 * scale), 4, Modifiers::ALT);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("curved connector").kind else {
        panic!("expected a path");
    };
    assert_eq!(path.connector, Some(ConnectorStyle::Curved));
    assert_eq!(path.points.len(), 2);
}

#[test]
fn dragging_a_connector_end_snaps_to_a_site_and_alt_disables_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let target = add_shape_for_test(&mut harness, 700.0, 400.0, 140.0, 90.0);
    let connector = add_connector_for_test(
        &mut harness,
        coursecraft_course::Point { x: 200.0, y: 300.0 },
        coursecraft_course::Point { x: 450.0, y: 380.0 },
    );
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some(connector.clone());
    harness.idle_frame();

    let start = path_screen_point(&harness, &connector, 1).expect("connector end");
    let end = shape_site_screen(&harness, &target, 3);
    harness.drag_by(start, end - start, 5, Modifiers::NONE);

    let (_, end_glue) = connector_glues(&harness, &connector);
    assert_eq!(end_glue.as_deref(), Some(target.as_str()));

    // Alt suppresses snapping, so the same gesture leaves the end free.
    let target2 = add_shape_for_test(&mut harness, 900.0, 520.0, 140.0, 90.0);
    let connector2 = add_connector_for_test(
        &mut harness,
        coursecraft_course::Point { x: 250.0, y: 320.0 },
        coursecraft_course::Point { x: 500.0, y: 400.0 },
    );
    harness.editor.selection.object = Some(connector2.clone());
    harness.idle_frame();
    let start = path_screen_point(&harness, &connector2, 1).expect("connector end");
    let end = shape_site_screen(&harness, &target2, 3);
    harness.drag_by(start, end - start, 5, Modifiers::ALT);
    let (_, end_glue) = connector_glues(&harness, &connector2);
    assert!(end_glue.is_none(), "Alt must disable site snapping");
}

#[test]
fn releasing_a_connector_end_away_from_a_site_clears_glue() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let target = add_shape_for_test(&mut harness, 700.0, 400.0, 140.0, 90.0);
    let connector = add_connector_for_test(
        &mut harness,
        coursecraft_course::Point { x: 200.0, y: 300.0 },
        coursecraft_course::Point { x: 550.0, y: 440.0 },
    );
    let glued_end = shape_site_screen(&harness, &target, 3);
    let start = path_screen_point(&harness, &connector, 0).expect("connector start");
    let transform = draw_transform(&harness);
    let start = transform.to_slide_pos(start);
    let glued_end = transform.to_slide_pos(glued_end);
    harness
        .editor
        .run(
            "object.set_path",
            serde_json::json!({
                "slide": "s-intro",
                "object": connector,
                "points": [
                    { "x": start.x, "y": start.y },
                    { "x": glued_end.x, "y": glued_end.y }
                ],
                "end_glue": { "object": target, "site": 3 }
            }),
        )
        .expect("glue connector");
    harness.editor.selection.object = Some(connector.clone());
    harness.idle_frame();
    assert_eq!(
        connector_glues(&harness, &connector).1.as_deref(),
        Some(target.as_str())
    );

    let start = path_screen_point(&harness, &connector, 1).expect("glued end");
    harness.drag_by(start, vec2(-160.0, -90.0), 5, Modifiers::NONE);
    assert!(connector_glues(&harness, &connector).1.is_none());
}

#[test]
fn a_glued_connector_body_only_selects_and_never_moves() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let target = add_shape_for_test(&mut harness, 700.0, 400.0, 140.0, 90.0);
    let connector = add_connector_for_test(
        &mut harness,
        coursecraft_course::Point { x: 200.0, y: 300.0 },
        coursecraft_course::Point { x: 550.0, y: 440.0 },
    );
    let glued_end = shape_site_screen(&harness, &target, 3);
    let start = path_screen_point(&harness, &connector, 0).expect("connector start");
    let transform = draw_transform(&harness);
    let start = transform.to_slide_pos(start);
    let glued_end = transform.to_slide_pos(glued_end);
    harness
        .editor
        .run(
            "object.set_path",
            serde_json::json!({
                "slide": "s-intro",
                "object": connector,
                "points": [
                    { "x": start.x, "y": start.y },
                    { "x": glued_end.x, "y": glued_end.y }
                ],
                "end_glue": { "object": target, "site": 3 }
            }),
        )
        .expect("glue connector");
    harness.editor.selection.object = Some(connector.clone());
    harness.idle_frame();
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&connector))
        .map(|(object, _)| object.frame)
        .expect("frame");
    let revision = harness.editor.session.revision;
    let body = path_screen_point(&harness, &connector, 0)
        .zip(path_screen_point(
            &harness,
            &connector,
            path_len(&harness, &connector),
        ))
        .map(|(a, b)| pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5))
        .expect("body point");

    harness.drag_by(body, vec2(90.0, 50.0), 4, Modifiers::NONE);

    let after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&connector))
        .map(|(object, _)| object.frame)
        .expect("frame");
    assert_eq!(after, before, "a glued connector must not body-drag");
    assert_eq!(harness.editor.session.revision, revision);
    assert_eq!(
        connector_glues(&harness, &connector).1.as_deref(),
        Some(target.as_str())
    );
}

fn path_len(harness: &TestHarness, id: &str) -> usize {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Path { path } => Some(path.points.len().saturating_sub(1)),
            _ => None,
        })
        .unwrap_or(0)
}

#[test]
fn placing_a_connector_snaps_both_ends_to_sites() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let left = add_shape_for_test(&mut harness, 300.0, 300.0, 140.0, 90.0);
    let right = add_shape_for_test(&mut harness, 700.0, 300.0, 140.0, 90.0);
    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::ElbowConnector));
    harness.idle_frame();
    let revision = harness.editor.session.revision;
    let start = shape_site_screen(&harness, &left, 1);
    let end = shape_site_screen(&harness, &right, 3);
    harness.drag_by(start, end - start, 5, Modifiers::NONE);
    assert_eq!(
        harness.editor.session.revision,
        revision + 1,
        "placement is one undo step"
    );

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let connector = slide.objects.last().expect("connector");
    let ObjectKind::Path { path } = &connector.kind else {
        panic!("expected a path");
    };
    assert_eq!(
        path.start_glue.as_ref().map(|g| g.object.as_str()),
        Some(left.as_str())
    );
    assert_eq!(
        path.end_glue.as_ref().map(|g| g.object.as_str()),
        Some(right.as_str())
    );
    assert!(path.points.len() > 2, "the elbow route is stored");
}

#[test]
fn curve_clicks_and_closes_and_freeform_keeps_clicking() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let transform = draw_transform(&harness);
    let p1 = transform.to_screen_pos(pos2(300.0, 300.0));
    let p2 = transform.to_screen_pos(pos2(400.0, 200.0));
    let p3 = transform.to_screen_pos(pos2(500.0, 320.0));

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Curve));
    harness.idle_frame();
    harness.click_at(p1);
    harness.click_at(p2);
    harness.click_at(p3);
    harness.click_at(p3);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let object = slide.objects.last().expect("curve");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.style, PathStyle::Curved);
    assert!(!path.closed);
    assert!(path.points.len() >= 3);
    assert_eq!(object.name, "Curve 1");

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Curve));
    harness.idle_frame();
    harness.click_at(p1);
    harness.click_at(p2);
    harness.click_at(p3);
    harness.click_at(p1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("closed curve").kind else {
        panic!("expected a path");
    };
    assert!(path.closed, "clicking the first point closes the curve");
    assert!(path.fill.is_some());

    // Freeform starts with a drag and then keeps taking clicks until Enter.
    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Freeform));
    harness.idle_frame();
    harness.drag_by(p1, vec2(60.0, 40.0), 5, Modifiers::NONE);
    harness.click_at(p2);
    harness.press(egui::Key::Enter, Modifiers::NONE);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("freeform").kind else {
        panic!("expected a path");
    };
    assert!(!path.closed);
    assert!(path.points.len() >= 3);
    assert_eq!(harness.editor.ui.placement_tool, None);
}

#[test]
fn escape_cancels_a_draw_and_backspace_removes_a_point() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let transform = draw_transform(&harness);
    let p1 = transform.to_screen_pos(pos2(300.0, 300.0));
    let p2 = transform.to_screen_pos(pos2(400.0, 200.0));
    let p3 = transform.to_screen_pos(pos2(500.0, 320.0));
    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .objects
        .len();

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Curve));
    harness.idle_frame();
    harness.click_at(p1);
    harness.press(egui::Key::Escape, Modifiers::NONE);
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .objects
            .len(),
        before
    );
    assert_eq!(harness.editor.ui.placement_tool, None);

    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Curve));
    harness.idle_frame();
    harness.click_at(p1);
    harness.click_at(p2);
    harness.click_at(p3);
    harness.press(egui::Key::Backspace, Modifiers::NONE);
    harness.press(egui::Key::Enter, Modifiers::NONE);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("curve").kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.len(), 2);
    assert!(!path.closed);
}

#[test]
fn scribble_simplifies_on_release() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.snap_to_guides = false;
    let transform = draw_transform(&harness);
    let start = transform.to_screen_pos(pos2(300.0, 300.0));
    let scale = transform.scale();
    harness.editor.ui.placement_tool = Some(PlacementTool::Draw(DrawTool::Scribble));
    harness.idle_frame();
    harness.drag_by(
        start,
        vec2(220.0 * scale, 20.0 * scale),
        30,
        Modifiers::NONE,
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let ObjectKind::Path { path } = &slide.objects.last().expect("scribble").kind else {
        panic!("expected a path");
    };
    assert!(!path.closed);
    assert!(path.points.len() >= 2);
    assert!(path.points.len() < 30, "RDP should simplify the drag");
    assert!(
        path.points
            .iter()
            .all(coursecraft_course::PathPoint::is_valid)
    );
    assert_eq!(harness.editor.ui.placement_tool, None);
}

#[test]
fn two_point_path_end_handles_move_one_end_as_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_path_for_test(
        &mut harness,
        &[
            coursecraft_course::Point { x: 150.0, y: 150.0 },
            coursecraft_course::Point { x: 500.0, y: 350.0 },
        ],
        false,
    );
    harness.editor.selection.set_many([id.clone()]);
    harness.idle_frame();
    let start = path_screen_point(&harness, &id, 0).expect("first end");
    let revision = harness.editor.session.revision;

    harness.drag_by(start, vec2(50.0, 40.0), 5, Modifiers::NONE);

    assert_eq!(harness.editor.session.revision, revision + 1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let (object, _) = slide.object(&id).expect("path");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    let first = path.points[0];
    let slide_x = object.frame.x + first.x * object.frame.w;
    let slide_y = object.frame.y + first.y * object.frame.h;
    let transform = draw_transform(&harness);
    let expected = transform.to_slide_pos(start);
    assert!((slide_x - (expected.x + 50.0 / transform.scale())).abs() < 2.0);
    assert!((slide_y - (expected.y + 40.0 / transform.scale())).abs() < 2.0);
}

#[test]
fn point_editing_adds_and_removes_points_with_one_undo_each() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_path_for_test(
        &mut harness,
        &[
            coursecraft_course::Point { x: 200.0, y: 200.0 },
            coursecraft_course::Point { x: 400.0, y: 200.0 },
            coursecraft_course::Point { x: 400.0, y: 400.0 },
        ],
        false,
    );
    crate::canvas::start_path_edit(&mut harness.editor, &id);
    harness.idle_frame();
    let transform = draw_transform(&harness);
    let line_mid = transform.to_screen_pos(pos2(300.0, 200.0));
    let revision = harness.editor.session.revision;
    harness.click_at(line_mid);
    harness.click_at(line_mid);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let (object, _) = slide.object(&id).expect("path");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(
        path.points.len(),
        4,
        "double-clicking the line adds a point"
    );
    assert_eq!(harness.editor.session.revision, revision + 1);

    let point = path_screen_point(&harness, &id, 0).expect("first point");
    harness.click_at(point);
    let revision = harness.editor.session.revision;
    harness.press(egui::Key::Delete, Modifiers::NONE);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let (object, _) = slide.object(&id).expect("path");
    let ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.len(), 3);
    assert_eq!(harness.editor.session.revision, revision + 1);
}

#[test]
fn editor_path_picking_ignores_the_bounding_box_off_the_line() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let button = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": { "type": "button", "label": "Under", "fill": "#2F6FEB" },
                "frame": { "x": 100, "y": 100, "w": 400, "h": 400 },
                "name": "Under",
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("button id");
    let path = add_path_for_test(
        &mut harness,
        &[
            coursecraft_course::Point { x: 100.0, y: 100.0 },
            coursecraft_course::Point { x: 500.0, y: 500.0 },
        ],
        false,
    );
    harness.editor.selection.clear_objects();
    harness.idle_frame();
    let transform = draw_transform(&harness);
    let off_line = transform.to_screen_pos(pos2(460.0, 140.0));
    harness.click_at(off_line);
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(button.as_str())
    );
    assert_ne!(
        harness.editor.selection.object.as_deref(),
        Some(path.as_str())
    );

    let on_line = transform.to_screen_pos(pos2(300.0, 300.0));
    harness.click_at(on_line);
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(path.as_str())
    );
}

#[test]
fn gallery_draw_group_arms_the_line_tool_and_hides_fixed_lines() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    open_toolbar_shape_gallery(&mut harness);
    let line = harness
        .editor
        .ui
        .shape_gallery_draw_rects
        .iter()
        .find_map(|(tool, rect)| (*tool == DrawTool::Line).then_some(*rect))
        .expect("Line draw tool in gallery");
    assert!(
        !harness
            .editor
            .ui
            .shape_gallery_rects
            .iter()
            .any(|(kind, _)| *kind == ShapeKind::LineHorizontal),
        "fixed-direction line kinds are hidden from the gallery"
    );
    harness.click_at(line.center());
    harness.idle_frame();
    assert_eq!(
        harness.editor.ui.placement_tool,
        Some(PlacementTool::Draw(DrawTool::Line))
    );
}

#[test]
fn format_panel_shows_path_controls_and_edit_points() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_path_for_test(
        &mut harness,
        &[
            coursecraft_course::Point { x: 150.0, y: 150.0 },
            coursecraft_course::Point { x: 450.0, y: 250.0 },
        ],
        false,
    );
    harness.editor.selection.set_many([id.clone()]);
    harness.idle_frame();
    assert!(harness.find_text("Begin").is_some());
    assert!(harness.find_text("End").is_some());
    assert!(harness.find_text("Closed").is_some());
    let edit = harness
        .find_text("Edit points")
        .expect("Edit points button");
    harness.click_at(edit);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .ui
            .path_edit
            .as_ref()
            .map(|edit| edit.object.as_str()),
        Some(id.as_str())
    );
}

#[test]
fn shift_resize_keeps_proportions_from_corners_and_sides() {
    use crate::canvas::shift_resize_for_test as resize;
    let orig = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), vec2(200.0, 100.0));

    // A corner dragged outward, then inward, keeps 2:1 and the opposite corner.
    let grown = resize(orig, crate::ResizeHandle::BottomRight, vec2(100.0, 10.0));
    assert_eq!(grown.size(), vec2(300.0, 150.0));
    assert_eq!(grown.min, orig.min);
    let shrunk = resize(orig, crate::ResizeHandle::BottomRight, vec2(-100.0, -5.0));
    assert_eq!(shrunk.size(), vec2(100.0, 50.0));
    let top_left = resize(orig, crate::ResizeHandle::TopLeft, vec2(-100.0, 0.0));
    assert_eq!(top_left.size(), vec2(300.0, 150.0));
    assert_eq!(top_left.max, orig.max);

    // A side handle keeps the opposite side and grows evenly along it.
    let right = resize(orig, crate::ResizeHandle::Right, vec2(-100.0, 30.0));
    assert_eq!(right.size(), vec2(100.0, 50.0));
    assert_eq!(right.min.x, orig.min.x);
    assert_eq!(right.center().y, orig.center().y);
    let top = resize(orig, crate::ResizeHandle::Top, vec2(0.0, -50.0));
    assert_eq!(top.size(), vec2(300.0, 150.0));
    assert_eq!(top.max.y, orig.max.y);
    assert_eq!(top.center().x, orig.center().x);
}

fn add_rotate_test_object(harness: &mut TestHarness) -> String {
    harness.editor.selection.slide = "s-intro".into();
    let id = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#D0D7DE" },
                "frame": { "x": 400, "y": 200, "w": 200, "h": 100 }
            }),
        )
        .and_then(|value| {
            value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("shape id");
    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();
    id
}

fn rotation_of(harness: &TestHarness, id: &str) -> f32 {
    harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .map_or(0.0, |(object, _)| object.rotation)
}

#[test]
fn dragging_the_rotate_handle_rotates_and_undoes_once() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_rotate_test_object(&mut harness);
    let central = harness.central_panel_rect();
    let rect = object_screen_rect(&harness.editor.session.course, "s-intro", &id, central)
        .expect("object screen rect");
    let centre = rect.center();
    let handle = pos2(centre.x, rect.top() - 20.0);
    let radius = centre.y - handle.y;

    let undo = harness.editor.session.undo_len();
    harness.drag_by(
        handle,
        pos2(centre.x + radius, centre.y) - handle,
        4,
        Modifiers::NONE,
    );
    harness.idle_frame();

    let rotation = rotation_of(&harness, &id);
    assert!(
        (rotation - 90.0).abs() < 1.0,
        "expected 90°, got {rotation}"
    );
    assert_eq!(harness.editor.session.undo_len(), undo + 1);
}

#[test]
fn shift_dragging_the_rotate_handle_snaps_to_15_degrees() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_rotate_test_object(&mut harness);
    let central = harness.central_panel_rect();
    let rect = object_screen_rect(&harness.editor.session.course, "s-intro", &id, central)
        .expect("object screen rect");
    let centre = rect.center();
    let handle = pos2(centre.x, rect.top() - 20.0);
    let radius = centre.y - handle.y;
    let angle = -40.0_f32.to_radians();
    let target = pos2(
        centre.x + radius * angle.cos(),
        centre.y + radius * angle.sin(),
    );

    harness.drag_by(handle, target - handle, 4, Modifiers::SHIFT);
    harness.idle_frame();

    let rotation = rotation_of(&harness, &id);
    assert!(
        (rotation - 45.0).abs() < 0.001,
        "expected a 15° snap to 45°, got {rotation}"
    );
}

#[test]
fn resizing_a_rotated_object_keeps_the_opposite_corner() {
    use crate::canvas::rotated_resize_for_test as resize;
    let orig = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), vec2(200.0, 100.0));
    let rotation = 30.0;
    let old = coursecraft_render::rotate_point(orig.max, orig.center(), rotation);
    let new = resize(
        orig,
        crate::ResizeHandle::TopLeft,
        vec2(-20.0, -10.0),
        false,
        rotation,
    );
    let anchored = coursecraft_render::rotate_point(new.max, new.center(), rotation);
    assert!(
        (anchored - old).length() < 0.01,
        "opposite corner moved from {old:?} to {anchored:?}"
    );
    assert!(new.width() > orig.width());
}

#[test]
fn shift_dragging_a_handle_on_the_canvas_keeps_the_object_proportions() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let central = harness.central_panel_rect();
    let rect = object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        "btn-start",
        central,
    )
    .expect("btn-start screen rect");
    harness.click_at(rect.center());
    harness.idle_frame();

    let before = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;
    harness.drag_by(rect.right_bottom(), vec2(80.0, 5.0), 4, Modifiers::SHIFT);
    let after = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("s-intro")
        .object("btn-start")
        .expect("btn-start")
        .0
        .frame;
    assert!(after.w > before.w, "{after:?}");
    assert!(
        (after.w / after.h - before.w / before.h).abs() < 0.03,
        "{before:?} {after:?}"
    );
}

#[test]
fn pinch_zooms_the_canvas_around_the_pointer() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let pointer = harness.central_panel_rect().center();
    let before = harness.editor.ui.last_slide_rect.expect("slide rect");
    let fit_scale = before.width() / harness.editor.session.course.slide_size.w as f32;

    harness.frame(vec![egui::Event::PointerMoved(pointer)]);
    harness.frame(vec![egui::Event::Zoom(1.5)]);
    harness.idle_frame();

    let scale = harness.editor.ui.zoom.fixed_scale().expect("a fixed scale");
    assert!(
        (scale - fit_scale * 1.5).abs() < 0.01,
        "{scale} {fit_scale}"
    );
    let after = harness.editor.ui.last_slide_rect.expect("slide rect");
    let spot = |rect: egui::Rect, scale: f32| (pointer - rect.min) / scale;
    let moved = spot(before, fit_scale) - spot(after, scale);
    assert!(moved.length() < 2.0, "{moved:?}");

    // Far past either limit it stops, and a preset still works afterwards.
    harness.frame(vec![egui::Event::Zoom(100.0)]);
    assert_eq!(harness.editor.ui.zoom.fixed_scale(), Some(4.0));
    assert_eq!(harness.editor.ui.zoom.zoom_out(), ZoomLevel::Percent200);
    harness.frame(vec![egui::Event::Zoom(0.0001)]);
    assert_eq!(harness.editor.ui.zoom.fixed_scale(), Some(0.25));
    assert_eq!(harness.editor.ui.zoom.zoom_in(), ZoomLevel::Percent50);
    assert_eq!(harness.editor.ui.zoom.label(), "25%");
}

#[test]
fn pinch_zooms_and_pans_the_timeline_and_bars_stay_out_of_the_names() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let id = "btn-start";
    let bar = harness.editor.ui.timeline.object_bar_rects[id];
    let pointer = egui::pos2(bar.center().x, bar.center().y);

    harness.frame(vec![egui::Event::PointerMoved(pointer)]);
    harness.frame(vec![egui::Event::Zoom(2.0)]);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.timeline.zoom, 2.0);
    let zoomed = harness.editor.ui.timeline.object_bar_rects[id];
    assert!((zoomed.width() - bar.width() * 2.0).abs() < 1.0);
    // The moment under the pointer stays under it.
    assert!((zoomed.center().x - bar.center().x).abs() < 1.0);

    // A sideways swipe pans, and pinching back out returns to the full view.
    harness.frame(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(-40.0, 0.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    }]);
    for _ in 0..8 {
        harness.idle_frame();
    }
    assert!(harness.editor.ui.timeline.scroll_x > 0.0);
    let scrolled = harness.editor.ui.timeline.object_bar_rects[id];
    assert!(scrolled.min.x < zoomed.min.x);

    // A press on the row name doesn't grab the bar hidden under it.
    let name_spot = harness.editor.ui.timeline.object_row_rects[id].min + vec2(20.0, 14.0);
    harness.editor.ui.timeline.scroll_x += scrolled.min.x - (name_spot.x - 10.0);
    harness.idle_frame();
    assert!(harness.editor.ui.timeline.object_bar_rects[id].contains(name_spot));
    harness.frame(vec![egui::Event::PointerMoved(name_spot)]);
    harness.frame(vec![egui::Event::PointerButton {
        pos: name_spot,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    assert!(harness.editor.ui.timeline.active_drag.is_none());
    harness.frame(vec![egui::Event::PointerButton {
        pos: name_spot,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);

    harness.frame(vec![egui::Event::PointerMoved(pointer)]);
    harness.frame(vec![egui::Event::Zoom(0.01)]);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.timeline.zoom, 1.0);
    assert_eq!(harness.editor.ui.timeline.scroll_x, 0.0);
}

#[test]
fn marker_and_panel_inspectors_are_wired() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();

    crate::interaction_insert::insert_scroll_panel(&mut harness.editor);
    harness.idle_frame();
    let panel = harness
        .editor
        .selection
        .object
        .clone()
        .expect("the panel is selected");
    assert!(
        harness
            .editor
            .ui
            .properties
            .scroll_content_height_rect
            .is_some_and(|rect| rect.width() > 0.0),
        "the inspector tracks its content-height field"
    );
    assert!(harness.editor.ui.properties.scroll_content_height > 0.0);

    crate::interaction_insert::insert_marker(&mut harness.editor);
    harness.idle_frame();
    let marker = harness
        .editor
        .selection
        .object
        .clone()
        .expect("the marker is selected");
    assert_eq!(harness.editor.ui.properties.marker_title, "Marker");
    assert_eq!(harness.editor.ui.properties.marker_icon_rects.len(), 5);
    assert!(
        harness.editor.ui.properties.marker_title_rect.is_some(),
        "the title field is tracked"
    );
    assert!(
        harness.editor.ui.properties.panel_target_rect.is_some(),
        "any object gets the Inside panel picker"
    );

    let star = harness.editor.ui.properties.marker_icon_rects[3];
    harness.click_at(star.center());
    harness.idle_frame();
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&marker))
        .map(|(object, _)| object.kind.clone())
        .expect("marker");
    assert!(matches!(
        kind,
        ObjectKind::Marker {
            icon: coursecraft_course::MarkerIcon::Star,
            ..
        }
    ));

    let title = harness
        .editor
        .ui
        .properties
        .marker_title_rect
        .expect("title field");
    harness.click_at(title.center());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("Note");
    harness.press(Key::Tab, Modifiers::NONE);
    harness.idle_frame();
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&marker))
        .map(|(object, _)| object.kind.clone())
        .expect("marker");
    assert!(
        matches!(&kind, ObjectKind::Marker { title, .. } if title == "Note"),
        "{kind:?}"
    );

    // The panel's command changed its kind, not a different object.
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&panel))
        .map(|(object, _)| object.kind.clone())
        .expect("panel");
    assert!(matches!(kind, ObjectKind::ScrollPanel { .. }));
}

#[test]
fn input_inspector_variable_picker_and_radio_options_are_wired() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    crate::input_insert::insert_radio_group(&mut harness.editor);
    harness.idle_frame();

    let id = harness
        .editor
        .selection
        .object
        .clone()
        .expect("the inserted radio group is selected");
    assert!(
        harness
            .editor
            .session
            .course
            .variables
            .iter()
            .any(|v| v.name == harness.editor.ui.properties.input_variable
                && v.value.kind() == coursecraft_course::ValueKind::Text),
        "the picker starts on the new text variable"
    );
    assert_eq!(harness.editor.ui.properties.input_options.len(), 2);
    assert!(
        harness
            .editor
            .ui
            .properties
            .input_variable_rect
            .is_some_and(|rect| rect.width() > 0.0),
        "the inspector tracks its variable picker"
    );

    // Edit both option labels through the inspector fields.
    for (index, text) in [(0usize, "Left"), (1usize, "Right")] {
        let rect = harness
            .editor
            .ui
            .properties
            .input_option_rects
            .get(index)
            .copied()
            .expect("option field rect");
        harness.click_at(rect.center());
        harness.press(Key::A, Modifiers::COMMAND);
        harness.type_text(text);
        harness.press(Key::Tab, Modifiers::NONE);
        harness.idle_frame();
    }

    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&id))
        .map(|(object, _)| object.kind.clone())
        .expect("radio group");
    assert!(matches!(
        &kind,
        ObjectKind::RadioGroup { options, .. }
            if options.first().is_some_and(|o| o.label == "Left")
                && options.get(1).is_some_and(|o| o.label == "Right")
    ));
}

fn add_crop_test_image(harness: &mut TestHarness, crop: Option<serde_json::Value>) -> String {
    harness.editor.set_media(vec![(
        "photo.png".to_owned(),
        std::sync::Arc::<[u8]>::from(test_png_bytes()),
    )]);
    let id = harness
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "kind": { "type": "image", "src": "photo.png", "alt": "A picture" },
                "frame": { "x": 600, "y": 10, "w": 200, "h": 100 }
            }),
        )
        .and_then(|result| {
            result
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .expect("image added");
    if let Some(crop) = crop {
        harness
            .editor
            .run(
                "object.set_crop",
                serde_json::json!({ "slide": "s-intro", "object": id, "crop": crop }),
            )
            .expect("crop set");
    }
    id
}

fn crop_of(harness: &TestHarness, id: &str) -> Option<coursecraft_course::CropRect> {
    match &harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(id))
        .expect("image object")
        .0
        .kind
    {
        ObjectKind::Image { crop, .. } => *crop,
        _ => None,
    }
}

fn image_screen_center(harness: &TestHarness, id: &str) -> egui::Pos2 {
    object_screen_rect(
        &harness.editor.session.course,
        "s-intro",
        id,
        harness.central_panel_rect(),
    )
    .expect("image rect")
    .center()
}

#[test]
fn double_click_an_image_enters_crop_mode_and_enter_applies_in_one_undo() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_crop_test_image(&mut harness, None);
    harness.idle_frame();
    let center = image_screen_center(&harness, &id);
    let original_frame = object_frame(&harness, "s-intro", &id);

    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(
        harness.editor.ui.crop_edit.is_some(),
        "double-click enters crop mode"
    );
    assert_eq!(
        harness.editor.selection.object.as_deref(),
        Some(id.as_str())
    );

    // Drag the right edge inward: the kept window shrinks and the frame follows.
    if let Some(session) = harness.editor.ui.crop_edit.as_mut() {
        session.edit =
            session
                .edit
                .drag_handle(crate::crop::CropHandle::Right, vec2(-40.0, 0.0), false);
    }
    let expected_frame = harness
        .editor
        .ui
        .crop_edit
        .as_ref()
        .map(|session| session.edit.frame)
        .expect("crop session");

    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.crop_edit.is_none());
    let crop = crop_of(&harness, &id).expect("crop applied");
    assert!(crop.w < 0.999 && crop.w > 0.0, "{crop:?}");
    let frame = object_frame(&harness, "s-intro", &id);
    assert!((frame.w - expected_frame.width()).abs() < 1e-3);
    assert!((frame.x - expected_frame.min.x).abs() < 1e-3);

    // One undo reverts the crop and the frame together.
    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(crop_of(&harness, &id), None);
    assert_eq!(object_frame(&harness, "s-intro", &id), original_frame);
}

#[test]
fn escape_cancels_crop_mode_without_changing_the_course() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_crop_test_image(&mut harness, None);
    harness.idle_frame();
    let before = harness.editor.session.course.clone();
    let center = image_screen_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.crop_edit.is_some());

    if let Some(session) = harness.editor.ui.crop_edit.as_mut() {
        session.edit =
            session
                .edit
                .drag_handle(crate::crop::CropHandle::Bottom, vec2(0.0, -30.0), false);
    }
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.ui.crop_edit.is_none());
    assert_eq!(harness.editor.session.course, before);
}

#[test]
fn nudges_do_not_move_an_image_while_cropping() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_crop_test_image(&mut harness, None);
    harness.idle_frame();
    let center = image_screen_center(&harness, &id);
    harness.click_at(center);
    harness.click_at(center);
    harness.idle_frame();
    assert!(harness.editor.ui.crop_edit.is_some());
    let before = object_frame(&harness, "s-intro", &id);

    for key in [Key::ArrowRight, Key::ArrowDown, Key::Delete] {
        harness.press(key, Modifiers::NONE);
    }
    harness.idle_frame();
    assert_eq!(object_frame(&harness, "s-intro", &id), before);
    assert!(harness.editor.ui.crop_edit.is_some());
}

#[test]
fn inspector_reset_clears_the_crop_and_grows_the_frame() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let id = add_crop_test_image(
        &mut harness,
        Some(serde_json::json!({ "x": 0.0, "y": 0.0, "w": 0.5, "h": 1.0 })),
    );
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some(id.clone());
    harness.editor.ui.inspector_tab = InspectorTab::Format;
    for _ in 0..3 {
        harness.idle_frame();
    }

    let reset = harness.find_text("Reset").expect("Reset button");
    harness.click_at(reset);
    harness.idle_frame();
    assert_eq!(crop_of(&harness, &id), None);
    let frame = object_frame(&harness, "s-intro", &id);
    assert!((frame.w - 400.0).abs() < 0.5, "{frame:?}");
    assert!((frame.h - 100.0).abs() < 0.5, "{frame:?}");
}

#[test]
fn trigger_form_builds_the_new_event_kinds() {
    use coursecraft_course::{Action, Event as CourseEvent};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.editor.run(
        "object.set_drag",
        serde_json::json!({
            "slide": "s-intro",
            "object": "btn-start",
            "drag": { "targets": ["title"] }
        }),
    );
    harness.idle_frame();

    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let mut form = TriggerFormState::new_for_owner(slide, None, &harness.editor.session.course);

    form.event_tag = EventKindTag::KeyPress;
    form.event_key = "arrow_down".into();
    form.event_key_object = Some("btn-start".into());
    assert_eq!(
        form.build_trigger(&harness.editor.session.course)
            .expect("key trigger")
            .when,
        CourseEvent::KeyPressed {
            key: "arrow_down".into(),
            object: Some("btn-start".into())
        }
    );

    form.event_tag = EventKindTag::DroppedOn;
    form.event_object = "btn-start".into();
    form.event_drop_target = "title".into();
    assert_eq!(
        form.build_trigger(&harness.editor.session.course)
            .expect("drop trigger")
            .when,
        CourseEvent::DroppedOn {
            object: "btn-start".into(),
            target: "title".into()
        }
    );

    form.event_tag = EventKindTag::ObjectsIntersect;
    form.event_object = "title".into();
    form.event_intersect_other = "btn-start".into();
    assert_eq!(
        form.build_trigger(&harness.editor.session.course)
            .expect("intersect trigger")
            .when,
        CourseEvent::ObjectsIntersect {
            a: "title".into(),
            b: "btn-start".into()
        }
    );

    form.event_tag = EventKindTag::ObjectsStopIntersecting;
    let trigger = form
        .build_trigger(&harness.editor.session.course)
        .expect("stop intersecting trigger");
    assert!(matches!(
        trigger.when,
        CourseEvent::ObjectsStopIntersecting { .. }
    ));
    assert!(matches!(trigger.action, Action::JumpToSlide { .. }));

    // The form round-trips an existing trigger back to the same event.
    let mut again = TriggerFormState::from_existing(
        0,
        &coursecraft_course::Trigger {
            action: Action::SubmitResults,
            when: CourseEvent::DroppedOn {
                object: "btn-start".into(),
                target: "title".into(),
            },
            condition: None,
            disabled: false,
        },
        slide,
        None,
        &harness.editor.session.course,
    );
    assert_eq!(again.event_tag, EventKindTag::DroppedOn);
    assert_eq!(again.event_object, "btn-start");
    assert_eq!(again.event_drop_target, "title");
    again.event_tag = EventKindTag::KeyPress;
    again.event_key = "x".into();
    again.event_key_object = None;
    assert_eq!(
        again
            .build_trigger(&harness.editor.session.course)
            .expect("round-trip key trigger")
            .when,
        CourseEvent::KeyPressed {
            key: "x".into(),
            object: None
        }
    );
}

#[test]
fn trigger_form_shows_the_key_picker_and_filtered_object_lists() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.editor.run(
        "object.set_drag",
        serde_json::json!({
            "slide": "s-intro",
            "object": "btn-start",
            "drag": { "targets": ["title"] }
        }),
    );
    harness.idle_frame();

    open_trigger_form(&mut harness, false);
    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.event_tag = EventKindTag::KeyPress;
        form.event_key = "arrow_left".into();
    }
    harness.idle_frame();
    harness.idle_frame();
    assert!(
        harness.find_text("arrow_left").is_some(),
        "the key picker shows the chosen key"
    );
    assert!(harness.find_text("Object (optional)").is_some());

    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.event_tag = EventKindTag::DroppedOn;
    }
    harness.idle_frame();
    harness.idle_frame();
    assert!(harness.find_text("Draggable object").is_some());
    assert!(harness.find_text("Target").is_some());

    {
        let form = harness.editor.ui.trigger_form.as_mut().expect("form");
        form.event_tag = EventKindTag::ObjectsIntersect;
    }
    harness.idle_frame();
    harness.idle_frame();
    assert!(harness.find_text("Object A").is_some());
    assert!(harness.find_text("Object B").is_some());
}

#[test]
fn inspector_draggable_toggle_writes_the_command_and_undoes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    for _ in 0..3 {
        harness.idle_frame();
    }
    // The draggable section sits near the bottom of the object inspector.
    let panel = harness.right_panel_rect();
    for _ in 0..4 {
        harness.scroll_over(panel.center(), -120.0);
        harness.idle_frame();
    }
    let label = harness.find_text("Draggable").expect("Draggable row");
    assert!(harness.find_text("Snap to center").is_none());
    harness.idle_frame();
    let toggle = harness
        .editor
        .ui
        .properties
        .drag_toggle_rect
        .expect("draggable checkbox rect");
    assert!(toggle.width() > 0.0);
    let _ = label;
    harness.click_at(toggle.center());
    harness.idle_frame();
    assert!(
        harness.editor.ui.properties.drag_enabled,
        "the inspector toggle turned on"
    );
    let drag = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object("btn-start"))
        .and_then(|(object, _)| object.drag.clone())
        .expect("drag enabled");
    assert!(drag.snap && drag.return_on_miss);
    assert!(harness.find_text("Snap to center").is_some());
    assert!(harness.find_text("Return on miss").is_some());

    harness.editor.run("edit.undo", serde_json::json!({}));
    assert!(
        harness
            .editor
            .session
            .course
            .slide("s-intro")
            .and_then(|slide| slide.object("btn-start"))
            .is_some_and(|(object, _)| object.drag.is_none())
    );
}

#[test]
fn timer_and_meter_insert_and_inspectors_are_wired() {
    use coursecraft_course::{Action, Event as CourseEvent, ObjectKind};

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();

    // Insert a timer through the Interaction menu path.
    crate::interaction_insert::insert_timer(&mut harness.editor);
    harness.idle_frame();
    let timer = harness
        .editor
        .selection
        .object
        .clone()
        .expect("the timer is selected");
    assert_eq!(harness.editor.ui.properties.timer_duration_ms, 60_000);
    assert!(
        harness
            .editor
            .ui
            .properties
            .timer_duration_rect
            .is_some_and(|rect| rect.width() > 0.0),
        "the inspector tracks its duration field"
    );
    assert!(harness.find_text("Duration").is_some());
    assert!(harness.find_text("Autostart").is_some());

    // Change the duration through the inspector field.
    let field = harness
        .editor
        .ui
        .properties
        .timer_duration_rect
        .expect("duration field");
    harness.click_at(field.center());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("45000");
    harness.press(Key::Tab, Modifiers::NONE);
    harness.idle_frame();
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&timer))
        .map(|(object, _)| object.kind.clone())
        .expect("timer");
    assert!(
        matches!(
            &kind,
            ObjectKind::Timer {
                duration_ms: 45000,
                ..
            }
        ),
        "{kind:?}"
    );

    // Undo returns to the inserted duration.
    harness.editor.run("edit.undo", serde_json::json!({}));
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&timer))
        .map(|(object, _)| object.kind.clone())
        .expect("timer");
    assert!(
        matches!(
            &kind,
            ObjectKind::Timer {
                duration_ms: 60000,
                ..
            }
        ),
        "{kind:?}"
    );

    // Insert a progress meter: it creates a new number variable.
    crate::interaction_insert::insert_progress_meter(&mut harness.editor);
    harness.idle_frame();
    let meter = harness
        .editor
        .selection
        .object
        .clone()
        .expect("the meter is selected");
    let kind = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .and_then(|slide| slide.object(&meter))
        .map(|(object, _)| object.kind.clone())
        .expect("meter");
    let ObjectKind::ProgressMeter { variable, .. } = &kind else {
        panic!("meter kind: {kind:?}");
    };
    assert_eq!(variable, "progress");
    assert!(
        harness
            .editor
            .session
            .course
            .variable("progress")
            .is_some_and(|def| def.value.kind() == coursecraft_course::ValueKind::Number)
    );
    assert_eq!(harness.editor.ui.properties.input_variable, "progress");
    assert!(harness.find_text("Show value").is_some());

    // The trigger form builds the timer actions and the timer_ends event.
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let mut form = TriggerFormState::new_for_owner(slide, None, &harness.editor.session.course);
    assert_eq!(form.timer_object, timer);
    form.action_tag = ActionTag::StartTimer;
    assert_eq!(
        form.build_trigger(&harness.editor.session.course)
            .expect("start trigger")
            .action,
        Action::StartTimer {
            timer: timer.clone()
        }
    );
    form.action_tag = ActionTag::ResetTimer;
    assert!(matches!(
        form.build_trigger(&harness.editor.session.course)
            .expect("reset trigger")
            .action,
        Action::ResetTimer { .. }
    ));
    form.event_tag = EventKindTag::TimerEnds;
    assert_eq!(
        form.build_trigger(&harness.editor.session.course)
            .expect("ends trigger")
            .when,
        CourseEvent::TimerEnds {
            timer: timer.clone()
        }
    );
    // Round trip back through the form.
    let again = TriggerFormState::from_existing(
        0,
        &coursecraft_course::Trigger {
            action: Action::PauseTimer {
                timer: timer.clone(),
            },
            when: CourseEvent::TimerEnds {
                timer: timer.clone(),
            },
            condition: None,
            disabled: false,
        },
        slide,
        None,
        &harness.editor.session.course,
    );
    assert_eq!(again.action_tag, ActionTag::PauseTimer);
    assert_eq!(again.event_tag, EventKindTag::TimerEnds);
    assert_eq!(again.timer_object, timer);
}

fn cue_ui_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "cue-ui",
        "title": "Cue UI",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [{
                    "id": "vid",
                    "name": "Video",
                    "kind": {
                        "type": "video",
                        "src": "clip.mp4",
                        "cues": [
                            { "id": "cue1", "at_ms": 1000, "name": "One" },
                            { "id": "cue2", "at_ms": 3000, "name": "Two" }
                        ]
                    },
                    "frame": { "x": 40.0, "y": 40.0, "w": 480.0, "h": 270.0 }
                }]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

fn cue_at(harness: &TestHarness, object: &str, cue: &str) -> f64 {
    harness
        .editor
        .session
        .course
        .slide("s1")
        .and_then(|slide| slide.object(object))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Video { cues, .. } => cues.iter().find(|c| c.id == cue).map(|c| c.at_ms),
            _ => None,
        })
        .expect("cue")
}

#[test]
fn timeline_cue_marker_drags_with_snapping_and_shift_unsnaps() {
    let mut harness = TestHarness::new(cue_ui_course());
    harness.editor.selection.slide = "s1".into();
    harness.editor.selection.set_many(["vid"]);
    harness.idle_frame();

    let key = ("vid".to_owned(), "cue1".to_owned());
    let marker = *harness
        .editor
        .ui
        .timeline
        .cue_marker_rects
        .get(&key)
        .expect("cue marker");
    let bar = *harness
        .editor
        .ui
        .timeline
        .object_bar_rects
        .get("vid")
        .expect("video bar");
    let duration = 5000.0;
    let delta = bar.width() * 260.0 / duration;
    harness.drag_by(marker.center(), vec2(delta, 0.0), 4, Modifiers::NONE);
    assert!(
        (cue_at(&harness, "vid", "cue1") - 1300.0).abs() < 1.0,
        "260 ms snaps to 300 ms: {}",
        cue_at(&harness, "vid", "cue1")
    );

    harness.idle_frame();
    let marker = *harness
        .editor
        .ui
        .timeline
        .cue_marker_rects
        .get(&key)
        .expect("moved cue marker");
    let delta = bar.width() * 250.0 / duration;
    harness.drag_by(marker.center(), vec2(delta, 0.0), 4, Modifiers::SHIFT);
    assert!(
        (cue_at(&harness, "vid", "cue1") - 1550.0).abs() < 5.0,
        "Shift disables snapping: {}",
        cue_at(&harness, "vid", "cue1")
    );

    harness.idle_frame();
    harness.editor.ui.timeline.cue_selected = Some(("vid".to_owned(), "cue1".to_owned()));
    harness.press(Key::Delete, Modifiers::NONE);
    harness.idle_frame();
    let slide = harness.editor.session.course.slide("s1").expect("slide");
    let ObjectKind::Video { cues, .. } = &slide.object("vid").expect("video").0.kind else {
        panic!("video");
    };
    assert_eq!(cues.len(), 1, "Delete removes the selected cue");
    assert!(cues.iter().all(|cue| cue.id != "cue1"));
}

#[test]
fn inspector_lists_cues_and_adds_one_at_the_playhead() {
    let mut harness = TestHarness::new(cue_ui_course());
    harness.editor.selection.slide = "s1".into();
    harness.editor.selection.set_many(["vid"]);
    harness.editor.ui.timeline.playhead_ms = 2000;
    harness.idle_frame();
    assert!(harness.find_text("Cues").is_some());
    let add = harness
        .find_text("Add cue at playhead")
        .expect("add cue button");
    harness.click_at(add);
    harness.idle_frame();
    let slide = harness.editor.session.course.slide("s1").expect("slide");
    let ObjectKind::Video { cues, .. } = &slide.object("vid").expect("video").0.kind else {
        panic!("video");
    };
    assert_eq!(cues.len(), 3);
    assert!(cues.iter().any(|c| (c.at_ms - 2000.0).abs() < 0.1));

    let delete = harness.find_text("Delete cue").expect("delete cue button");
    harness.click_at(delete);
    harness.idle_frame();
    let slide = harness.editor.session.course.slide("s1").expect("slide");
    let ObjectKind::Video { cues, .. } = &slide.object("vid").expect("video").0.kind else {
        panic!("video");
    };
    assert_eq!(cues.len(), 2, "the inspector deletes the first cue row");
}

#[test]
fn add_question_here_creates_a_layer_and_trigger_form_lists_video_cues() {
    let mut harness = TestHarness::new(cue_ui_course());
    harness.editor.selection.slide = "s1".into();
    harness.editor.selection.set_many(["vid"]);
    harness.editor.ui.timeline.playhead_ms = 2500;
    harness.idle_frame();
    let add = harness
        .find_text("Add question here")
        .expect("add question button");
    harness.click_at(add);
    harness.idle_frame();
    let slide = harness.editor.session.course.slide("s1").expect("slide");
    assert_eq!(slide.layers.len(), 1);
    let ObjectKind::Video { cues, .. } = &slide.object("vid").expect("video").0.kind else {
        panic!("video");
    };
    let question = cues.iter().find(|c| c.pause).expect("pausing cue");
    assert_eq!(question.name, "Question 1");
    assert!(slide.triggers.iter().any(|t| matches!(
        &t.when,
        coursecraft_course::Event::CueReached { object, cue }
            if object == "vid" && cue == &question.id
    )));

    let slide = slide.clone();
    let mut form = TriggerFormState::new_for_owner(&slide, None, &harness.editor.session.course);
    form.event_tag = EventKindTag::CueReached;
    form.event_object = "vid".to_owned();
    form.event_cue = "cue1".to_owned();
    form.action_tag = ActionTag::PauseMedia;
    form.media_object = "vid".to_owned();
    let trigger = form
        .build_trigger(&harness.editor.session.course)
        .expect("trigger");
    assert!(matches!(
        &trigger.when,
        coursecraft_course::Event::CueReached { object, cue } if object == "vid" && cue == "cue1"
    ));
    let again =
        TriggerFormState::from_existing(0, &trigger, &slide, None, &harness.editor.session.course);
    assert_eq!(again.event_tag, EventKindTag::CueReached);
    assert_eq!(again.event_cue, "cue1");

    harness.editor.ui.trigger_form = Some(form);
    harness.idle_frame();
    assert!(harness.find_text("Video cue reached").is_some());
    assert!(harness.find_text("One (1000 ms)").is_some());
}

#[test]
fn timeline_format_and_snap_helpers() {
    assert_eq!(format_timeline_ms(0), "0:00.0");
    assert_eq!(format_timeline_ms(3200), "0:03.2");
    assert_eq!(format_timeline_ms(10_000), "0:10.0");
    assert_eq!(format_timeline_ms(65_000), "1:05.0");
    assert_eq!(format_timeline_readout(3200, 10_000), "0:03.2 / 0:10.0");
    // track 200..1200 (width 1000) for 10000 ms: 1 tenth is 100 ms.
    // Cue at 2000 ms sits at x = 200 + 200 = 400.
    assert_eq!(
        snapped_playhead_ms(403.0, 200.0, 1000.0, 10_000, &[2000], false),
        2000,
        "within 6 px snaps to the cue"
    );
    assert_eq!(
        snapped_playhead_ms(403.0, 200.0, 1000.0, 10_000, &[2000], true),
        2030,
        "Shift disables snapping"
    );
    assert_eq!(
        snapped_playhead_ms(500.0, 200.0, 1000.0, 10_000, &[2000], false),
        3000,
        "away from any cue stays fractional"
    );
}

fn timeline_media_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "timeline-media-ui",
        "title": "Timeline media UI",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "duration_ms": 10000,
                    "objects": [
                        {
                            "id": "vid",
                            "kind": {
                                "type": "video",
                                "src": "clip.mp4",
                                "cues": [
                                    { "id": "cue1", "at_ms": 3000, "name": "Mid cue" }
                                ]
                            },
                            "frame": { "x": 40.0, "y": 40.0, "w": 640.0, "h": 360.0 },
                            "timing": { "start_ms": 1000, "end_ms": 6000 }
                        },
                        {
                            "id": "aud",
                            "kind": { "type": "audio", "src": "note.wav" },
                            "frame": { "x": 700.0, "y": 400.0, "w": 48.0, "h": 48.0 },
                            "timing": { "start_ms": 2000, "end_ms": 8000 }
                        },
                        {
                            "id": "txt",
                            "kind": { "type": "text", "text": "Hello" },
                            "frame": { "x": 40.0, "y": 450.0, "w": 300.0, "h": 60.0 }
                        }
                    ]
                },
                {
                    "id": "s2",
                    "title": "Two",
                    "duration_ms": 5000,
                    "objects": []
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid timeline media course")
}

fn timeline_harness_with_fake() -> TestHarness {
    use crate::timeline_playback::tests::FakeTimelineMedia;
    use std::sync::Arc;
    let mut harness = TestHarness::new(timeline_media_course());
    harness.editor.media.set_all(vec![
        (
            "clip.mp4".to_owned(),
            Arc::from(vec![1u8; 64].into_boxed_slice()),
        ),
        (
            "note.wav".to_owned(),
            Arc::from(vec![2u8; 64].into_boxed_slice()),
        ),
    ]);
    harness
        .editor
        .set_timeline_playback(crate::timeline_playback::TimelinePlayback::with_backend(
            Box::new(FakeTimelineMedia::new()) as Box<dyn crate::timeline_playback::TimelineMedia>,
        ));
    harness.editor.selection.slide = "s1".into();
    harness
}

#[test]
fn timeline_button_plays_and_pauses_through_the_harness() {
    let mut harness = timeline_harness_with_fake();
    harness.idle_frame();
    assert!(harness.find_text("0:00.0 / 0:10.0").is_some());
    assert!(!harness.editor.timeline_playback.is_playing());
    // Play from inside both windows so both tracks start at once.
    harness.editor.ui.timeline.playhead_ms = 2500;
    let button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("play button laid out");
    harness.click_at(button.center());
    harness.idle_frame();
    assert!(
        harness.editor.timeline_playback.is_playing(),
        "clicking the timeline play button starts Play"
    );
    // Playing from 0: the video and audio tracks both started.
    let calls = harness.editor.timeline_playback.backend_calls_for_test();
    assert!(
        calls.iter().any(|call| call.starts_with("play:vid")),
        "video started: {calls:?}"
    );
    let pause_button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("pause button laid out");
    harness.click_at(pause_button.center());
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "clicking again pauses"
    );
}

#[test]
fn timeline_play_and_scrub_write_no_undo_entries() {
    let mut harness = timeline_harness_with_fake();
    harness.idle_frame();
    let before_revision = harness.editor.session.revision;
    let before_course = harness.editor.session.course.clone();
    let button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("play button laid out");
    harness.click_at(button.center());
    harness.idle_frame();
    // Scrub via the playhead (a click on the ruler seeks).
    harness.editor.timeline_playback.seek_to(3500, 10000, 0);
    harness.editor.timeline_playback.flush_seeks(3500);
    harness.editor.ui.timeline.playhead_ms = 3500;
    harness.idle_frame();
    harness.press(Key::Space, Modifiers::NONE);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.revision, before_revision,
        "no undo entries after play and scrub"
    );
    assert_eq!(
        harness.editor.session.course, before_course,
        "document unchanged after play and scrub"
    );
}

#[test]
fn timeline_slide_change_and_document_edit_stop_playback() {
    let mut harness = timeline_harness_with_fake();
    harness.idle_frame();
    let button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("play button laid out");
    harness.click_at(button.center());
    harness.idle_frame();
    assert!(harness.editor.timeline_playback.is_playing());
    // Slide change stops playback.
    harness.editor.selection.slide = "s2".into();
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "selecting another slide stops playback"
    );
    // Back on s1, play again and edit the document.
    harness.editor.selection.slide = "s1".into();
    harness.idle_frame();
    let button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("play button laid out");
    harness.click_at(button.center());
    harness.idle_frame();
    assert!(harness.editor.timeline_playback.is_playing());
    harness.editor.run(
        "slide.update",
        serde_json::json!({ "slide": "s1", "title": "Renamed" }),
    );
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "a document edit stops playback"
    );
}

#[test]
fn timeline_space_does_nothing_in_a_text_field() {
    let mut harness = timeline_harness_with_fake();
    harness.idle_frame();
    // In-place text editing counts as a focused text field.
    harness.editor.ui.inline_edit = Some(crate::canvas::InlineEdit {
        slide: "s1".to_owned(),
        object: "txt".to_owned(),
        buffer: "x".to_owned(),
        original: String::new(),
        is_button: false,
        is_shape: false,
        cell: None,
        just_entered: false,
        live_runs: Vec::new(),
        live_paragraphs: Vec::new(),
        last_revision: 0,
        state_name: None,
        extra_id: None,
    });
    harness.idle_frame();
    harness.press(Key::Space, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "Space in a text field must not toggle timeline Play"
    );
    // And without a text field, Space does toggle.
    harness.editor.ui.inline_edit = None;
    harness.idle_frame();
    harness.press(Key::Space, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.timeline_playback.is_playing(),
        "Space outside a text field toggles timeline Play"
    );
    harness.press(Key::Space, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "Space toggles back to paused"
    );
}

#[test]
fn timeline_opening_preview_stops_playback() {
    let mut harness = timeline_harness_with_fake();
    harness.idle_frame();
    let button = harness
        .editor
        .ui
        .timeline
        .play_button_rect
        .expect("play button laid out");
    harness.click_at(button.center());
    harness.idle_frame();
    assert!(harness.editor.timeline_playback.is_playing());
    harness.editor.open_preview_at(Some("s1"));
    harness.idle_frame();
    assert!(
        !harness.editor.timeline_playback.is_playing(),
        "opening preview stops timeline playback"
    );
}

#[test]
fn timeline_cue_at_playhead_reads_the_playhead_the_author_sees() {
    let mut harness = timeline_harness_with_fake();
    harness.editor.selection.set_many(["vid"]);
    harness.editor.ui.timeline.playhead_ms = 3000;
    harness.idle_frame();
    let add = harness
        .find_text("Add cue at playhead")
        .expect("add cue button");
    harness.click_at(add);
    harness.idle_frame();
    let slide = harness.editor.session.course.slide("s1").expect("slide");
    let ObjectKind::Video { cues, .. } = &slide.object("vid").expect("video").0.kind else {
        panic!("video");
    };
    assert!(
        cues.iter().any(|cue| (cue.at_ms - 3000.0).abs() < 0.1),
        "the new cue sits at the visible playhead: {cues:?}"
    );
}

/// Open the Course map and return the on-screen rect of a slide's map
/// thumbnail.
fn open_map(harness: &mut TestHarness, slide: &str) -> Rect {
    harness.editor.ui.story_view = true;
    harness.idle_frame();
    harness
        .editor
        .ui
        .course_map
        .thumb_rects
        .iter()
        .find(|(id, _)| id == slide)
        .map(|(_, rect)| *rect)
        .expect("map thumbnail rect")
}

#[test]
fn story_view_toggle_toolbar_and_shortcut() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    assert!(!harness.editor.ui.story_view);
    // Toolbar toggle.
    let story = harness.find_text("Map").expect("Map toolbar toggle");
    harness.click_at(story);
    harness.idle_frame();
    assert!(harness.editor.ui.story_view, "toolbar toggle opens the map");
    assert!(harness.find_text("Jump").is_some(), "legend is shown");
    // Shortcut toggles back and forth.
    harness.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    harness.idle_frame();
    assert!(!harness.editor.ui.story_view, "Ctrl+Shift+M closes the map");
    harness.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    harness.idle_frame();
    assert!(harness.editor.ui.story_view, "Ctrl+Shift+M reopens the map");
}

#[test]
fn story_view_click_selects_and_arrows_move_and_enter_opens() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".to_owned();
    let rect = open_map(&mut harness, "s-tabs");
    harness.click_at(rect.center());
    harness.idle_frame();
    assert_eq!(
        harness.editor.selection.slide, "s-tabs",
        "click selects the slide"
    );
    // Arrows move the selection in play order.
    harness.press(Key::ArrowRight, Modifiers::NONE);
    harness.idle_frame();
    assert_eq!(harness.editor.selection.slide, "s-quiz");
    harness.press(Key::ArrowLeft, Modifiers::NONE);
    harness.idle_frame();
    assert_eq!(harness.editor.selection.slide, "s-tabs");
    // Enter opens the slide in the editor.
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();
    assert!(!harness.editor.ui.story_view, "Enter opens the slide");
    assert_eq!(harness.editor.selection.slide, "s-tabs");
}

#[test]
fn story_view_double_click_opens_the_slide() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".to_owned();
    let rect = open_map(&mut harness, "s-results");
    harness.click_at(rect.center());
    harness.click_at(rect.center());
    harness.idle_frame();
    assert!(
        !harness.editor.ui.story_view,
        "double-click opens the slide"
    );
    assert_eq!(harness.editor.selection.slide, "s-results");
}

#[test]
fn story_view_double_click_scene_title_renames_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.ui.story_view = true;
    harness.idle_frame();
    let header = harness
        .editor
        .ui
        .course_map
        .scene_header_rects
        .first()
        .map(|(_, rect)| *rect)
        .expect("scene header");
    let rev_before = harness.editor.session.revision;
    harness.click_at(header.center());
    harness.click_at(header.center());
    harness.idle_frame();
    assert!(
        harness.editor.ui.course_map.rename.is_some(),
        "double-click starts the rename"
    );
    let edit = harness
        .editor
        .ui
        .course_map
        .rename_edit_rect
        .expect("rename field");
    harness.set_field(edit.center(), "Basics");
    harness.idle_frame();
    assert_eq!(harness.editor.session.revision, rev_before + 1);
    assert_eq!(
        harness.editor.session.course.scenes[0].title, "Basics",
        "scene renamed in one undo step"
    );
    // Undo restores the title.
    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(harness.editor.session.course.scenes[0].title, "Main");
}

/// Two filled scenes side by side.
fn two_filled_scenes_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "two-filled",
        "title": "Two Filled",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [
            { "id": "sc1", "title": "First", "slides": [
                { "id": "s1", "title": "One", "duration_ms": 3000 },
                { "id": "s2", "title": "Two", "duration_ms": 3000 }
            ] },
            { "id": "sc2", "title": "Second", "slides": [
                { "id": "s3", "title": "Three", "duration_ms": 3000 }
            ] }
        ]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

/// A three-scene course: two filled scenes around an empty one.
fn two_scene_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "two-scene",
        "title": "Two Scene",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [
            { "id": "sc1", "title": "First", "slides": [
                { "id": "s1", "title": "One", "duration_ms": 3000 },
                { "id": "s2", "title": "Two", "duration_ms": 3000 }
            ] },
            { "id": "sc2", "title": "Empty", "slides": [] },
            { "id": "sc3", "title": "Third", "slides": [
                { "id": "s3", "title": "Three", "duration_ms": 3000 }
            ] }
        ]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

#[test]
fn story_view_columns_start_at_the_top() {
    let mut harness = TestHarness::new(two_filled_scenes_course());
    harness.editor.ui.story_view = true;
    harness.idle_frame();
    let thumbs = &harness.editor.ui.course_map.thumb_rects;
    let first = thumbs
        .iter()
        .find(|(id, _)| id == "s1")
        .map(|(_, r)| *r)
        .expect("s1 thumb");
    let second = thumbs
        .iter()
        .find(|(id, _)| id == "s3")
        .map(|(_, r)| *r)
        .expect("s3 thumb");
    assert_eq!(
        first.min.y, second.min.y,
        "each scene column starts at the top"
    );
    assert!(
        ((second.min.x - first.min.x)
            - (crate::course_map::MAP_COL_W + crate::course_map::MAP_COL_GAP))
            .abs()
            < 1.0,
        "columns are one col+gap apart: {} vs {}",
        second.min.x,
        first.min.x
    );
}

#[test]
fn story_view_keys_do_nothing_while_a_text_field_has_focus() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".to_owned();
    harness.editor.ui.story_view = true;
    harness.idle_frame();
    // Focus the Slide title field in the inspector.
    let field = harness
        .find_text("Slide title")
        .map(|p| p + vec2(110.0, 0.0))
        .expect("Slide title field");
    harness.click_at(field);
    harness.idle_frame();
    assert!(
        harness.ctx.egui_wants_keyboard_input(),
        "text field owns the keyboard"
    );
    // Arrows must not leave the field, Enter must not close the Course map.
    harness.press(Key::ArrowRight, Modifiers::NONE);
    harness.idle_frame();
    assert_eq!(harness.editor.selection.slide, "s-intro");
    assert!(harness.editor.ui.story_view);
    harness.press(Key::Enter, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.ui.story_view,
        "Enter in a text field does not close the Course map"
    );
}

#[test]
fn story_view_drag_can_land_in_an_empty_scene() {
    let mut harness = TestHarness::new(two_scene_course());
    harness.editor.ui.story_view = true;
    harness.idle_frame();
    let from = harness
        .editor
        .ui
        .course_map
        .thumb_rects
        .iter()
        .find(|(id, _)| id == "s1")
        .map(|(_, r)| *r)
        .expect("s1 thumb")
        .center();
    let header = harness
        .editor
        .ui
        .course_map
        .scene_header_rects
        .iter()
        .find(|(id, _)| id == "sc2")
        .map(|(_, r)| *r)
        .expect("empty scene header");
    harness.drag_by(from, header.center() - from, 4, Modifiers::NONE);
    harness.idle_frame();
    let empty_scene = harness
        .editor
        .session
        .course
        .scenes
        .iter()
        .find(|scene| scene.id == "sc2")
        .expect("sc2");
    let ids: Vec<&str> = empty_scene.slides.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["s1"], "drop lands at index 0 of the empty scene");
}

#[test]
fn story_view_toggle_does_nothing_in_master_view() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    assert!(!harness.editor.ui.story_view);
    // Entering Master View leaves the Course map.
    harness.editor.ui.story_view = true;
    harness.editor.enter_master_view();
    assert!(
        !harness.editor.ui.story_view,
        "entering masters leaves Story"
    );
    assert!(harness.editor.ui.master_view);
    // Toolbar toggle does nothing.
    let story = harness.find_text("Map").expect("Map toolbar toggle");
    harness.click_at(story);
    harness.idle_frame();
    assert!(
        !harness.editor.ui.story_view,
        "toolbar toggle is inert in masters"
    );
    // Shortcut does nothing.
    harness.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    harness.idle_frame();
    assert!(
        !harness.editor.ui.story_view,
        "Ctrl+Shift+M is inert in masters"
    );
    assert!(harness.editor.ui.master_view);
}

// ---------------------------------------------------------------- question banks (M13 Step 3d)

/// A tall window, so the whole slides panel (scenes + banks) fits without scrolling.
fn bank_harness() -> TestHarness {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new_with_size(course, (1280.0, 1400.0));
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    harness
}

fn create_bank(harness: &mut TestHarness) -> String {
    harness
        .editor
        .run("bank.create", serde_json::json!({}))
        .expect("bank.create")["id"]
        .as_str()
        .expect("bank id")
        .to_owned()
}

fn add_bank_question(harness: &mut TestHarness, bank: &str, title: &str) -> String {
    harness
        .editor
        .run(
            "bank.add_question",
            serde_json::json!({ "bank": bank, "title": title }),
        )
        .expect("bank.add_question")["id"]
        .as_str()
        .expect("slide id")
        .to_owned()
}

fn bank_header_pos(harness: &mut TestHarness) -> egui::Pos2 {
    let title = harness.editor.session.course.banks[0].title.clone();
    let panel = harness.left_panel_rect();
    // A draw slide takes its bank's title, so the title text occurs once for
    // the draw slide and once for the bank header; the header is painted last.
    harness
        .find_texts(&title)
        .into_iter()
        .rfind(|pos| panel.contains(*pos))
        .expect("bank header")
}

fn open_bank_menu(harness: &mut TestHarness, item: &str) -> egui::Pos2 {
    let header = bank_header_pos(harness);
    harness.right_click_at(header);
    harness.idle_frame();
    harness.find_text(item).expect("bank menu item")
}

#[test]
fn question_bank_plus_creates_bank_and_add_question_selects_it() {
    let mut harness = bank_harness();
    let plus = harness
        .find_text_in("+", harness.left_panel_rect())
        .expect("banks + button");
    harness.click_at(plus);
    harness.idle_frame();
    assert_eq!(harness.editor.session.course.banks.len(), 1);
    assert_eq!(
        harness.editor.session.course.banks[0].title,
        "Question Bank 1"
    );

    let add = open_bank_menu(&mut harness, "Add question");
    harness.click_at(add);
    harness.idle_frame();
    let bank = &harness.editor.session.course.banks[0];
    assert_eq!(bank.slides.len(), 1);
    let new_slide = bank.slides[0].id.clone();
    assert_eq!(harness.editor.selection.slide, new_slide);
    assert!(
        harness
            .editor
            .session
            .course
            .bank_slide(&new_slide)
            .expect("bank slide")
            .question
            .is_some()
    );

    // Editing the prompt in the Question panel changes the bank slide.
    harness.editor.ui.bottom_tab = BottomTab::Question;
    harness.idle_frame();
    let prompt_rect = harness
        .editor
        .ui
        .question
        .prompt_edit_rect
        .expect("prompt rect");
    let prompt_label = harness.find_text("Prompt").expect("Prompt label");
    harness.set_multiline_field(prompt_rect.center(), "Edited bank prompt", prompt_label);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .bank_slide(&new_slide)
            .expect("bank slide")
            .question
            .as_ref()
            .expect("question")
            .prompt,
        "Edited bank prompt"
    );
}

#[test]
fn question_bank_double_click_renames_it() {
    let mut harness = bank_harness();
    let _bank_id = create_bank(&mut harness);
    harness.idle_frame();
    let title = harness.editor.session.course.banks[0].title.clone();
    let header = harness
        .find_text_in(&title, harness.left_panel_rect())
        .expect("bank header");
    harness.click_at(header);
    harness.click_at(header);
    harness.idle_frame();
    assert!(harness.editor.ui.bank_rename.is_some());
    let edit = harness.find_text_containing(&title).expect("rename field");
    harness.set_field(edit, "Renamed Bank");
    harness.idle_frame();
    assert_eq!(harness.editor.session.course.banks[0].title, "Renamed Bank");
}

#[test]
fn draw_from_bank_inserts_draw_slide_and_inspector_edits_it() {
    let mut harness = bank_harness();
    let bank_id = create_bank(&mut harness);
    add_bank_question(&mut harness, &bank_id, "Bank Q1");
    add_bank_question(&mut harness, &bank_id, "Bank Q2");
    harness.idle_frame();

    let draw_item = open_bank_menu(&mut harness, "Draw from this bank");
    harness.click_at(draw_item);
    harness.idle_frame();
    let draw_slide = harness
        .editor
        .session
        .course
        .slides()
        .find(|slide| slide.draw.is_some())
        .expect("draw slide")
        .id
        .clone();
    let draw = harness
        .editor
        .session
        .course
        .slide(&draw_slide)
        .expect("draw slide")
        .draw
        .clone()
        .expect("draw");
    assert_eq!(draw.bank, bank_id);
    assert_eq!(draw.count, 1);
    assert_eq!(harness.editor.selection.slide, draw_slide);

    // The Question panel and the toolbar's Question action are hidden.
    harness.editor.ui.bottom_tab = BottomTab::Question;
    harness.idle_frame();
    assert!(harness.find_text("Prompt").is_none());
    let toolbar_rect = Rect::from_min_max(
        pos2(0.0, TOP_BAR_HEIGHT),
        pos2(
            harness.screen_rect.max.x,
            TOP_BAR_HEIGHT + crate::toolbar::TOOLBAR_HEIGHT,
        ),
    );
    assert!(harness.find_text_in("Question", toolbar_rect).is_none());
    harness.editor.ui.bottom_tab = BottomTab::Timeline;
    harness.idle_frame();

    // The inspector's count updates the draw.
    let count_rect = harness
        .editor
        .ui
        .properties
        .draw_count_rect
        .expect("draw count rect");
    harness.set_field(count_rect.center(), "2");
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(&draw_slide)
            .expect("draw slide")
            .draw
            .as_ref()
            .expect("draw")
            .count,
        2
    );

    // The count clamps to the bank size (2).
    let count_rect = harness
        .editor
        .ui
        .properties
        .draw_count_rect
        .expect("draw count rect");
    harness.set_field(count_rect.center(), "99");
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(&draw_slide)
            .expect("draw slide")
            .draw
            .as_ref()
            .expect("draw")
            .count,
        2
    );

    // The bank combo retargets the draw (the new bank needs enough questions
    // for the current count).
    let other = create_bank(&mut harness);
    add_bank_question(&mut harness, &other, "Other Q1");
    add_bank_question(&mut harness, &other, "Other Q2");
    harness.idle_frame();
    let combo_label = harness
        .editor
        .session
        .course
        .bank(&bank_id)
        .expect("bank")
        .title
        .clone();
    let other_title = harness
        .editor
        .session
        .course
        .bank(&other)
        .expect("bank")
        .title
        .clone();
    // Adding the other bank's questions selected one of them; go back to the draw slide.
    harness.editor.selection.slide = draw_slide.clone();
    harness.idle_frame();
    let combo = harness
        .find_text_in(&combo_label, harness.right_panel_rect())
        .expect("draw bank combo");
    harness.click_at(combo);
    harness.idle_frame();
    let item = harness
        .find_text_last(&other_title)
        .expect("other bank in popup");
    harness.click_at(item);
    harness.idle_frame();
    assert_eq!(
        harness
            .editor
            .session
            .course
            .slide(&draw_slide)
            .expect("draw slide")
            .draw
            .as_ref()
            .expect("draw")
            .bank,
        other
    );
}

#[test]
fn move_to_bank_moves_a_question_slide() {
    let mut harness = bank_harness();
    let bank_id = create_bank(&mut harness);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .session
            .course
            .slides()
            .any(|slide| slide.id == "s-quiz")
    );
    let thumb = slide_thumbnail_rect(
        &harness.editor.session.course,
        harness.left_panel_rect(),
        "s-quiz",
    )
    .expect("s-quiz thumbnail");
    harness.right_click_at(thumb.center());
    harness.idle_frame();
    let submenu = harness
        .find_text("Move to bank ▸")
        .expect("Move to bank submenu");
    harness.click_at(submenu);
    harness.idle_frame();
    let bank_title = harness
        .editor
        .session
        .course
        .bank(&bank_id)
        .expect("bank")
        .title
        .clone();
    let item = harness
        .find_text_last(&bank_title)
        .expect("bank in submenu");
    harness.click_at(item);
    harness.idle_frame();
    assert!(
        !harness
            .editor
            .session
            .course
            .slides()
            .any(|slide| slide.id == "s-quiz"),
        "s-quiz left the scenes"
    );
    assert!(
        harness
            .editor
            .session
            .course
            .bank(&bank_id)
            .expect("bank")
            .slides
            .iter()
            .any(|slide| slide.id == "s-quiz"),
        "s-quiz joined the bank"
    );
}

#[test]
fn deleting_a_drawn_bank_shows_the_error_and_keeps_it() {
    let mut harness = bank_harness();
    let bank_id = create_bank(&mut harness);
    add_bank_question(&mut harness, &bank_id, "Bank Q1");
    harness
        .editor
        .run(
            "draw.insert",
            serde_json::json!({ "bank": bank_id, "count": 1 }),
        )
        .expect("draw.insert");
    harness.idle_frame();

    let delete = open_bank_menu(&mut harness, "Delete");
    harness.click_at(delete);
    harness.idle_frame();
    assert!(
        harness.editor.session.course.bank(&bank_id).is_some(),
        "a drawn bank must be kept"
    );
    let (msg, _) = harness.editor.ui.last_error.clone().expect("engine error");
    assert!(msg.contains("drawn from"), "error shown: {msg}");
}

#[test]
fn deleting_a_selected_bank_slide_leaves_a_valid_selection() {
    let mut harness = bank_harness();
    let bank_id = create_bank(&mut harness);
    let bank_slide = add_bank_question(&mut harness, &bank_id, "Bank Q1");
    harness.editor.selection.slide = bank_slide.clone();
    harness.idle_frame();
    // Expand the bank and delete its slide from the row menu.
    let header = bank_header_pos(&mut harness);
    harness.click_at(header);
    harness.idle_frame();
    let pos = harness
        .find_text_in("Bank Q1", harness.left_panel_rect())
        .expect("bank slide title");
    harness.right_click_at(pos);
    harness.idle_frame();
    let delete = harness.find_text("Delete").expect("Delete menu item");
    harness.click_at(delete);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .session
            .course
            .bank(&bank_id)
            .expect("bank")
            .slides
            .is_empty()
    );
    assert!(
        harness
            .editor
            .session
            .course
            .slides()
            .any(|slide| slide.id == harness.editor.selection.slide),
        "selection must be a real slide"
    );
}

#[test]
fn story_view_and_preview_do_not_crash_with_a_bank_slide_selected() {
    let mut harness = bank_harness();
    let bank_id = create_bank(&mut harness);
    let bank_slide = add_bank_question(&mut harness, &bank_id, "Bank Q1");
    harness.editor.selection.slide = bank_slide.clone();
    harness.idle_frame();

    harness.editor.ui.story_view = true;
    harness.idle_frame();
    assert!(
        !harness
            .editor
            .ui
            .course_map
            .thumb_rects
            .iter()
            .any(|(id, _)| id == &bank_slide),
        "Course map lists only scene slides"
    );

    harness.editor.open_preview();
    harness.idle_frame();
    let preview = harness.editor.preview.as_ref().expect("preview");
    assert!(preview.is_valid());
    assert_eq!(preview.state.as_ref().map(|state| state.slide), Some(0));
    assert_ne!(
        preview.course.start_slide.as_deref(),
        Some(bank_slide.as_str())
    );
}

fn media_inspector_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "media-inspector",
        "title": "Media inspector",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "One",
                "duration_ms": 10000,
                "objects": [
                    {
                        "id": "aud",
                        "name": "Narration",
                        "kind": { "type": "audio", "src": "note.wav" },
                        "frame": { "x": 40.0, "y": 40.0, "w": 48.0, "h": 48.0 }
                    },
                    {
                        "id": "vid",
                        "name": "Clip",
                        "kind": { "type": "video", "src": "clip.mp4" },
                        "frame": { "x": 40.0, "y": 120.0, "w": 640.0, "h": 360.0 }
                    }
                ]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid media inspector course")
}

fn media_inspector_harness(object: &str) -> TestHarness {
    let mut harness = TestHarness::new(media_inspector_course());
    harness.editor.selection.slide = "s1".into();
    harness.editor.selection.set_many([object]);
    harness.idle_frame();
    harness
}

fn media_kind(harness: &TestHarness, object: &str) -> ObjectKind {
    harness
        .editor
        .session
        .course
        .slide("s1")
        .and_then(|slide| slide.object(object))
        .map(|(obj, _)| obj.kind.clone())
        .expect("media object")
}

#[test]
fn media_inspector_volume_slider_commits_half_volume() {
    let mut harness = media_inspector_harness("aud");
    assert!(harness.find_text("Volume").is_some());
    let rect = harness
        .editor
        .ui
        .properties
        .media_volume_rect
        .expect("volume slider laid out");
    assert!(rect.width() > 0.0);
    // Type 50 into the slider's value box; committing it writes half volume.
    let value = harness
        .find_text_in("100", rect)
        .expect("the slider shows its value");
    harness.set_field(value, "50");
    harness.idle_frame();
    let ObjectKind::Audio { volume, .. } = media_kind(&harness, "aud") else {
        panic!("audio");
    };
    assert!(
        (volume - 0.5).abs() < 0.001,
        "the slider writes half volume, got {volume}"
    );
}

#[test]
fn media_inspector_trim_text_commits_and_reset_clears_it() {
    let mut harness = media_inspector_harness("aud");
    let start_rect = harness
        .editor
        .ui
        .properties
        .media_trim_start_rect
        .expect("trim start field");
    let end_rect = harness
        .editor
        .ui
        .properties
        .media_trim_end_rect
        .expect("trim end field");
    assert!(harness.find_text("Trim").is_some());
    assert!(
        harness
            .find_text("End empty = to the end of the file")
            .is_some()
    );

    harness.click_at(start_rect.center());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("0:05.0");
    harness.press(Key::Tab, Modifiers::NONE);
    harness.idle_frame();

    harness.click_at(end_rect.center());
    harness.press(Key::A, Modifiers::COMMAND);
    harness.type_text("0:12.5");
    harness.press(Key::Tab, Modifiers::NONE);
    harness.idle_frame();

    let ObjectKind::Audio { trim, .. } = media_kind(&harness, "aud") else {
        panic!("audio");
    };
    assert_eq!(
        trim,
        Some(coursecraft_course::Trim {
            start_ms: 5000,
            end_ms: Some(12500),
        })
    );

    let reset = harness.find_text("Reset").expect("Reset button");
    harness.click_at(reset);
    harness.idle_frame();
    let ObjectKind::Audio { trim, .. } = media_kind(&harness, "aud") else {
        panic!("audio");
    };
    assert_eq!(trim, None, "Reset clears the trim");
}

#[test]
fn parse_and_format_clip_time() {
    use crate::properties::{format_clip_time, parse_clip_time};

    assert_eq!(parse_clip_time("0"), Some(0));
    assert_eq!(parse_clip_time("5"), Some(5000));
    assert_eq!(parse_clip_time("0:05"), Some(5000));
    assert_eq!(parse_clip_time("0:05.0"), Some(5000));
    assert_eq!(parse_clip_time("0:12.5"), Some(12500));
    assert_eq!(parse_clip_time("1:02.3"), Some(62300));
    assert_eq!(parse_clip_time("1:02:03"), Some(3_723_000));
    assert_eq!(parse_clip_time(" 0:07.5 "), Some(7500));

    assert_eq!(parse_clip_time(""), None);
    assert_eq!(parse_clip_time("abc"), None);
    assert_eq!(parse_clip_time("1:2:3:4"), None);
    assert_eq!(parse_clip_time("0:60"), None);
    assert_eq!(parse_clip_time("1:60:00"), None);
    assert_eq!(parse_clip_time("0:05.x"), None);
    assert_eq!(parse_clip_time("-5"), None);
    assert_eq!(parse_clip_time("0:05.1234"), None);
    assert_eq!(parse_clip_time(":"), None);

    assert_eq!(format_clip_time(0), "0:00.0");
    assert_eq!(format_clip_time(5000), "0:05.0");
    assert_eq!(format_clip_time(12500), "0:12.5");
    assert_eq!(format_clip_time(65000), "1:05.0");
}

#[test]
fn timeline_labels_a_trimmed_media_row() {
    let mut harness = TestHarness::new(media_inspector_course());
    harness.editor.selection.slide = "s1".into();
    harness.idle_frame();
    assert!(harness.find_text("Clip").is_some());
    assert!(harness.find_text("Narration").is_some());

    harness.editor.run(
        "object.set_media",
        serde_json::json!({
            "slide": "s1",
            "object": "aud",
            "trim": { "start_ms": 1000, "end_ms": 2000 },
        }),
    );
    harness.idle_frame();
    assert!(
        harness.find_text("Narration (trimmed)").is_some(),
        "a trimmed media row says so"
    );
    assert!(harness.find_text("Clip").is_some());
}

// ---------------------------------------------------------------- Record Narration

fn record_narration_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "record-narration-ui",
        "title": "Record Narration UI",
        "scenes": [{
            "id": "sc-one",
            "title": "Scene",
            "slides": [{
                "id": "s-one",
                "title": "Slide",
                "objects": [],
            }],
        }],
    });
    Course::from_json(&json.to_string()).expect("valid record narration course")
}

struct FakeRecorder {
    level: f32,
    elapsed: u64,
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

impl FakeRecorder {
    fn one_second() -> Self {
        Self {
            level: 0.5,
            elapsed: 1000,
            sample_rate: 1000,
            channels: 1,
            samples: vec![0; 1000],
        }
    }
}

impl coursecraft_playback::record::RecorderBackend for FakeRecorder {
    fn level(&self) -> f32 {
        self.level
    }

    fn elapsed_ms(&self) -> u64 {
        self.elapsed
    }

    fn stop(
        self: Box<Self>,
    ) -> Result<coursecraft_playback::record::Recording, coursecraft_playback::record::RecordError>
    {
        if self.samples.is_empty() {
            return Err(coursecraft_playback::record::RecordError::Empty);
        }
        Ok(coursecraft_playback::record::Recording {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples: self.samples,
            truncated: false,
        })
    }
}

fn open_record_narration(h: &mut TestHarness) {
    h.editor.open_record_narration();
    h.idle_frame();
}

fn record_and_stop(h: &mut TestHarness) {
    crate::record_narration::start_with(&mut h.editor, Box::new(FakeRecorder::one_second()));
    h.idle_frame();
    let stop = h.find_text("Stop").expect("Stop button");
    h.click_at(stop);
    h.idle_frame();
}

#[test]
fn opening_record_narration_lists_devices_without_panicking() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);

    assert!(h.editor.ui.record_narration.open);
    assert!(
        h.find_text("Default").is_some(),
        "the default device is listed"
    );
    assert!(h.find_text("Record").is_some(), "Record is offered");
    assert!(
        h.find_text("Records from the microphone into this slide as narration.")
            .is_some()
    );
}

#[test]
fn record_then_stop_keeps_a_take_and_shows_its_length() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);
    record_and_stop(&mut h);

    let take = h
        .editor
        .ui
        .record_narration
        .take
        .as_ref()
        .expect("a finished take");
    assert_eq!(take.channels, 1);
    assert!(h.find_text("Take: 0:01").is_some(), "the take's length");
}

#[test]
fn use_recording_adds_one_undoable_narration_wav() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);
    record_and_stop(&mut h);

    let before = h.editor.undo_len_for_test();
    let use_recording = h.find_text("Use recording").expect("Use recording button");
    h.click_at(use_recording);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before + 1, "one undo step");
    assert!(!h.editor.ui.record_narration.open, "the window closes");
    assert!(
        h.editor
            .media()
            .iter()
            .any(|(name, _)| name == "narration-1.wav"),
        "the take is narration-1.wav"
    );
    let narration_is_placed = {
        let slide = h.editor.session.course.slide("s-one").expect("slide");
        slide.objects.iter().any(|object| {
            matches!(
                &object.kind,
                ObjectKind::Audio { src, autoplay: true, .. } if src == "narration-1.wav"
            )
        })
    };
    assert!(narration_is_placed, "autoplaying narration audio");

    let media_before_undo = h.editor.media().len();
    h.editor.run("edit.undo", serde_json::json!({}));
    let slide = h.editor.session.course.slide("s-one").expect("slide");
    assert!(
        !slide.objects.iter().any(|object| matches!(
            &object.kind,
            ObjectKind::Audio { src, .. } if src == "narration-1.wav"
        )),
        "undo removes the narration object"
    );
    assert_eq!(h.editor.media().len(), media_before_undo);
}

#[test]
fn a_second_take_becomes_narration_2_wav() {
    let mut h = TestHarness::new(record_narration_course());

    open_record_narration(&mut h);
    record_and_stop(&mut h);
    let use_recording = h.find_text("Use recording").expect("Use recording");
    h.click_at(use_recording);
    h.idle_frame();

    open_record_narration(&mut h);
    record_and_stop(&mut h);
    let use_recording = h.find_text("Use recording").expect("Use recording");
    h.click_at(use_recording);
    h.idle_frame();

    assert!(
        h.editor
            .media()
            .iter()
            .any(|(name, _)| name == "narration-1.wav")
    );
    assert!(
        h.editor
            .media()
            .iter()
            .any(|(name, _)| name == "narration-2.wav"),
        "the second take is narration-2.wav"
    );
}

#[test]
fn re_record_discards_the_take() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);
    record_and_stop(&mut h);
    assert!(h.editor.ui.record_narration.take.is_some());

    let rerecord = h.find_text("Re-record").expect("Re-record button");
    h.click_at(rerecord);
    h.idle_frame();

    assert!(h.editor.ui.record_narration.take.is_none());
    assert!(h.editor.ui.record_narration.wav.is_none());
    assert!(h.find_text("Record").is_some(), "back to idle");
}

#[test]
fn escape_while_recording_stops_and_leaves_no_take() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);
    crate::record_narration::start_with(&mut h.editor, Box::new(FakeRecorder::one_second()));
    h.idle_frame();

    h.press(Key::Escape, Modifiers::NONE);

    assert!(!h.editor.ui.record_narration.open);
    assert!(h.editor.ui.record_narration.recorder.is_none());
    assert!(h.editor.ui.record_narration.take.is_none());
}

#[test]
fn no_microphone_shows_a_plain_message() {
    let mut h = TestHarness::new(record_narration_course());
    open_record_narration(&mut h);

    crate::record_narration::start_with_error(
        &mut h.editor,
        coursecraft_playback::record::RecordError::NoDevice,
    );
    h.idle_frame();

    assert!(h.find_text("No microphone found").is_some());
}

#[test]
fn record_narration_is_refused_in_master_view() {
    let mut h = TestHarness::new(record_narration_course());
    h.editor.ui.master_view = true;

    h.editor.open_record_narration();

    assert!(!h.editor.ui.record_narration.open);
    assert_eq!(
        h.editor
            .ui
            .status_message
            .as_ref()
            .map(|(message, _)| message.as_str()),
        Some("Not available in slide masters")
    );
}

fn open_export_video_from_file_menu(harness: &mut TestHarness) {
    let file = harness.find_text("File").expect("File menu");
    harness.click_at(file);
    harness.idle_frame();
    let export = harness.find_text_last("Export").expect("Export submenu");
    harness.click_at(export);
    harness.idle_frame();
    let item = harness
        .find_text_last("Video (MP4)…")
        .expect("Video (MP4)… menu item");
    harness.click_at(item);
    harness.idle_frame();
}

#[test]
fn export_video_opens_from_the_file_menu() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    open_export_video_from_file_menu(&mut harness);
    assert!(
        harness.editor.ui.show_export_video,
        "Export Video dialog opened"
    );
    assert!(
        harness.find_text("Export video").is_some(),
        "dialog title is painted"
    );
}

#[test]
fn export_video_1080p_queues_export_mp4() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_export_dialog();
    harness.idle_frame();

    let hd = harness.find_text_last("1080p").expect("1080p radio label");
    harness.click_at(hd);
    harness.idle_frame();
    assert_eq!(harness.editor.ui.export_width, 1920);

    let export = harness.find_text_last("Export…").expect("Export button");
    let requests = click_for_requests(&mut harness, export);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::ExportMp4 {
                width: 1920,
                fps: 30
            }
        )),
        "1080p at 30 fps is sent with the request: {requests:?}"
    );
}

#[test]
fn export_job_view_shows_progress_and_cancel_sets_the_flag() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    let cancel = Arc::new(AtomicBool::new(false));
    harness.editor.ui.export_job = Some(ExportJobView {
        slide_index: 2,
        slide_total: 12,
        frames: 90,
        cancel: Arc::clone(&cancel),
    });
    harness.idle_frame();
    assert!(
        harness
            .find_text_containing("Exporting video: slide 3 of 12")
            .is_some(),
        "progress sentence is painted"
    );

    let button = harness.find_text_last("Cancel").expect("Cancel button");
    harness.click_at(button);
    assert!(cancel.load(Ordering::Relaxed), "Cancel sets the flag");
}

#[test]
fn export_video_is_disabled_on_the_web() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.set_file_ops_available(false);
    harness.idle_frame();
    harness.editor.open_export_dialog();
    harness.idle_frame();

    assert!(
        harness
            .find_text_containing("Available in the desktop app")
            .is_some(),
        "the web note is painted"
    );
    let export = harness.find_text_last("Export…").expect("Export button");
    let requests = click_for_requests(&mut harness, export);
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, EditorRequest::ExportMp4 { .. })),
        "the web build must not queue an export"
    );
}

fn open_export_review_from_file_menu(harness: &mut TestHarness, item_label: &str) {
    let file = harness.find_text("File").expect("File menu");
    harness.click_at(file);
    harness.idle_frame();
    let export = harness.find_text_last("Export").expect("Export submenu");
    harness.click_at(export);
    harness.idle_frame();
    let item = harness
        .find_text_last(item_label)
        .expect("review menu item");
    harness.click_at(item);
    harness.idle_frame();
}

#[test]
fn export_review_menu_items_open_the_dialog_with_the_right_format() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();

    open_export_review_from_file_menu(&mut harness, "PDF for review…");
    assert!(harness.editor.ui.show_export_review, "the dialog opened");
    assert_eq!(harness.editor.ui.review_format, ReviewFormat::Pdf);
    assert!(
        harness.find_text("Export for review").is_some(),
        "dialog title is painted"
    );

    harness.editor.ui.show_export_review = false;
    harness.idle_frame();
    open_export_review_from_file_menu(&mut harness, "Word for review…");
    assert_eq!(harness.editor.ui.review_format, ReviewFormat::Word);
    assert!(harness.editor.ui.show_export_review);
}

#[test]
fn export_review_queues_export_review_with_the_checkbox_values() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    harness.editor.open_export_review(ReviewFormat::Word);
    harness.idle_frame();

    let layers = harness.find_text_last("Layers").expect("Layers checkbox");
    harness.click_at(layers);
    harness.idle_frame();
    let notes = harness.find_text_last("Notes").expect("Notes checkbox");
    harness.click_at(notes);
    harness.idle_frame();

    let export = harness.find_text_last("Export…").expect("Export button");
    let requests = click_for_requests(&mut harness, export);
    assert!(
        requests.iter().any(|request| matches!(
            request,
            EditorRequest::ExportReview {
                format: ReviewFormat::Word,
                options: ReviewOptionsView {
                    include_layers: true,
                    include_notes: false,
                    include_text: true,
                    include_banks: true,
                },
            }
        )),
        "the checkbox values travel with the request: {requests:?}"
    );
}

#[test]
fn color_to_hex_keeps_alpha_only_when_needed() {
    assert_eq!(
        crate::properties::color_to_hex(egui::Color32::from_rgb(0x32, 0xA8, 0x52)),
        "#32A852"
    );
    assert_eq!(
        crate::properties::color_to_hex(egui::Color32::from_rgba_unmultiplied(255, 0, 0, 128)),
        "#FF000080"
    );
}

#[test]
fn dragging_in_the_color_picker_changes_the_color_in_one_undo_step() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let field = harness.editor.ui.properties.color_rect.expect("color rect");
    let swatch = egui::pos2(field.left() - 15.0, field.center().y);
    let before = harness.editor.session.course.to_json().expect("json");
    let undo_before = harness.editor.session.undo_len();

    harness.click_at(swatch);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.undo_len(),
        undo_before,
        "opening the picker changes nothing"
    );

    // The hue and saturation square is the picker's last and largest control.
    let start = swatch + egui::vec2(-150.0, 240.0);
    let end = swatch + egui::vec2(-60.0, 300.0);
    harness.frame(vec![Event::PointerMoved(start)]);
    harness.frame(vec![Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerMoved(end)]);
    harness.idle_frame();
    assert_eq!(
        harness.editor.session.undo_len(),
        undo_before,
        "nothing is committed while the pointer is down"
    );
    harness.frame(vec![Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    harness.idle_frame();

    assert_eq!(harness.editor.session.undo_len(), undo_before + 1);
    assert_ne!(
        harness.editor.session.course.to_json().expect("json"),
        before,
        "the picked color reached the course"
    );
}

#[test]
fn clicking_a_theme_swatch_changes_the_color_in_one_undo_step_and_is_remembered() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("btn-start".into());
    harness.idle_frame();

    let field = harness.editor.ui.properties.color_rect.expect("color rect");
    let swatch = egui::pos2(field.left() - 15.0, field.center().y);
    let before = harness.editor.session.course.to_json().expect("json");
    let undo_before = harness.editor.session.undo_len();
    harness.click_at(swatch);
    harness.idle_frame();

    let label = harness.find_text("Theme").expect("Theme row");
    // The second swatch (Background) differs from the button's fill.
    harness.click_at(label + egui::vec2(16.0, 19.0));
    harness.idle_frame();

    assert_eq!(harness.editor.session.undo_len(), undo_before + 1);
    assert_ne!(
        harness.editor.session.course.to_json().expect("json"),
        before
    );
    let recent = crate::properties::recent_colors(&harness.ctx);
    assert_eq!(recent.len(), 1, "{recent:?}");
}

#[test]
fn recent_colors_are_newest_first_unique_and_capped_at_eight() {
    let ctx = egui::Context::default();
    for n in 0..12u8 {
        crate::properties::remember_color(&ctx, &format!("#0000{n:02X}"));
    }
    crate::properties::remember_color(&ctx, "#00000b");
    crate::properties::remember_color(&ctx, "not a colour");
    let recent = crate::properties::recent_colors(&ctx);
    assert_eq!(recent.len(), 8);
    assert_eq!(recent[0], "#00000B");
    assert_eq!(recent.iter().filter(|c| *c == "#00000B").count(), 1);
    assert_eq!(recent[1], "#00000A");
}

#[test]
fn interaction_gallery_opens_from_the_insert_menu_and_lists_every_card() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();

    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness
        .find_text("Interaction…")
        .expect("Interaction… menu item");
    harness.click_at(item);
    harness.idle_frame();

    assert!(harness.editor.ui.show_interaction_gallery);
    let ids: Vec<&str> = harness
        .editor
        .ui
        .interaction_gallery_rects
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        ids,
        vec![
            "tabs",
            "accordion",
            "marker",
            "scroll_panel",
            "timer",
            "progress",
            "flip_cards",
            "hotspot_reveal",
            "process_steps",
            "timeline",
            "reveal_grid",
            "scenario_choice",
        ],
        "all twelve interactions are in the gallery"
    );
    for name in [
        "Tabs",
        "Accordion",
        "Marker",
        "Scrolling panel",
        "Timer",
        "Progress meter",
        "Flip cards",
        "Hotspot reveal",
        "Process steps",
        "Timeline",
        "Reveal grid",
        "Scenario choice",
    ] {
        assert!(
            harness.find_text(name).is_some(),
            "{name} is painted on its card"
        );
    }
}

/// Find an inserted object's id by its readable name, in base objects or layers.
fn inserted_interaction_id(course: &coursecraft_course::Course, name: &str) -> String {
    course
        .slide("slide1")
        .and_then(|slide| {
            slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
                .find(|object| object.name == name)
                .map(|object| object.id.clone())
        })
        .unwrap_or_else(|| panic!("no object named {name}"))
}

fn interaction_click(o: &str) -> coursecraft_logic::Input {
    coursecraft_logic::Input::Click { object: o.into() }
}

#[test]
fn inserted_interactions_play_the_way_they_are_described() {
    use coursecraft_logic::{Input, Player};

    let click = interaction_click;

    // Flip cards: the second card turns over, then back.
    let mut session = coursecraft_engine::Session::blank("Flip");
    session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "flip_cards" }),
        )
        .expect("insert flip cards");
    let card = inserted_interaction_id(&session.course, "Card 2");
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    assert_eq!(player.object_state(&card), Some("normal"));
    player.handle(&click(&card));
    assert_eq!(player.object_state(&card), Some("Back"));
    player.handle(&click(&card));
    assert_eq!(player.object_state(&card), Some("normal"));

    // Hotspots: marker 2 opens callout 2 alone; its close button hides it.
    let mut session = coursecraft_engine::Session::blank("Hotspots");
    session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "hotspot_reveal" }),
        )
        .expect("insert hotspots");
    let marker = inserted_interaction_id(&session.course, "Hotspot 2 marker");
    let close = inserted_interaction_id(&session.course, "Close hotspot 2");
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    player.handle(&click(&marker));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Hotspot 2")
    );
    player.handle(&click(&close));
    assert!(player.state().visible_layers.is_empty());

    // Process steps: buttons and circle clicks share the one variable.
    let mut session = coursecraft_engine::Session::blank("Steps");
    let result = session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "process_steps" }),
        )
        .expect("insert steps");
    let variable = result["variables"][0]
        .as_str()
        .expect("step variable")
        .to_owned();
    let circle4 = inserted_interaction_id(&session.course, "Step 4 circle");
    let next = inserted_interaction_id(&session.course, "Next");
    let back = inserted_interaction_id(&session.course, "Back");
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    assert_eq!(player.state().visible_layers.len(), 1);
    player.handle(&click(&next));
    assert_eq!(
        player.variable(&variable),
        Some(&coursecraft_course::Value::Number(2.0))
    );
    player.handle(&click(&circle4));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert_eq!(player.object_state(&circle4), Some("Selected"));
    for _ in 0..3 {
        player.handle(&click(&next));
    }
    assert_eq!(
        player.variable(&variable),
        Some(&coursecraft_course::Value::Number(4.0)),
        "Next stops at 4"
    );
    for _ in 0..5 {
        player.handle(&click(&back));
    }
    assert_eq!(
        player.variable(&variable),
        Some(&coursecraft_course::Value::Number(1.0)),
        "Back stops at 1"
    );
}

#[test]
fn inserted_timeline_plays_the_way_it_is_described() {
    use coursecraft_logic::{Input, Player};
    let click = interaction_click;

    let mut session = coursecraft_engine::Session::blank("Timeline");
    session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "timeline" }),
        )
        .expect("insert timeline");
    let point3 = inserted_interaction_id(&session.course, "Point 3");
    let point1 = inserted_interaction_id(&session.course, "Point 1");
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    assert!(player.state().visible_layers.is_empty());
    player.handle(&click(&point3));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Point 3")
    );
    assert_eq!(player.object_state(&point3), Some("Visited"));
    player.handle(&click(&point1));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Point 1")
    );
    assert_eq!(player.object_state(&point3), Some("Visited"));
}

#[test]
fn inserted_reveal_grid_plays_the_way_it_is_described() {
    use coursecraft_logic::{Input, Player};
    let click = interaction_click;

    let mut session = coursecraft_engine::Session::blank("Reveal");
    let result = session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "reveal_grid" }),
        )
        .expect("insert reveal grid");
    let explored = result["variables"][0]
        .as_str()
        .expect("explored variable")
        .to_owned();
    let tile2 = inserted_interaction_id(&session.course, "Tile 2");
    let tile4 = inserted_interaction_id(&session.course, "Tile 4");
    let tile5 = inserted_interaction_id(&session.course, "Tile 5");
    let counter_text = session
        .course
        .slide("slide1")
        .and_then(|slide| slide.objects.iter().find(|o| o.name == "Explored counter"))
        .and_then(|o| match &o.kind {
            coursecraft_course::ObjectKind::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    player.handle(&click(&tile2));
    assert_eq!(player.object_state(&tile2), Some("Revealed"));
    assert_eq!(
        player.variable(&explored),
        Some(&coursecraft_course::Value::Number(1.0))
    );
    player.handle(&click(&tile2));
    assert_eq!(
        player.variable(&explored),
        Some(&coursecraft_course::Value::Number(1.0)),
        "a second click does not count twice"
    );
    player.handle(&click(&tile4));
    player.handle(&click(&tile5));
    assert_eq!(
        player.variable(&explored),
        Some(&coursecraft_course::Value::Number(3.0))
    );
    assert_eq!(player.display_text(&counter_text), "3 of 6 explored");
}

#[test]
fn inserted_scenario_choice_plays_the_way_it_is_described() {
    use coursecraft_logic::{Input, Player};
    let click = interaction_click;

    let mut session = coursecraft_engine::Session::blank("Scenario");
    session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": "scenario_choice" }),
        )
        .expect("insert scenario choice");
    let choice1 = inserted_interaction_id(&session.course, "Choice 1");
    let choice2 = inserted_interaction_id(&session.course, "Choice 2");
    let retry1 = inserted_interaction_id(&session.course, "Try again 1");
    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    assert!(player.state().visible_layers.is_empty());
    player.handle(&click(&choice1));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Choice 1")
    );
    player.handle(&click(&retry1));
    assert!(player.state().visible_layers.is_empty());
    player.handle(&click(&choice2));
    assert_eq!(player.state().visible_layers.len(), 1);
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Choice 2")
    );
    player.handle(&click(&choice1));
    assert!(
        player
            .current_slide()
            .and_then(|slide| slide.layer(&player.state().visible_layers[0]))
            .is_some_and(|layer| layer.name == "Choice 1")
    );
}

#[test]
fn interaction_gallery_flip_cards_pick_inserts_and_closes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.ui.show_interaction_gallery = true;
    harness.idle_frame();

    let card = harness.find_text("Flip cards").expect("Flip cards card");
    harness.click_at(card);
    harness.idle_frame();

    assert!(
        !harness.editor.ui.show_interaction_gallery,
        "picking a card closes the gallery"
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(
        slide
            .objects
            .iter()
            .filter(|o| o.name.starts_with("Card "))
            .count(),
        3,
        "three cards were inserted"
    );
    assert_eq!(harness.editor.selection.ids().len(), 3);
}

#[test]
fn interaction_gallery_scenario_choice_pick_inserts_and_closes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.ui.show_interaction_gallery = true;
    harness.idle_frame();

    let card = harness
        .find_text("Scenario choice")
        .expect("Scenario choice card");
    harness.click_at(card);
    harness.idle_frame();

    assert!(
        !harness.editor.ui.show_interaction_gallery,
        "picking a card closes the gallery"
    );
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    assert_eq!(
        slide
            .layers
            .iter()
            .filter(|layer| layer.name.starts_with("Choice "))
            .count(),
        3,
        "three feedback layers were inserted"
    );
    assert!(
        slide.question.is_none(),
        "scenario choice is not a graded question"
    );
}

#[test]
fn record_narration_opens_from_the_insert_menu() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let insert = harness.find_text("Insert").expect("Insert menu");
    harness.click_at(insert);
    harness.idle_frame();
    let item = harness
        .find_text_last("Record narration…")
        .expect("Record narration menu item");
    harness.click_at(item);
    harness.idle_frame();
    assert!(harness.editor.ui.record_narration.open);
}

#[test]
fn record_narration_opens_from_the_toolbar_media_popup() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.idle_frame();
    let toolbar_rect = Rect::from_min_max(
        pos2(0.0, TOP_BAR_HEIGHT),
        pos2(
            harness.screen_rect.max.x,
            TOP_BAR_HEIGHT + crate::toolbar::TOOLBAR_HEIGHT,
        ),
    );
    let media_btn = harness
        .find_text_in("Media", toolbar_rect)
        .expect("Media toolbar button");
    harness.click_at(media_btn);
    harness.idle_frame();
    let item = harness
        .find_text("Record narration…")
        .expect("Record narration… in Media popup");
    harness.click_at(item);
    harness.idle_frame();
    assert!(harness.editor.ui.record_narration.open);
}

#[test]
fn find_replace_message_clears_after_undo() {
    let mut session = coursecraft_engine::Session::blank("Cat course");
    session
        .execute(
            "object.add",
            &serde_json::json!({
                "slide": "slide1",
                "id": "plain",
                "kind": { "type": "text", "text": "Hello cat world" },
                "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 60.0 },
            }),
        )
        .expect("add text");
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("cat");
    harness.editor.ui.find.replacement = "dog".to_owned();
    harness.idle_frame();

    let button = harness
        .find_text("Replace all")
        .expect("Replace all button");
    harness.click_at(button);
    harness.idle_frame();
    assert!(harness.editor.ui.find.message.is_some());
    assert!(harness.find_text_containing("Replaced").is_some());

    harness
        .editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    harness.idle_frame();
    assert!(
        harness.editor.ui.find.message.is_none(),
        "find message must clear after undo"
    );
    assert!(
        harness.find_text_containing("Replaced").is_none(),
        "find message UI must clear after undo"
    );
}

fn lock_hide_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1, "id": "lh", "title": "LH",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "scene1", "title": "S", "slides": [{
            "id": "slide1", "title": "One",
            "objects": [
                { "id": "a", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                  "frame": { "x": 100, "y": 100, "w": 200, "h": 120 } },
                { "id": "b", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                  "frame": { "x": 500, "y": 100, "w": 200, "h": 120 } }
            ]
        }]}]
    });
    Course::from_json(&json.to_string()).expect("course")
}

fn lock_hide_setup() -> (TestHarness, SlideTransform) {
    let mut h = TestHarness::new(lock_hide_course());
    h.editor.selection.slide = "slide1".into();
    h.editor.ui.snap_to_guides = false;
    h.idle_frame();
    let t = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(h.central_panel_rect())),
        h.editor.session.course.slide_size,
    )
    .expect("transform");
    (h, t)
}

#[test]
fn a_locked_object_is_not_picked_moved_or_marqueed() {
    let (mut h, t) = lock_hide_setup();
    h.editor.run(
        "object.lock",
        serde_json::json!({ "slide": "slide1", "object": "a", "locked": true }),
    );
    h.click_at(t.to_screen_pos(pos2(200.0, 160.0)));
    assert_eq!(h.editor.selection.object, None, "click passes through");

    let before = h.editor.session.course.to_json().expect("json");
    h.drag_by(
        t.to_screen_pos(pos2(200.0, 160.0)),
        vec2(60.0, 40.0),
        4,
        Modifiers::NONE,
    );
    assert_eq!(h.editor.session.course.to_json().expect("json"), before);

    // A marquee over both objects takes only the unlocked one.
    h.drag_by(
        t.to_screen_pos(pos2(50.0, 50.0)),
        t.to_screen_pos(pos2(750.0, 260.0)) - t.to_screen_pos(pos2(50.0, 50.0)),
        4,
        Modifiers::NONE,
    );
    assert_eq!(h.editor.selection.ids(), vec!["b".to_owned()]);

    // The inspector side can still select it.
    h.editor.selection.set_many(["a"]);
    h.idle_frame();
    assert_eq!(h.editor.selection.object.as_deref(), Some("a"));
}

#[test]
fn a_hidden_object_is_not_on_the_canvas_but_still_in_the_course() {
    let (mut h, t) = lock_hide_setup();
    h.editor.run(
        "object.hide_in_editor",
        serde_json::json!({ "slide": "slide1", "object": "b", "hidden": true }),
    );
    h.idle_frame();
    h.click_at(t.to_screen_pos(pos2(600.0, 160.0)));
    assert_eq!(h.editor.selection.object, None, "nothing drawn to click");
    let slide = h.editor.session.course.slide("slide1").expect("slide");
    assert!(slide.object("b").is_some());
    let items = compose_authoring(&h.editor.session.course, slide, None);
    assert!(
        items
            .iter()
            .any(|i| i.hit == coursecraft_render::Hit::Object("b".into())),
        "preview and player still compose it"
    );
    h.click_at(t.to_screen_pos(pos2(200.0, 160.0)));
    assert_eq!(h.editor.selection.object.as_deref(), Some("a"));
}

#[test]
fn opening_a_course_uses_its_own_hidden_and_locked_objects() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.run(
        "object.hide_in_editor",
        serde_json::json!({ "slide": "slide1", "object": "a", "hidden": true }),
    );
    h.editor.run(
        "object.lock",
        serde_json::json!({ "slide": "slide1", "object": "b", "locked": true }),
    );
    h.editor.replace_course(lock_hide_course());
    let slide = h.editor.session.course.slide("slide1").expect("slide");
    assert!(!slide.object("a").expect("a").0.editor_hidden);
    assert!(!slide.object("b").expect("b").0.locked);
}

#[test]
fn view_menu_toggles_rulers_grid_snap_and_guides() {
    let (mut h, _t) = lock_hide_setup();
    assert!(
        !h.editor.session.course.editor.show_rulers && !h.editor.session.course.editor.show_grid
    );
    for (label, get) in [
        (
            "Rulers",
            (|e: &Editor| e.session.course.editor.show_rulers) as fn(&Editor) -> bool,
        ),
        ("Grid", |e: &Editor| e.session.course.editor.show_grid),
        ("Snap to grid", |e: &Editor| {
            e.session.course.editor.snap_to_grid
        }),
        ("Guides", |e: &Editor| e.session.course.editor.show_guides),
    ] {
        let view = h.find_text("View").expect("View menu");
        h.click_at(view);
        h.idle_frame();
        let item = h.find_text(label).unwrap_or_else(|| panic!("{label} item"));
        h.click_at(item);
        h.idle_frame();
        assert!(get(&h.editor), "{label} turned on");
    }
}

#[test]
fn dragging_from_a_ruler_makes_a_guide_and_dragging_it_back_deletes_it() {
    let (mut h, t) = lock_hide_setup();
    h.editor
        .run("view.set", serde_json::json!({ "show_rulers": true }));
    h.idle_frame();
    let canvas = canvas_rect(h.central_panel_rect());
    let start = crate::rulers::left_strip(canvas).center();
    let onto = t.to_screen_pos(pos2(400.0, 300.0));
    h.drag_by(start, onto - start, 4, Modifiers::NONE);
    let guides = h.editor.session.course.editor.guides.clone();
    assert_eq!(guides.len(), 1, "{guides:?}");
    assert!(guides[0].vertical);
    assert!((guides[0].at - 400.0).abs() < 2.0, "{}", guides[0].at);

    // Drag it back onto the ruler.
    let guide_screen = t.to_screen_pos(pos2(guides[0].at, 300.0));
    let back = crate::rulers::left_strip(canvas).center();
    h.drag_by(guide_screen, back - guide_screen, 4, Modifiers::NONE);
    assert!(h.editor.session.course.editor.guides.is_empty());
}

#[test]
fn a_move_snaps_to_a_guide() {
    let (mut h, t) = lock_hide_setup();
    h.editor.run(
        "view.guides.add",
        serde_json::json!({ "vertical": true, "at": 400.0 }),
    );
    h.idle_frame();
    // Object "a" spans x 100..300; moving it 97px puts its right edge at 397.
    h.drag_by(
        t.to_screen_pos(pos2(200.0, 160.0)),
        vec2(97.0 * t.scale(), 0.0),
        4,
        Modifiers::NONE,
    );
    let slide = h.editor.session.course.slide("slide1").expect("slide");
    let a = slide.object("a").expect("a").0;
    assert!((a.frame.x - 200.0).abs() < 0.5, "x {}", a.frame.x);
}

#[test]
fn slide_size_dialog_opens_from_the_file_menu_and_apply_dispatches() {
    let (mut h, _t) = lock_hide_setup();
    let file = h.find_text("File").expect("File menu");
    h.click_at(file);
    h.idle_frame();
    let item = h.find_text("Slide size…").expect("Slide size item");
    h.click_at(item);
    h.idle_frame();
    assert!(h.editor.ui.slide_size_dialog.open);

    let combo = h.find_text("16:9 HD (1280 × 720)").expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let preset = h.find_text("4:3 Standard (960 × 720)").expect("4:3 preset");
    h.click_at(preset);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (960, 720)
    );
    let undo = h.editor.session.undo_len();
    let apply = h.find_text("Apply").expect("Apply");
    h.click_at(apply);
    h.idle_frame();
    let size = h.editor.session.course.slide_size;
    assert_eq!((size.w, size.h), (960, 720));
    assert_eq!(h.editor.session.undo_len(), undo + 1);
    assert!(!h.editor.ui.slide_size_dialog.open);
}

#[test]
fn slide_size_dialog_disables_apply_out_of_range() {
    let mut state = crate::slide_size_dialog::SlideSizeState::default();
    assert!(state.valid());
    state.w = 99;
    assert!(!state.valid());
    state.w = 1280;
    state.h = 4321;
    assert!(!state.valid());
}

#[test]
fn slide_size_dialog_preset_combo_and_custom_switching() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.open_slide_size_dialog();
    h.idle_frame();

    // Select 16:9 Full HD (1920 × 1080) -> ratio "16:9"
    let combo = h.find_text("16:9 HD (1280 × 720)").expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    // Esc while combo popup is open closes only the popup, not the dialog (popup-first).
    assert!(egui::Popup::is_any_open(&h.ctx));
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(h.editor.ui.slide_size_dialog.open);

    // Reopen combo and pick Full HD.
    let combo = h.find_text("16:9 HD (1280 × 720)").expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let fhd = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("Full HD preset");
    h.click_at(fhd);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1920, 1080)
    );
    assert!(
        h.find_text("16:9").is_some(),
        "ratio badge should show 16:9"
    );

    // Select 9:16 Portrait (1080 × 1920).
    let combo = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let portrait = h
        .find_text("9:16 Portrait (1080 × 1920)")
        .expect("Portrait preset");
    h.click_at(portrait);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1080, 1920)
    );

    // Typing a custom width switches combo label to "Custom".
    let w_label = h.find_text("W").expect("W label");
    h.click_at(egui::Pos2::new(w_label.x + 30.0, w_label.y));
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("1500");
    h.press(Key::Tab, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.ui.slide_size_dialog.w, 1500);
    assert!(
        h.find_text("Custom").is_some(),
        "combo label must switch to Custom"
    );
}

#[test]
fn slide_size_dialog_aspect_lock_keeps_and_updates_ratio() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.open_slide_size_dialog();
    h.idle_frame();
    assert!(h.editor.ui.slide_size_dialog.lock_aspect);
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1280, 720)
    );

    // With lock ON at 1280x720, changing W to 1920 updates H to 1080.
    let w_label = h.find_text("W").expect("W label");
    h.click_at(egui::Pos2::new(w_label.x + 30.0, w_label.y));
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("1920");
    h.press(Key::Tab, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1920, 1080)
    );

    // Changing H to 900 updates W to 1600.
    let h_label = h.find_text("H").expect("H label");
    h.click_at(egui::Pos2::new(h_label.x + 30.0, h_label.y));
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("900");
    h.press(Key::Tab, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1600, 900)
    );

    // Uncheck lock -> W changes independently.
    let lock = h
        .find_text("Lock aspect ratio")
        .expect("Lock aspect ratio checkbox");
    h.click_at(lock);
    h.idle_frame();
    assert!(!h.editor.ui.slide_size_dialog.lock_aspect);

    let w_label = h.find_text("W").expect("W label");
    h.click_at(egui::Pos2::new(w_label.x + 30.0, w_label.y));
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("1000");
    h.press(Key::Tab, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (1000, 900)
    );

    // Re-checking lock captures the new 10:9 ratio.
    let lock = h
        .find_text("Lock aspect ratio")
        .expect("Lock aspect ratio checkbox");
    h.click_at(lock);
    h.idle_frame();
    assert!(h.editor.ui.slide_size_dialog.lock_aspect);

    let w_label = h.find_text("W").expect("W label");
    h.click_at(egui::Pos2::new(w_label.x + 30.0, w_label.y));
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("2000");
    h.press(Key::Tab, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        (
            h.editor.ui.slide_size_dialog.w,
            h.editor.ui.slide_size_dialog.h
        ),
        (2000, 1800)
    );
}

#[test]
fn slide_size_dialog_use_as_default_emits_request_and_disables_when_already_default() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.open_slide_size_dialog();
    h.idle_frame();

    // At 1280x720 (already default), button is disabled with explanatory tooltip.
    let btn_pos = h
        .find_text("Use as default for new courses")
        .expect("Use as default button");
    let tip = h
        .probe_tooltip(btn_pos)
        .expect("disabled default button should explain why");
    assert!(
        tip.contains("Already the default"),
        "unexpected tooltip: {tip}"
    );

    // Switch to 1920x1080 and click "Use as default for new courses".
    let combo = h.find_text("16:9 HD (1280 × 720)").expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let fhd = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("Full HD preset");
    h.click_at(fhd);
    h.idle_frame();

    let btn_pos = h
        .find_text("Use as default for new courses")
        .expect("Use as default button");
    let mut requests = Vec::new();
    h.frame(vec![Event::PointerMoved(btn_pos)]);
    for pressed in [true, false] {
        requests.extend(h.frame(vec![Event::PointerButton {
            pos: btn_pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]));
    }
    h.idle_frame();
    assert_eq!(h.editor.ui.prefs.default_slide, (1920, 1080));
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::SetPreferences {
                default_slide: (1920, 1080),
                ..
            }
        )),
        "expected SetPreferences request, got {requests:?}"
    );
    assert!(
        h.find_text("Saved as default").is_some(),
        "expected Saved as default confirmation"
    );
    h.hover_at(egui::Pos2::ZERO);
    h.idle_frame();
    let btn_pos = h
        .find_text("Use as default for new courses")
        .expect("Use as default button");
    let tip = h
        .probe_tooltip(btn_pos)
        .expect("button should now be disabled with tooltip");
    assert!(
        tip.contains("Already the default"),
        "unexpected tooltip: {tip}"
    );
}

#[test]
fn slide_size_dialog_content_modes_dispatch_and_undo() {
    let (mut h, _t) = lock_hide_setup();
    let orig_size = h.editor.session.course.slide_size;
    let slide_id = h.editor.selection.slide.clone();
    let orig_frames: Vec<_> = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .expect("slide")
        .objects
        .iter()
        .map(|o| (o.id.clone(), o.frame))
        .collect();

    for (radio_label, target_label, expected_size, check_enter) in [
        (
            "Scale to fit",
            "4:3 Standard (960 × 720)",
            (960, 720),
            false,
        ),
        (
            "Fill slide",
            "9:16 Portrait (1080 × 1920)",
            (1080, 1920),
            false,
        ),
        (
            "Keep as is",
            "16:9 Full HD (1920 × 1080)",
            (1920, 1080),
            true,
        ),
    ] {
        h.editor.open_slide_size_dialog();
        h.idle_frame();
        let current_label = crate::slide_size_dialog::PRESETS
            .iter()
            .find(|p| {
                (p.w, p.h)
                    == (
                        h.editor.session.course.slide_size.w,
                        h.editor.session.course.slide_size.h,
                    )
            })
            .expect("current preset")
            .label;
        let combo = h.find_text(current_label).expect("preset combo");
        h.click_at(combo);
        h.idle_frame();
        let target = h.find_text(target_label).expect("target preset");
        h.click_at(target);
        h.idle_frame();

        let radio = h.find_text(radio_label).expect("mode radio");
        h.click_at(radio);
        h.idle_frame();

        if check_enter {
            h.press(Key::Enter, Modifiers::NONE);
        } else {
            let apply = h.find_text("Apply").expect("Apply button");
            h.click_at(apply);
        }
        h.idle_frame();

        assert!(!h.editor.ui.slide_size_dialog.open);
        let size = h.editor.session.course.slide_size;
        assert_eq!((size.w, size.h), expected_size);

        // Undo restores prior course size and frames.
        h.press(Key::Z, Modifiers::COMMAND);
        h.idle_frame();
        assert_eq!(h.editor.session.course.slide_size, orig_size);
        let restored_frames: Vec<_> = h
            .editor
            .session
            .course
            .slide(&slide_id)
            .expect("slide")
            .objects
            .iter()
            .map(|o| (o.id.clone(), o.frame))
            .collect();
        assert_eq!(restored_frames, orig_frames);
    }
}

#[test]
fn slide_size_dialog_live_preview_and_course_wide_impact_counts() {
    let (mut h, _t) = lock_hide_setup();
    // Add an object on the current slide that sits at (900, 500) so it is inside 1280x720
    // but completely outside 640x360 when keeping positions.
    h.editor.run(
        "object.add",
        serde_json::json!({
            "slide": h.editor.selection.slide,
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 900.0, "y": 500.0, "w": 120.0, "h": 80.0 }
        }),
    );
    assert!(
        h.editor.ui.last_error.is_none(),
        "unexpected error adding object: {:?}",
        h.editor.ui.last_error
    );
    h.editor.open_slide_size_dialog();
    h.idle_frame();

    // 1) 1280x720 -> 1920x1080 (scale): reports "0 outside" and 150%.
    let combo = h.find_text("16:9 HD (1280 × 720)").expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let fhd = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("Full HD preset");
    h.click_at(fhd);
    h.idle_frame();
    assert!(
        h.find_text_containing("0 outside").is_some(),
        "expected '0 outside' for 1280x720 -> 1920x1080 (scale)"
    );
    assert!(
        h.find_text_containing("150%").is_some(),
        "expected '150%' scale hint for 1280x720 -> 1920x1080 (scale)"
    );

    // 2) 1280x720 -> 1080x1080 (scale): reports margin hint.
    let combo = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("preset combo");
    h.click_at(combo);
    h.idle_frame();
    let square = h
        .find_text("1:1 Square (1080 × 1080)")
        .expect("Square preset");
    h.click_at(square);
    h.idle_frame();
    assert!(
        h.find_text_containing("Adds top/bottom margins").is_some(),
        "expected margin hint for 1280x720 -> 1080x1080 (scale)"
    );

    // 3) 1280x720 -> 640x360 (keep): reports exact off-slide object count across the course.
    let keep = h.find_text("Keep as is").expect("Keep as is radio");
    h.click_at(keep);
    h.idle_frame();
    let lock = h
        .find_text("Lock aspect ratio")
        .expect("Lock aspect ratio checkbox");
    h.click_at(lock);
    h.idle_frame();
    h.editor.ui.slide_size_dialog.w = 640;
    h.editor.ui.slide_size_dialog.h = 360;
    h.idle_frame();

    let expected_outside = h
        .editor
        .session
        .course
        .slides()
        .chain(h.editor.session.course.masters.iter())
        .chain(h.editor.session.course.templates.iter())
        .chain(
            h.editor
                .session
                .course
                .banks
                .iter()
                .flat_map(|b| b.slides.iter()),
        )
        .flat_map(|s| {
            s.objects
                .iter()
                .chain(s.layers.iter().flat_map(|l| l.objects.iter()))
        })
        .filter(|o| {
            !matches!(o.kind, ObjectKind::Audio { .. })
                && o.panel.is_none()
                && (o.frame.x + o.frame.w <= 0.0
                    || o.frame.x >= 640.0
                    || o.frame.y + o.frame.h <= 0.0
                    || o.frame.y >= 360.0)
        })
        .count();
    assert!(
        expected_outside >= 1,
        "expected at least 1 off-slide object at 640x360"
    );
    let needle = format!("{expected_outside} outside");
    assert!(
        h.find_text_containing(&needle).is_some(),
        "expected '{needle}' in preview impact summary"
    );
}

#[test]
fn slide_size_dialog_preview_idle_frames_do_not_recompute() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.open_slide_size_dialog();
    h.idle_frame();
    assert_eq!(h.editor.ui.slide_size_dialog.impact_recomputes, 1);

    for _ in 0..5 {
        h.idle_frame();
    }
    assert_eq!(
        h.editor.ui.slide_size_dialog.impact_recomputes, 1,
        "idle frames must not recompute course-wide impact"
    );

    h.editor.ui.slide_size_dialog.mode = crate::slide_size_dialog::ResizeModeChoice::Fill;
    h.idle_frame();
    assert_eq!(h.editor.ui.slide_size_dialog.impact_recomputes, 2);
    for _ in 0..3 {
        h.idle_frame();
    }
    assert_eq!(h.editor.ui.slide_size_dialog.impact_recomputes, 2);

    h.editor.ui.slide_size_dialog.w = 1920;
    h.editor.ui.slide_size_dialog.h = 1080;
    h.idle_frame();
    assert_eq!(h.editor.ui.slide_size_dialog.impact_recomputes, 3);
    for _ in 0..3 {
        h.idle_frame();
    }
    assert_eq!(h.editor.ui.slide_size_dialog.impact_recomputes, 3);
}

#[test]
fn slide_size_dialog_opens_from_all_five_entry_points() {
    let (mut h, _t) = lock_hide_setup();

    // 1. File ▸ Slide size…
    let file = h.find_text("File").expect("File menu");
    h.click_at(file);
    h.idle_frame();
    let item = h.find_text("Slide size…").expect("File > Slide size…");
    h.click_at(item);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_size_dialog.open,
        "File menu must open Slide size"
    );
    h.editor.ui.slide_size_dialog.open = false;
    h.idle_frame();

    // 2. Slide ▸ Slide size…
    let slide_menu = h.find_text("Slide").expect("Slide menu");
    h.click_at(slide_menu);
    h.idle_frame();
    let item = h.find_text("Slide size…").expect("Slide > Slide size…");
    h.click_at(item);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_size_dialog.open,
        "Slide menu must open Slide size"
    );
    h.editor.ui.slide_size_dialog.open = false;
    h.idle_frame();

    // 3. Course Inspector "Change…"
    h.editor.ui.inspector_tab = crate::InspectorTab::Course;
    h.idle_frame();
    let change = h.find_text("Change…").expect("Course inspector Change…");
    h.click_at(change);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_size_dialog.open,
        "Course inspector Change… must open Slide size"
    );
    h.editor.ui.slide_size_dialog.open = false;
    h.idle_frame();

    // 4. Course Settings "Change…"
    h.editor.ui.inspector_tab = crate::InspectorTab::Format;
    h.editor.ui.show_course_settings_window = true;
    h.idle_frame();
    h.idle_frame();
    let change = h.find_text("Change…").expect("Course settings Change…");
    h.click_at(change);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_size_dialog.open,
        "Course settings Change… must open Slide size"
    );
    h.editor.ui.slide_size_dialog.open = false;
    h.editor.ui.show_course_settings_window = false;
    h.idle_frame();

    // 5. Status bar "1280 × 720 px" pill
    let pill = h
        .find_text("1280 × 720 px")
        .expect("Status bar slide size pill");
    h.click_at(pill);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_size_dialog.open,
        "Status bar pill must open Slide size"
    );
}

#[test]
fn check_course_off_slide_warning_navigates_to_stranded_object() {
    let (mut h, _t) = lock_hide_setup();
    let layer_res = h
        .editor
        .run(
            "layer.add",
            serde_json::json!({ "slide": "slide1", "name": "Stranded Layer" }),
        )
        .expect("layer.add");
    let layer_id = layer_res["id"].as_str().expect("layer id").to_owned();
    let obj_res = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "slide1",
                "layer": layer_id,
                "name": "Stranded Box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#3B82F6" },
                "frame": { "x": 1000.0, "y": 100.0, "w": 150.0, "h": 80.0 }
            }),
        )
        .expect("object.add");
    let obj_id = obj_res["id"].as_str().expect("object id").to_owned();

    h.editor
        .run(
            "course.set_slide_size",
            serde_json::json!({ "w": 800, "h": 600, "mode": "keep" }),
        )
        .expect("resize keep");
    h.editor.selection.slide = String::new();
    h.editor.selection.layer = None;
    h.editor.selection.clear_objects();
    h.editor.refresh_validate_only();
    let issue_label = h
        .editor
        .ui
        .validation_issues
        .iter()
        .find(|i| i.message.contains("lies outside the 800×600 slide"))
        .map(|i| format!("{} — {}", i.path, i.message))
        .unwrap_or_else(|| {
            panic!(
                "expected off-slide warning, got {:?}",
                h.editor.ui.validation_issues
            )
        });
    h.editor.ui.show_validation_window = true;
    h.idle_frame();
    h.idle_frame();

    let warning_pos = h
        .find_text(&issue_label)
        .expect("off-slide warning row in Check Course");
    h.click_at(warning_pos);
    h.idle_frame();

    assert_eq!(h.editor.selection.slide, "slide1");
    assert_eq!(h.editor.selection.layer.as_deref(), Some(layer_id.as_str()));
    assert_eq!(h.editor.selection.object.as_deref(), Some(obj_id.as_str()));
}

#[test]
fn inspector_strikethrough_button_styles_the_text() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.object = Some("title".into());
    harness.idle_frame();
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let Some(object) = slide
        .objects
        .iter()
        .find(|o| matches!(o.kind, ObjectKind::Text { .. }))
    else {
        panic!("no text object");
    };
    let id = object.id.clone();
    harness.editor.selection.object = Some(id.clone());
    harness.idle_frame();
    let undo = harness.editor.session.undo_len();
    let s_button = harness.find_text("S").expect("S button");
    harness.click_at(s_button);
    harness.idle_frame();
    assert_eq!(harness.editor.session.undo_len(), undo + 1);
    let slide = harness
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide");
    let struck = match &slide.object(&id).expect("object").0.kind {
        ObjectKind::Text { runs, style, .. } => {
            style.strikethrough || runs.iter().any(|r| r.strikethrough == Some(true))
        }
        _ => false,
    };
    assert!(struck);
}

#[test]
fn format_aspect_ratio_unit_matrix() {
    use crate::slide_size_dialog::format_aspect_ratio;
    assert_eq!(format_aspect_ratio(1920, 1080), "16:9");
    assert_eq!(format_aspect_ratio(1280, 720), "16:9");
    assert_eq!(format_aspect_ratio(1280, 800), "16:10");
    assert_eq!(format_aspect_ratio(960, 720), "4:3");
    assert_eq!(format_aspect_ratio(1080, 1080), "1:1");
    assert_eq!(format_aspect_ratio(1080, 1920), "9:16");
    assert_eq!(format_aspect_ratio(1500, 900), "5:3");
    assert_eq!(format_aspect_ratio(1001, 700), "1.43:1");
    assert_eq!(format_aspect_ratio(0, 720), "—");
}

#[test]
fn preferences_open_from_the_menu_and_changes_are_requested() {
    let (mut h, _t) = lock_hide_setup();
    let file = h.find_text("File").expect("File menu");
    h.click_at(file);
    h.idle_frame();
    let item = h.find_text("Preferences…").expect("Preferences item");
    h.click_at(item);
    h.idle_frame();
    assert!(h.editor.ui.prefs.open);
    // Open the default slide size combo and select 16:9 Full HD (1920 × 1080).
    let combo = h
        .find_text("16:9 HD (1280 × 720)")
        .expect("default slide combo");
    h.click_at(combo);
    h.idle_frame();
    let fhd = h
        .find_text("16:9 Full HD (1920 × 1080)")
        .expect("Full HD preset");
    let mut requests = Vec::new();
    h.frame(vec![Event::PointerMoved(fhd)]);
    for pressed in [true, false] {
        requests.extend(h.frame(vec![Event::PointerButton {
            pos: fhd,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]));
    }
    assert!(requests.iter().any(|r| matches!(
        r,
        EditorRequest::SetPreferences {
            default_slide: (1920, 1080),
            ..
        }
    )));

    // Verify set_preferences accepts custom sizes and the combo shows "Custom".
    h.editor.set_preferences(1, (1440, 900));
    h.idle_frame();
    assert_eq!(h.editor.ui.prefs.default_slide, (1440, 900));
    assert!(h.find_text("Custom").is_some());
}

#[test]
fn cmd_comma_opens_preferences() {
    let (mut h, _t) = lock_hide_setup();
    h.press(Key::Comma, Modifiers::COMMAND);
    h.idle_frame();
    assert!(h.editor.ui.prefs.open);
}

#[test]
fn preferences_checkbox_flips_as_you_type_spelling() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.open_preferences();
    h.idle_frame();
    let checkbox = h
        .find_text("Check spelling as you type")
        .expect("spelling checkbox");
    let mut requests = Vec::new();
    for pressed in [true, false] {
        requests.extend(h.frame(vec![Event::PointerButton {
            pos: checkbox,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]));
    }
    assert!(
        !h.editor.ui.prefs.spell_check_as_you_type,
        "the checkbox turns checking off"
    );
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::SetPreferences {
                spell_check_as_you_type: false,
                ..
            }
        )),
        "the change is sent to the app to save"
    );
}

/// U-11: Preferences Performance radio switches editor profile, updates cache caps,
/// and emits `EditorRequest::SetPreferences` with `performance_mode`.
#[test]
fn u11_preferences_performance_mode_updates_editor_profile_and_emits_request() {
    use coursecraft_render::PerformanceMode;

    let (mut h, _t) = lock_hide_setup();
    h.editor.open_preferences();
    h.idle_frame();
    assert_eq!(h.editor.performance_mode, PerformanceMode::Auto);
    assert!(
        h.find_text_containing("Published courses are not affected.")
            .is_some(),
        "Preferences must show the Lite mode helper note"
    );

    let lite = h.find_text("Lite").expect("Lite radio button");
    let mut requests = Vec::new();
    for pressed in [true, false] {
        requests.extend(h.frame(vec![Event::PointerButton {
            pos: lite,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]));
    }
    assert_eq!(h.editor.performance_mode, PerformanceMode::Lite);
    assert_eq!(h.editor.ui.prefs.performance_mode, PerformanceMode::Lite);
    assert_eq!(h.editor.media_cache.picture_budget(), 64 * 1024 * 1024);
    assert_eq!(h.editor.media_cache.max_canvas_dim(), 1024);
    assert_eq!(h.editor.media_cache.max_thumb_dim(), 256);
    assert!(
        requests.iter().any(|r| matches!(
            r,
            EditorRequest::SetPreferences {
                performance_mode: PerformanceMode::Lite,
                ..
            }
        )),
        "expected SetPreferences with performance_mode=Lite, got {requests:?}"
    );
}

/// U-11: Large media library triggers a one-time non-blocking status notice and resets on `replace_course`.
#[test]
fn u11_memory_pressure_notice_fires_once_and_resets_on_replace_course() {
    use std::sync::Arc;

    let (mut h, _t) = lock_hide_setup();
    let one_mib: Arc<[u8]> = Arc::from(vec![0u8; 1024 * 1024]);
    let media: Vec<(String, Arc<[u8]>)> = (0..513)
        .map(|i| (format!("img/big-{i}.bin"), Arc::clone(&one_mib)))
        .collect();
    h.editor.set_media(media);
    assert!(!h.editor.ui.memory_pressure_warned);

    h.idle_frame();
    assert!(h.editor.ui.memory_pressure_warned);
    assert!(
        h.editor
            .ui
            .status_message
            .as_ref()
            .is_some_and(|(msg, _)| msg.contains("Large media library detected")),
        "expected memory pressure status notice, got {:?}",
        h.editor.ui.status_message
    );

    // Must only fire once per opened course.
    h.editor.ui.status_message = None;
    h.idle_frame();
    assert!(
        h.editor.ui.status_message.is_none(),
        "memory pressure notice must not repeat on subsequent frames"
    );

    // Replacing the course resets the one-time guard.
    let fresh = Course::from_json(COURSE_JSON).expect("valid course");
    h.editor.replace_course(fresh);
    assert!(!h.editor.ui.memory_pressure_warned);
}

fn triggers_harness() -> TestHarness {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut harness = TestHarness::new(course);
    harness.editor.selection.slide = "s-intro".into();
    harness.editor.selection.layer = None;
    harness.editor.ui.bottom_tab = BottomTab::Triggers;
    harness.idle_frame();
    harness
}

/// Hover the first trigger row and open its "more" menu.
fn open_first_trigger_menu(harness: &mut TestHarness) {
    let row = harness
        .find_text("1.")
        .expect("first trigger row index label");
    harness.hover_at(row);
    harness.idle_frame();
    let more = harness
        .find_icon(coursecraft_render::Icon::DotsThree)
        .expect("more button");
    harness.click_at(more);
    harness.idle_frame();
}

#[test]
fn trigger_menu_duplicates_a_trigger() {
    let mut h = triggers_harness();
    let before = h
        .editor
        .session
        .course
        .slide("s-intro")
        .unwrap()
        .triggers
        .clone();
    open_first_trigger_menu(&mut h);
    let item = h.find_text("Duplicate").expect("Duplicate");
    h.click_at(item);
    h.idle_frame();
    let after = &h.editor.session.course.slide("s-intro").unwrap().triggers;
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[0], before[0]);
    assert_eq!(
        after[1], before[0],
        "the copy sits right after the original"
    );
}

#[test]
fn trigger_menu_disables_the_trigger_and_the_row_is_dimmed() {
    let mut h = triggers_harness();
    let undo = h.editor.session.undo_len();
    open_first_trigger_menu(&mut h);
    let item = h.find_text("Disable").expect("Disable");
    h.click_at(item);
    h.idle_frame();
    h.idle_frame();
    let triggers = &h.editor.session.course.slide("s-intro").unwrap().triggers;
    assert!(triggers[0].disabled);
    assert_eq!(h.editor.session.undo_len(), undo + 1);
    assert!(h.find_text("Off").is_some(), "a disabled row says Off");
}

#[test]
fn trigger_copy_then_paste_adds_it_to_another_owner_and_skips_misfits() {
    let mut h = triggers_harness();
    let first = h.editor.session.course.slide("s-intro").unwrap().triggers[0].clone();
    open_first_trigger_menu(&mut h);
    let item = h.find_text("Copy").expect("Copy");
    h.click_at(item);
    h.idle_frame();
    assert_eq!(h.editor.ui.copied_triggers, vec![first.clone()]);

    h.editor.selection.slide = "s-tabs".into();
    h.idle_frame();
    let before = h
        .editor
        .session
        .course
        .slide("s-tabs")
        .unwrap()
        .triggers
        .len();
    let undo = h.editor.session.undo_len();
    let paste = h.find_text("Paste triggers").expect("Paste triggers");
    h.click_at(paste);
    h.idle_frame();
    let slide = h.editor.session.course.slide("s-tabs").unwrap();
    let pasted = slide.triggers.len();
    let skipped = h.editor.ui.status_message.is_some();
    // Either it fit (one undo step) or it was skipped with a message.
    if pasted == before + 1 {
        assert_eq!(h.editor.session.undo_len(), undo + 1);
    } else {
        assert_eq!(pasted, before);
        assert!(skipped, "a skipped paste says so");
    }

    // A trigger naming an object that is absent must be skipped.
    let mut bad = first;
    bad.when = coursecraft_course::Event::Click {
        object: "no-such-object".into(),
    };
    h.editor.ui.copied_triggers = vec![bad];
    let before = h
        .editor
        .session
        .course
        .slide("s-tabs")
        .unwrap()
        .triggers
        .len();
    crate::triggers::paste_triggers(&mut h.editor, "s-tabs", None);
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s-tabs")
            .unwrap()
            .triggers
            .len(),
        before
    );
    assert!(h.editor.ui.status_message.is_some());
}

/// Click at `pos` and return every request raised by the press and release.
fn click_collect(h: &mut TestHarness, pos: egui::Pos2) -> Vec<EditorRequest> {
    let mut requests = h.frame(vec![Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        requests.extend(h.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]));
    }
    requests
}

#[test]
fn version_history_menu_item_is_disabled_without_a_file_and_opens_with_one() {
    use crate::version_history::VersionRow;
    let (mut h, _t) = lock_hide_setup();

    // An unsaved course has no history: the item does nothing.
    let file = h.find_text("File").expect("File menu");
    h.click_at(file);
    h.idle_frame();
    let item = h.find_text("Version history…").expect("menu item");
    let requests = click_collect(&mut h, item);
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, EditorRequest::ShowVersionHistory))
    );
    assert!(!h.editor.ui.versions.open);

    // A saved project enables it and the click raises the request.
    h.editor.set_project_has_file(true);
    h.idle_frame();
    if h.find_text("Version history…").is_none() {
        let file = h.find_text("File").expect("File menu");
        h.click_at(file);
        h.idle_frame();
    }
    let item = h.find_text("Version history…").expect("menu item");
    let requests = click_collect(&mut h, item);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, EditorRequest::ShowVersionHistory))
    );

    // The app answers with rows; the window lists them and acts on a click.
    h.editor.show_version_history(vec![
        VersionRow {
            id: "20261111-153045".into(),
            when: "2026-11-11 15:30:45 UTC".into(),
            size: "1.2 MB".into(),
            slides: Some(12),
        },
        VersionRow {
            id: "20261110-090000".into(),
            when: "2026-11-10 09:00:00 UTC".into(),
            size: "900 KB".into(),
            slides: None,
        },
    ]);
    h.idle_frame();
    assert!(h.find_text("2026-11-11 15:30:45 UTC").is_some());
    assert!(h.find_text("12 slides").is_some());
    assert!(h.find_text("Damaged").is_some());
    let open = h.find_text("Open as copy").expect("Open as copy button");
    let requests = click_collect(&mut h, open);
    assert!(requests.iter().any(|r| matches!(
        r,
        EditorRequest::OpenVersionCopy { id } if id == "20261111-153045"
    )));
    // The damaged row can only be deleted: one Open as copy button in all.
    h.idle_frame();
    assert_eq!(h.find_texts("Open as copy").len(), 1);
    assert_eq!(h.find_texts("Delete").len(), 2);
}

#[test]
fn preferences_window_carries_the_versions_to_keep_value() {
    let (mut h, _t) = lock_hide_setup();
    h.editor.set_versions_to_keep(250);
    assert_eq!(h.editor.ui.prefs.versions_to_keep, 100, "clamped to 100");
    h.editor.set_versions_to_keep(0);
    assert_eq!(h.editor.ui.prefs.versions_to_keep, 0);
    h.editor.open_preferences();
    h.idle_frame();
    assert!(h.find_text("Versions to keep").is_some());
}

#[test]
fn folder_menu_items_raise_requests_and_history_says_git_is_the_history() {
    let (mut h, _t) = lock_hide_setup();
    let file = h.find_text("File").expect("File menu");
    h.click_at(file);
    h.idle_frame();
    let open = h.find_text("Open folder…").expect("Open folder item");
    let requests = click_collect(&mut h, open);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, EditorRequest::OpenFolder))
    );

    if h.find_text("Save as project folder (for git)…").is_none() {
        let file = h.find_text("File").expect("File menu");
        h.click_at(file);
        h.idle_frame();
    }
    let save = h
        .find_text("Save as project folder (for git)…")
        .expect("Save as project folder item");
    let requests = click_collect(&mut h, save);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, EditorRequest::SaveAsFolder))
    );

    h.editor.set_project_has_file(true);
    h.editor.set_project_is_folder(true);
    h.editor.show_version_history(Vec::new());
    h.idle_frame();
    assert!(h.find_text_containing("git is its history").is_some());
    assert!(h.find_text("Open as Copy").is_none());
}

// --- Tester report, 2026-11-19: animations, timeline, scenes ---------------

fn txt_object(harness: &TestHarness) -> coursecraft_course::Object {
    let Some(slide) = harness.editor.session.course.slide("s1") else {
        panic!("slide s1");
    };
    let Some((obj, _)) = slide.object("txt") else {
        panic!("object txt");
    };
    obj.clone()
}

#[test]
fn animation_panel_settings_are_stored_for_every_effect() {
    use coursecraft_course::{AnimEffect, Direction, Easing};
    let mut harness = TestHarness::new(timeline_media_course());
    harness.editor.selection.slide = "s1".into();
    for (effect, expected) in [
        ("fade", AnimEffect::new("fade")),
        ("fly_in", AnimEffect::with_from("fly_in", Direction::Top)),
        ("zoom", AnimEffect::new("zoom")),
        ("wipe", AnimEffect::with_from("wipe", Direction::Top)),
        ("float", AnimEffect::with_from("float", Direction::Top)),
    ] {
        for which in ["entrance", "exit"] {
            let (cmd, params) = crate::properties::animation_command(
                which,
                effect,
                &coursecraft_course::AnimEffect {
                    from: Some(Direction::Top),
                    ..coursecraft_course::AnimEffect::default()
                },
                700,
                Easing::EaseIn,
                "s1",
                "txt",
            );
            assert!(
                harness.editor.run(cmd, params).is_some(),
                "{which} {effect} is accepted: {:?}",
                harness.editor.ui.last_error
            );
            let obj = txt_object(&harness);
            let stored = if which == "entrance" {
                obj.entrance
            } else {
                obj.exit
            };
            let Some(anim) = stored else {
                panic!("{which} {effect} stored");
            };
            assert_eq!(anim.effect, expected);
            assert_eq!(anim.duration_ms, 700);
            assert_eq!(anim.easing, Easing::EaseIn);
        }
    }
    let (cmd, params) = crate::properties::animation_command(
        "entrance",
        "none",
        &coursecraft_course::AnimEffect {
            from: Some(Direction::Bottom),
            ..coursecraft_course::AnimEffect::default()
        },
        500,
        Easing::EaseOut,
        "s1",
        "txt",
    );
    assert!(harness.editor.run(cmd, params).is_some());
    assert!(txt_object(&harness).entrance.is_none(), "None clears it");
}

#[test]
fn moving_an_object_on_the_timeline_keeps_its_animation() {
    use coursecraft_course::{Direction, Easing};
    let mut harness = TestHarness::new(timeline_media_course());
    harness.editor.selection.slide = "s1".into();
    harness.editor.selection.set_many(["txt"]);
    harness.idle_frame();
    let (cmd, params) = crate::properties::animation_command(
        "entrance",
        "fly_in",
        &coursecraft_course::AnimEffect {
            from: Some(Direction::Left),
            ..coursecraft_course::AnimEffect::default()
        },
        800,
        Easing::EaseOut,
        "s1",
        "txt",
    );
    assert!(harness.editor.run(cmd, params).is_some());
    harness.idle_frame();
    assert_eq!(harness.editor.ui.properties.entrance_effect, "fly_in");
    // What a timeline bar drag commits.
    let moved = harness.editor.run(
        "object.update",
        serde_json::json!({
            "slide": "s1", "object": "txt",
            "timing": { "start_ms": 1500, "end_ms": null }
        }),
    );
    assert!(moved.is_some());
    harness.idle_frame();
    assert!(
        txt_object(&harness).entrance.is_some(),
        "stored animation kept"
    );
    assert_eq!(
        harness.editor.ui.properties.entrance_effect, "fly_in",
        "the inspector still shows it"
    );
    assert_eq!(
        harness.editor.ui.properties.entrance_fx.direction(),
        Some(Direction::Left)
    );
}

/// Every command id the editor dispatches exists in the engine. The
/// Animations section once sent `object.set_entrance`, which the engine
/// does not have, so nothing an author chose was ever stored.
#[test]
fn every_command_the_editor_sends_exists() {
    let known: std::collections::BTreeSet<&str> = coursecraft_engine::registry()
        .iter()
        .map(|cmd| cmd.id)
        .collect();
    let prefixes: std::collections::BTreeSet<&str> =
        known.iter().filter_map(|id| id.split('.').next()).collect();
    // Menu aliases resolved in menu.rs before anything runs.
    let aliases = [
        "object.bring_forward",
        "object.send_backward",
        "object.reorder_backward",
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        panic!("read {dir:?}");
    };
    let mut unknown = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".rs") || name.contains("test") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for literal in source.split('"').skip(1).step_by(2) {
            let Some((prefix, rest)) = literal.split_once('.') else {
                continue;
            };
            let word =
                |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            // File names such as `course.json` are not commands.
            let file = [
                "ccraft", "json", "zip", "png", "tsv", "html", "md", "txt", "vtt", "mp3", "mp4",
                "jpg",
            ]
            .contains(&rest);
            if word(prefix)
                && word(rest)
                && !file
                && prefixes.contains(prefix)
                && !known.contains(literal)
                && !aliases.contains(&literal)
            {
                unknown.push(format!("{name}: {literal}"));
            }
        }
    }
    assert!(unknown.is_empty(), "unknown command ids: {unknown:?}");
}

#[test]
fn timeline_play_shows_entrance_and_exit_on_the_canvas() {
    use coursecraft_course::{AnimEffect, Animation, Direction, Easing};
    let mut course = timeline_media_course();
    let Some(slide) = course.slide_mut("s1") else {
        panic!("slide s1");
    };
    let Some(obj) = slide.objects.iter_mut().find(|o| o.id == "txt") else {
        panic!("txt");
    };
    obj.timing = coursecraft_course::Timing {
        start_ms: 1000,
        end_ms: Some(5000),
    };
    obj.entrance = Some(Animation {
        effect: AnimEffect::with_from("fly_in", Direction::Left),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    obj.exit = Some(Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    let Some(slide) = course.slide("s1") else {
        panic!("slide s1");
    };
    let items = coursecraft_render::compose_authoring_layers(&course, slide, None, &[], false);
    let rest = |items: &[coursecraft_render::DrawItem]| {
        items
            .iter()
            .find(|i| i.hit == coursecraft_render::Hit::Object("txt".into()))
            .map(|i| (i.rect.min.x, i.opacity))
    };
    let Some((rest_x, _)) = rest(&items) else {
        panic!("txt drawn at rest");
    };
    let at = |ms| crate::canvas::timeline_animated_items(&items, slide, course.slide_size, ms);
    assert_eq!(rest(&at(500)), None, "not on screen before it starts");
    let Some((mid_x, _)) = rest(&at(1500)) else {
        panic!("flying in");
    };
    assert!(
        mid_x < rest_x - 100.0,
        "halfway in from the left: {mid_x} vs {rest_x}"
    );
    assert_eq!(
        rest(&at(2500)),
        Some((rest_x, 1.0)),
        "at rest in the middle"
    );
    let Some((_, fading)) = rest(&at(4500)) else {
        panic!("fading out");
    };
    assert!((fading - 0.5).abs() < 0.01, "half faded: {fading}");
    assert_eq!(rest(&at(5000)), None, "gone after its exit");
    // The video and audio objects outside the text keep their own rules.
    assert!(at(2500).len() >= items.len() - 1);
}

#[test]
fn new_scene_starts_with_a_selected_slide_in_one_undo() {
    let mut harness = TestHarness::new(timeline_media_course());
    harness.idle_frame();
    let scenes_before = harness.editor.session.course.scenes.len();
    crate::toolbar::add_new_scene(&mut harness.editor);
    let course = &harness.editor.session.course;
    assert_eq!(course.scenes.len(), scenes_before + 1);
    let Some(scene) = course.scenes.last() else {
        panic!("new scene");
    };
    assert_eq!(scene.slides.len(), 1, "the new scene has a slide");
    let new_slide = scene.slides.first().map(|s| s.id.clone());
    assert_eq!(Some(harness.editor.selection.slide.clone()), new_slide);
    // New Slide now adds to the new scene.
    crate::toolbar::add_new_slide(&mut harness.editor);
    let Some(scene) = harness.editor.session.course.scenes.last() else {
        panic!("new scene");
    };
    assert_eq!(
        scene.slides.len(),
        2,
        "New Slide adds to the selected scene"
    );
    harness.editor.run("edit.undo", serde_json::json!({}));
    harness.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(
        harness.editor.session.course.scenes.len(),
        scenes_before,
        "scene and its first slide undo together"
    );
}

#[test]
fn reselecting_the_same_animation_adds_no_undo_step() {
    use coursecraft_course::{Direction, Easing};
    let mut harness = TestHarness::new(timeline_media_course());
    let make = |effect, ms| {
        crate::properties::animation_command(
            "entrance",
            effect,
            &coursecraft_course::AnimEffect {
                from: Some(Direction::Left),
                ..coursecraft_course::AnimEffect::default()
            },
            ms,
            Easing::EaseOut,
            "s1",
            "txt",
        )
    };
    let none = make("none", 500);
    assert!(
        crate::properties::animation_unchanged(&none, &txt_object(&harness)),
        "None over no animation changes nothing"
    );
    let (cmd, params) = make("fly_in", 600);
    assert!(harness.editor.run(cmd, params).is_some());
    let obj = txt_object(&harness);
    assert!(crate::properties::animation_unchanged(
        &make("fly_in", 600),
        &obj
    ));
    assert!(!crate::properties::animation_unchanged(
        &make("fly_in", 700),
        &obj
    ));
    assert!(!crate::properties::animation_unchanged(
        &make("fade", 600),
        &obj
    ));
    assert!(!crate::properties::animation_unchanged(&none, &obj));
}

#[test]
fn scene_header_plus_adds_a_slide_to_that_scene() {
    let mut course = timeline_media_course();
    course.scenes.push(coursecraft_course::Scene {
        id: "scene2".into(),
        title: "Scene 2".into(),
        slides: Vec::new(),
    });
    let mut harness = TestHarness::new(course);
    harness.idle_frame();
    let Some(title) = harness.find_text("Scene 2") else {
        panic!("scene 2 header drawn");
    };
    // The + is the rightmost text on the header row (an icon glyph).
    let raw_input = egui::RawInput {
        screen_rect: Some(harness.screen_rect),
        ..Default::default()
    };
    let output = harness.ctx.run_ui(raw_input, |ui| {
        let _ = harness.editor.show(ui);
    });
    let mut plus: Option<egui::Pos2> = None;
    let mut walk = vec![];
    for clipped in &output.shapes {
        walk.push(clipped.shape.clone());
    }
    while let Some(shape) = walk.pop() {
        match shape {
            egui::epaint::Shape::Vec(v) => walk.extend(v),
            egui::epaint::Shape::Text(t) => {
                let c = crate::test_harness::text_center(&t);
                if (c.y - title.y).abs() < 4.0
                    && c.x > title.x + 40.0
                    && c.x < 400.0
                    && plus.is_none_or(|p| c.x > p.x)
                {
                    plus = Some(c);
                }
            }
            _ => {}
        }
    }
    output.drop_without_applying_deltas();
    let Some(plus) = plus else {
        panic!("+ drawn on the scene header");
    };
    harness.click_at(plus);
    harness.idle_frame();
    let Some(scene) = harness
        .editor
        .session
        .course
        .scenes
        .iter()
        .find(|s| s.id == "scene2")
    else {
        panic!("scene2");
    };
    assert_eq!(scene.slides.len(), 1, "a slide was added to Scene 2");
    let added = scene.slides.first().map(|s| s.id.clone());
    assert_eq!(
        Some(harness.editor.selection.slide.clone()),
        added,
        "and selected"
    );
}

#[test]
fn edit_menu_shows_named_undo_and_redo() {
    let mut harness = TestHarness::new(Session::blank("Course").course);
    harness.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Renamed" }),
    );
    harness.idle_frame();

    let edit = harness.find_text("Edit").expect("Edit menu");
    harness.click_at(edit);
    harness.idle_frame();
    assert!(
        harness.find_text_last("Undo course title").is_some(),
        "Edit menu should show named Undo"
    );
    assert!(
        harness.find_text_last("Redo").is_some(),
        "Edit menu should show plain Redo when redo stack is empty"
    );

    // Undo and re-open the Edit menu to verify named Redo.
    harness.editor.run("edit.undo", serde_json::json!({}));
    harness.idle_frame();
    assert!(
        harness.find_text_last("Redo course title").is_some(),
        "Edit menu should show named Redo"
    );
}

#[test]
fn sanitize_media_src_avoids_case_only_collisions() {
    let existing = vec![("Logo.png".to_owned(), std::sync::Arc::from(&b"a"[..]))];
    let picked = crate::sanitize_media_src("logo.png", &existing);
    assert!(!picked.eq_ignore_ascii_case("Logo.png"));
    assert_eq!(picked, "logo-2.png");
}

#[test]
fn exit_animations_play_in_preview_with_slide_transition() {
    let mut harness = TestHarness::new(timeline_media_course());
    harness.editor.selection.slide = "s1".into();
    // Add a second object on s1 for the Disappear/Fade exit.
    let added = harness.editor.run(
        "object.add",
        serde_json::json!({
            "slide": "s1",
            "id": "shape2",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#10B981" },
            "frame": { "x": 400.0, "y": 120.0, "w": 160.0, "h": 80.0 }
        }),
    );
    assert!(added.is_some());

    let (cmd1, params1) = crate::properties::animation_command(
        "exit",
        "fly_in",
        &coursecraft_course::AnimEffect {
            from: Some(coursecraft_course::Direction::Left),
            ..coursecraft_course::AnimEffect::default()
        },
        1000,
        coursecraft_course::Easing::Linear,
        "s1",
        "txt",
    );
    assert!(harness.editor.run(cmd1, params1).is_some());

    let (cmd2, params2) = crate::properties::animation_command(
        "exit",
        "fade",
        &coursecraft_course::AnimEffect {
            from: Some(coursecraft_course::Direction::Bottom),
            ..coursecraft_course::AnimEffect::default()
        },
        1000,
        coursecraft_course::Easing::Linear,
        "s1",
        "shape2",
    );
    assert!(harness.editor.run(cmd2, params2).is_some());

    // Add slide transition on s1.
    assert!(
        harness
            .editor
            .run(
                "slide.set_transition",
                serde_json::json!({
                    "slide": "s1",
                    "transition": { "type": "fade", "duration_ms": 500 }
                }),
            )
            .is_some()
    );

    let slide = harness.editor.session.course.slide("s1").expect("s1");
    let duration_ms = slide.duration_ms;
    assert!(slide.transition.is_some());
    assert!(slide.object("txt").expect("txt").0.exit.is_some());
    assert!(slide.object("shape2").expect("shape2").0.exit.is_some());

    let mut preview = crate::preview::Preview::new(&harness.editor.session.course, Some("s1"));
    let state = preview.state.take().expect("preview state");
    let mut player =
        coursecraft_logic::Player::resume(&preview.course, state).expect("resume player");

    // Advance to mid-exit (duration_ms - 500ms).
    let mid_ms = duration_ms.saturating_sub(500);
    player.handle(&coursecraft_logic::Input::Tick { ms: mid_ms });
    let txt_mid = player.sample_object("txt").expect("txt mid-exit");
    assert!(txt_mid.offset.0 < 0.0, "fly_in exit should shift left");
    let shape_mid = player.sample_object("shape2").expect("shape2 mid-exit");
    assert!((shape_mid.opacity - 0.5).abs() < 1e-2);

    // Advance to end of timeline.
    player.handle(&coursecraft_logic::Input::Tick { ms: 500 });
    assert!(player.sample_object("txt").is_none());
    assert!(player.sample_object("shape2").is_none());
}

#[test]
fn find_glossary_hit_opens_course_settings_with_resources_expanded() {
    let mut session = coursecraft_engine::Session::blank("Glossary Find");
    session
        .execute(
            "glossary.add",
            &serde_json::json!({
                "id": "g1",
                "term": "Telemetry",
                "definition": "Automated measurement of remote course events."
            }),
        )
        .expect("add glossary");
    let mut harness = TestHarness::new(session.course);
    harness.idle_frame();
    harness.press(Key::F, egui::Modifiers::COMMAND);
    harness.type_text("Telemetry");
    harness.idle_frame();

    let hit_pos = harness
        .find_text_last("Telemetry")
        .expect("glossary hit in Find & Replace");
    harness.click_at(hit_pos);
    harness.idle_frame();

    assert!(
        harness.editor.ui.show_course_settings_window,
        "clicking a glossary hit should open Course Settings"
    );
    let cs_pos = harness
        .find_text("Course settings")
        .expect("Course settings title");
    harness.scroll_over(egui::pos2(cs_pos.x, cs_pos.y + 200.0), -600.0);
    harness.idle_frame();
    assert!(
        harness.find_text("Add term").is_some(),
        "Resources & glossary section should be expanded so Add term is visible"
    );
}

#[test]
fn resources_glossary_sort_button_and_order_controls_follow_ui_rules() {
    // C-06: "Sort A–Z" button dispatches glossary.sort, disables with tooltip when < 2 terms
    // or already sorted, and ↑/↓/✕ buttons are >= 24×24 pt with disabled tooltips.
    let mut session = coursecraft_engine::Session::blank("Glossary UI");
    session
        .execute(
            "glossary.add",
            &serde_json::json!({ "term": "OnlyOne", "definition": "Single entry." }),
        )
        .expect("add single glossary term");

    let mut harness = TestHarness::new_with_size(session.course, (1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    harness.editor.ui.course_settings.open_resources = true;
    for _ in 0..3 {
        harness.idle_frame();
    }
    let cs_pos = harness
        .find_text("Course settings")
        .expect("Course settings title");
    harness.scroll_over(egui::pos2(cs_pos.x, cs_pos.y + 200.0), -600.0);
    harness.idle_frame();

    let find_widget_for_text = |h: &mut TestHarness, label: &str| {
        h.last_widgets
            .iter()
            .find(|n| n.label.as_deref() == Some(label))
            .cloned()
            .unwrap_or_else(|| panic!("missing widget for {label:?}"))
    };

    // With < 2 terms, "Sort A–Z" is disabled with explanatory tooltip.
    let sort_btn = find_widget_for_text(&mut harness, "Sort A–Z");
    assert!(
        sort_btn.disabled,
        "Sort A–Z must be disabled with < 2 terms"
    );
    harness.advance_time(1.0);
    harness.idle_frame();
    let tip = harness.probe_tooltip(sort_btn.bounds.center());
    assert_eq!(
        tip.as_deref(),
        Some("Add at least two terms to sort"),
        "unexpected tooltip for < 2 terms: {tip:?}"
    );

    // Replace with 3 unsorted terms: ["Zebra", "alpha", "Beta"].
    let mut s2 = coursecraft_engine::Session::blank("Glossary Unsorted");
    for term in ["Zebra", "alpha", "Beta"] {
        s2.execute("glossary.add", &serde_json::json!({ "term": term }))
            .expect("add term");
    }
    let mut harness = TestHarness::new_with_size(s2.course, (1280.0, 1100.0));
    harness.editor.ui.show_course_settings_window = true;
    harness.editor.ui.course_settings.open_resources = true;
    for _ in 0..3 {
        harness.idle_frame();
    }
    let cs_pos = harness
        .find_text("Course settings")
        .expect("Course settings title");
    harness.scroll_over(egui::pos2(cs_pos.x, cs_pos.y + 200.0), -600.0);
    harness.idle_frame();

    // Check ↑/↓/✕ button sizes (>= 24×24 pt) and disabled tooltips on first ↑ / last ↓.
    let mut up_buttons: Vec<_> = harness
        .last_widgets
        .iter()
        .filter(|n| n.label.as_deref() == Some("↑"))
        .cloned()
        .collect();
    up_buttons.sort_by(|a, b| a.bounds.min.y.total_cmp(&b.bounds.min.y));
    assert_eq!(up_buttons.len(), 3, "expected 3 ↑ buttons");
    for btn in &up_buttons {
        assert!(
            btn.bounds.width() >= 24.0 && btn.bounds.height() >= 24.0,
            "↑ button must be >= 24×24 pt, got {:?}",
            btn.bounds.size()
        );
    }
    assert!(up_buttons[0].disabled, "first ↑ button must be disabled");
    harness.advance_time(1.0);
    harness.idle_frame();
    assert_eq!(
        harness
            .probe_tooltip(up_buttons[0].bounds.center())
            .as_deref(),
        Some("Already at the top")
    );

    let mut down_buttons: Vec<_> = harness
        .last_widgets
        .iter()
        .filter(|n| n.label.as_deref() == Some("↓"))
        .cloned()
        .collect();
    down_buttons.sort_by(|a, b| a.bounds.min.y.total_cmp(&b.bounds.min.y));
    assert_eq!(down_buttons.len(), 3, "expected 3 ↓ buttons");
    for btn in &down_buttons {
        assert!(
            btn.bounds.width() >= 24.0 && btn.bounds.height() >= 24.0,
            "↓ button must be >= 24×24 pt, got {:?}",
            btn.bounds.size()
        );
    }
    assert!(down_buttons[2].disabled, "last ↓ button must be disabled");
    assert_eq!(
        harness
            .probe_tooltip(down_buttons[2].bounds.center())
            .as_deref(),
        Some("Already at the bottom")
    );

    let remove_buttons: Vec<_> = harness
        .last_widgets
        .iter()
        .filter(|n| n.label.as_deref() == Some("✕"))
        .cloned()
        .collect();
    assert_eq!(remove_buttons.len(), 3, "expected 3 ✕ buttons");
    for btn in &remove_buttons {
        assert!(
            btn.bounds.width() >= 24.0 && btn.bounds.height() >= 24.0,
            "✕ button must be >= 24×24 pt, got {:?}",
            btn.bounds.size()
        );
    }

    // "Sort A–Z" is enabled and clicking it sorts the glossary via glossary.sort.
    let sort_btn = find_widget_for_text(&mut harness, "Sort A–Z");
    assert!(!sort_btn.disabled, "Sort A–Z must be enabled when unsorted");
    harness.click_at(sort_btn.bounds.center());
    harness.idle_frame();

    let terms: Vec<&str> = harness
        .editor
        .session
        .course
        .glossary
        .iter()
        .map(|g| g.term.as_str())
        .collect();
    assert_eq!(terms, vec!["alpha", "Beta", "Zebra"]);

    // After sorting, "Sort A–Z" is disabled with the already-sorted tooltip.
    let sort_btn = find_widget_for_text(&mut harness, "Sort A–Z");
    assert!(
        sort_btn.disabled,
        "Sort A–Z must be disabled once already sorted"
    );
    harness.advance_time(1.0);
    harness.idle_frame();
    assert_eq!(
        harness.probe_tooltip(sort_btn.bounds.center()).as_deref(),
        Some("Glossary is already sorted A–Z")
    );
}

#[test]
fn timeline_show_all_and_unlock_all_dispatch_single_undo_batches() {
    let mut h = TestHarness::new_with_size(two_scene_course(), (1440.0, 900.0));
    let slide_id = h.editor.selection.slide.clone();
    // Ensure at least 3 objects exist on the active slide.
    for i in 0..3 {
        h.editor.run(
            "object.add",
            serde_json::json!({
                "slide": slide_id,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
                "frame": { "x": 100.0 + i as f32 * 40.0, "y": 100.0, "w": 60.0, "h": 40.0 }
            }),
        );
        assert!(h.editor.ui.last_error.is_none());
    }
    let obj_ids: Vec<String> = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .expect("slide")
        .objects
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert!(obj_ids.len() >= 3);
    let hide_ids = obj_ids[0..3].to_vec();
    let lock_ids = obj_ids[0..2].to_vec();

    // Hide 3 objects and lock 2 objects.
    h.editor.run(
        "object.hide_in_editor",
        serde_json::json!({ "slide": slide_id, "objects": hide_ids, "hidden": true }),
    );
    h.editor.run(
        "object.lock",
        serde_json::json!({ "slide": slide_id, "objects": lock_ids, "locked": true }),
    );
    h.idle_frame();

    // "Show all (3)" appears in the Timeline header and unhides all 3 in 1 undo step.
    let show_all_pos = h
        .find_text_containing("Show all (3)")
        .expect("Show all (3) button");
    h.click_at(show_all_pos);
    h.idle_frame();

    let slide = h.editor.session.course.slide(&slide_id).expect("slide");
    assert!(
        slide.objects.iter().all(|o| !o.editor_hidden),
        "all objects must be unhidden after clicking Show all (3)"
    );
    assert!(
        h.find_text_containing("Show all (").is_none(),
        "Show all button must disappear when no hidden objects remain"
    );

    // Ctrl+Z restores all 3 hidden objects in 1 undo step.
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    let slide = h.editor.session.course.slide(&slide_id).expect("slide");
    assert_eq!(
        slide.objects.iter().filter(|o| o.editor_hidden).count(),
        3,
        "1 undo step must restore all 3 hidden objects"
    );

    // Re-unhide via Show all (3), then test "Unlock all (2)".
    let show_all_pos = h
        .find_text_containing("Show all (3)")
        .expect("Show all (3) button");
    h.click_at(show_all_pos);
    h.idle_frame();

    let unlock_all_pos = h
        .find_text_containing("Unlock all (2)")
        .expect("Unlock all (2) button");
    h.click_at(unlock_all_pos);
    h.idle_frame();

    let slide = h.editor.session.course.slide(&slide_id).expect("slide");
    assert!(
        slide.objects.iter().all(|o| !o.locked),
        "all objects must be unlocked after clicking Unlock all (2)"
    );
    assert!(
        h.find_text_containing("Unlock all (").is_none(),
        "Unlock all button must disappear when no locked objects remain"
    );

    // Ctrl+Z restores both locked objects in 1 undo step.
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    let slide = h.editor.session.course.slide(&slide_id).expect("slide");
    assert_eq!(
        slide.objects.iter().filter(|o| o.locked).count(),
        2,
        "1 undo step must restore both locked objects"
    );
}

#[test]
fn view_menu_show_all_and_unlock_all_disabled_tooltips_and_batch_undo() {
    let mut h = TestHarness::new_with_size(two_scene_course(), (1440.0, 900.0));
    let find_widget_for_text = |h: &mut TestHarness, label: &str| {
        let pos = h.find_text(label).expect("label position");
        h.last_widgets
            .iter()
            .find(|n| {
                n.role == egui::Role::Button
                    && (n.label.as_deref() == Some(label) || n.bounds.contains(pos))
            })
            .cloned()
            .expect("widget node")
    };
    let slide_id = h.editor.selection.slide.clone();
    for i in 0..2 {
        h.editor.run(
            "object.add",
            serde_json::json!({
                "slide": slide_id,
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#445566" },
                "frame": { "x": 80.0 + i as f32 * 50.0, "y": 80.0, "w": 60.0, "h": 40.0 }
            }),
        );
        assert!(h.editor.ui.last_error.is_none());
    }

    // Open View menu when no objects are hidden or locked -> both items disabled with tooltips.
    let view_menu = h.find_text("View").expect("View menu");
    h.click_at(view_menu);
    h.idle_frame();

    let show_item = find_widget_for_text(&mut h, "Show all hidden objects");
    assert!(
        show_item.disabled,
        "Show all hidden objects must be disabled when none hidden"
    );
    h.advance_time(1.0);
    h.idle_frame();
    assert_eq!(
        h.probe_tooltip(show_item.bounds.center()).as_deref(),
        Some("No hidden objects on this slide")
    );

    let unlock_item = find_widget_for_text(&mut h, "Unlock all objects");
    assert!(
        unlock_item.disabled,
        "Unlock all objects must be disabled when none locked"
    );
    assert_eq!(
        h.probe_tooltip(unlock_item.bounds.center()).as_deref(),
        Some("No locked objects on this slide")
    );

    // Close menu, hide and lock 2 objects, then test View menu batch actions.
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    let obj_ids: Vec<String> = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .expect("slide")
        .objects
        .iter()
        .take(2)
        .map(|o| o.id.clone())
        .collect();
    h.editor.run(
        "object.hide_in_editor",
        serde_json::json!({ "slide": slide_id, "objects": obj_ids, "hidden": true }),
    );
    h.editor.run(
        "object.lock",
        serde_json::json!({ "slide": slide_id, "objects": obj_ids, "locked": true }),
    );
    h.idle_frame();

    let view_menu = h.find_text("View").expect("View menu");
    h.click_at(view_menu);
    h.idle_frame();
    let show_item = find_widget_for_text(&mut h, "Show all hidden objects");
    assert!(!show_item.disabled);
    h.click_at(show_item.bounds.center());
    h.idle_frame();
    assert_eq!(
        h.editor
            .session
            .course
            .slide(&slide_id)
            .expect("slide")
            .objects
            .iter()
            .filter(|o| o.editor_hidden)
            .count(),
        0
    );

    let view_menu = h.find_text("View").expect("View menu");
    h.click_at(view_menu);
    h.idle_frame();
    let unlock_item = find_widget_for_text(&mut h, "Unlock all objects");
    assert!(!unlock_item.disabled);
    h.click_at(unlock_item.bounds.center());
    h.idle_frame();
    assert_eq!(
        h.editor
            .session
            .course
            .slide(&slide_id)
            .expect("slide")
            .objects
            .iter()
            .filter(|o| o.locked)
            .count(),
        0
    );

    // Single undo restores both locked objects.
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(
        h.editor
            .session
            .course
            .slide(&slide_id)
            .expect("slide")
            .objects
            .iter()
            .filter(|o| o.locked)
            .count(),
        2
    );
}

#[test]
fn m31_01_file_shortcuts_emit_requests_and_shift_s_beats_s() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = TestHarness::new(course);
    h.idle_frame();

    let key_event = |key: Key, modifiers: Modifiers| {
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]
    };

    let reqs = h.frame(key_event(Key::S, Modifiers::COMMAND));
    assert!(
        reqs.iter().any(|r| matches!(r, EditorRequest::Save)),
        "Cmd/Ctrl+S must emit EditorRequest::Save, got {reqs:?}"
    );

    let reqs = h.frame(key_event(Key::S, Modifiers::COMMAND | Modifiers::SHIFT));
    assert!(
        reqs.iter().any(|r| matches!(r, EditorRequest::SaveAs)),
        "Cmd/Ctrl+Shift+S must emit EditorRequest::SaveAs, got {reqs:?}"
    );
    assert!(
        !reqs.iter().any(|r| matches!(r, EditorRequest::Save)),
        "Cmd/Ctrl+Shift+S must not also emit EditorRequest::Save, got {reqs:?}"
    );

    let reqs = h.frame(key_event(Key::O, Modifiers::COMMAND));
    assert!(
        reqs.iter().any(|r| matches!(r, EditorRequest::Open)),
        "Cmd/Ctrl+O must emit EditorRequest::Open, got {reqs:?}"
    );

    let reqs = h.frame(key_event(Key::N, Modifiers::COMMAND));
    assert!(
        reqs.iter().any(|r| matches!(r, EditorRequest::New)),
        "Cmd/Ctrl+N must emit EditorRequest::New, got {reqs:?}"
    );
}

#[test]
fn m31_02_file_shortcuts_suppressed_in_text_edit_inline_preview_and_welcome() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = TestHarness::new(course);
    h.idle_frame();

    let key_event = |key: Key, modifiers: Modifiers| {
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]
    };
    let is_file_req = |r: &EditorRequest| {
        matches!(
            r,
            EditorRequest::Save | EditorRequest::SaveAs | EditorRequest::Open | EditorRequest::New
        )
    };

    // 1. TextEdit focused (Find & Replace dialog focuses its search TextEdit).
    h.press(Key::F, Modifiers::COMMAND);
    h.idle_frame();
    assert!(
        h.ctx.egui_wants_keyboard_input(),
        "Find dialog must own keyboard input"
    );
    for (key, mods) in [
        (Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
        (Key::O, Modifiers::COMMAND),
        (Key::N, Modifiers::COMMAND),
        (Key::S, Modifiers::COMMAND),
    ] {
        let reqs = h.frame(key_event(key, mods));
        assert!(
            !reqs.iter().any(is_file_req),
            "file shortcut ({key:?}) must be suppressed while TextEdit is focused: {reqs:?}"
        );
    }
    // Ctrl+S in a field is the exception (R4): the field commits first, and the
    // save follows one frame later, never in the same frame.
    let after = h.idle_frame();
    assert!(
        after.iter().any(|r| matches!(r, EditorRequest::Save)),
        "Ctrl+S in a text field saves after the field commits: {after:?}"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    // 2. Inline canvas text edit active.
    let slide_id = h.editor.selection.slide.clone();
    let text_obj = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .and_then(|s| {
            s.objects
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Text { .. }))
        })
        .map(|o| o.id.clone())
        .expect("text object in hello course");
    h.editor.ui.inline_edit = Some(crate::canvas::InlineEdit {
        slide: slide_id,
        object: text_obj,
        buffer: "Editing".to_owned(),
        original: "Editing".to_owned(),
        is_button: false,
        is_shape: false,
        cell: None,
        just_entered: false,
        live_runs: Vec::new(),
        live_paragraphs: Vec::new(),
        last_revision: 0,
        state_name: None,
        extra_id: None,
    });
    for (key, mods) in [
        (Key::S, Modifiers::COMMAND),
        (Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
        (Key::O, Modifiers::COMMAND),
        (Key::N, Modifiers::COMMAND),
    ] {
        let reqs = h.frame(key_event(key, mods));
        assert!(
            !reqs.iter().any(is_file_req),
            "file shortcut ({key:?}) must be suppressed during inline edit: {reqs:?}"
        );
    }
    h.editor.ui.inline_edit = None;
    h.idle_frame();

    // 3. Preview focused.
    h.editor.open_preview_at(None);
    h.idle_frame();
    for (key, mods) in [
        (Key::S, Modifiers::COMMAND),
        (Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
        (Key::O, Modifiers::COMMAND),
        (Key::N, Modifiers::COMMAND),
    ] {
        let reqs = h.frame(key_event(key, mods));
        assert!(
            !reqs.iter().any(is_file_req),
            "file shortcut ({key:?}) must be suppressed while preview is open: {reqs:?}"
        );
    }
    h.editor.preview = None;
    h.idle_frame();

    // 4. Welcome screen: Save/SaveAs suppressed, Open/New allowed.
    h.editor.ui.show_welcome = true;
    h.idle_frame();
    let save_reqs = h.frame(key_event(Key::S, Modifiers::COMMAND));
    assert!(!save_reqs.iter().any(is_file_req));
    let save_as_reqs = h.frame(key_event(Key::S, Modifiers::COMMAND | Modifiers::SHIFT));
    assert!(!save_as_reqs.iter().any(is_file_req));
    let open_reqs = h.frame(key_event(Key::O, Modifiers::COMMAND));
    assert!(open_reqs.iter().any(|r| matches!(r, EditorRequest::Open)));
    let new_reqs = h.frame(key_event(Key::N, Modifiers::COMMAND));
    assert!(new_reqs.iter().any(|r| matches!(r, EditorRequest::New)));
}

#[test]
fn m31_04_file_menu_hints_and_compound_shortcut_formatting() {
    assert_eq!(
        crate::menu::format_shortcut("Ctrl+S / Ctrl+Shift+S", true, |_| true),
        "⌘S / ⇧⌘S"
    );
    assert_eq!(
        crate::menu::format_shortcut("Ctrl+S / Ctrl+Shift+S", false, |_| true),
        "Ctrl+S / Ctrl+Shift+S"
    );
    assert_eq!(
        crate::menu::format_shortcut("Ctrl+N / Ctrl+O", true, |_| true),
        "⌘N / ⌘O"
    );

    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = TestHarness::new(course);
    h.idle_frame();

    let file_menu = h.find_text("File").expect("File menu");
    h.click_at(file_menu);
    h.idle_frame();

    for (label, sc) in [
        ("New", "Ctrl+N"),
        ("Open…", "Ctrl+O"),
        ("Save", "Ctrl+S"),
        ("Save as…", "Ctrl+Shift+S"),
    ] {
        assert!(
            h.find_text(label).is_some(),
            "File menu must show '{label}'"
        );
        let hint = crate::menu::shortcut_label(&h.ctx, sc);
        assert!(
            h.find_text(&hint).is_some(),
            "File menu must show shortcut hint '{hint}' for '{label}'"
        );
    }
}

/// L-04: Static sentence-case lint over all production `Dialog::new("...")` titles
/// in `crates/ui-egui/src/` and menu/toolbar/navigator button literals in
/// `menu.rs`, `toolbar.rs`, and `slides.rs`.
#[test]
fn static_sentence_case_lint_for_dialogs_menus_toolbar_and_slides() {
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
        "Esc",
        "Enter",
        "Delete",
        "Ctrl",
        "Cmd",
        "Shift",
        "Alt",
        "F2",
        "F7",
        "F11",
        "F12",
    ];

    fn strip_test_module(src: &str) -> &str {
        src.find("\n#[cfg(test)]").map_or(src, |idx| &src[..idx])
    }

    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|line| {
                if line.trim_start().starts_with("//") {
                    ""
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn parse_quoted_literal(s: &str) -> Option<(String, usize)> {
        let mut chars = s.char_indices();
        let (_, first) = chars.next()?;
        if first != '"' {
            return None;
        }
        let mut out = String::new();
        let mut escaped = false;
        for (idx, ch) in chars {
            if escaped {
                out.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                return Some((out, idx + 1));
            } else {
                out.push(ch);
            }
        }
        None
    }

    fn collect_first_string_arg_after(src: &str, needle: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut offset = 0;
        while let Some(pos) = src[offset..].find(needle) {
            let after = offset + pos + needle.len();
            let rest = src[after..].trim_start();
            if let Some((lit, consumed)) = parse_quoted_literal(rest) {
                results.push(lit);
                offset = after + (src[after..].len() - rest.len()) + consumed;
            } else {
                offset = after;
            }
        }
        results
    }

    fn collect_nth_string_arg_after(src: &str, needle: &str, arg_index: usize) -> Vec<String> {
        let mut results = Vec::new();
        let mut offset = 0;
        while let Some(pos) = src[offset..].find(needle) {
            let after = offset + pos + needle.len();
            let rest = &src[after..];
            let mut depth = 0usize;
            let mut comma_count = 0usize;
            let mut in_str = false;
            let mut escaped = false;
            let mut arg_start = if arg_index == 0 { Some(0) } else { None };
            for (idx, ch) in rest.char_indices() {
                if in_str {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        in_str = false;
                    }
                    continue;
                }
                match ch {
                    '"' => {
                        if let Some(start) = arg_start
                            && rest[start..idx].trim().is_empty()
                        {
                            if let Some((lit, _)) = parse_quoted_literal(&rest[idx..]) {
                                results.push(lit);
                            }
                            break;
                        }
                        in_str = true;
                    }
                    '(' | '[' | '{' => depth += 1,
                    ')' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                    ']' | '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        comma_count += 1;
                        if comma_count == arg_index {
                            arg_start = Some(idx + 1);
                        } else if comma_count > arg_index {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            offset = after;
        }
        results
    }

    fn check_sentence_case(phrase: &str, context: &str, violations: &mut Vec<String>) {
        if phrase.is_empty() || phrase == "Don't Save" {
            return;
        }
        if let Some(first_alpha) = phrase.chars().find(|c| c.is_alphabetic())
            && !first_alpha.is_uppercase()
        {
            violations.push(format!(
                "{context}: {phrase:?} must start with an uppercase letter"
            ));
            return;
        }
        let tokens: Vec<&str> = phrase
            .split(|c: char| c.is_whitespace() || c == '/')
            .filter(|t| !t.is_empty())
            .collect();
        for &token in tokens.iter().skip(1) {
            let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '–');
            if trimmed.is_empty() || ALLOWED_CAPS.contains(&trimmed) {
                continue;
            }
            if trimmed.chars().any(char::is_uppercase) {
                violations.push(format!(
                    "{context}: unexpected Title Case token {trimmed:?} in {phrase:?}"
                ));
            }
        }
    }

    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    let mut dialog_count = 0usize;
    let mut button_count = 0usize;

    let entries = std::fs::read_dir(&src_dir).expect("read crates/ui-egui/src");
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if file_name == "tests.rs"
            || file_name == "test_harness.rs"
            || file_name.ends_with("_tests.rs")
            || file_name.starts_with("bughunt_")
        {
            continue;
        }

        let raw = std::fs::read_to_string(&path).expect("read source file");
        let prod = strip_line_comments(strip_test_module(&raw));

        for title in collect_first_string_arg_after(&prod, "Dialog::new(") {
            dialog_count += 1;
            check_sentence_case(&title, &format!("{file_name} Dialog::new"), &mut violations);
        }

        if matches!(file_name, "menu.rs" | "toolbar.rs" | "slides.rs") {
            for needle in ["Button::new(", ".button(", ".menu_button("] {
                for label in collect_first_string_arg_after(&prod, needle) {
                    button_count += 1;
                    check_sentence_case(&label, &format!("{file_name} {needle}"), &mut violations);
                }
            }
            for (needle, arg_idx) in [
                (".checkbox(", 1),
                ("quiet_button(", 1),
                ("primary_button(", 1),
                ("toolbar_toggle(", 2),
                ("toolbar_toggle(", 4),
                ("toolbar_button_enabled(", 2),
            ] {
                for label in collect_nth_string_arg_after(&prod, needle, arg_idx) {
                    button_count += 1;
                    check_sentence_case(
                        &label,
                        &format!("{file_name} {needle} arg {arg_idx}"),
                        &mut violations,
                    );
                }
            }
        }

        if file_name == "properties.rs" {
            for heading in collect_nth_string_arg_after(&prod, "inspector_section_header(", 1) {
                button_count += 1;
                check_sentence_case(
                    &heading,
                    "properties.rs inspector_section_header",
                    &mut violations,
                );
            }
        }
    }

    assert!(
        dialog_count >= 25,
        "expected >= 25 Dialog::new titles scanned, found {dialog_count}"
    );
    assert!(
        button_count >= 80,
        "expected >= 80 menu/toolbar/navigator button literals scanned, found {button_count}"
    );
    assert!(
        violations.is_empty(),
        "sentence-case lint violations:\n{}",
        violations.join("\n")
    );
}

/// D-02: Static CI guard ensuring every `.add_enabled(` and `.add_enabled_ui(` call
/// in production UI code attaches `.on_disabled_hover_text(` and zero `ui.disable()`
/// calls exist (`UX-08` / `M32-05`).
#[test]
fn every_add_enabled_has_disabled_reason() {
    fn strip_comments_keep_lines(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let bytes = src.as_bytes();
        let mut i = 0usize;
        let mut in_str = false;
        let mut in_char = false;
        let mut escaped = false;
        let mut block_depth = 0usize;

        while i < bytes.len() {
            let b = bytes[i];
            if block_depth > 0 {
                if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    block_depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    block_depth -= 1;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
            if in_str {
                out.push(b as char);
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if in_char {
                out.push(b as char);
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'\'' {
                    in_char = false;
                }
                i += 1;
                continue;
            }
            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                block_depth = 1;
                out.push_str("  ");
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = true;
                out.push('"');
                i += 1;
                continue;
            }
            // Only enter char literal if it looks like `'x'` or `'\x'`, not a lifetime `'a`.
            if b == b'\''
                && ((i + 2 < bytes.len() && bytes[i + 2] == b'\'')
                    || (i + 3 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 3] == b'\''))
            {
                in_char = true;
                out.push('\'');
                i += 1;
                continue;
            }
            let ch = src[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn strip_inline_cfg_test_modules(comment_free: &str) -> String {
        let lines: Vec<&str> = comment_free.lines().collect();
        let mut kept: Vec<String> = Vec::with_capacity(lines.len());
        let mut idx = 0usize;
        while idx < lines.len() {
            let trimmed = lines[idx].trim();
            let mut inline_mod_start = None;
            if trimmed.starts_with("#[cfg(test)]") {
                let rest = trimmed.trim_start_matches("#[cfg(test)]").trim();
                let is_mod_decl = |s: &str| {
                    (s.starts_with("mod ")
                        || s.starts_with("pub mod ")
                        || s.starts_with("pub(crate) mod "))
                        && !s.trim_end().ends_with(';')
                };
                if is_mod_decl(rest) {
                    inline_mod_start = Some(idx);
                } else if rest.is_empty() {
                    let mut next = idx + 1;
                    while next < lines.len() {
                        let t = lines[next].trim();
                        if t.is_empty() || t.starts_with("#[") {
                            next += 1;
                            continue;
                        }
                        if is_mod_decl(t) {
                            inline_mod_start = Some(next);
                        }
                        break;
                    }
                }
            }
            if let Some(mod_line) = inline_mod_start {
                while idx < mod_line {
                    kept.push(String::new());
                    idx += 1;
                }
                let mut depth = 0usize;
                let mut opened = false;
                while idx < lines.len() {
                    let line = lines[idx];
                    let bytes = line.as_bytes();
                    let mut i = 0usize;
                    let mut in_str = false;
                    let mut in_char = false;
                    let mut escaped = false;
                    while i < bytes.len() {
                        let b = bytes[i];
                        if in_str {
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'"' {
                                in_str = false;
                            }
                            i += 1;
                            continue;
                        }
                        if in_char {
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'\'' {
                                in_char = false;
                            }
                            i += 1;
                            continue;
                        }
                        if b == b'"' {
                            in_str = true;
                        } else if b == b'\''
                            && ((i + 2 < bytes.len() && bytes[i + 2] == b'\'')
                                || (i + 3 < bytes.len()
                                    && bytes[i + 1] == b'\\'
                                    && bytes[i + 3] == b'\''))
                        {
                            in_char = true;
                        } else if b == b'{' {
                            depth += 1;
                            opened = true;
                        } else if b == b'}' && depth > 0 {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    kept.push(String::new());
                    idx += 1;
                    if opened && depth == 0 {
                        break;
                    }
                }
                continue;
            }
            kept.push(lines[idx].to_owned());
            idx += 1;
        }
        kept.join("\n")
    }

    fn find_matching_paren(src: &str, open_paren_idx: usize) -> Option<usize> {
        let bytes = src.as_bytes();
        let mut depth = 0usize;
        let mut in_str = false;
        let mut in_char = false;
        let mut escaped = false;
        let mut i = open_paren_idx;
        while i < bytes.len() {
            let b = bytes[i];
            if in_str {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if in_char {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'\'' {
                    in_char = false;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_str = true,
                b'\''
                    if (i + 2 < bytes.len() && bytes[i + 2] == b'\'')
                        || (i + 3 < bytes.len()
                            && bytes[i + 1] == b'\\'
                            && bytes[i + 3] == b'\'') =>
                {
                    in_char = true;
                }
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn statement_end_after(src: &str, start_idx: usize) -> usize {
        let bytes = src.as_bytes();
        let mut depth = 0usize;
        let mut in_str = false;
        let mut in_char = false;
        let mut escaped = false;
        let mut i = start_idx;
        while i < bytes.len() {
            let b = bytes[i];
            if in_str {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if in_char {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'\'' {
                    in_char = false;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_str = true,
                b'\''
                    if (i + 2 < bytes.len() && bytes[i + 2] == b'\'')
                        || (i + 3 < bytes.len()
                            && bytes[i + 1] == b'\\'
                            && bytes[i + 3] == b'\'') =>
                {
                    in_char = true;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                b';' | b',' if depth == 0 => return i,
                _ => {}
            }
            i += 1;
        }
        src.len()
    }

    fn bound_var_before(src: &str, call_idx: usize) -> Option<&str> {
        let prefix = &src[..call_idx];
        let stmt_start = prefix.rfind([';', '{', '}']).map_or(0, |p| p + 1);
        let stmt = prefix[stmt_start..].trim_start();
        let after_let = stmt.strip_prefix("let ")?.trim_start();
        let after_mut = after_let
            .strip_prefix("mut ")
            .unwrap_or(after_let)
            .trim_start();
        let ident_end = after_mut
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after_mut.len());
        let ident = &after_mut[..ident_end];
        if ident.is_empty() { None } else { Some(ident) }
    }

    fn line_number_at(src: &str, byte_idx: usize) -> usize {
        src[..byte_idx].bytes().filter(|&b| b == b'\n').count() + 1
    }

    fn scan_source(file_name: &str, raw: &str) -> (usize, Vec<String>) {
        let prod = strip_inline_cfg_test_modules(&strip_comments_keep_lines(raw));
        let mut violations = Vec::new();
        let mut checked = 0usize;

        // 1. Forbid `ui.disable()` in production code.
        let mut disable_offset = 0usize;
        while let Some(pos) = prod[disable_offset..].find(".disable()") {
            let abs = disable_offset + pos;
            let line = line_number_at(&prod, abs);
            violations.push(format!(
                "{file_name}:{line}: forbidden `ui.disable()` in production code; use `add_enabled_ui` with `.on_disabled_hover_text(...)`"
            ));
            disable_offset = abs + ".disable()".len();
        }

        // 2. Check every `.add_enabled(` and `.add_enabled_ui(`.
        for needle in [".add_enabled(", ".add_enabled_ui("] {
            let is_ui_block = needle == ".add_enabled_ui(";
            let mut offset = 0usize;
            while let Some(pos) = prod[offset..].find(needle) {
                let abs = offset + pos;
                let open_paren = abs + needle.len() - 1;
                offset = open_paren + 1;
                checked += 1;

                let Some(close_paren) = find_matching_paren(&prod, open_paren) else {
                    let line = line_number_at(&prod, abs);
                    violations.push(format!("{file_name}:{line}: unmatched paren in `{needle}`"));
                    continue;
                };

                let after_call = close_paren + 1;
                let stmt_end = statement_end_after(&prod, after_call);
                let chain = &prod[after_call..stmt_end];
                let inside = &prod[open_paren..=close_paren];

                let mut has_reason = chain.contains(".on_disabled_hover_text(")
                    || (is_ui_block && inside.contains(".on_disabled_hover_text("));

                if !has_reason && let Some(var) = bound_var_before(&prod, abs) {
                    let lookahead_end = prod
                        .char_indices()
                        .map(|(i, _)| i)
                        .find(|&i| i >= (stmt_end + 800).min(prod.len()))
                        .unwrap_or(prod.len());
                    let lookahead_compact: String = prod[stmt_end..lookahead_end]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let direct = format!("{var}.on_disabled_hover_text(");
                    let chained = format!("{var}.on_hover_text(");
                    if lookahead_compact.contains(&direct)
                        || (lookahead_compact.contains(&chained)
                            && lookahead_compact.contains(".on_disabled_hover_text("))
                    {
                        has_reason = true;
                    }
                }

                if !has_reason {
                    let line = line_number_at(&prod, abs);
                    violations.push(format!(
                        "{file_name}:{line}: `{needle}` is missing `.on_disabled_hover_text(...)`"
                    ));
                }
            }
        }

        (checked, violations)
    }

    // Self-check: out-of-line `#[cfg(test)] mod foo;` (like lib.rs & main.rs) must NOT blind
    // the scanner, inline `#[cfg(test)] mod tests { ... }` must be ignored, and char literals
    // like `'"'` must not break paren matching.
    let (_, synthetic_violations) = scan_source(
        "synthetic.rs",
        r#"
#[cfg(test)]
mod external_tests;

fn prod(ui: &mut egui::Ui, ok: bool) {
    #[cfg(test)]
    let _c = '"';
    ui.add_enabled(ok, egui::Button::new("Bare"));
    ui.disable();
}

#[cfg(test)]
mod tests {
    fn ignored(ui: &mut egui::Ui) {
        ui.disable();
        ui.add_enabled(false, egui::Button::new("In test"));
    }
}
"#,
    );
    assert_eq!(
        synthetic_violations.len(),
        2,
        "guard self-check must catch prod violations after out-of-line #[cfg(test)] mod and ignore inline #[cfg(test)] mod tests, got: {synthetic_violations:?}"
    );

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dirs = [
        manifest.join("src"),
        manifest.join("../../apps/coursecraft/src"),
    ];
    let mut violations = Vec::new();
    let mut scanned_files = 0usize;
    let mut checked_calls = 0usize;
    let mut lib_rs_checked_calls = 0usize;

    for dir in &dirs {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if file_name == "tests.rs"
                || file_name == "test_harness.rs"
                || file_name.ends_with("_tests.rs")
                || file_name.starts_with("bughunt_")
            {
                continue;
            }
            scanned_files += 1;
            let raw = std::fs::read_to_string(&path).expect("read production source file");
            let (file_checked, file_violations) = scan_source(file_name, &raw);
            if file_name == "lib.rs" {
                lib_rs_checked_calls = file_checked;
            }
            checked_calls += file_checked;
            violations.extend(file_violations);
        }
    }

    assert!(
        scanned_files >= 30,
        "expected >= 30 production files scanned, found {scanned_files}"
    );
    assert!(
        lib_rs_checked_calls >= 1,
        "lib.rs must be scanned past its out-of-line #[cfg(test)] mod declarations (expected quiet_button_enabled)"
    );
    assert!(
        checked_calls >= 60,
        "expected >= 60 add_enabled/add_enabled_ui calls checked, found {checked_calls}"
    );
    assert!(
        violations.is_empty(),
        "disabled-control guard violations ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn r1_03_ui_media_walkers_golden() {
    let json = r##"{
        "schema_version": 1,
        "id": "r1-03-ui-media",
        "title": "R1-03 UI Media",
        "slide_size": { "w": 1280, "h": 720 },
        "fonts": [
            { "family": "Lato", "style": "regular", "src": "fonts/lato.ttf" },
            { "family": "Empty", "style": "bold", "src": "" }
        ],
        "background_audio": { "src": "bgm.mp3" },
        "player": { "logo": "logo.png" },
        "resources": [
            { "id": "res-file", "title": "PDF", "target": { "kind": "file", "src": "handout.pdf" } },
            { "id": "res-url", "title": "URL", "target": { "kind": "url", "url": "https://example.com" } }
        ],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "obj-img",
                        "name": "Hero",
                        "kind": { "type": "image", "src": "hero.png", "alt": "" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    },
                    {
                        "id": "obj-lot",
                        "name": "Anim",
                        "kind": { "type": "lottie", "src": "anim.json" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    },
                    {
                        "id": "obj-aud",
                        "name": "Voice",
                        "kind": { "type": "audio", "src": "voice.mp3", "captions": "voice.vtt" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
                    },
                    {
                        "id": "obj-vid",
                        "name": "Clip",
                        "kind": {
                            "type": "video",
                            "src": "clip.mp4",
                            "captions": "clip.vtt",
                            "poster": "clip-poster.png"
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 320.0, "h": 180.0 }
                    },
                    {
                        "id": "obj-shp",
                        "name": "ShapePic",
                        "kind": {
                            "type": "shape",
                            "shape": "rectangle",
                            "fill": "#ffffff",
                            "fill_style": { "type": "picture", "src": "shape-fill.png" }
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    },
                    {
                        "id": "obj-btn",
                        "name": "BtnPic",
                        "kind": {
                            "type": "button",
                            "label": "Go",
                            "fill_style": { "type": "picture", "src": "btn-fill.png" }
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 120.0, "h": 44.0 }
                    },
                    {
                        "id": "obj-path",
                        "name": "PathPic",
                        "kind": {
                            "type": "path",
                            "points": [
                                { "x": 0.0, "y": 0.0 },
                                { "x": 1.0, "y": 0.0 },
                                { "x": 0.5, "y": 1.0 }
                            ],
                            "closed": true,
                            "fill": "#ffffff",
                            "fill_style": { "type": "picture", "src": "path-fill.png" },
                            "outline": { "color": "#000000", "width": 1.0 }
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    },
                    {
                        "id": "obj-web",
                        "name": "SiteWeb",
                        "kind": {
                            "type": "web",
                            "source": { "type": "folder", "path": "web/site", "entry": "index.html" }
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0 }
                    },
                    {
                        "id": "obj-st",
                        "name": "Stateful",
                        "kind": { "type": "image", "src": "base-state.png", "alt": "" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 },
                        "states": [{
                            "name": "hover",
                            "src": "state-img.png",
                            "fill_style": { "type": "picture", "src": "state-fill.png" },
                            "extras": [
                                {
                                    "id": "ex-img",
                                    "kind": { "type": "image", "src": "extra-img.png" },
                                    "frame": { "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0 }
                                },
                                {
                                    "id": "ex-shp",
                                    "kind": {
                                        "type": "shape",
                                        "shape": "ellipse",
                                        "fill": "#ffffff",
                                        "fill_style": { "type": "picture", "src": "extra-shape-fill.png" }
                                    },
                                    "frame": { "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0 }
                                },
                                {
                                    "id": "ex-path",
                                    "kind": {
                                        "type": "path",
                                        "points": [
                                            { "x": 0.0, "y": 0.0 },
                                            { "x": 1.0, "y": 0.0 },
                                            { "x": 0.5, "y": 1.0 }
                                        ],
                                        "closed": true,
                                        "fill": "#ffffff",
                                        "fill_style": { "type": "picture", "src": "extra-path-fill.png" },
                                        "outline": { "color": "#000000", "width": 1.0 }
                                    },
                                    "frame": { "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0 }
                                },
                                {
                                    "id": "ex-txt",
                                    "kind": { "type": "text", "text": "Plain" },
                                    "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 20.0 }
                                }
                            ]
                        }]
                    }
                ],
                "layers": [{
                    "id": "l1",
                    "name": "Layer 1",
                    "objects": [{
                        "id": "l-img",
                        "name": "LayerPic",
                        "kind": { "type": "image", "src": "layer-pic.png", "alt": "" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 80.0, "h": 80.0 }
                    }]
                }]
            }]
        }],
        "masters": [{
            "id": "m1",
            "title": "Master 1",
            "duration_ms": 5000,
            "objects": [{
                "id": "m-img",
                "name": "MasterBg",
                "kind": { "type": "image", "src": "master-base.png", "alt": "" },
                "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
            }],
            "layers": [{
                "id": "ml1",
                "name": "Master Layer",
                "objects": [{
                    "id": "ml-img",
                    "name": "MasterLayerPic",
                    "kind": { "type": "image", "src": "master-layer.png", "alt": "" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 }
                }]
            }]
        }],
        "templates": [{
            "id": "t1",
            "title": "Template 1",
            "duration_ms": 5000,
            "objects": [{
                "id": "t-img",
                "name": "TemplatePic",
                "kind": { "type": "image", "src": "template-pic.png", "alt": "" },
                "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
            }]
        }],
        "banks": [{
            "id": "b1",
            "title": "Bank 1",
            "slides": [{
                "id": "bs1",
                "title": "Bank Slide 1",
                "duration_ms": 5000,
                "objects": [{
                    "id": "b-img",
                    "name": "BankPic",
                    "kind": { "type": "image", "src": "bank-base.png", "alt": "" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                }],
                "layers": [{
                    "id": "bl1",
                    "name": "Bank Layer",
                    "objects": [{
                        "id": "bl-img",
                        "name": "BankLayerPic",
                        "kind": { "type": "image", "src": "bank-layer.png", "alt": "" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 }
                    }]
                }]
            }]
        }]
    }"##;
    let course = Course::from_json(json).expect("valid R1-03 course");

    let expected_referenced: std::collections::HashSet<String> = [
        "fonts/lato.ttf",
        "bgm.mp3",
        "logo.png",
        "handout.pdf",
        "hero.png",
        "anim.json",
        "voice.mp3",
        "voice.vtt",
        "clip.mp4",
        "clip.vtt",
        "clip-poster.png",
        "shape-fill.png",
        "btn-fill.png",
        "path-fill.png",
        "web/site/index.html",
        "base-state.png",
        "state-img.png",
        "state-fill.png",
        "extra-img.png",
        "extra-shape-fill.png",
        "extra-path-fill.png",
        "layer-pic.png",
        "master-base.png",
        "master-layer.png",
        "template-pic.png",
        "bank-base.png",
        "bank-layer.png",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    assert_eq!(referenced_media_srcs(&course), expected_referenced);
    assert_eq!(web_folder_prefixes(&course), vec!["web/site/".to_owned()]);

    let mut editor = Editor::new(course);
    // Load a subset of referenced files, plus a web-folder subfile and an unused file.
    editor.set_media(vec![
        ("hero.png".into(), Arc::from(b"hero".as_slice())),
        ("layer-pic.png".into(), Arc::from(b"layer".as_slice())),
        ("state-img.png".into(), Arc::from(b"state".as_slice())),
        ("extra-path-fill.png".into(), Arc::from(b"epath".as_slice())),
        (
            "web/site/index.html".into(),
            Arc::from(b"<h1>hi</h1>".as_slice()),
        ),
        ("web/site/app.js".into(), Arc::from(b"1;".as_slice())),
        ("bgm.mp3".into(), Arc::from(b"bgm".as_slice())),
        ("master-base.png".into(), Arc::from(b"master".as_slice())),
        ("template-pic.png".into(), Arc::from(b"tpl".as_slice())),
        ("bank-layer.png".into(), Arc::from(b"bank".as_slice())),
        ("unused.png".into(), Arc::from(b"unused".as_slice())),
    ]);

    let used_names: Vec<String> = editor.used_media().into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        used_names,
        vec![
            "hero.png",
            "layer-pic.png",
            "state-img.png",
            "extra-path-fill.png",
            "web/site/index.html",
            "web/site/app.js",
            "bgm.mp3",
            "master-base.png",
            "template-pic.png",
            "bank-layer.png",
        ]
    );

    let slide = &editor.session.course.scenes[0].slides[0];
    let slide_media_names: Vec<String> = editor
        .slide_media(slide)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        slide_media_names,
        vec![
            "hero.png",
            "layer-pic.png",
            "state-img.png",
            "extra-path-fill.png",
            "web/site/index.html",
            "web/site/app.js",
        ]
    );

    assert_eq!(
        editor.missing_media(),
        vec![
            "anim.json",
            "bank-base.png",
            "base-state.png",
            "btn-fill.png",
            "clip-poster.png",
            "clip.mp4",
            "clip.vtt",
            "extra-img.png",
            "extra-shape-fill.png",
            "fonts/lato.ttf",
            "handout.pdf",
            "logo.png",
            "master-layer.png",
            "path-fill.png",
            "shape-fill.png",
            "state-fill.png",
            "voice.mp3",
            "voice.vtt",
        ]
    );
}

fn setup_gb_13_group_slide(
    h: &mut TestHarness,
) -> (String, Vec<String>, coursecraft_course::Trigger, usize) {
    let rect_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "id": "gb-rect",
                "name": "Card BG",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                "frame": { "x": 100.0, "y": 100.0, "w": 220.0, "h": 64.0 }
            }),
        )
        .expect("add rect")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("rect id")
        .to_owned();
    let icon_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "id": "gb-icon",
                "name": "Arrow Icon",
                "kind": { "type": "shape", "shape": "triangle", "fill": "#FFFFFF" },
                "frame": { "x": 116.0, "y": 116.0, "w": 24.0, "h": 32.0 }
            }),
        )
        .expect("add icon")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("icon id")
        .to_owned();
    let label_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "id": "gb-label",
                "name": "Label",
                "kind": { "type": "text", "text": "Go back" },
                "frame": { "x": 150.0, "y": 116.0, "w": 150.0, "h": 32.0 }
            }),
        )
        .expect("add label")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("label id")
        .to_owned();

    assert!(
        h.editor
            .run(
                "object.add_state",
                serde_json::json!({
                    "slide": "s-intro",
                    "object": rect_id,
                    "state": { "name": "hover", "opacity": 0.85 }
                }),
            )
            .is_some(),
        "add hover state on rect"
    );

    let member_trigger = coursecraft_course::Trigger {
        when: coursecraft_course::Event::Click {
            object: label_id.clone(),
        },
        action: coursecraft_course::Action::PauseTimeline,
        condition: None,
        disabled: false,
    };
    assert!(
        h.editor
            .run(
                "trigger.add",
                serde_json::json!({
                    "slide": "s-intro",
                    "trigger": member_trigger,
                }),
            )
            .is_some(),
        "add existing member click trigger"
    );
    let triggers_before = h
        .editor
        .session
        .course
        .slide("s-intro")
        .expect("slide")
        .triggers
        .len();

    let member_ids = vec![rect_id, icon_id, label_id];
    let group_id = h
        .editor
        .run(
            "object.group",
            serde_json::json!({
                "slide": "s-intro",
                "objects": member_ids,
            }),
        )
        .expect("group 3 objects")
        .get("group")
        .and_then(|v| v.as_str())
        .expect("group id")
        .to_owned();
    (group_id, member_ids, member_trigger, triggers_before)
}

#[test]
fn gb_13_interactive_group_authoring_ui_end_to_end() {
    let course = Course::from_json(COURSE_JSON).expect("hello course");
    let mut h = TestHarness::new(course);
    h.editor.selection.slide = "s-intro".into();
    h.editor.ui.inspector_tab = InspectorTab::Format;

    // 1. Place 3 objects (rect + shape + text), add a custom state on one member,
    //    add an existing click trigger on one member, and group them.
    let (group_id, member_ids, member_trigger, triggers_before) = setup_gb_13_group_slide(&mut h);

    // 2. Select the complete group, check Arrange menu shows "Make button from group",
    //    and toggle "Act as one button" in Group Inspector.
    h.editor.selection.set_many(member_ids);
    h.idle_frame();
    h.idle_frame();

    let arrange_pos = h.find_text("Arrange").expect("Arrange menu in menu bar");
    h.click_at(arrange_pos);
    h.idle_frame();
    assert!(
        h.find_text("Make button from group").is_some(),
        "Arrange menu should show 'Make button from group' for a non-interactive group"
    );
    // Close Arrange menu before interacting with inspector.
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    let toggle_pos = h
        .find_text("Act as one button")
        .expect("'Act as one button' checkbox in Group Inspector");
    h.click_at(toggle_pos);
    h.idle_frame();
    h.idle_frame();

    {
        let slide = h.editor.session.course.slide("s-intro").expect("slide");
        assert!(
            slide.interactive_group(&group_id).is_some(),
            "group should now be interactive"
        );
        assert_eq!(
            slide.triggers.len(),
            triggers_before,
            "making group interactive must not add or delete triggers"
        );
        assert!(
            slide.triggers.contains(&member_trigger),
            "existing member trigger must remain untouched"
        );
    }

    // Verify Arrange menu now offers "Remove button behaviour".
    let arrange_pos = h.find_text("Arrange").expect("Arrange menu");
    h.click_at(arrange_pos);
    h.idle_frame();
    assert!(
        h.find_text("Remove button behaviour").is_some(),
        "Arrange menu should show 'Remove button behaviour' for an interactive group"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    // 3. Edit group Name ("Back button") and Accessible description in Group Inspector.
    let name_rect = h
        .editor
        .ui
        .properties
        .group_name_rect
        .expect("group Name TextEdit rect");
    h.set_field(name_rect.center(), "Back button");
    h.idle_frame();

    let desc_rect = h
        .editor
        .ui
        .properties
        .group_description_rect
        .expect("group Accessible description TextEdit rect");
    h.set_field(desc_rect.center(), "Return to previous step");
    h.idle_frame();

    {
        let slide = h.editor.session.course.slide("s-intro").expect("slide");
        let (group, _) = slide
            .interactive_group(&group_id)
            .expect("interactive group");
        assert_eq!(group.name, "Back button");
        assert_eq!(
            group.description.as_deref(),
            Some("Return to previous step")
        );
        assert_eq!(
            crate::triggers::object_name(slide, &group_id),
            "Back button"
        );
        assert_eq!(
            crate::triggers::object_label(slide, &group_id),
            "Back button (button group)"
        );
    }

    // 4. Verify timeline folder row reflects "Back button" and the "Button" indicator.
    h.editor.ui.bottom_tab = BottomTab::Timeline;
    h.idle_frame();
    h.idle_frame();
    let group_row_rect = *h
        .editor
        .ui
        .timeline
        .group_row_rects
        .get(&group_id)
        .expect("timeline group row rect");
    assert!(
        h.find_text_in("Back button", group_row_rect).is_some(),
        "timeline folder row should display group name 'Back button'"
    );
    assert!(
        h.find_text_in("Button", group_row_rect).is_some(),
        "timeline folder row should display 'Button' pill"
    );

    // 5. Open Trigger Editor via Group Inspector "Add trigger…" -> assert default target
    //    is group_id -> add a Click trigger.
    let add_trig_pos = h
        .find_text("Add trigger…")
        .expect("'Add trigger…' button in Group Inspector");
    h.click_at(add_trig_pos);
    h.idle_frame();
    assert_eq!(
        h.editor
            .ui
            .trigger_form
            .as_ref()
            .expect("trigger editor open")
            .event_object,
        group_id,
        "default event target should be the interactive group id"
    );
    let ok_pos = h.find_text("OK").expect("Trigger Editor OK button");
    h.click_at(ok_pos);
    h.idle_frame();
    h.idle_frame();
    assert!(
        h.editor.ui.trigger_form.is_none(),
        "trigger editor should close after OK"
    );

    // Also verify Triggers tab "+ Add trigger" defaults to group_id when the group is selected.
    h.editor.ui.bottom_tab = BottomTab::Triggers;
    h.idle_frame();
    h.idle_frame();
    let tab_add_pos = h
        .find_text("+ Add trigger")
        .expect("'+ Add trigger' in Triggers tab");
    h.click_at(tab_add_pos);
    h.idle_frame();
    assert_eq!(
        h.editor
            .ui
            .trigger_form
            .as_ref()
            .expect("trigger editor open from Triggers tab")
            .event_object,
        group_id,
        "Triggers tab '+ Add trigger' should default to selected interactive group id"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    // 6. Verify trigger list displays "Back button (button group)" cleanly with zero
    //    broken-reference warnings.
    let warning_glyph = coursecraft_render::icon(
        coursecraft_render::Icon::Warning,
        coursecraft_render::ICON_SM,
    )
    .text()
    .to_owned();
    assert!(
        h.find_text_containing("Back button (button group)")
            .is_some(),
        "trigger list should display 'Back button (button group)'"
    );
    assert!(
        h.find_text(&warning_glyph).is_none(),
        "group trigger should have zero broken-reference warning badges"
    );
    assert!(
        h.find_text_containing("needs attention").is_none()
            && h.find_text_containing("need attention").is_none(),
        "group trigger should not trigger any attention banner"
    );

    // 7. Attempt to turn "Act as one button" off while the group trigger exists ->
    //    verify blocked with an error message and the group stays interactive.
    h.editor.ui.last_error = None;
    let toggle_pos = h
        .find_text("Act as one button")
        .expect("'Act as one button' checkbox");
    h.click_at(toggle_pos);
    h.idle_frame();
    h.idle_frame();

    assert!(
        h.editor.ui.last_error.is_some(),
        "turning off interactive group while referenced by a trigger should show an error"
    );
    assert!(
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .interactive_group(&group_id)
            .is_some(),
        "group should remain interactive after blocked toggle-off"
    );

    // 8. Undo reverses each edit cleanly in 1 step.
    assert_gb_13_undo_chain(&mut h, &group_id, triggers_before);
}

fn assert_gb_13_undo_chain(h: &mut TestHarness, group_id: &str, triggers_before: usize) {
    assert!(h.editor.run("edit.undo", serde_json::json!({})).is_some());
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .triggers
            .len(),
        triggers_before,
        "1st undo removes the added group trigger"
    );
    assert!(h.editor.run("edit.undo", serde_json::json!({})).is_some());
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .interactive_group(group_id)
            .expect("group")
            .0
            .description,
        None,
        "2nd undo clears Accessible description"
    );
    assert!(h.editor.run("edit.undo", serde_json::json!({})).is_some());
    assert_ne!(
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .interactive_group(group_id)
            .expect("group")
            .0
            .name,
        "Back button",
        "3rd undo reverts group name"
    );
    assert!(h.editor.run("edit.undo", serde_json::json!({})).is_some());
    assert!(
        h.editor
            .session
            .course
            .slide("s-intro")
            .expect("slide")
            .interactive_group(group_id)
            .is_none(),
        "4th undo reverts interactive group back to plain group"
    );
}

#[test]
fn gb_13_layer_group_labels_and_toolbar_insert_trigger() {
    let course = Course::from_json(COURSE_JSON).expect("hello course");
    let mut h = TestHarness::new(course);
    h.editor.selection.slide = "s-intro".into();

    let layer_id = h
        .editor
        .run(
            "layer.add",
            serde_json::json!({
                "slide": "s-intro",
                "id": "ly-panel",
                "name": "Overlay"
            }),
        )
        .expect("add layer")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("layer id")
        .to_owned();

    let a_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "layer": layer_id,
                "id": "ly-bg",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#1F6FEB" },
                "frame": { "x": 40.0, "y": 40.0, "w": 160.0, "h": 48.0 }
            }),
        )
        .expect("add a")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("a id")
        .to_owned();
    let b_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s-intro",
                "layer": layer_id,
                "id": "ly-txt",
                "kind": { "type": "text", "text": "Close" },
                "frame": { "x": 50.0, "y": 48.0, "w": 140.0, "h": 32.0 }
            }),
        )
        .expect("add b")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("b id")
        .to_owned();

    let group_id = h
        .editor
        .run(
            "object.group",
            serde_json::json!({
                "slide": "s-intro",
                "objects": [a_id, b_id]
            }),
        )
        .expect("group layer objects")
        .get("group")
        .and_then(|v| v.as_str())
        .expect("group id")
        .to_owned();

    assert!(
        h.editor
            .run(
                "group.set_interactive",
                serde_json::json!({
                    "slide": "s-intro",
                    "group": group_id,
                    "interactive": true,
                    "name": "Dismiss card"
                }),
            )
            .is_some()
    );

    let slide = h.editor.session.course.slide("s-intro").expect("slide");
    assert_eq!(
        crate::triggers::object_name(slide, &group_id),
        "Dismiss card"
    );
    assert_eq!(
        crate::triggers::object_label(slide, &group_id),
        "Dismiss card (button group, Overlay)"
    );

    // Selecting the layer group and invoking toolbar::insert_trigger targets the group id.
    h.editor.selection.layer = Some(layer_id);
    h.editor.selection.set_many(vec![a_id, b_id]);
    crate::toolbar::insert_trigger(&mut h.editor);
    let form = h
        .editor
        .ui
        .trigger_form
        .as_ref()
        .expect("trigger form opened via toolbar::insert_trigger");
    assert_eq!(form.event_object, group_id);
}

#[test]
fn tr_15_tracking_ui_course_settings_and_inspector() {
    use coursecraft_course::{QuizTracking, SlidesViewedTracking};

    let mut session = Session::blank("Course");
    let scene_id = session.course.scenes[0].id.clone();
    session
        .execute(
            "slide.add",
            &serde_json::json!({ "scene": scene_id, "title": "Slide 2" }),
        )
        .expect("add slide 2");
    let mut h = TestHarness::new_with_size(session.course, (1280.0, 1200.0));

    // 1. Course Settings: initially no tracking and no triggers -> banner is visible.
    h.editor.ui.show_course_settings_window = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    assert!(
        h.find_text(
            "this course never reports a result to the LMS: add a Finish button on the last slide"
        )
        .is_some(),
        "missing-result banner should be visible initially"
    );

    let pos = h.find_text("Course title").expect("Course title");
    h.scroll_over(pos, -250.0);
    h.idle_frame();

    // Toggle Slides viewed on -> defaults to All, clears the banner.
    let sv_pos = h
        .find_text_last("Slides viewed")
        .expect("Slides viewed checkbox");
    h.click_at(sv_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.slides_viewed,
        Some(SlidesViewedTracking::All)
    );
    let pos = h.find_text("Slides viewed").expect("Slides viewed");
    h.scroll_over(pos, 250.0);
    h.idle_frame();
    assert!(
        h.find_text(
            "this course never reports a result to the LMS: add a Finish button on the last slide"
        )
        .is_none(),
        "banner must clear once valid Slides viewed tracking is enabled"
    );
    let pos = h.find_text("Course title").expect("Course title");
    h.scroll_over(pos, -250.0);
    h.idle_frame();

    // Switch to At least N, then set N > slide_count -> inline warning renders cleanly.
    let at_least_pos = h.find_text_last("At least").expect("At least radio");
    h.click_at(at_least_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.slides_viewed,
        Some(SlidesViewedTracking::AtLeast { count: 2 })
    );
    h.editor.run(
        "course.set_tracking",
        serde_json::json!({ "slides_viewed": { "mode": "at_least", "count": 5 } }),
    );
    h.idle_frame();
    assert!(
        h.find_text("Exceeds course slide count (2); completes when all slides are viewed.")
            .is_some(),
        "warning must render when N > slide_count"
    );

    // Toggle Quiz result on -> no passing score & no graded questions/results slide warnings render.
    let qr_pos = h
        .find_text_last("Quiz result")
        .expect("Quiz result checkbox");
    h.click_at(qr_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.quiz,
        Some(QuizTracking { slide: None })
    );
    let pos = h.find_text("Quiz result").expect("Quiz result");
    h.scroll_over(pos, -150.0);
    h.idle_frame();
    assert!(
        h.find_text("No passing score is set in Quiz settings.")
            .is_some(),
        "warning must render when no passing score is set"
    );
    assert!(
        h.find_text("This course has no graded questions or results slide.")
            .is_some(),
        "warning must render when no graded questions or results slide exists"
    );

    // 2. Course Inspector reflects the exact same live state.
    h.editor.ui.show_course_settings_window = false;
    h.editor.ui.inspector_tab = crate::InspectorTab::Course;
    h.idle_frame();
    assert!(
        h.find_text("Slides viewed (5), Quiz result, Triggers")
            .is_some(),
        "Course Inspector must display live tracking summary"
    );
}

#[test]
fn tr_15_tracking_ui_results_combo_publish_undo_and_web() {
    use coursecraft_course::{QuizTracking, SlidesViewedTracking};

    let mut session = Session::blank("Course");
    let first_slide_id = session.course.scenes[0].slides[0].id.clone();
    session
        .execute(
            "question.insert",
            &serde_json::json!({ "slide": first_slide_id }),
        )
        .expect("insert question");
    let res = session
        .execute("results.insert", &serde_json::json!({}))
        .expect("insert results slide");
    let res_id = res
        .get("slide")
        .and_then(|v| v.as_str())
        .expect("results slide id")
        .to_owned();
    let mut h = TestHarness::new_with_size(session.course, (1280.0, 1200.0));
    let res_title = h
        .editor
        .session
        .course
        .results_slides()
        .find(|s| s.id == res_id)
        .map(|s| s.title.clone())
        .expect("results slide");

    // 1. Toggle Quiz result on in Course Settings -> automatically picks the first results slide.
    h.editor.ui.show_course_settings_window = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    let pos = h.find_text("Course title").expect("Course title");
    h.scroll_over(pos, -250.0);
    h.idle_frame();

    let qr_pos = h
        .find_text_last("Quiz result")
        .expect("Quiz result checkbox");
    h.click_at(qr_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.quiz,
        Some(QuizTracking {
            slide: Some(res_id.clone())
        })
    );

    // Switch combo back to "Course quiz (all graded questions)".
    let combo_pos = h.find_text_last(&res_title).expect("Results slide combo");
    h.click_at(combo_pos);
    h.idle_frame();
    let course_quiz_opt = h
        .find_text_last("Course quiz (all graded questions)")
        .expect("Course quiz option");
    h.click_at(course_quiz_opt);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.quiz,
        Some(QuizTracking { slide: None })
    );

    // 2. Publish window reflects the same state, edits directly, and undoes/redoes in 1 step.
    h.editor.ui.show_course_settings_window = false;
    h.editor.open_publish_dialog();
    for _ in 0..3 {
        h.idle_frame();
    }
    let total_slides = h.editor.session.course.play_slide_count();
    let all_label = format!("All slides ({total_slides})");
    let pos = h.find_text("SCORM 1.2").expect("SCORM 1.2");
    h.scroll_over(pos, -200.0);
    h.idle_frame();
    assert!(
        h.find_text("Course quiz (all graded questions)").is_some(),
        "Publish dialog must reflect Quiz result state set in Course Settings"
    );

    let sv_pos = h
        .find_text_last("Slides viewed")
        .expect("Slides viewed checkbox");
    h.click_at(sv_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.slides_viewed,
        Some(SlidesViewedTracking::All)
    );

    // 1-step Ctrl+Z undo & Ctrl+Shift+Z redo.
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(h.editor.session.course.tracking.slides_viewed, None);
    h.press(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.tracking.slides_viewed,
        Some(SlidesViewedTracking::All)
    );

    // 3. Web standard shows quiet note instead of LMS tracking checkboxes.
    let pos = h.find_text(&all_label).expect("All slides radio");
    h.scroll_over(pos, 200.0);
    h.idle_frame();
    let web_pos = h
        .find_text_last("Web (any website, no LMS)")
        .expect("Web standard radio");
    h.click_at(web_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.ui.publish.standard,
        crate::publish::PublishStandard::Web
    );
    let pos = h
        .find_text("Web (any website, no LMS)")
        .expect("Web standard radio");
    h.scroll_over(pos, -200.0);
    h.idle_frame();
    assert!(
        h.find_text("Web packages do not report completion to an LMS.")
            .is_some()
    );
    assert!(h.find_text("Slides viewed").is_none());
    assert!(h.find_text("Quiz result").is_none());
}

#[test]
fn tr_15_tracking_ui_custom_and_missing_quiz_slide_labels() {
    let session = Session::blank("Course");
    let custom_slide_id = session.course.scenes[0].slides[0].id.clone();
    let custom_slide_title = session.course.scenes[0].slides[0].title.clone();
    let mut h = TestHarness::new_with_size(session.course, (1280.0, 1200.0));

    // A play-order slide without a SubmitResults trigger must show its title, not "Missing slide".
    h.editor.run(
        "course.set_tracking",
        serde_json::json!({ "quiz": { "slide": custom_slide_id } }),
    );
    h.editor.ui.show_course_settings_window = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    let pos = h.find_text("Course title").expect("Course title");
    h.scroll_over(pos, -250.0);
    h.idle_frame();
    assert!(
        h.find_text_last(&custom_slide_title).is_some(),
        "valid play-order non-results slide must display its title in Results slide combo"
    );
    assert!(
        h.find_text_containing("Missing slide").is_none(),
        "valid play-order slide must not be labeled Missing slide"
    );
    assert!(
        h.find_text("Selected results slide no longer exists.")
            .is_none(),
        "valid play-order slide must not show missing-slide warning"
    );

    // A genuinely missing slide id must display both the fallback combo label and warning.
    h.editor.run(
        "course.set_tracking",
        serde_json::json!({ "quiz": { "slide": "deleted-slide" } }),
    );
    h.idle_frame();
    assert!(
        h.find_text("Missing slide (deleted-slide)").is_some(),
        "missing slide id must show fallback combo label"
    );
    assert!(
        h.find_text("Selected results slide no longer exists.")
            .is_some(),
        "missing slide id must show inline warning"
    );
}

#[test]
fn inline_button_label_edit_uses_the_painted_label_colour() {
    let mut button: coursecraft_course::Object = serde_json::from_str(
        r##"{"id":"b","name":"b","kind":{"type":"button","label":"Go","fill":"#1F4E8C"},"frame":{"x":0,"y":0,"w":100,"h":40}}"##,
    )
    .expect("button json");
    assert_eq!(
        crate::canvas::inline_button_label_color_for_test(&button).as_deref(),
        Some("#ffffff")
    );
    if let coursecraft_course::ObjectKind::Button { fill, .. } = &mut button.kind {
        *fill = coursecraft_course::Color("#F2B705".into());
    }
    assert_ne!(
        crate::canvas::inline_button_label_color_for_test(&button).as_deref(),
        Some("#ffffff"),
        "a light fill must not get white text while editing"
    );
}
