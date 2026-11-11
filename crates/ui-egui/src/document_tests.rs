//! Headless tests for Course from Document (§ M9 step 3).

use coursecraft_ai::{ChatReply, MessageContent, Role};
use coursecraft_course::Course;
use egui::{Event, Modifiers, PointerButton};

use crate::test_harness::TestHarness;
use crate::{AiPurpose, DocumentRequestState, EditorRequest};

const OUTLINE: &str = "# Safety\n\
## Welcome\n\
- Keep exits clear\n\
## Quiz\n\
? What do you do when the alarm sounds?\n\
[x] Leave by the nearest exit\n\
[ ] Wait for an email";

fn example() -> &'static str {
    include_str!("../../../examples/hello-course/course.json")
}

fn ready_config() -> coursecraft_ai::AiConfig {
    coursecraft_ai::AiConfig {
        base_url: "http://localhost:1234/v1".to_string(),
        model: "test-model".to_string(),
        api_key: String::new(),
        speech_to_text_model: "whisper-1".to_string(),
        text_to_speech_model: "tts-1".to_string(),
        voice: "alloy".to_string(),
    }
}

fn harness() -> TestHarness {
    let course = Course::from_json(example()).expect("example course");
    let mut h = TestHarness::new(course);
    h.editor.set_ai_config(Some(ready_config()));
    h
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

fn start_generation(harness: &mut TestHarness) -> Vec<EditorRequest> {
    harness.editor.open_document_dialog();
    harness
        .editor
        .set_document_text_for_test("A document about safety.");
    harness.idle_frame();
    let generate = harness
        .find_text("Generate outline")
        .expect("Generate outline button");
    click_for_requests(harness, generate)
}

#[test]
fn insert_menu_opens_course_from_document() {
    let mut h = harness();
    h.idle_frame();
    assert!(!h.editor.document_dialog_open_for_test());

    let insert = h.find_text("Insert").expect("Insert menu");
    h.click_at(insert);
    h.idle_frame();
    let item = h
        .find_text_last("Course from Document…")
        .expect("Course from Document menu item");
    h.click_at(item);
    h.idle_frame();

    assert!(h.editor.document_dialog_open_for_test());
}

#[test]
fn not_ready_shows_the_settings_button_and_sends_nothing() {
    let course = Course::from_json(example()).expect("example course");
    let mut h = TestHarness::new(course);
    h.editor.set_ai_config(None);
    h.editor.open_document_dialog();
    h.idle_frame();

    let button = h
        .find_text("Open AI Settings")
        .expect("Open AI Settings button");
    assert!(h.find_text("Generate outline").is_none());
    let requests = click_for_requests(&mut h, button);
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, EditorRequest::AiChat { .. }))
    );
    assert!(h.editor.ui.ai.open);
    assert!(!h.editor.document_dialog_open_for_test());
}

#[test]
fn the_consent_line_names_the_host_and_says_when_it_is_local() {
    let mut h = harness();
    h.editor.open_document_dialog();
    h.idle_frame();
    assert!(
        h.find_text("This sends the text above to localhost, on this computer.")
            .is_some()
    );

    let mut remote = ready_config();
    remote.base_url = "https://api.example.com/v1".to_string();
    h.editor.set_ai_config(Some(remote));
    h.idle_frame();
    assert!(
        h.find_text("This sends the text above to api.example.com.")
            .is_some()
    );
}

#[test]
fn choose_file_emits_the_document_picker_request() {
    let mut h = harness();
    h.editor.open_document_dialog();
    h.idle_frame();
    let choose = h.find_text("Choose file…").expect("Choose file button");
    let requests = click_for_requests(&mut h, choose);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::PickDocument))
    );
}

#[test]
fn generate_emits_an_outline_request_and_shows_progress() {
    let mut h = harness();
    let requests = start_generation(&mut h);

    let request = requests
        .iter()
        .find_map(|request| match request {
            EditorRequest::AiChat { purpose, request } => Some((purpose.clone(), request)),
            _ => None,
        })
        .expect("AI chat request");
    assert_eq!(request.0, AiPurpose::Outline);
    assert_eq!(request.1.messages.len(), 2);
    assert!(request.1.messages[0].role == Role::System);
    assert!(request.1.messages[1].role == Role::User);
    assert!(matches!(
        &request.1.messages[1].content,
        MessageContent::Text(text) if text.contains("A document about safety.")
    ));
    assert!(matches!(
        h.editor.ui.document.state,
        DocumentRequestState::Running
    ));
    assert!(h.find_text("Generating…").is_some());
}

#[test]
fn a_good_answer_opens_the_review_window_and_does_not_change_the_course() {
    let mut h = harness();
    let before_course = h.editor.session.course.clone();
    let before_undo = h.editor.undo_len_for_test();
    let _ = start_generation(&mut h);

    h.editor.set_ai_result(
        AiPurpose::Outline,
        Ok(ChatReply {
            text: OUTLINE.to_string(),
            truncated: false,
        }),
    );
    h.idle_frame();

    assert!(!h.editor.document_dialog_open_for_test());
    assert!(h.editor.outline_dialog_open_for_test());
    assert_eq!(h.editor.ui.outline_text, OUTLINE);
    assert!(h.editor.outline_dialog_can_add());
    assert!(
        h.find_text_containing("1 scene, 2 slides, 1 question")
            .is_some()
    );
    assert_eq!(h.editor.session.course, before_course);
    assert_eq!(h.editor.undo_len_for_test(), before_undo);

    let add = h.find_text("Add slides").expect("Add slides button");
    h.click_at(add);
    h.idle_frame();
    assert_eq!(h.editor.undo_len_for_test(), before_undo + 1);
}

#[test]
fn a_fenced_answer_is_unfenced_before_review() {
    let mut h = harness();
    let _ = start_generation(&mut h);
    h.editor.set_ai_result(
        AiPurpose::Outline,
        Ok(ChatReply {
            text: format!("```markdown\n{OUTLINE}\n```"),
            truncated: false,
        }),
    );
    h.idle_frame();

    assert!(h.editor.outline_dialog_open_for_test());
    assert_eq!(h.editor.ui.outline_text, OUTLINE);
    assert!(h.editor.outline_dialog_can_add());
}

#[test]
fn a_garbage_answer_shows_problems_and_changes_nothing() {
    let mut h = harness();
    let before_course = h.editor.session.course.clone();
    let before_undo = h.editor.undo_len_for_test();
    let _ = start_generation(&mut h);
    h.editor.set_ai_result(
        AiPurpose::Outline,
        Ok(ChatReply {
            text: "This is not an outline.".to_string(),
            truncated: false,
        }),
    );
    h.idle_frame();

    assert!(h.editor.outline_dialog_open_for_test());
    assert!(!h.editor.outline_dialog_can_add());
    assert!(
        h.find_text_containing("needs a slide above it").is_some(),
        "the problem is shown in the review window"
    );
    assert_eq!(h.editor.session.course, before_course);
    assert_eq!(h.editor.undo_len_for_test(), before_undo);
}

#[test]
fn an_error_keeps_the_text_and_the_window_open() {
    let mut h = harness();
    let _ = start_generation(&mut h);
    h.editor.set_ai_result(
        AiPurpose::Outline,
        Err("The endpoint rejected the key.".to_string()),
    );
    h.idle_frame();

    assert!(h.editor.document_dialog_open_for_test());
    assert_eq!(
        h.editor.ui.document.state,
        DocumentRequestState::Failed("The endpoint rejected the key.".to_string())
    );
    assert_eq!(
        h.editor.ui.document.text, "A document about safety.",
        "the text is kept for a retry"
    );
    assert!(
        h.find_text("The endpoint rejected the key.").is_some(),
        "the error is shown"
    );
    assert!(
        h.find_text("Generate outline").is_some(),
        "retry is offered"
    );
}

#[test]
fn cancel_drops_a_late_answer() {
    let mut h = harness();
    let requests = start_generation(&mut h);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, EditorRequest::AiChat { .. }))
    );

    let cancel = h.find_text("Cancel").expect("Cancel button");
    let requests = click_for_requests(&mut h, cancel);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, EditorRequest::CancelAi(AiPurpose::Outline)))
    );
    assert_eq!(h.editor.ui.document.state, DocumentRequestState::Idle);

    h.editor.set_ai_result(
        AiPurpose::Outline,
        Ok(ChatReply {
            text: OUTLINE.to_string(),
            truncated: false,
        }),
    );
    h.idle_frame();
    assert!(!h.editor.outline_dialog_open_for_test());
}

#[test]
fn an_answer_for_another_purpose_is_ignored() {
    let mut h = harness();
    let _ = start_generation(&mut h);

    h.editor.set_ai_result(
        AiPurpose::TestConnection,
        Ok(ChatReply {
            text: "OK".to_string(),
            truncated: false,
        }),
    );
    h.idle_frame();

    assert!(h.editor.document_dialog_open_for_test());
    assert!(matches!(
        h.editor.ui.document.state,
        DocumentRequestState::Running
    ));
    assert!(!h.editor.outline_dialog_open_for_test());
}

#[test]
fn a_truncated_answer_warns_in_the_review_window() {
    let mut h = harness();
    let _ = start_generation(&mut h);
    h.editor.set_ai_result(
        AiPurpose::Outline,
        Ok(ChatReply {
            text: OUTLINE.to_string(),
            truncated: true,
        }),
    );
    h.idle_frame();

    assert!(h.editor.outline_dialog_open_for_test());
    assert!(h.find_text_containing("The answer was cut off").is_some());
}

#[test]
fn a_long_document_shows_the_character_warning() {
    let mut h = harness();
    h.editor.open_document_dialog();
    h.editor.set_document_text_for_test(&"a".repeat(60_001));
    h.idle_frame();

    assert!(
        h.find_text("Only the first 60,000 characters will be sent.")
            .is_some()
    );
    assert!(h.find_text_containing("60001 characters").is_some());
}

#[test]
fn slide_options_are_clamped_to_the_window_range() {
    assert_eq!(crate::document::requested_slides(""), None);
    assert_eq!(crate::document::requested_slides(" 12 "), Some(12));
    assert_eq!(crate::document::requested_slides("0"), Some(1));
    assert_eq!(crate::document::requested_slides("1000"), Some(100));
    assert_eq!(crate::document::requested_slides("many"), None);
}

#[test]
fn the_window_takes_the_keyboard_from_the_canvas() {
    let mut h = harness();
    h.editor.open_document_dialog();
    h.idle_frame();
    h.type_text("a");
    h.press(egui::Key::ArrowRight, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.ui.document.text, "a");
}
