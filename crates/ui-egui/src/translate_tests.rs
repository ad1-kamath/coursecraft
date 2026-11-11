//! Headless tests for Translate with AI (§ M9 step 5).

use coursecraft_ai::{AiConfig, AiError, ChatReply, ChatRequest, MessageContent};
use coursecraft_course::Course;
use egui::{Event, Key, Modifiers, PointerButton};
use serde_json::{Value, json};

use crate::test_harness::TestHarness;
use crate::{AiPurpose, EditorRequest};

fn ready_config() -> AiConfig {
    AiConfig {
        base_url: "http://localhost:1234/v1".to_string(),
        model: "test-model".to_string(),
        api_key: String::new(),
        speech_to_text_model: "whisper-1".to_string(),
        text_to_speech_model: "tts-1".to_string(),
        voice: "alloy".to_string(),
    }
}

fn small_course() -> Course {
    let value = json!({
        "schema_version": 1,
        "id": "translate-test",
        "title": "Hello %learner%",
        "scenes": [{
            "id": "sc",
            "title": "Scene title",
            "slides": [{
                "id": "s1",
                "title": "Slide title",
                "objects": [{
                    "id": "t1",
                    "kind": { "type": "text", "text": "Body text" },
                    "frame": { "x": 40, "y": 40, "w": 300, "h": 80 },
                }],
            }],
        }],
    });
    Course::from_json(&value.to_string()).expect("small course")
}

fn many_slides_course(count: usize) -> Course {
    let slides: Vec<Value> = (0..count)
        .map(|index| {
            json!({
                "id": format!("s{index}"),
                "title": format!("Slide {index} text"),
            })
        })
        .collect();
    let value = json!({
        "schema_version": 1,
        "id": "translate-many",
        "title": "",
        "scenes": [{ "id": "sc", "title": "", "slides": slides }],
    });
    Course::from_json(&value.to_string()).expect("many slides course")
}

fn harness(course: Course) -> TestHarness {
    let mut harness = TestHarness::new(course);
    harness.editor.set_ai_config(Some(ready_config()));
    harness
}

fn click_for_requests(harness: &mut TestHarness, pos: egui::Pos2) -> Vec<EditorRequest> {
    harness.frame(vec![Event::PointerMoved(pos)]);
    harness.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    harness.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }])
}

fn open(harness: &mut TestHarness) {
    harness.editor.open_translate_dialog();
    harness.idle_frame();
}

fn click_translate(harness: &mut TestHarness) -> Vec<EditorRequest> {
    let button = harness.find_text("Translate").expect("Translate button");
    click_for_requests(harness, button)
}

fn first_chat(requests: &[EditorRequest]) -> Option<(AiPurpose, &ChatRequest)> {
    requests.iter().find_map(|request| match request {
        EditorRequest::AiChat { purpose, request } => Some((purpose.clone(), request)),
        _ => None,
    })
}

fn batch_of(request: &ChatRequest) -> Vec<Value> {
    let text = match &request.messages[1].content {
        MessageContent::Text(text) => text,
        other => panic!("expected text, got {other:?}"),
    };
    let value: Value = serde_json::from_str(text).expect("batch json");
    value.as_array().cloned().unwrap_or_default()
}

fn answer_for(request: &ChatRequest, drop_id: Option<&str>) -> String {
    let items: Vec<Value> = batch_of(request)
        .into_iter()
        .filter(|unit| unit.get("id").and_then(Value::as_str) != drop_id)
        .map(|unit| {
            let id = unit.get("id").and_then(Value::as_str).unwrap_or_default();
            let text = unit.get("text").and_then(Value::as_str).unwrap_or_default();
            json!({ "id": id, "text": format!("[fr] {text}") })
        })
        .collect();
    serde_json::to_string(&Value::Array(items)).expect("json")
}

fn answer(harness: &mut TestHarness, purpose: AiPurpose, text: String) {
    harness.editor.set_ai_result(
        purpose,
        Ok(ChatReply {
            text,
            truncated: false,
        }),
    );
}

/// Translate every batch, answering each one. Returns the requests from the
/// last frame, when no batch is left.
fn translate_all(harness: &mut TestHarness) -> Vec<EditorRequest> {
    translate_all_dropping(harness, None)
}

/// Translate every batch, dropping `drop_id` from the first batch that has it.
fn translate_all_dropping(harness: &mut TestHarness, drop_id: Option<&str>) -> Vec<EditorRequest> {
    let mut requests = click_translate(harness);
    let mut dropped = false;
    for _ in 0..64 {
        let Some((purpose, request)) = first_chat(&requests) else {
            break;
        };
        let contains = drop_id.is_some_and(|id| {
            batch_of(request)
                .iter()
                .any(|unit| unit.get("id").and_then(Value::as_str) == Some(id))
        });
        let text = answer_for(request, if contains && !dropped { drop_id } else { None });
        if contains {
            dropped = true;
        }
        answer(harness, purpose, text);
        requests = harness.idle_frame();
    }
    requests
}

#[test]
fn project_menu_opens_the_window() {
    let mut h = harness(small_course());
    h.idle_frame();
    let project = h.find_text("Project").expect("Project menu");
    h.click_at(project);
    h.idle_frame();
    let item = h
        .find_text_last("Translate with AI…")
        .expect("Translate with AI menu item");
    h.click_at(item);
    h.idle_frame();
    assert!(h.editor.ui.translate.open);
}

#[test]
fn not_ready_shows_settings_and_sends_nothing() {
    let mut h = harness(small_course());
    h.editor.set_ai_config(None);
    open(&mut h);

    assert!(h.find_text("Translate").is_none());
    let button = h
        .find_text("Open AI Settings")
        .expect("Open AI Settings button");
    let requests = click_for_requests(&mut h, button);
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request, EditorRequest::AiChat { .. }))
    );
    assert!(h.editor.ui.ai.open);
    assert!(!h.editor.ui.translate.open);
}

#[test]
fn the_consent_line_names_the_count_and_the_host() {
    let mut h = harness(small_course());
    open(&mut h);
    let count = h.editor.ui.translate.units.len();
    assert!(
        h.find_text(&format!(
            "This sends {count} pieces of text from this course to localhost, on this computer."
        ))
        .is_some()
    );

    let mut remote = ready_config();
    remote.base_url = "https://api.example.com/v1".to_string();
    h.editor.set_ai_config(Some(remote));
    h.idle_frame();
    assert!(
        h.find_text(&format!(
            "This sends {count} pieces of text from this course to api.example.com."
        ))
        .is_some()
    );
}

#[test]
fn batches_go_one_at_a_time() {
    let mut h = harness(many_slides_course(45));
    open(&mut h);
    let total = h.editor.ui.translate.units.len();
    assert!(total > 40, "the test needs more than one batch");
    let requests = click_translate(&mut h);
    h.idle_frame();
    let (purpose, request) = first_chat(&requests).expect("first batch");
    assert_eq!(purpose, AiPurpose::Translate { batch: 0 });
    let first_len = batch_of(request).len();
    assert_eq!(first_len, 40);
    assert!(
        h.find_text(&format!("Batch 1 of {}", total.div_ceil(40)))
            .is_some()
    );
    assert!(
        first_chat(&h.idle_frame()).is_none(),
        "nothing else is sent while the first batch runs"
    );

    let text = answer_for(request, None);
    answer(&mut h, purpose, text);
    let mut requests = h.idle_frame();
    let mut seen = first_len;
    let mut batch = 1;
    while let Some((purpose, request)) = first_chat(&requests) {
        assert_eq!(purpose, AiPurpose::Translate { batch });
        let len = batch_of(request).len();
        assert!(len > 0 && len <= 40, "batch {batch} has {len} units");
        let text = answer_for(request, None);
        answer(&mut h, purpose, text);
        seen += len;
        requests = h.idle_frame();
        batch += 1;
        assert!(batch <= total.div_ceil(40), "more batches than units");
    }
    assert_eq!(seen, total, "every unit was sent exactly once");
}

#[test]
fn a_failed_batch_retries_once_then_marks_its_units_failed() {
    let mut h = harness(small_course());
    open(&mut h);
    let mut requests = click_translate(&mut h);
    let mut batches = 0;
    while let Some(purpose) = first_chat(&requests).map(|(purpose, _)| purpose) {
        h.editor
            .set_ai_result(purpose.clone(), Err("The endpoint is busy.".to_string()));
        let retry = h.idle_frame();
        assert_eq!(
            first_chat(&retry).map(|(purpose, _)| purpose),
            Some(purpose.clone()),
            "the same batch is retried once"
        );

        h.editor
            .set_ai_result(purpose, Err("The endpoint is busy.".to_string()));
        requests = h.idle_frame();
        batches += 1;
        assert!(batches <= 64, "the retry loop must terminate");
    }
    assert!(!h.editor.ui.translate.running);
    assert!(
        h.editor
            .ui
            .translate
            .rows
            .iter()
            .all(|row| row.problem.as_deref() == Some("The endpoint is busy."))
    );
}

#[test]
fn a_rejected_key_stops_the_run() {
    let mut h = harness(many_slides_course(45));
    open(&mut h);
    let requests = click_translate(&mut h);
    let (purpose, _) = first_chat(&requests).expect("first batch");
    h.editor
        .set_ai_result(purpose, Err(AiError::RejectedKey.to_string()));
    let requests = h.idle_frame();

    assert!(first_chat(&requests).is_none(), "nothing else is sent");
    assert!(!h.editor.ui.translate.is_running());
    assert!(h.editor.ui.translate.queue.is_empty());
}

#[test]
fn cancel_drops_the_late_answer() {
    let mut h = harness(small_course());
    open(&mut h);
    let requests = click_translate(&mut h);
    h.idle_frame();
    let (purpose, request) = first_chat(&requests).expect("first batch");
    let text = answer_for(request, None);

    let cancel = h.find_text("Cancel").expect("Cancel button");
    let requests = click_for_requests(&mut h, cancel);
    assert!(requests.iter().any(|request| matches!(
        request,
        EditorRequest::CancelAi(AiPurpose::Translate { batch: 0 })
    )));

    answer(&mut h, purpose, text);
    h.idle_frame();
    assert!(
        h.editor
            .ui
            .translate
            .rows
            .iter()
            .all(|row| row.target.is_empty())
    );
}

#[test]
fn the_review_shows_counts_and_marks_a_dropped_unit() {
    let mut h = harness(small_course());
    open(&mut h);
    let _ = translate_all_dropping(&mut h, Some("course/title"));

    let total = h.editor.ui.translate.rows.len();
    assert!(
        h.find_text(&format!(
            "{} translated, 1 need attention.",
            total.saturating_sub(1)
        ))
        .is_some()
    );
    let row = h
        .editor
        .ui
        .translate
        .rows
        .iter()
        .find(|row| row.id == "course/title")
        .expect("title row");
    assert_eq!(
        row.problem.as_deref(),
        Some("The model didn't translate this one.")
    );
}

#[test]
fn an_edit_that_breaks_a_reference_is_flagged_live() {
    let mut h = harness(small_course());
    open(&mut h);
    let _ = translate_all(&mut h);

    if let Some(row) = h
        .editor
        .ui
        .translate
        .rows
        .iter_mut()
        .find(|row| row.id == "course/title")
    {
        row.target = "Bonjour".to_string();
    }
    h.idle_frame();
    let row = h
        .editor
        .ui
        .translate
        .rows
        .iter()
        .find(|row| row.id == "course/title")
        .expect("title row");
    assert!(
        row.problem
            .as_deref()
            .is_some_and(|problem| problem.contains("%name%")),
        "{:?}",
        row.problem
    );
    let total = h.editor.ui.translate.rows.len();
    assert!(
        h.find_text(&format!(
            "{} translated, 1 need attention.",
            total.saturating_sub(1)
        ))
        .is_some()
    );
}

#[test]
fn apply_is_one_undo_step_and_undo_restores_the_course_exactly() {
    let mut h = harness(small_course());
    let before = h.editor.session.course.clone();
    let before_undo = h.editor.undo_len_for_test();
    open(&mut h);
    let _ = translate_all(&mut h);

    let total = h.editor.ui.translate.rows.len();
    let apply = h
        .find_text(&format!("Apply {total}"))
        .expect("Apply button");
    h.click_at(apply);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before_undo + 1);
    assert_eq!(h.editor.session.course.language.as_deref(), Some("fr"));
    assert_ne!(h.editor.session.course.title, before.title);
    assert!(h.editor.ui.translation_report.is_some());

    h.editor.session.undo().expect("undo");
    assert_eq!(h.editor.session.course, before);
}

#[test]
fn a_course_change_disables_apply() {
    let mut h = harness(small_course());
    open(&mut h);
    let _ = translate_all(&mut h);

    h.editor.run(
        "course.set_title",
        json!({ "title": "Changed while reviewing" }),
    );
    h.idle_frame();

    assert!(h.editor.ui.translate.changed);
    assert!(
        h.find_text("The course changed. Translate again.")
            .is_some()
    );

    let total = h.editor.ui.translate.rows.len();
    let before_undo = h.editor.undo_len_for_test();
    let apply = h
        .find_text(&format!("Apply {total}"))
        .expect("Apply button");
    h.click_at(apply);
    h.idle_frame();
    assert_eq!(
        h.editor.undo_len_for_test(),
        before_undo,
        "Apply is disabled after the course changed"
    );
}

#[test]
fn escape_closes_the_window() {
    let mut h = harness(small_course());
    open(&mut h);
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.translate.open);
}
