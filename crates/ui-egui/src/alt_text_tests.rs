//! Headless tests for Suggest Alt Text (§ M9 step 4).

use std::sync::Arc;

use coursecraft_ai::{AiConfig, AiError, ChatReply, MessageContent};
use coursecraft_course::Course;
use egui::{Event, Modifiers, PointerButton};
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

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = vec![200_u8; (width as usize) * (height as usize) * 4];
    coursecraft_media::encode_png_rgba(width, height, &pixels).expect("png")
}

fn media(pairs: &[(&str, Vec<u8>)]) -> Vec<(String, Arc<[u8]>)> {
    pairs
        .iter()
        .map(|(name, bytes)| ((*name).to_string(), Arc::from(bytes.as_slice())))
        .collect()
}

fn image_object(id: &str, name: &str, src: &str, alt: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "kind": { "type": "image", "src": src, "alt": alt },
        "frame": { "x": 40, "y": 40, "w": 200, "h": 150 },
    })
}

fn test_course() -> Course {
    let value = json!({
        "schema_version": 1,
        "id": "alt-test",
        "title": "Alt Test",
        "scenes": [{
            "id": "sc-one",
            "title": "Scene",
            "slides": [{
                "id": "s-one",
                "title": "Banner Slide",
                "objects": [
                    image_object("img-banner", "Banner Graphic", "media/a.png", ""),
                    image_object("img-decorative", "Decorative", "media/a.png", ""),
                    image_object("img-filled", "Filled", "media/a.png", "Already described"),
                    image_object("img-missing", "Missing", "media/missing.png", ""),
                    {
                        "id": "body-text",
                        "kind": { "type": "text", "text": "Nearby words on the slide." },
                        "frame": { "x": 40, "y": 240, "w": 600, "h": 100 },
                    },
                ],
                "layers": [{
                    "id": "layer-one",
                    "name": "Overlay",
                    "objects": [
                        image_object("img-layer", "Layer Image", "media/a.png", ""),
                    ],
                }],
            }],
        }],
        "masters": [{
            "id": "master-one",
            "title": "Brand Master",
            "objects": [
                image_object("img-master", "Master Image", "media/b.png", ""),
            ],
        }],
        "templates": [{
            "id": "template-one",
            "title": "A Template",
            "objects": [
                image_object("img-template", "Template Image", "media/c.png", ""),
            ],
        }],
    });
    // Mark one image decorative after building it, so the shared helper stays
    // simple.
    let mut value = value;
    if let Some(objects) = value
        .get_mut("scenes")
        .and_then(Value::as_array_mut)
        .and_then(|scenes| scenes.first_mut())
        .and_then(|scene| scene.get_mut("slides"))
        .and_then(Value::as_array_mut)
        .and_then(|slides| slides.first_mut())
        .and_then(|slide| slide.get_mut("objects"))
        .and_then(Value::as_array_mut)
        && let Some(object) = objects.get_mut(1)
    {
        object["decorative"] = json!(true);
    }
    Course::from_json(&value.to_string()).expect("test course")
}

fn harness() -> TestHarness {
    let mut h = TestHarness::new(test_course());
    h.editor.set_ai_config(Some(ready_config()));
    h.editor.set_media(media(&[
        ("media/a.png", png(4, 4)),
        ("media/b.png", png(4, 4)),
    ]));
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

fn open(harness: &mut TestHarness) {
    harness.editor.open_alt_text_dialog();
    harness.idle_frame();
}

fn click_suggest(harness: &mut TestHarness) -> Vec<EditorRequest> {
    let suggest = harness.find_text("Suggest").expect("Suggest button");
    click_for_requests(harness, suggest)
}

fn first_chat(requests: &[EditorRequest]) -> Option<(AiPurpose, &coursecraft_ai::ChatRequest)> {
    requests.iter().find_map(|request| match request {
        EditorRequest::AiChat { purpose, request } => Some((purpose.clone(), request)),
        _ => None,
    })
}

fn answer(harness: &mut TestHarness, src: &str, text: &str) {
    harness.editor.set_ai_result(
        AiPurpose::AltText {
            src: src.to_string(),
        },
        Ok(ChatReply {
            text: text.to_string(),
            truncated: false,
        }),
    );
}

#[test]
fn project_menu_opens_the_window() {
    let mut h = harness();
    h.idle_frame();
    let project = h.find_text("Project").expect("Project menu");
    h.click_at(project);
    h.idle_frame();
    let item = h
        .find_text_last("Suggest Alt Text…")
        .expect("Suggest Alt Text menu item");
    h.click_at(item);
    h.idle_frame();
    assert!(h.editor.ui.alt.open);
}

#[test]
fn the_validation_window_offers_the_button_when_alt_is_missing() {
    let mut h = harness();
    h.editor.refresh_validation();
    assert!(
        h.editor
            .ui
            .audit_findings
            .iter()
            .any(|finding| finding.check == "alt-text"),
        "the audit has an alt-text finding"
    );
    h.editor.ui.show_validation_window = true;
    for _ in 0..5 {
        h.idle_frame();
    }
    let positions = h.find_texts("Suggest alt text…");
    assert!(
        !positions.is_empty(),
        "the Suggest alt text button is painted"
    );
    for position in positions {
        h.click_at(position);
        h.idle_frame();
        if h.editor.ui.alt.open {
            break;
        }
    }
    assert!(h.editor.ui.alt.open);
}

#[test]
fn the_list_skips_decorative_filled_template_and_missing_images() {
    let mut h = harness();
    open(&mut h);
    let state = &h.editor.ui.alt;
    assert_eq!(state.rows.len(), 3, "banner, layer and master images");
    assert_eq!(state.queue.len(), 2, "the banner and layer share one src");
    assert_eq!(state.missing_files, 1);

    let locations: Vec<&str> = state.rows.iter().map(|row| row.location.as_str()).collect();
    assert!(
        locations.contains(&"Slide 1 ▸ Banner Graphic"),
        "{locations:?}"
    );
    assert!(
        locations.contains(&"Slide 1 ▸ Overlay ▸ Layer Image"),
        "{locations:?}"
    );
    assert!(
        locations.contains(&"Master ▸ Brand Master ▸ Master Image"),
        "{locations:?}"
    );
    assert!(
        !locations
            .iter()
            .any(|location| location.contains("Decorative"))
    );
    assert!(!locations.iter().any(|location| location.contains("Filled")));
    assert!(
        !locations
            .iter()
            .any(|location| location.contains("Template"))
    );

    assert!(
        h.find_text("1 image has no file yet.").is_some(),
        "the missing file is counted in one line"
    );
}

#[test]
fn the_consent_line_names_the_count_and_the_host() {
    let mut h = harness();
    open(&mut h);
    assert!(
        h.find_text("This sends 2 images and nearby slide text to localhost, on this computer.")
            .is_some()
    );

    let mut remote = ready_config();
    remote.base_url = "https://api.example.com/v1".to_string();
    h.editor.set_ai_config(Some(remote));
    h.idle_frame();
    assert!(
        h.find_text("This sends 2 images and nearby slide text to api.example.com.")
            .is_some()
    );
}

#[test]
fn not_ready_shows_the_settings_button_and_sends_nothing() {
    let mut h = harness();
    h.editor.set_ai_config(None);
    open(&mut h);

    assert!(h.find_text("Suggest").is_none());
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
    assert!(!h.editor.ui.alt.open);
}

#[test]
fn no_images_to_do_says_so_and_offers_no_suggest() {
    let mut h = TestHarness::new(
        Course::from_json(include_str!("../../../examples/hello-course/course.json"))
            .expect("example"),
    );
    h.editor.set_ai_config(Some(ready_config()));
    open(&mut h);

    assert!(h.find_text("No images need alt text.").is_some());
    assert!(h.find_text("Suggest").is_none());
    assert!(h.find_text("Apply").is_none());
}

#[test]
fn the_second_request_starts_only_when_the_first_returns() {
    let mut h = harness();
    open(&mut h);
    let requests = click_suggest(&mut h);
    let (purpose, _) = first_chat(&requests).expect("first request");
    assert_eq!(
        purpose,
        AiPurpose::AltText {
            src: "media/a.png".to_string()
        }
    );
    assert!(h.find_text("Image 1 of 2").is_some());

    assert!(
        first_chat(&h.idle_frame()).is_none(),
        "nothing else is sent while the first is running"
    );

    answer(&mut h, "media/a.png", "A wide banner with the word Safety.");
    let requests = h.idle_frame();
    let (purpose, _) = first_chat(&requests).expect("second request");
    assert_eq!(
        purpose,
        AiPurpose::AltText {
            src: "media/b.png".to_string()
        }
    );
}

#[test]
fn one_request_fills_every_row_with_the_same_src() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/a.png", "\"A banner with the word Safety.\"");
    h.idle_frame();

    let banner = h
        .editor
        .ui
        .alt
        .rows
        .iter()
        .find(|row| row.object == "img-banner")
        .expect("banner row");
    let layer = h
        .editor
        .ui
        .alt
        .rows
        .iter()
        .find(|row| row.object == "img-layer")
        .expect("layer row");
    assert_eq!(banner.suggestion, "A banner with the word Safety.");
    assert_eq!(layer.suggestion, banner.suggestion);
    assert!(banner.ticked && layer.ticked);
}

#[test]
fn a_failure_shows_on_its_row_and_the_run_continues() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    h.editor.set_ai_result(
        AiPurpose::AltText {
            src: "media/a.png".to_string(),
        },
        Err("The endpoint is busy.".to_string()),
    );
    let requests = h.idle_frame();

    let banner = h
        .editor
        .ui
        .alt
        .rows
        .iter()
        .find(|row| row.object == "img-banner")
        .expect("banner row");
    assert_eq!(banner.error.as_deref(), Some("The endpoint is busy."));
    assert!(first_chat(&requests).is_some(), "the next image still runs");
}

#[test]
fn a_rejected_key_stops_the_run() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    h.editor.set_ai_result(
        AiPurpose::AltText {
            src: "media/a.png".to_string(),
        },
        Err(AiError::RejectedKey.to_string()),
    );
    let requests = h.idle_frame();

    assert!(first_chat(&requests).is_none(), "nothing else is sent");
    assert!(!h.editor.ui.alt.is_running());
    assert!(h.editor.ui.alt.queue.is_empty());
}

#[test]
fn cancel_drops_the_late_answer() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    let cancel = h.find_text("Cancel").expect("Cancel button");
    let requests = click_for_requests(&mut h, cancel);
    assert!(requests.iter().any(|request| matches!(
        request,
        EditorRequest::CancelAi(AiPurpose::AltText { src }) if src == "media/a.png"
    )));

    answer(&mut h, "media/a.png", "This answer is too late.");
    h.idle_frame();
    let row = h
        .editor
        .ui
        .alt
        .rows
        .iter()
        .find(|row| row.object == "img-banner")
        .expect("banner row");
    assert!(row.suggestion.is_empty());
}

#[test]
fn a_result_for_another_src_is_ignored() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/other.png", "Wrong picture.");
    h.idle_frame();

    assert!(h.editor.ui.alt.is_running());
    assert!(
        h.editor
            .ui
            .alt
            .rows
            .iter()
            .all(|row| row.suggestion.is_empty())
    );
}

#[test]
fn apply_is_one_undo_step_and_writes_only_ticked_rows_with_the_edited_value() {
    let mut h = harness();
    let before_undo = h.editor.undo_len_for_test();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/a.png", "A banner with the word Safety.");
    h.idle_frame();
    answer(&mut h, "media/b.png", "A logo on a blue field.");
    h.idle_frame();

    if let Some(row) = h.editor.ui.alt.rows.get_mut(0) {
        row.suggestion = "Edited banner description.".to_string();
    }
    if let Some(row) = h.editor.ui.alt.rows.get_mut(1) {
        row.ticked = false;
    }

    let apply = h.find_text("Apply").expect("Apply button");
    h.click_at(apply);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before_undo + 1);
    let course = &h.editor.session.course;
    let slide = course.slide("s-one").expect("slide");
    let kind = |id: &str| {
        slide
            .object(id)
            .map(|(object, _)| object.kind.clone())
            .expect("object")
    };
    match kind("img-banner") {
        coursecraft_course::ObjectKind::Image { alt, .. } => {
            assert_eq!(alt, "Edited banner description.");
        }
        other => panic!("expected an image, got {other:?}"),
    }
    match kind("img-layer") {
        coursecraft_course::ObjectKind::Image { alt, .. } => assert!(alt.is_empty()),
        other => panic!("expected an image, got {other:?}"),
    }
    let master = course.master("master-one").expect("master");
    match &master
        .object("img-master")
        .map(|(object, _)| object.kind.clone())
        .expect("master image")
    {
        coursecraft_course::ObjectKind::Image { alt, .. } => {
            assert_eq!(alt, "A logo on a blue field.");
        }
        other => panic!("expected an image, got {other:?}"),
    }
}

#[test]
fn an_object_deleted_while_waiting_is_skipped() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/a.png", "A banner.");
    h.idle_frame();
    answer(&mut h, "media/b.png", "A logo.");
    h.idle_frame();

    if let Some(objects) = h
        .editor
        .session
        .course
        .scenes
        .first_mut()
        .and_then(|scene| scene.slides.first_mut())
        .map(|slide| &mut slide.objects)
    {
        objects.retain(|object| object.id != "img-banner");
    }
    for row in &mut h.editor.ui.alt.rows {
        row.ticked = row.object == "img-banner";
    }
    let before_undo = h.editor.undo_len_for_test();
    let apply = h.find_text("Apply").expect("Apply button");
    h.click_at(apply);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before_undo);
    assert!(
        h.find_text_containing("Skipped 1 image").is_some(),
        "the window says how many were skipped"
    );
}

#[test]
fn an_alt_filled_meanwhile_is_not_overwritten() {
    let mut h = harness();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/a.png", "A banner.");
    h.idle_frame();
    answer(&mut h, "media/b.png", "A logo.");
    h.idle_frame();

    h.editor.run(
        "object.update",
        json!({
            "slide": "s-one",
            "object": "img-banner",
            "kind": { "type": "image", "src": "media/a.png", "alt": "Filled meanwhile" },
        }),
    );
    for row in &mut h.editor.ui.alt.rows {
        row.ticked = row.object == "img-banner";
    }
    let before_undo = h.editor.undo_len_for_test();
    let apply = h.find_text("Apply").expect("Apply button");
    h.click_at(apply);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before_undo);
    let alt = h
        .editor
        .session
        .course
        .slide("s-one")
        .and_then(|slide| slide.object("img-banner"))
        .map(|(object, _)| match &object.kind {
            coursecraft_course::ObjectKind::Image { alt, .. } => alt.clone(),
            _ => String::new(),
        })
        .expect("object");
    assert_eq!(alt, "Filled meanwhile");
    assert!(h.find_text_containing("Skipped 1 image").is_some());
}

#[test]
fn applying_reduces_the_audit_findings() {
    let mut h = harness();
    let before = h
        .editor
        .session
        .course
        .audit()
        .findings
        .iter()
        .filter(|finding| finding.check == "alt-text")
        .count();
    open(&mut h);
    let _ = click_suggest(&mut h);
    answer(&mut h, "media/a.png", "A banner with the word Safety.");
    h.idle_frame();
    answer(&mut h, "media/b.png", "A logo on a blue field.");
    h.idle_frame();
    let apply = h.find_text("Apply").expect("Apply button");
    h.click_at(apply);
    h.idle_frame();

    let after = h
        .editor
        .session
        .course
        .audit()
        .findings
        .iter()
        .filter(|finding| finding.check == "alt-text")
        .count();
    assert!(after < before, "before {before}, after {after}");
}

#[test]
fn unsupported_and_oversized_files_get_row_errors() {
    let value = json!({
        "schema_version": 1,
        "id": "alt-errors",
        "title": "Alt Errors",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s",
                "title": "Slide",
                "objects": [
                    image_object("svg", "Vector", "media/vector.svg", ""),
                    image_object("big", "Huge", "media/huge.png", ""),
                ],
            }],
        }],
    });
    let course = Course::from_json(&value.to_string()).expect("course");
    let mut h = TestHarness::new(course);
    h.editor.set_ai_config(Some(ready_config()));
    h.editor.set_media(media(&[
        ("media/vector.svg", b"<svg></svg>".to_vec()),
        (
            "media/huge.png",
            vec![0_u8; crate::alt_text::MAX_SEND_BYTES + 1],
        ),
    ]));
    open(&mut h);
    let requests = click_suggest(&mut h);
    h.idle_frame();

    assert!(first_chat(&requests).is_none(), "nothing was sent");
    let errors: Vec<&str> = h
        .editor
        .ui
        .alt
        .rows
        .iter()
        .filter_map(|row| row.error.as_deref())
        .collect();
    assert!(errors.contains(&"This format can't be sent."), "{errors:?}");
    assert!(errors.contains(&"Too large to send."), "{errors:?}");
}

#[test]
fn larger_images_are_downscaled_to_1024_on_the_longest_side() {
    let original = png(2000, 1000);
    let (media_type, prepared) = crate::alt_text::prepare_image_bytes(&original).expect("prepared");
    assert_eq!(media_type, "image/png");
    let decoded = coursecraft_media::image::load_from_memory(&prepared).expect("resized png");
    assert_eq!(decoded.width(), 1024);
    assert_eq!(decoded.height(), 512);

    let small = png(40, 30);
    let (media_type, prepared) = crate::alt_text::prepare_image_bytes(&small).expect("prepared");
    assert_eq!(media_type, "image/png");
    assert_eq!(prepared, small, "a small image keeps its original bytes");
}

#[test]
fn prompt_carries_the_slide_context_and_the_image() {
    let mut h = harness();
    open(&mut h);
    let requests = click_suggest(&mut h);
    let (_, request) = first_chat(&requests).expect("request");
    assert_eq!(request.messages.len(), 2);
    match &request.messages[1].content {
        MessageContent::TextAndImages { text, images } => {
            assert!(text.contains("Banner Slide"), "{text}");
            assert!(text.contains("Nearby words on the slide."), "{text}");
            assert_eq!(images.len(), 1);
            assert!(images[0].starts_with("data:image/png;base64,"));
        }
        other => panic!("expected text and images, got {other:?}"),
    }
}

#[test]
fn the_inspector_suggest_button_does_not_write_until_committed() {
    let mut h = harness();
    h.editor.selection.slide = "s-one".to_string();
    h.editor.selection.object = Some("img-banner".to_string());
    h.idle_frame();
    let before_undo = h.editor.undo_len_for_test();
    let suggest = h.find_text("Suggest").expect("inspector Suggest button");
    let requests = click_for_requests(&mut h, suggest);
    let (purpose, _) = first_chat(&requests).expect("inline request");
    assert_eq!(
        purpose,
        AiPurpose::AltText {
            src: "media/a.png".to_string()
        }
    );
    h.editor.set_ai_result(
        purpose,
        Ok(ChatReply {
            text: "A banner with the word Safety.".to_string(),
            truncated: false,
        }),
    );
    h.idle_frame();

    assert_eq!(
        h.editor.ui.properties.image_alt,
        "A banner with the word Safety."
    );
    assert_eq!(h.editor.undo_len_for_test(), before_undo);
}
