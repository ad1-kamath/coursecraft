//! Tests for `course.find` and `course.replace`.

use coursecraft_course::{course_strings, string_mut};
use serde_json::{Value as Json, json};

use crate::{EngineError, Session};

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    s.execute(id, &p)
        .unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

fn frame() -> Json {
    json!({ "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 })
}

/// A course with a title, a plain text box, a text box with runs, a button, a
/// question and a master, all containing `cat` in some form.
fn fixture() -> Session {
    let mut s = Session::blank("Cat course");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "plain", "kind": { "type": "text", "text": "Hello cat world" }, "frame": frame() }),
    );
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "runs", "kind": { "type": "text", "text": "abcdef" }, "frame": frame() }),
    );
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": "runs", "start": 1, "end": 3, "bold": true }),
    );
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "btn", "kind": { "type": "button", "label": "Cat button" }, "frame": frame() }),
    );
    ok(
        &mut s,
        "question.set",
        json!({
            "slide": "slide1",
            "question": {
                "kind": "multiple_choice",
                "prompt": "Which cat?",
                "choices": [
                    { "id": "a", "text": "A cat", "correct": true },
                    { "id": "b", "text": "A dog" }
                ]
            }
        }),
    );
    let master = ok(&mut s, "master.add", json!({ "title": "Cat master" }));
    let master_id = master
        .get("id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_owned();
    ok(
        &mut s,
        "object.add",
        json!({ "slide": master_id, "id": "mtext", "kind": { "type": "text", "text": "A cat in master" }, "frame": frame() }),
    );
    s
}

fn object_text(s: &Session, object: &str) -> String {
    let slide = s.course.play_slide("slide1").expect("slide1");
    let (found, _) = slide.object(object).expect("object");
    match &found.kind {
        coursecraft_course::ObjectKind::Text { text, .. } => text.clone(),
        _ => panic!("not a text object"),
    }
}

#[test]
fn find_reaches_title_text_button_question_and_master() {
    let mut s = fixture();
    let result = ok(&mut s, "course.find", json!({ "query": "cat" }));
    let hits = result["hits"].as_array().expect("hits");
    let labels: Vec<&str> = hits
        .iter()
        .filter_map(|hit| hit["label"].as_str())
        .collect();
    assert_eq!(hits.len(), 7, "{labels:#?}");
    for needle in [
        "Course ▸ Title",
        "Button label",
        "Question prompt",
        "Choice 1",
        "Master \"Cat master\" ▸ Title",
        "Master \"Cat master\" ▸ ",
    ] {
        assert!(
            labels.iter().any(|label| label.contains(needle)),
            "no hit labelled {needle:?} in {labels:#?}"
        );
    }
    assert_eq!(result["truncated"], json!(false));
}

#[test]
fn find_and_replace_reach_shape_text() {
    let mut s = Session::blank("Shapes");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "shape",
            "kind": {
                "type": "shape",
                "shape": "rectangle",
                "fill": "#D0D7DE",
                "text": {
                    "text": "A cat in a shape",
                    "style": { "color": "#1F1F1F" }
                }
            },
            "frame": frame()
        }),
    );
    let found = ok(&mut s, "course.find", json!({ "query": "cat" }));
    let hits = found["hits"].as_array().expect("hits");
    assert!(
        hits.iter().any(|hit| hit["field"] == "text"
            && hit["snippet"].as_str().is_some_and(|s| s.contains("cat"))),
        "{hits:#?}"
    );
    ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "dog" }),
    );
    let text = s
        .course
        .play_slide("slide1")
        .and_then(|slide| slide.object("shape"))
        .and_then(|(object, _)| match &object.kind {
            coursecraft_course::ObjectKind::Shape { text, .. } => {
                text.as_ref().map(|text| text.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(text, "A dog in a shape");
}

#[test]
fn find_is_case_sensitive_when_asked_and_whole_word_when_asked() {
    let mut s = fixture();
    let sensitive = ok(
        &mut s,
        "course.find",
        json!({ "query": "Cat", "case": true }),
    );
    assert_eq!(sensitive["hits"].as_array().map(Vec::len), Some(3));
    let partial = ok(&mut s, "course.find", json!({ "query": "ca" }));
    assert_eq!(partial["hits"].as_array().map(Vec::len), Some(7));
    let whole = ok(
        &mut s,
        "course.find",
        json!({ "query": "ca", "whole_word": true }),
    );
    assert_eq!(whole["hits"].as_array().map(Vec::len), Some(0));
}

#[test]
fn find_does_not_change_the_course_or_the_undo_stack() {
    let mut s = Session::blank("Only");
    let revision = s.revision;
    ok(&mut s, "course.find", json!({ "query": "Only" }));
    assert_eq!(s.revision, revision);
    assert!(!s.can_undo());
}

#[test]
fn find_with_an_empty_query_returns_nothing() {
    let mut s = fixture();
    let result = ok(&mut s, "course.find", json!({ "query": "" }));
    assert_eq!(result["hits"].as_array().map(Vec::len), Some(0));
}

#[test]
fn replace_all_then_undo_restores_the_course_exactly() {
    let mut s = fixture();
    let before = serde_json::to_string(&s.course).expect("json");
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "dog", "case": false }),
    );
    assert_eq!(result["replaced"], json!(7));
    assert_eq!(object_text(&s, "plain"), "Hello dog world");
    s.undo().expect("undo");
    assert_eq!(serde_json::to_string(&s.course).expect("json"), before);
}

#[test]
fn replace_reaches_choices_and_master_strings() {
    let mut s = fixture();
    ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "dog" }),
    );
    let slide = s.course.play_slide("slide1").expect("slide1");
    let question = slide.question.as_ref().expect("question");
    assert_eq!(question.choices[0].text, "A dog");
    assert_eq!(question.choices[0].id, "a", "choice ids are never touched");
    let master_text = s
        .course
        .masters
        .first()
        .and_then(|m| m.objects.first())
        .map(|o| match &o.kind {
            coursecraft_course::ObjectKind::Text { text, .. } => text.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    assert_eq!(master_text, "A dog in master");
}

#[test]
fn zero_matches_make_no_undo_step() {
    let mut s = Session::blank("Plain");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "t", "kind": { "type": "text", "text": "one" }, "frame": frame() }),
    );
    let revision = s.revision;
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "zebra", "replacement": "horse" }),
    );
    assert_eq!(result["replaced"], json!(0));
    assert_eq!(s.revision, revision, "no revision bump for a no-op");
    // The one undo step left is the object add, so one undo removes the object.
    s.undo().expect("undo");
    let slide = s.course.play_slide("slide1").expect("slide1");
    assert!(slide.objects.is_empty());
    assert!(!s.can_undo());
}

#[test]
fn only_limits_the_replace_to_the_given_locations() {
    let mut s = fixture();
    let result = ok(
        &mut s,
        "course.replace",
        json!({
            "query": "cat",
            "replacement": "dog",
            "only": [{ "slide": null, "field": "course/title" }]
        }),
    );
    assert_eq!(result["replaced"], json!(1));
    assert_eq!(s.course.title, "dog course");
    assert_eq!(object_text(&s, "plain"), "Hello cat world");
}

#[test]
fn a_match_across_runs_keeps_the_first_touched_run_formatting() {
    let mut s = fixture();
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "bcd", "replacement": "X" }),
    );
    assert_eq!(result["replaced"], json!(1));
    assert_eq!(object_text(&s, "runs"), "aXef");
    let slide = s.course.play_slide("slide1").expect("slide1");
    let (object, _) = slide.object("runs").expect("runs");
    let runs = match &object.kind {
        coursecraft_course::ObjectKind::Text { runs, .. } => runs.clone(),
        _ => panic!("not text"),
    };
    let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, vec!["a", "X", "ef"]);
    assert!(runs.iter().all(|run| !run.text.is_empty()));
    assert_eq!(
        runs[1].bold,
        Some(true),
        "the replacement keeps the bold run"
    );
    assert_eq!(runs[0].bold, None);
}

#[test]
fn variable_references_are_protected_and_counted() {
    let mut s = Session::blank("Vars");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "v", "kind": { "type": "text", "text": "Hi %name% and name" }, "frame": frame() }),
    );
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "name", "replacement": "x" }),
    );
    assert_eq!(result["replaced"], json!(1));
    assert_eq!(result["skipped_variable_refs"], json!(1));
    assert_eq!(object_text(&s, "v"), "Hi %name% and x");
}

#[test]
fn a_replace_that_only_hits_variable_references_changes_nothing() {
    let mut s = Session::blank("Vars");
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "v", "kind": { "type": "text", "text": "Hi %name%" }, "frame": frame() }),
    );
    let revision = s.revision;
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "name", "replacement": "x" }),
    );
    assert_eq!(result["replaced"], json!(0));
    assert_eq!(result["skipped_variable_refs"], json!(1));
    assert_eq!(s.revision, revision);
    assert_eq!(object_text(&s, "v"), "Hi %name%");
}

#[test]
fn hostile_sizes_are_refused_and_roll_back() {
    let mut s = fixture();
    let before = serde_json::to_string(&s.course).expect("json");

    let long_query = "q".repeat(5000);
    let refused = s.execute(
        "course.replace",
        &json!({ "query": long_query, "replacement": "x" }),
    );
    assert!(matches!(refused, Err(EngineError::BadParams { .. })));

    let long_replacement = "r".repeat(5000);
    let refused = s.execute(
        "course.replace",
        &json!({ "query": "cat", "replacement": long_replacement }),
    );
    assert!(matches!(refused, Err(EngineError::BadParams { .. })));

    let big = "a".repeat(4000);
    ok(
        &mut s,
        "object.add",
        json!({ "slide": "slide1", "id": "big", "kind": { "type": "text", "text": big }, "frame": frame() }),
    );
    let before_growth = serde_json::to_string(&s.course).expect("json");
    let refused = s.execute(
        "course.replace",
        &json!({ "query": "a", "replacement": "bb" }),
    );
    assert!(matches!(refused, Err(EngineError::BadParams { .. })));
    assert_eq!(
        serde_json::to_string(&s.course).expect("json"),
        before_growth,
        "a refused replace leaves the course as it was"
    );
    assert_ne!(before, before_growth);
}

#[test]
fn an_empty_query_replaces_nothing_and_makes_no_undo_step() {
    let mut s = Session::blank("Empty");
    let revision = s.revision;
    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "", "replacement": "x" }),
    );
    assert_eq!(result["replaced"], json!(0));
    assert_eq!(s.revision, revision);
    assert!(!s.can_undo());
}

#[test]
fn replace_leaves_paragraph_styles_untouched() {
    let mut s = fixture();
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "para",
            "kind": { "type": "text", "text": "Cat line\nsecond cat line" },
            "frame": frame()
        }),
    );
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({
            "slide": "slide1",
            "object": "para",
            "start": 0,
            "end": 2,
            "list": "number",
            "level": 1,
            "line_spacing": 1.5
        }),
    );
    let before = s.course.slide("slide1").unwrap().object("para").unwrap().0;
    let coursecraft_course::ObjectKind::Text { paragraphs, .. } = &before.kind else {
        panic!("not text");
    };
    let before_paragraphs = paragraphs.clone();

    let result = ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "dog", "only": [
            { "slide": "slide1", "object": "para", "field": "text" }
        ] }),
    );
    assert_eq!(result["replaced"], json!(2));

    let after = s.course.slide("slide1").unwrap().object("para").unwrap().0;
    let coursecraft_course::ObjectKind::Text {
        text, paragraphs, ..
    } = &after.kind
    else {
        panic!("not text");
    };
    assert_eq!(text, "dog line\nsecond dog line");
    assert_eq!(paragraphs, &before_paragraphs);
}

#[test]
fn find_and_replace_reach_table_cell_text() {
    let mut s = Session::blank("Table find");
    ok(
        &mut s,
        "object.add",
        json!({
            "slide": "slide1",
            "id": "table1",
            "kind": {
                "type": "table",
                "columns": [0.5, 0.5],
                "rows": [{
                    "height": 1.0,
                    "cells": [
                        { "text": "A cat here" },
                        { "text": "Another cat" }
                    ]
                }]
            },
            "frame": frame()
        }),
    );
    let found = ok(&mut s, "course.find", json!({ "query": "cat" }));
    let hits = found["hits"].as_array().expect("hits");
    assert!(
        hits.iter().any(|hit| {
            hit["field"]
                .as_str()
                .is_some_and(|field| field == "table/0/0/text")
        }),
        "{found}"
    );
    assert!(
        hits.iter().any(|hit| {
            hit["field"]
                .as_str()
                .is_some_and(|field| field == "table/0/1/text")
        }),
        "{found}"
    );

    let replaced = ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "dog" }),
    );
    assert_eq!(replaced["replaced"], json!(2));
    let coursecraft_course::ObjectKind::Table { rows, .. } = &s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object("table1"))
        .map(|(object, _)| &object.kind)
        .expect("table")
    else {
        panic!("table");
    };
    assert_eq!(rows[0].cells[0].text, "A dog here");
    assert_eq!(rows[0].cells[1].text, "Another dog");
}

#[test]
fn every_walked_string_resolves_to_a_mutable_target() {
    let mut s = fixture();
    let items = course_strings(&s.course);
    assert!(items.len() >= 7);
    for item in &items {
        assert!(
            string_mut(&mut s.course, &item.location).is_some(),
            "{} did not resolve",
            item.label
        );
    }
}
