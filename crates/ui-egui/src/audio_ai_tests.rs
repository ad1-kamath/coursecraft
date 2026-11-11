//! Headless tests for captions from audio and narration from text (§ M9 step 6).

use std::sync::Arc;

use coursecraft_ai::{
    AiConfig, AiError, AiReply, ElevenLabsConfig, ElevenLabsModel, parse_transcription,
};
use coursecraft_course::{Course, ObjectKind};
use egui::{Event, Key, Modifiers, PointerButton};
use serde_json::{Value, json};

use crate::test_harness::TestHarness;
use crate::{AiPurpose, EditorRequest, SpeechSource};

const VTT: &str = "WEBVTT\n\n\
00:00:00.000 --> 00:00:02.000\nHello there.\n\n\
00:00:02.000 --> 00:00:04.000\nThis is a test.\n";

const MP3: &[u8] = &[0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00, 0xFF, 0x80];

fn config() -> AiConfig {
    AiConfig {
        base_url: "http://localhost:1234/v1".to_string(),
        model: "test-model".to_string(),
        api_key: "sk-secret-value".to_string(),
        speech_to_text_model: "whisper-1".to_string(),
        text_to_speech_model: "tts-1".to_string(),
        voice: "alloy".to_string(),
    }
}

fn audio_object(id: &str, src: &str) -> Value {
    json!({
        "id": id,
        "name": "Narration",
        "kind": { "type": "audio", "src": src, "autoplay": false },
        "frame": { "x": 40, "y": 40, "w": 48, "h": 48 },
    })
}

fn test_course() -> Course {
    let value = json!({
        "schema_version": 1,
        "id": "audio-test",
        "title": "Audio Test",
        "scenes": [{
            "id": "sc-one",
            "title": "Scene",
            "slides": [{
                "id": "s-one",
                "title": "Sound Slide",
                "notes": "These are the speaker notes.",
                "objects": [
                    audio_object("audio1", "sound.mp3"),
                    {
                        "id": "clip1",
                        "name": "Clip",
                        "kind": { "type": "video", "src": "clip.mp4", "autoplay": false },
                        "frame": { "x": 40, "y": 120, "w": 320, "h": 180 },
                    },
                ],
            }],
        }],
    });
    Course::from_json(&value.to_string()).expect("test course")
}

fn media(pairs: &[(&str, Vec<u8>)]) -> Vec<(String, Arc<[u8]>)> {
    pairs
        .iter()
        .map(|(name, bytes)| ((*name).to_string(), Arc::from(bytes.as_slice())))
        .collect()
}

fn harness() -> TestHarness {
    let mut h = TestHarness::new(test_course());
    h.editor.set_ai_config(Some(config()));
    h.editor.set_media(media(&[
        ("sound.mp3", b"ID3 fake audio".to_vec()),
        ("clip.mp4", b"fake video".to_vec()),
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

fn open_captions(harness: &mut TestHarness) {
    assert!(harness.editor.open_captions_dialog("s-one", "audio1"));
    harness.idle_frame();
}

fn open_speech(harness: &mut TestHarness) {
    harness.editor.open_speech_dialog();
    harness.idle_frame();
}

fn generate_captions(harness: &mut TestHarness) -> (AiPurpose, String, String, Vec<u8>) {
    let generate = harness.find_text("Generate").expect("Generate button");
    let requests = click_for_requests(harness, generate);
    requests
        .into_iter()
        .find_map(|request| match request {
            EditorRequest::AiTranscribe {
                purpose,
                model,
                file_name,
                bytes,
            } => Some((purpose, model, file_name, bytes)),
            _ => None,
        })
        .expect("transcription request")
}

fn answer_captions(harness: &mut TestHarness, purpose: AiPurpose) {
    harness
        .editor
        .set_audio_result(purpose, Ok(AiReply::Captions(VTT.to_string())));
    harness.idle_frame();
}

fn captions_of(harness: &TestHarness, object: &str) -> Option<String> {
    harness
        .editor
        .session
        .course
        .slide("s-one")
        .and_then(|slide| slide.object(object))
        .and_then(|(object, _)| match &object.kind {
            ObjectKind::Audio { captions, .. } | ObjectKind::Video { captions, .. } => {
                captions.clone()
            }
            _ => None,
        })
}

#[test]
fn a_missing_model_says_so_and_offers_open_ai_settings() {
    let mut h = harness();
    let mut config = config();
    config.speech_to_text_model.clear();
    h.editor.set_ai_config(Some(config));
    open_captions(&mut h);

    assert!(
        h.find_text("Speech-to-text model isn't set.").is_some(),
        "the window names the missing model"
    );
    assert!(h.find_text("Generate").is_none(), "nothing can be sent");
    let button = h
        .find_text("Open AI Settings")
        .expect("Open AI Settings button");
    let requests = click_for_requests(&mut h, button);
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request, EditorRequest::AiTranscribe { .. }))
    );
    assert!(h.editor.ui.ai.open);
    assert!(!h.editor.ui.captions.open);
}

#[test]
fn the_consent_line_names_the_file_size_and_host() {
    let mut h = harness();
    let size = (3.2_f64 * 1024.0 * 1024.0) as usize;
    h.editor
        .set_media(media(&[("sound.mp3", vec![b'x'; size])]));
    let mut remote = config();
    remote.base_url = "https://api.example.com/v1".to_string();
    h.editor.set_ai_config(Some(remote));
    open_captions(&mut h);
    assert!(
        h.find_text("This sends sound.mp3 (3.2 MB) to api.example.com.")
            .is_some(),
        "the line names the file, its size and the host"
    );

    h.editor.set_ai_config(Some(config()));
    h.idle_frame();
    assert!(
        h.find_text("This sends sound.mp3 (3.2 MB) to localhost, on this computer.")
            .is_some(),
        "a local server adds the on-this-computer form"
    );
}

#[test]
fn use_these_captions_is_one_undo_and_undo_restores() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, model, file_name, bytes) = generate_captions(&mut h);
    assert_eq!(model, "whisper-1");
    assert_eq!(file_name, "sound.mp3");
    assert_eq!(bytes, b"ID3 fake audio");
    assert!(h.find_text("Transcribing…").is_some());

    answer_captions(&mut h, purpose);
    assert!(h.find_text("Found 2 cues.").is_some());
    assert!(h.find_text_containing("Hello there.").is_some());

    let before = h.editor.undo_len_for_test();
    let button = h
        .find_text("Use these captions")
        .expect("Use these captions button");
    h.click_at(button);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before + 1, "one undo step");
    assert_eq!(captions_of(&h, "audio1").as_deref(), Some("sound.vtt"));
    assert!(
        h.editor.media().iter().any(|(name, _)| name == "sound.vtt"),
        "the VTT is in the media store"
    );
    assert!(!h.editor.ui.captions.open, "the window closes after use");

    h.editor.run("edit.undo", json!({}));
    assert_eq!(captions_of(&h, "audio1"), None, "undo restores the object");
}

#[test]
fn a_replaced_caption_uses_the_replace_label_and_does_not_overwrite_a_name() {
    let mut h = harness();
    // A name that is already used elsewhere in the media store.
    h.editor.set_media(media(&[
        ("sound.mp3", b"ID3 fake audio".to_vec()),
        ("clip.mp4", b"fake video".to_vec()),
        ("sound.vtt", VTT.as_bytes().to_vec()),
    ]));
    h.editor.run(
        "object.set_captions",
        json!({ "slide": "s-one", "object": "audio1", "captions": "sound.vtt" }),
    );
    open_captions(&mut h);
    assert!(h.find_text("Replace captions").is_none(), "not ready yet");
    let (purpose, _, _, _) = generate_captions(&mut h);
    answer_captions(&mut h, purpose);
    assert!(
        h.find_text("Replace captions").is_some(),
        "existing captions change the button"
    );

    let button = h
        .find_text("Replace captions")
        .expect("Replace captions button");
    h.click_at(button);
    h.idle_frame();
    assert_eq!(captions_of(&h, "audio1").as_deref(), Some("sound-2.vtt"));
    assert!(
        h.editor
            .media()
            .iter()
            .any(|(name, _)| name == "sound-2.vtt")
    );
}

#[test]
fn a_non_timed_answer_stops_at_an_error_and_changes_nothing() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, _, _, _) = generate_captions(&mut h);
    let before = h.editor.undo_len_for_test();
    let message = parse_transcription(200, br#"{"text":"not captions"}"#)
        .expect_err("must refuse")
        .to_string();
    h.editor.set_audio_result(purpose, Err(message));
    h.idle_frame();

    assert!(
        h.find_text("The server didn't return timed captions.")
            .is_some(),
        "the window shows the plain reason"
    );
    assert_eq!(h.editor.undo_len_for_test(), before);
    assert_eq!(captions_of(&h, "audio1"), None);
    assert!(h.editor.ui.captions.vtt.is_none());
    assert!(h.find_text("Use these captions").is_none());
}

#[test]
fn close_throws_the_answer_away() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, _, _, _) = generate_captions(&mut h);
    answer_captions(&mut h, purpose);
    let before = h.editor.undo_len_for_test();
    let close = h.find_text("Close").expect("Close button");
    h.click_at(close);
    h.idle_frame();

    assert!(!h.editor.ui.captions.open);
    assert_eq!(h.editor.undo_len_for_test(), before);
    assert_eq!(captions_of(&h, "audio1"), None);
}

#[test]
fn a_404_reads_as_the_plain_server_message() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, _, _, _) = generate_captions(&mut h);
    let message = parse_transcription(404, b"").expect_err("404").to_string();
    h.editor.set_audio_result(purpose, Err(message));
    h.idle_frame();
    assert!(
        h.find_text(
            "This server doesn't offer speech-to-text. Local servers often don't: try a hosted one."
        )
        .is_some()
    );
}

#[test]
fn cancel_drops_a_late_answer() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, _, _, _) = generate_captions(&mut h);
    let cancel = h.find_text("Cancel").expect("Cancel button");
    let requests = click_for_requests(&mut h, cancel);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::CancelAi(_)))
    );
    assert!(!h.editor.ui.captions.is_running());

    answer_captions(&mut h, purpose);
    assert!(h.editor.ui.captions.vtt.is_none());
    assert_eq!(captions_of(&h, "audio1"), None);
}

#[test]
fn narrate_fills_the_window_from_the_slide_notes() {
    let mut h = harness();
    h.editor.selection.slide = "s-one".to_string();
    h.editor.selection.object = None;
    h.editor.ui.inspector_tab = crate::InspectorTab::Slide;
    h.idle_frame();
    let narrate = h.find_text("Narrate…").expect("Narrate button");
    h.click_at(narrate);
    h.idle_frame();

    assert!(h.editor.ui.speech.open);
    assert_eq!(h.editor.ui.speech.text, "These are the speaker notes.");
}

#[test]
fn add_to_slide_is_one_undo_and_the_course_still_validates() {
    let mut h = harness();
    open_speech(&mut h);
    let before = h.editor.undo_len_for_test();

    h.editor.ui.speech.text = "Welcome to the course.".to_string();
    h.idle_frame();
    let generate = h.find_text("Generate").expect("Generate button");
    let requests = click_for_requests(&mut h, generate);
    let purpose = requests
        .into_iter()
        .find_map(|request| match request {
            EditorRequest::AiSpeech {
                purpose,
                model,
                voice,
                text,
            } => {
                assert_eq!(model, "tts-1");
                assert_eq!(voice, "alloy");
                assert_eq!(text, "Welcome to the course.");
                Some(purpose)
            }
            _ => None,
        })
        .expect("speech request");
    h.editor
        .set_audio_result(purpose, Ok(AiReply::Speech(MP3.to_vec())));
    h.idle_frame();

    assert!(
        h.find_text("Play").is_some(),
        "Play button is offered for ready audio"
    );

    let add = h.find_text("Add to slide").expect("Add to slide button");
    h.click_at(add);
    h.idle_frame();

    assert_eq!(h.editor.undo_len_for_test(), before + 1, "one undo step");
    let slide = h.editor.session.course.slide("s-one").expect("slide");
    let narration = slide
        .objects
        .iter()
        .find(|object| {
            matches!(
                &object.kind,
                ObjectKind::Audio { src, autoplay: true, .. } if src == "narration.mp3"
            )
        })
        .expect("autoplaying narration audio");
    assert!(narration.frame.x >= 0.0 && narration.frame.y >= 0.0);
    assert!(
        h.editor
            .media()
            .iter()
            .any(|(name, _)| name == "narration.mp3"),
        "the MP3 is in the media store"
    );

    let issues = h.editor.run("course.validate", json!({}));
    let issues = issues.expect("validation runs");
    assert!(
        !issues.as_array().is_some_and(|items| items
            .iter()
            .any(|item| item.get("severity").and_then(Value::as_str) == Some("error"))),
        "the course validates: {issues}"
    );

    let media_before_undo = h.editor.media().len();
    h.editor.run("edit.undo", json!({}));
    let slide = h.editor.session.course.slide("s-one").expect("slide");
    assert!(
        !slide.objects.iter().any(|object| matches!(
            &object.kind,
            ObjectKind::Audio { src, .. } if src == "narration.mp3"
        )),
        "undo removes the narration object"
    );
    assert_eq!(h.editor.media().len(), media_before_undo);
}

#[test]
fn publish_preflight_finds_the_new_narration_media() {
    let mut h = harness();
    open_speech(&mut h);
    h.editor.ui.speech.text = "Welcome.".to_string();
    h.idle_frame();
    let generate = h.find_text("Generate").expect("Generate button");
    let requests = click_for_requests(&mut h, generate);
    let purpose = requests
        .into_iter()
        .find_map(|request| match request {
            EditorRequest::AiSpeech { purpose, .. } => Some(purpose),
            _ => None,
        })
        .expect("speech request");
    h.editor
        .set_audio_result(purpose, Ok(AiReply::Speech(MP3.to_vec())));
    h.idle_frame();
    let add = h.find_text("Add to slide").expect("Add to slide button");
    h.click_at(add);
    h.idle_frame();

    assert!(
        h.editor.missing_media().is_empty(),
        "publish would not block on the new MP3: {:?}",
        h.editor.missing_media()
    );
    assert!(
        h.editor
            .used_media()
            .iter()
            .any(|(name, _)| name == "narration.mp3")
    );
}

#[test]
fn speech_consent_and_disclosure_name_the_host_and_ai_voice() {
    let mut h = harness();
    let mut remote = config();
    remote.base_url = "https://api.example.com/v1".to_string();
    h.editor.set_ai_config(Some(remote));
    open_speech(&mut h);
    h.editor.ui.speech.text = "Hello.".to_string();
    h.idle_frame();
    assert!(
        h.find_text("This sends 6 characters of text to api.example.com.")
            .is_some()
    );
    assert!(
        h.find_text(
            "Tell your learners this voice is AI-generated, if your provider's terms require it."
        )
        .is_some()
    );
    assert!(
        h.find_text_containing("%name% references are read aloud literally")
            .is_none(),
        "no reference in plain text"
    );

    h.editor.ui.speech.text = "Hello %learner%.".to_string();
    h.idle_frame();
    assert!(
        h.find_text_containing("%name% references are read aloud literally")
            .is_some(),
        "a reference is called out"
    );
}

#[test]
fn a_missing_speech_model_says_so_and_sends_nothing() {
    let mut h = harness();
    let mut config = config();
    config.text_to_speech_model.clear();
    h.editor.set_ai_config(Some(config));
    open_speech(&mut h);
    assert!(h.find_text("Text-to-speech model isn't set.").is_some());
    assert!(h.find_text("Generate").is_none());
}

fn elevenlabs_config() -> ElevenLabsConfig {
    ElevenLabsConfig {
        api_key: "el-secret-value".to_string(),
        voice_id: "voice123".to_string(),
        voice_name: "Rachel".to_string(),
        model_id: "eleven_multilingual_v2".to_string(),
    }
}

fn open_speech_elevenlabs(harness: &mut TestHarness) {
    harness.editor.set_speech_source(SpeechSource::ElevenLabs);
    harness
        .editor
        .set_elevenlabs_config(Some(elevenlabs_config()));
    harness.editor.open_speech_dialog();
    harness.idle_frame();
}

#[test]
fn switching_to_elevenlabs_removes_the_ai_gate_and_is_remembered() {
    let mut h = harness();
    h.editor.set_ai_config(None);
    open_speech(&mut h);
    assert!(
        h.find_text("Text-to-speech model isn't set.").is_some(),
        "the AI provider source gates on its settings"
    );

    let elevenlabs = h.find_text("ElevenLabs").expect("ElevenLabs source");
    let requests = click_for_requests(&mut h, elevenlabs);
    assert_eq!(h.editor.ui.speech_source, SpeechSource::ElevenLabs);
    assert!(
        requests
            .iter()
            .any(|request| matches!(request, EditorRequest::SaveElevenLabs(_))),
        "the choice is saved: {requests:?}"
    );
    assert!(
        h.find_text("Text-to-speech model isn't set.").is_none(),
        "ElevenLabs does not inherit the AI provider gate"
    );
    assert!(h.find_text("API key").is_some());
    assert!(h.find_text("Voice source").is_some());
}

#[test]
fn elevenlabs_generate_disabled_reasons_are_shown() {
    let mut h = harness();
    open_speech_elevenlabs(&mut h);
    h.scroll_over(h.screen_rect.center(), -1200.0);
    h.editor.ui.speech.text.clear();
    h.idle_frame();
    assert!(h.find_text("Type the words to speak.").is_some());

    h.editor.ui.elevenlabs.config.api_key.clear();
    h.idle_frame();
    assert!(
        h.find_text("Enter your ElevenLabs API key.").is_some(),
        "a missing key is named"
    );

    h.editor.ui.elevenlabs.config.api_key = "el-secret-value".to_string();
    h.editor.ui.elevenlabs.config.voice_id.clear();
    h.idle_frame();
    assert!(
        h.find_text("Choose a voice or paste a voice ID.").is_some(),
        "a missing voice is named"
    );

    h.editor.ui.elevenlabs.config.voice_id = "voice123".to_string();
    // A small model limit keeps the reason modest and proves the model wins over the default.
    h.editor.ui.elevenlabs.models = vec![ElevenLabsModel {
        id: "eleven_multilingual_v2".to_string(),
        name: "Multilingual v2".to_string(),
        max_chars: Some(4),
    }];
    h.editor.ui.speech.text = "hello".to_string();
    h.idle_frame();
    assert!(
        h.find_text_containing("That's too much text for this model")
            .is_some(),
        "the model limit is named"
    );

    let generate = h.find_text("Generate").expect("Generate button");
    let requests = click_for_requests(&mut h, generate);
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request, EditorRequest::ElevenLabsSpeech { .. })),
        "an over-limit button sends nothing"
    );
}

#[test]
fn pasted_voice_lookup_warns_but_generate_still_works() {
    let mut h = harness();
    open_speech_elevenlabs(&mut h);
    h.editor.ui.elevenlabs.config.voice_id.clear();
    h.editor.ui.elevenlabs.config.voice_name.clear();
    h.editor.ui.elevenlabs.pasted_id.clear();
    h.idle_frame();

    let field = h
        .find_text("From your ElevenLabs voice library")
        .expect("paste field hint");
    h.click_at(field);
    h.idle_frame();
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("voice999");
    let enter = |pressed| Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let requests = h.frame(vec![enter(true), enter(false)]);

    assert_eq!(h.editor.ui.elevenlabs.config.voice_id, "voice999");
    assert!(
        requests.iter().any(|request| matches!(
            request,
            EditorRequest::ElevenLabsVoiceLookup { voice_id, .. } if voice_id == "voice999"
        )),
        "the paste starts a lookup: {requests:?}"
    );

    // A restricted key can speak with a voice it cannot read.
    h.editor.set_elevenlabs_result(
        AiPurpose::ElevenLabsVoiceLookup,
        Err("This API key isn't allowed to read voices.".to_string()),
    );
    h.idle_frame();
    assert!(
        h.find_text("This API key isn't allowed to read voices.")
            .is_some(),
        "the lookup failure is shown"
    );
    assert_eq!(h.editor.ui.elevenlabs.config.voice_id, "voice999");

    h.editor.ui.speech.text = "Welcome.".to_string();
    h.idle_frame();
    h.scroll_over(h.screen_rect.center(), -1200.0);
    let generate = h.find_text("Generate").expect("Generate button");
    let requests = click_for_requests(&mut h, generate);
    assert!(
        requests.iter().any(|request| matches!(
            request,
            EditorRequest::ElevenLabsSpeech { text, .. } if text == "Welcome."
        )),
        "speech still starts with the pasted id: {requests:?}"
    );
}

#[test]
fn no_audio_error_shown_to_the_author_does_not_contain_the_key() {
    let mut h = harness();
    open_captions(&mut h);
    let (purpose, _, _, _) = generate_captions(&mut h);
    let message = AiError::RejectedKey.to_string();
    h.editor.set_audio_result(purpose, Err(message));
    h.idle_frame();
    let error = h
        .editor
        .ui
        .captions
        .error
        .clone()
        .expect("an error is shown");
    assert!(!error.contains("sk-secret-value"), "{error}");
}
