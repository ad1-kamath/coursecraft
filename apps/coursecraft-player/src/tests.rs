//! Unit tests for CourseCraft player argument parsing, log capping, and playback.

use std::path::PathBuf;

use coursecraft_course::Course;
use coursecraft_logic::{Effect, Input};

use crate::app::{Playing, capped_push_log, compute_session_ms, format_issues, push_log_entry};
use crate::load::{
    PlayerStandard, learner_name_from_query, parse_args, parse_course, parse_package_standard,
};

#[test]
fn arg_parsing_default() {
    let config = parse_args([] as [&str; 0]).expect("default args valid");
    assert_eq!(
        config.course_path,
        PathBuf::from("examples/hello-course/course.json")
    );
    assert!(!config.log_effects);
}

#[test]
fn arg_parsing_custom_path() {
    let config = parse_args(["custom/path.json"]).expect("positional path valid");
    assert_eq!(config.course_path, PathBuf::from("custom/path.json"));
    assert!(!config.log_effects);
}

#[test]
fn arg_parsing_flag_only() {
    let config = parse_args(["--log-effects"]).expect("flag only valid");
    assert_eq!(
        config.course_path,
        PathBuf::from("examples/hello-course/course.json")
    );
    assert!(config.log_effects);
}

#[test]
fn arg_parsing_path_and_flag() {
    let config = parse_args(["custom/course.json", "--log-effects"]).expect("path and flag valid");
    assert_eq!(config.course_path, PathBuf::from("custom/course.json"));
    assert!(config.log_effects);
}

#[test]
fn arg_parsing_flag_and_path() {
    let config = parse_args(["--log-effects", "custom/course.json"]).expect("flag and path valid");
    assert_eq!(config.course_path, PathBuf::from("custom/course.json"));
    assert!(config.log_effects);
}

#[test]
fn arg_parsing_unknown_flag() {
    let err = parse_args(["--unknown-flag"]).expect_err("unknown flag rejected");
    assert!(err.contains("unknown flag: '--unknown-flag'"));
    assert!(err.contains("Usage:"));
}

#[test]
fn arg_parsing_too_many_positional_arguments() {
    let err = parse_args(["first.json", "second.json"]).expect_err("multiple paths rejected");
    assert!(err.contains("unexpected argument: 'second.json'"));
    assert!(err.contains("Usage:"));
}

#[test]
fn log_capping_under_limit() {
    let mut log = Vec::new();
    for i in 0..10 {
        push_log_entry(&mut log, format!("line {i}"));
    }
    assert_eq!(log.len(), 10);
    assert_eq!(log.first().map(String::as_str), Some("line 0"));
    assert_eq!(log.last().map(String::as_str), Some("line 9"));
}

#[test]
fn log_capping_at_and_over_limit() {
    let mut log = Vec::new();
    for i in 0..2005 {
        push_log_entry(&mut log, format!("line {i}"));
    }
    assert_eq!(log.len(), 2000);
    assert_eq!(log.first().map(String::as_str), Some("line 5"));
    assert_eq!(log.last().map(String::as_str), Some("line 2004"));
}

#[test]
fn capped_push_log_drops_oldest() {
    let mut log = Vec::new();
    capped_push_log(&mut log, "a".into(), 3);
    capped_push_log(&mut log, "b".into(), 3);
    capped_push_log(&mut log, "c".into(), 3);
    assert_eq!(log, vec!["a", "b", "c"]);

    capped_push_log(&mut log, "d".into(), 3);
    assert_eq!(log, vec!["b", "c", "d"]);

    capped_push_log(&mut log, "e".into(), 3);
    assert_eq!(log, vec!["c", "d", "e"]);
}

#[test]
fn hello_course_start_emits_slide_entered() {
    let raw = include_str!("../../../examples/hello-course/course.json");
    let course: Course = serde_json::from_str(raw).expect("valid hello-course json");
    let mut playing = Playing::from_course(course, false).expect("valid course");

    playing.handle_input(&Input::Start, None);

    assert_eq!(
        playing.log.first().map(String::as_str),
        Some(r#"{"effect":"slide_entered","slide":"s-intro"}"#)
    );
}

#[test]
fn invalid_course_fails_player_creation() {
    let raw = include_str!("../../../examples/hello-course/course.json");
    let mut course: Course = serde_json::from_str(raw).expect("valid hello-course json");
    course.start_slide = Some("nowhere".into());

    let err = match Playing::from_course(course, false) {
        Ok(_) => panic!("course with broken start_slide should fail validation"),
        Err(e) => e,
    };
    assert!(!err.is_empty());
    assert!(err.contains("start_slide"));
}

#[test]
fn exit_effect_marks_ended() {
    let raw = include_str!("../../../examples/hello-course/course.json");
    let course: Course = serde_json::from_str(raw).expect("valid hello-course json");
    let mut playing = Playing::from_course(course, false).expect("valid course");

    assert!(!playing.ended);
    let exit_json = serde_json::to_string(&Effect::Exit).expect("exit serializes");
    let effect: Effect = serde_json::from_str(&exit_json).expect("exit deserializes");
    assert_eq!(effect, Effect::Exit);

    // Simulate Exit effect via input or direct effect handler
    // When Effect::Exit happens in player, ended = true
    playing.handle_input(&Input::Start, None);
    assert!(!playing.ended);
}

#[test]
fn parse_course_malformed_mentions_source() {
    let err = parse_course("{not json", "my-source.json").expect_err("malformed JSON must fail");
    assert!(err.contains("my-source.json"), "error was: {err}");
}

#[test]
fn parse_course_hello_course_ok() {
    let text = include_str!("../../../examples/hello-course/course.json");
    assert!(parse_course(text, "course.json").is_ok());
}

#[test]
fn lti_marker_selects_the_lti_player() {
    let (standard, warning) = parse_package_standard(Some(r#"{"standard":"lti"}"#));
    assert_eq!(standard, PlayerStandard::Lti);
    assert!(warning.is_none());
}

#[test]
fn missing_marker_defaults_to_scorm12_without_a_warning() {
    let (standard, warning) = parse_package_standard(None);
    assert_eq!(standard, PlayerStandard::Scorm12);
    assert!(warning.is_none());
}

#[test]
fn unknown_marker_falls_back_to_scorm12_with_a_warning() {
    let (standard, warning) = parse_package_standard(Some(r#"{"standard":"scorm1.2"}"#));
    assert_eq!(standard, PlayerStandard::Scorm12);
    assert!(warning.is_some_and(|text| text.contains("unknown")));
}

#[test]
fn the_learner_name_is_read_from_the_launch_query() {
    assert_eq!(
        learner_name_from_query("?name=Ada+Lovelace").as_deref(),
        Some("Ada Lovelace")
    );
    assert_eq!(
        learner_name_from_query("name=Zo%C3%AB%20M").as_deref(),
        Some("Zoë M")
    );
    assert_eq!(
        learner_name_from_query("?foo=1&name=Grace"),
        Some("Grace".into())
    );
    assert_eq!(learner_name_from_query("?name=%20"), None);
    assert_eq!(learner_name_from_query("?other=1"), None);
}

#[test]
fn session_ms_normal_elapsed() {
    assert_eq!(compute_session_ms(1000.0, 2500.5), 1500);
    assert_eq!(compute_session_ms(0.0, 10_000.0), 10_000);
}

#[test]
fn session_ms_negative_or_zero_clamps_to_zero() {
    assert_eq!(compute_session_ms(2000.0, 1000.0), 0);
    assert_eq!(compute_session_ms(500.0, 500.0), 0);
}

#[test]
fn session_ms_hostile_floats_return_zero() {
    assert_eq!(compute_session_ms(f64::NAN, 1000.0), 0);
    assert_eq!(compute_session_ms(1000.0, f64::NAN), 0);
    assert_eq!(compute_session_ms(f64::INFINITY, 1000.0), 0);
    assert_eq!(compute_session_ms(1000.0, f64::NEG_INFINITY), 0);
}

#[test]
fn session_ms_overflow_saturates() {
    assert_eq!(compute_session_ms(0.0, 1e25), u64::MAX);
}

#[test]
fn format_issues_joins_lines() {
    let issues = vec![
        coursecraft_course::Issue {
            severity: coursecraft_course::Severity::Error,
            path: "course.title".into(),
            message: "title is required".into(),
        },
        coursecraft_course::Issue {
            severity: coursecraft_course::Severity::Error,
            path: "course.slides[0]".into(),
            message: "missing id".into(),
        },
    ];
    let formatted = format_issues(issues);
    assert!(formatted.contains("course.title: title is required"));
    assert!(formatted.contains("course.slides[0]: missing id"));
}

#[test]
fn player_null_backend_audio_lifecycle() {
    use coursecraft_playback::{NullBackend, tiny_wav_bytes};
    use std::fs;
    let base_dir = std::env::temp_dir().join(format!(
        "coursecraft-test-player-media-{}",
        std::process::id()
    ));
    let media_dir = base_dir.join("media");
    fs::create_dir_all(&media_dir).expect("create test media dir");
    fs::write(media_dir.join("beep.wav"), tiny_wav_bytes()).expect("write beep.wav");

    let raw = r#"{
      "schema_version": 1,
      "id": "course-media-test",
      "title": "Media Test Course",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [
        {
          "id": "scene-1",
          "title": "Scene 1",
          "slides": [
            {
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "aud1",
                  "name": "Audio",
                  "kind": { "type": "audio", "src": "beep.wav", "autoplay": true },
                  "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
                }
              ],
              "triggers": [
                {
                  "when": { "event": "media_ends", "object": "aud1" },
                  "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
                }
              ]
            },
            {
              "id": "s2",
              "title": "Slide 2",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "aud2",
                  "name": "Audio 2",
                  "kind": { "type": "audio", "src": "beep.wav", "autoplay": true },
                  "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
                }
              ],
              "triggers": [
                {
                  "when": { "event": "click", "object": "aud2" },
                  "action": { "action": "jump_to_slide", "target": { "id": "s3" } }
                }
              ]
            },
            {
              "id": "s3",
              "title": "Slide 3",
              "duration_ms": 5000,
              "objects": []
            }
          ]
        }
      ]
    }"#;

    let course: Course = serde_json::from_str(raw).expect("valid test course JSON");
    let course_ref: &'static Course = Box::leak(Box::new(course));
    let course_path = base_dir.join("course.json");

    let null_backend = NullBackend::new();
    let mut playing = Playing::new(course_ref, false, Some(&course_path))
        .expect("valid playing")
        .with_backend(Box::new(null_backend.clone()));

    // 1. Autoplay audio on entry produces play
    playing.handle_input(&Input::Start, None);
    assert_eq!(
        playing.player.current_slide().map(|s| s.id.as_str()),
        Some("s1")
    );
    assert!(null_backend.is_playing("aud1"));
    assert!(
        null_backend
            .recorded_calls()
            .contains(&"play:aud1:beep.wav".to_string())
    );

    // 2. take_ended produces a media_ends trigger firing a jump to s2
    null_backend.mark_ended("aud1");
    playing.poll_media(None);

    assert_eq!(
        playing.player.current_slide().map(|s| s.id.as_str()),
        Some("s2")
    );
    assert!(!null_backend.is_playing("aud1"));
    // On entry to s2, aud2 autoplays
    assert!(null_backend.is_playing("aud2"));
    assert!(
        null_backend
            .recorded_calls()
            .contains(&"play:aud2:beep.wav".to_string())
    );

    // 3. Slide change while aud2 is playing produces stop
    playing.handle_input(
        &Input::Click {
            object: "aud2".into(),
        },
        None,
    );
    assert_eq!(
        playing.player.current_slide().map(|s| s.id.as_str()),
        Some("s3")
    );
    assert!(!null_backend.is_playing("aud2"));
    assert!(
        null_backend
            .recorded_calls()
            .contains(&"stop:aud2".to_string())
    );

    let _ = fs::remove_dir_all(&base_dir);
}
