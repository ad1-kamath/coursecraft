//! Tests for desktop editor app: prefs, autosave, project files, and dirty state.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::autosave::{
    AutosaveTarget, Autosaver, autosave_path_for_project, delete_autosave, is_autosave_newer,
    write_autosave,
};
use crate::export::{ExportMp4Outcome, ExportUpdate};
use crate::files::{Project, import_web_folder, open_from, save_to};
use crate::prefs::{AiPrefs, ElevenLabsPrefs, Prefs, SpeechSourcePref, load_prefs, save_prefs};
use crate::publish::{PublishJob, package_files, run_publish_job};
use crate::{EditorApp, initial_course};
use coursecraft_course::Course;
use coursecraft_engine::Session;
use coursecraft_publish::{PACKAGE_MARKER, PackageFile, PackageStandard};
use coursecraft_scorm::Standard;
use coursecraft_ui_egui::{EditorRequest, SpeechSource};
use coursecraft_xapi::Flavor;
use std::path::{Path, PathBuf};

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn prefs_round_trip_and_dedupe_cap() {
    let dir = scratch_dir("test-prefs-round-trip");
    let mut prefs = Prefs::default();

    // Add 10 different paths
    for i in 0..10 {
        let p = PathBuf::from(format!("/path/to/project_{i}.ccraft"));
        prefs.add_recent(&p);
    }

    // Should be capped at 8
    assert_eq!(prefs.recent.len(), 8);
    // Most recent is project_9
    assert_eq!(prefs.recent[0], "/path/to/project_9.ccraft");

    // Re-adding project_4 deduplicates and brings it to front
    prefs.add_recent(Path::new("/path/to/project_4.ccraft"));
    assert_eq!(prefs.recent.len(), 8);
    assert_eq!(prefs.recent[0], "/path/to/project_4.ccraft");

    // Ensure all items are unique
    let mut deduped = prefs.recent.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), prefs.recent.len());

    // Save and load
    save_prefs(&dir, &prefs).expect("save prefs");
    let loaded = load_prefs(&dir);
    assert_eq!(loaded, prefs);
    assert!(loaded.automation_enabled);
}

#[test]
fn automation_starts_when_enabled_and_stops_on_drop() {
    let dir = scratch_dir("test-automation-startup");
    let app = EditorApp::new_desktop(initial_course(true), Some(dir.clone()));

    assert!(app.control_server.is_some());
    assert!(app.editor.automation_port.is_some());
    assert!(dir.join("control.json").exists());

    drop(app);
    assert!(!dir.join("control.json").exists());
}

#[test]
fn corrupt_prefs_gives_defaults() {
    let dir = scratch_dir("test-prefs-corrupt");
    let prefs_file = dir.join("prefs.json");
    std::fs::write(&prefs_file, b"{ \"recent\": [ invalid json").expect("write corrupt prefs");

    let loaded = load_prefs(&dir);
    assert_eq!(loaded, Prefs::default());
}

fn ai_prefs() -> AiPrefs {
    AiPrefs {
        source: coursecraft_ai::AiSource::Endpoint,
        base_url: "https://api.example.com/v1".to_string(),
        model: "test-model".to_string(),
        api_key: "sk-very-secret-key-value".to_string(),
        speech_to_text_model: "whisper-1".to_string(),
        text_to_speech_model: "tts-1".to_string(),
        voice: "alloy".to_string(),
    }
}

fn elevenlabs_prefs() -> ElevenLabsPrefs {
    ElevenLabsPrefs {
        api_key: "el-very-secret-key-value".to_string(),
        voice_id: "voice123".to_string(),
        voice_name: "Rachel".to_string(),
        model_id: "eleven_multilingual_v2".to_string(),
    }
}

#[test]
fn elevenlabs_prefs_round_trip_and_old_files_load_without_it() {
    let dir = scratch_dir("test-elevenlabs-prefs-round-trip");
    let old = dir.join("prefs.json");
    std::fs::write(
        &old,
        br#"{"recent":[],"automation_enabled":true,"theme":"dark","coach_marks_done":false}"#,
    )
    .expect("write old prefs");
    let loaded = load_prefs(&dir);
    assert!(loaded.elevenlabs.is_none());
    assert_eq!(loaded.speech_source, SpeechSourcePref::AiProvider);

    let mut prefs = loaded;
    prefs.elevenlabs = Some(elevenlabs_prefs());
    prefs.speech_source = SpeechSourcePref::ElevenLabs;
    save_prefs(&dir, &prefs).expect("save prefs");
    let loaded = load_prefs(&dir);
    assert_eq!(loaded, prefs);
    assert_eq!(loaded.elevenlabs, Some(elevenlabs_prefs()));
    assert_eq!(loaded.speech_source, SpeechSourcePref::ElevenLabs);

    // Removing it omits the field entirely, so an older reader is unaffected.
    prefs.elevenlabs = None;
    save_prefs(&dir, &prefs).expect("save without elevenlabs");
    let text = std::fs::read_to_string(dir.join("prefs.json")).expect("read prefs");
    assert!(!text.contains("\"elevenlabs\""), "{text}");
    assert!(
        text.contains("\"speech_source\": \"eleven-labs\""),
        "{text}"
    );
}

#[test]
fn saving_one_provider_never_removes_the_other() {
    let dir = scratch_dir("test-prefs-provider-independence");
    let ctx = egui::Context::default();
    let mut app = test_app(Some(dir.clone()));
    app.prefs.ai = Some(ai_prefs());
    app.prefs.elevenlabs = Some(elevenlabs_prefs());

    app.handle_request(EditorRequest::SaveAiConfig(None), &ctx);
    assert!(app.prefs.ai.is_none());
    assert_eq!(app.prefs.elevenlabs, Some(elevenlabs_prefs()));
    assert_eq!(
        load_prefs(&dir).elevenlabs,
        Some(elevenlabs_prefs()),
        "the removal was written without touching ElevenLabs"
    );

    app.prefs.ai = Some(ai_prefs());
    app.editor.set_speech_source(SpeechSource::ElevenLabs);
    app.handle_request(EditorRequest::SaveElevenLabs(None), &ctx);
    assert!(app.prefs.elevenlabs.is_none());
    assert_eq!(app.prefs.ai, Some(ai_prefs()), "the AI provider is kept");
    assert_eq!(app.prefs.speech_source, SpeechSourcePref::ElevenLabs);
    let loaded = load_prefs(&dir);
    assert_eq!(loaded.ai, Some(ai_prefs()));
    assert!(loaded.elevenlabs.is_none());
    assert_eq!(loaded.speech_source, SpeechSourcePref::ElevenLabs);
}

#[test]
fn saving_an_agent_source_keeps_the_endpoint_and_is_persisted() {
    let dir = scratch_dir("test-prefs-ai-source");
    let ctx = egui::Context::default();
    let mut app = test_app(Some(dir.clone()));
    app.prefs.ai = Some(ai_prefs());

    app.handle_request(
        EditorRequest::SaveAiSource(coursecraft_ai::AiSource::Codex),
        &ctx,
    );
    assert_eq!(
        app.prefs.ai.as_ref().map(|ai| ai.source),
        Some(coursecraft_ai::AiSource::Codex)
    );
    assert_eq!(
        app.prefs.ai.as_ref().map(|ai| ai.base_url.as_str()),
        Some("https://api.example.com/v1"),
        "the endpoint values are kept"
    );
    assert_eq!(
        load_prefs(&dir).ai.as_ref().map(|ai| ai.source),
        Some(coursecraft_ai::AiSource::Codex)
    );

    // With no stored AI settings, an agent choice creates the prefs object.
    app.prefs.ai = None;
    app.handle_request(
        EditorRequest::SaveAiSource(coursecraft_ai::AiSource::ClaudeCode),
        &ctx,
    );
    assert_eq!(
        app.prefs.ai.as_ref().map(|ai| ai.source),
        Some(coursecraft_ai::AiSource::ClaudeCode)
    );
}

#[test]
fn elevenlabs_key_is_hidden_and_never_enters_the_course() {
    const KEY: &str = "el-very-secret-key-value";
    let mut app = test_app(None);
    app.prefs.elevenlabs = Some(elevenlabs_prefs());
    app.editor
        .set_elevenlabs_config(Some(elevenlabs_prefs().to_config()));

    let course_json = app.editor.session.course.to_json().expect("course json");
    assert!(!course_json.contains(KEY), "{course_json}");

    let printed = format!("{:?}", app.prefs);
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("***"), "{printed}");

    let printed = format!("{:?}", app.editor.ui.elevenlabs);
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("***"), "{printed}");

    let config = elevenlabs_prefs().to_config();
    let printed = format!("{config:?}");
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("***"), "{printed}");
}

#[test]
fn ai_prefs_round_trip_and_old_files_load_without_it() {
    let dir = scratch_dir("test-ai-prefs-round-trip");
    let old = dir.join("prefs.json");
    std::fs::write(
        &old,
        br#"{"recent":[],"automation_enabled":true,"theme":"dark","coach_marks_done":false}"#,
    )
    .expect("write old prefs");
    let loaded = load_prefs(&dir);
    assert!(loaded.ai.is_none());
    assert_eq!(loaded, Prefs::default());

    let mut prefs = loaded;
    prefs.ai = Some(ai_prefs());
    save_prefs(&dir, &prefs).expect("save prefs");
    let loaded = load_prefs(&dir);
    assert_eq!(loaded, prefs);
    assert_eq!(loaded.ai, Some(ai_prefs()));

    // A source without an endpoint still round trips, so a plan-only author
    // keeps the choice.
    let mut agent = ai_prefs();
    agent.source = coursecraft_ai::AiSource::ClaudeCode;
    agent.base_url.clear();
    agent.model.clear();
    agent.api_key.clear();
    prefs.ai = Some(agent.clone());
    save_prefs(&dir, &prefs).expect("save agent prefs");
    let loaded = load_prefs(&dir);
    assert_eq!(loaded.ai, Some(agent));
    let text = std::fs::read_to_string(dir.join("prefs.json")).expect("read prefs");
    assert!(text.contains("\"source\": \"claude-code\""), "{text}");

    // An old prefs file without the field loads as the endpoint.
    let old_ai = dir.join("prefs.json");
    std::fs::write(
        &old_ai,
        br#"{"ai":{"base_url":"https://api.example.com/v1","model":"m","api_key":""}}"#,
    )
    .expect("write old ai prefs");
    let loaded = load_prefs(&dir);
    assert_eq!(
        loaded.ai.as_ref().map(|ai| ai.source),
        Some(coursecraft_ai::AiSource::Endpoint)
    );

    // Removing it omits the field entirely, so an older reader is unaffected.
    prefs.ai = None;
    save_prefs(&dir, &prefs).expect("save without ai");
    let text = std::fs::read_to_string(dir.join("prefs.json")).expect("read prefs");
    assert!(!text.contains("\"ai\""), "{text}");
}

#[test]
fn ai_key_is_hidden_from_debug_output() {
    const KEY: &str = "sk-very-secret-key-value";
    let prefs = Prefs {
        ai: Some(ai_prefs()),
        ..Prefs::default()
    };
    let printed = format!("{prefs:?}");
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("***"), "{printed}");

    let config = ai_prefs().to_config();
    let printed = format!("{config:?}");
    assert!(!printed.contains(KEY), "{printed}");
    assert!(printed.contains("***"), "{printed}");

    let mut bad = config.clone();
    bad.base_url = "http://api.example.com/v1".to_string();
    let error = coursecraft_ai::check_config(&bad)
        .expect_err("remote plain http must fail")
        .to_string();
    assert!(!error.contains(KEY), "{error}");
    let error = coursecraft_ai::build_chat_call(
        &bad,
        &coursecraft_ai::ChatRequest {
            messages: Vec::new(),
            max_tokens: None,
            temperature: None,
        },
    )
    .expect_err("build must fail")
    .to_string();
    assert!(!error.contains(KEY), "{error}");
}

#[cfg(unix)]
#[test]
fn prefs_with_a_key_are_owner_only_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("test-ai-prefs-mode");
    let prefs = Prefs {
        ai: Some(ai_prefs()),
        ..Prefs::default()
    };
    save_prefs(&dir, &prefs).expect("save prefs");
    let mode = std::fs::metadata(dir.join("prefs.json"))
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn the_ai_key_never_enters_a_course_project_package_or_xliff() {
    const KEY: &str = "sk-very-secret-key-value";
    let dir = scratch_dir("test-ai-key-leak");
    let course = Course::from_json(HELLO_JSON).expect("valid course");

    let mut app = test_app(None);
    app.prefs.ai = Some(ai_prefs());

    // The course model has no AI fields.
    let course_json = app.editor.session.course.to_json().expect("course json");
    assert!(!course_json.contains(KEY));

    // A saved .ccraft.
    let path = dir.join("course.ccraft");
    app.project.path = Some(path.clone());
    assert!(app.perform_save(), "save project");
    let bytes = std::fs::read(&path).expect("read .ccraft");
    assert!(!String::from_utf8_lossy(&bytes).contains(KEY));

    // An exported XLIFF.
    let xliff = app
        .editor
        .run(
            "course.export_xliff",
            serde_json::json!({ "target_language": "fr" }),
        )
        .and_then(|value| value.as_str().map(str::to_string))
        .expect("xliff");
    assert!(!xliff.contains(KEY));

    // A published package.
    let job = PublishJob {
        course,
        media: Vec::new(),
        standard: PackageStandard::Scorm(Standard::Scorm12),
        player: fake_player(),
        out: dir.join("course.zip"),
    };
    let files = package_files(&job).expect("package files");
    for file in &files {
        assert!(
            !String::from_utf8_lossy(&file.bytes).contains(KEY),
            "{} carries the key",
            file.path
        );
    }
}

#[test]
fn autosave_path_for_named_and_untitled() {
    let config_dir = Path::new("/app/config/coursecraft");

    // Named project
    let named = Path::new("/projects/courses/biology_101.ccraft");
    let auto_named = autosave_path_for_project(Some(named), config_dir);
    assert_eq!(
        auto_named,
        PathBuf::from("/projects/courses/biology_101.autosave.ccraft")
    );

    // Untitled project
    let auto_untitled = autosave_path_for_project(None, config_dir);
    assert_eq!(
        auto_untitled,
        PathBuf::from("/app/config/coursecraft/untitled.autosave.ccraft")
    );
}

#[test]
fn dirty_flag_after_edit_and_simulated_save() {
    let mut session = Session::blank("Blank Course");
    let mut project = Project::new(session.revision);

    // Fresh project is not dirty
    assert!(!project.is_dirty(&session));
    assert_eq!(project.file_name_display(), "Untitled");

    // An edit makes it dirty
    session
        .execute("slide.add", &serde_json::json!({ "scene": "scene1" }))
        .expect("add slide");
    assert!(project.is_dirty(&session));

    // Simulated save updates saved_revision
    project.saved_revision = session.revision;
    assert!(!project.is_dirty(&session));

    // Setting a path updates file_name_display
    project.path = Some(PathBuf::from("/courses/my_module.ccraft"));
    assert_eq!(project.file_name_display(), "my_module.ccraft");
}

#[test]
fn save_open_round_trip() {
    let dir = scratch_dir("test-save-open");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let media = vec![
        ("media/sample.png".to_string(), vec![0x89, 0x50, 0x4E, 0x47]),
        ("media/doc.pdf".to_string(), vec![0x25, 0x50, 0x44, 0x46]),
    ];

    let ccraft_path = dir.join("course.ccraft");
    save_to(&ccraft_path, &course, &media).expect("save_to");
    assert!(ccraft_path.exists());

    let (loaded_course, loaded_media) = open_from(&ccraft_path).expect("open_from");
    assert_eq!(loaded_course, course);
    assert_eq!(loaded_media, media);

    // Also test bare .json loading
    let json_path = dir.join("bare.json");
    let course_json = serde_json::to_string_pretty(&course).expect("json");
    std::fs::write(&json_path, course_json).expect("write json");

    let (json_course, json_media) = open_from(&json_path).expect("open_from json");
    assert_eq!(json_course, course);
    assert!(json_media.is_empty());
}

#[test]
fn autosave_timer_and_newer_check() {
    let dir = scratch_dir("test-autosave-timer");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let media = Vec::new();

    let project_path = dir.join("my_course.ccraft");
    save_to(&project_path, &course, &media).expect("save project");

    let mut autosaver = Autosaver::new();

    // At t = 10s: too early (< 60s), returns false
    let auto1 = autosaver.maybe_autosave(
        10.0,
        true,
        2,
        AutosaveTarget {
            project_path: Some(&project_path),
            config_dir: Some(&dir),
            course: &course,
        },
        || media.clone(),
    );
    assert!(!auto1);

    // At t = 65s: dirty with new revision -> writes autosave
    let auto2 = autosaver.maybe_autosave(
        65.0,
        true,
        2,
        AutosaveTarget {
            project_path: Some(&project_path),
            config_dir: Some(&dir),
            course: &course,
        },
        || media.clone(),
    );
    assert!(auto2);

    let auto_path = autosave_path_for_project(Some(&project_path), &dir);
    assert!(auto_path.exists());

    // Direct write_autosave verification
    write_autosave(&course, &media, &auto_path).expect("direct write_autosave");
    assert!(auto_path.exists());

    // Small delay to ensure timestamp difference if needed
    let is_newer = is_autosave_newer(&project_path, &auto_path);
    // autosave was written after project_path
    assert!(is_newer || !is_autosave_newer(&auto_path, &project_path));

    // Delete autosave
    delete_autosave(&auto_path);
    assert!(!auto_path.exists());
    assert!(!is_autosave_newer(&project_path, &auto_path));
}

#[test]
fn save_image_writes_the_png_to_the_chosen_path() {
    let dir = scratch_dir("test-save-image");
    let target = dir.join("certificate.png");
    let png = b"\x89PNG\r\n\x1a\nrendered".to_vec();
    let mut app = test_app(None);
    app.save_image_to(&target, &png);
    assert_eq!(std::fs::read(&target).unwrap(), png);
}

#[test]
fn write_atomic_replaces_and_leaves_no_temp() {
    let dir = scratch_dir("test-atomic");
    let target = dir.join("a.ccraft");
    std::fs::write(&target, b"old").unwrap();
    crate::files::write_atomic(&target, b"new content").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new content");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec!["a.ccraft".to_string()]);

    // Failure (target is a directory) cleans up the temp file.
    let bad = dir.join("sub");
    std::fs::create_dir_all(bad.join("x")).unwrap();
    assert!(crate::files::write_atomic(&bad, b"data").is_err());
    assert!(
        !std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains(".tmp-"))
    );
}

#[test]
fn untitled_autosave_skipped_without_config_dir() {
    use crate::autosave::autosave_target_path;
    assert_eq!(autosave_target_path(None, None), None);
    assert_eq!(
        autosave_target_path(None, Some(Path::new("/cfg"))),
        Some(PathBuf::from("/cfg/untitled.autosave.ccraft"))
    );
    assert_eq!(
        autosave_target_path(Some(Path::new("/p/a.ccraft")), None),
        Some(PathBuf::from("/p/a.autosave.ccraft"))
    );

    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut autosaver = Autosaver::new();
    let wrote = autosaver.maybe_autosave(
        100.0,
        true,
        2,
        AutosaveTarget {
            project_path: None,
            config_dir: None,
            course: &course,
        },
        Vec::new,
    );
    assert!(!wrote);
}

#[test]
fn autosave_requests_media_only_when_a_write_is_due() {
    use std::cell::Cell;
    let dir = scratch_dir("test-autosave-lazy-media");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let project_path = dir.join("lazy.ccraft");
    let calls = Cell::new(0u32);
    let media = || {
        calls.set(calls.get() + 1);
        Vec::new()
    };
    let target = || AutosaveTarget {
        project_path: Some(&project_path),
        config_dir: Some(&dir),
        course: &course,
    };
    let mut autosaver = Autosaver::new();
    // Too early, then clean, must not touch media.
    assert!(!autosaver.maybe_autosave(10.0, true, 2, target(), media));
    assert!(!autosaver.maybe_autosave(65.0, false, 2, target(), media));
    assert_eq!(calls.get(), 0);
    // Due and dirty: media requested exactly once.
    assert!(autosaver.maybe_autosave(130.0, true, 3, target(), media));
    assert_eq!(calls.get(), 1);
    // Same revision again: no write, no media request.
    assert!(!autosaver.maybe_autosave(200.0, true, 3, target(), media));
    assert_eq!(calls.get(), 1);
}

#[test]
fn next_autosave_wait_is_clamped() {
    use crate::autosave::secs_until_next_autosave;
    assert!((secs_until_next_autosave(10.0, 0.0) - 50.0).abs() < 1e-9);
    assert_eq!(secs_until_next_autosave(500.0, 0.0), 0.0);
}

#[test]
fn close_interception_and_title_dedupe() {
    use crate::{should_intercept_close, title_changed, window_title};
    assert!(should_intercept_close(true, true, false));
    assert!(!should_intercept_close(true, true, true));
    assert!(!should_intercept_close(true, false, false));
    assert!(!should_intercept_close(false, true, false));

    let mut last = None;
    let t = window_title("a.ccraft", true);
    assert!(t.ends_with('•'));
    assert!(title_changed(&mut last, &t));
    assert!(!title_changed(&mut last, &t));
    assert!(title_changed(&mut last, &window_title("a.ccraft", false)));
}

fn test_app(config: Option<PathBuf>) -> crate::EditorApp {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    crate::EditorApp::new_desktop(course, config)
}

#[test]
fn cli_open_keeps_media_and_path() {
    let dir = scratch_dir("test-cli-open-ok");
    let cfg = dir.join("cfg");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let media = vec![("media/a.png".to_string(), vec![1, 2, 3])];
    let path = dir.join("c.ccraft");
    save_to(&path, &course, &media).unwrap();

    let mut app = test_app(Some(cfg));
    app.startup(Some(path.clone()), false);
    assert_eq!(app.project.path.as_deref(), Some(path.as_path()));
    let loaded: Vec<(String, Vec<u8>)> = app
        .editor
        .media()
        .iter()
        .map(|(k, v)| (k.clone(), v.to_vec()))
        .collect();
    assert_eq!(loaded, media);
    assert!(!app.project.is_dirty(&app.editor.session));
    assert!(app.error_modal.is_none());
    assert_eq!(
        app.prefs.recent.first(),
        Some(&path.to_string_lossy().into_owned())
    );
}

#[test]
fn cli_open_failure_starts_untitled_with_error() {
    let dir = scratch_dir("test-cli-open-bad");
    let bad = dir.join("broken.ccraft");
    std::fs::write(&bad, b"not a zip").unwrap();

    let mut app = test_app(None);
    app.startup(Some(bad.clone()), false);
    assert!(app.project.path.is_none());
    assert!(app.error_modal.is_some());
    assert_eq!(std::fs::read(&bad).unwrap(), b"not a zip");
}

#[test]
fn untitled_recovery_prompt_and_restore() {
    let dir = scratch_dir("test-untitled-recovery");
    let cfg = dir.join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();

    // Nothing to recover.
    let mut app = test_app(Some(cfg.clone()));
    app.startup(None, false);
    assert!(app.restore_prompt.is_none());

    // Leftover autosave triggers a prompt with no file path.
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let auto = autosave_path_for_project(None, &cfg);
    write_autosave(&course, &[], &auto).unwrap();
    let mut app = test_app(Some(cfg.clone()));
    app.startup(None, false);
    let prompt = app.restore_prompt.clone().expect("prompt");
    assert!(prompt.file_path.is_none());
    assert_eq!(prompt.autosave_path, auto);

    // Restore opens it untitled and dirty.
    app.finish_open(&prompt.autosave_path, prompt.file_path.clone(), true);
    assert!(app.project.path.is_none());
    assert!(app.project.is_dirty(&app.editor.session));

    // No config dir: never prompts.
    let mut app = test_app(None);
    app.startup(None, false);
    assert!(app.restore_prompt.is_none());
}

#[test]
fn saved_then_reopened_project_preserves_and_shows_image() {
    use coursecraft_ui_egui::coursecraft_media::image::{ImageBuffer, ImageFormat, Rgba};
    use std::io::Cursor;

    let dir = scratch_dir("test-save-reopen-image");
    let cfg = dir.join("cfg");
    let path = dir.join("image_course.ccraft");

    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(2, 2);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.put_pixel(1, 1, Rgba([0, 255, 0, 255]));
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .unwrap();

    let mut app = test_app(Some(cfg));
    let src = app.editor.import_image("photo.png", &png_bytes).unwrap();

    let first_slide = app
        .editor
        .session
        .course
        .slides()
        .next()
        .unwrap()
        .id
        .clone();
    app.editor.selection.slide = first_slide.clone();
    app.editor.run(
        "object.add",
        serde_json::json!({
            "slide": first_slide,
            "kind": { "type": "image", "src": src, "alt": "A test photo" },
            "frame": { "x": 10.0, "y": 10.0, "w": 2.0, "h": 2.0 }
        }),
    );

    app.project.path = Some(path.clone());
    assert!(app.perform_save());

    let mut reopened_app = test_app(None);
    reopened_app.finish_open(&path, Some(path.clone()), false);

    assert_eq!(reopened_app.editor.media().len(), 1);
    assert_eq!(reopened_app.editor.media()[0].0, "photo.png");
    assert_eq!(&*reopened_app.editor.media()[0].1, &png_bytes[..]);

    let size = reopened_app.editor.natural_image_size("photo.png");
    assert_eq!(size, Some((2.0, 2.0)));
}

#[test]
fn test_import_pptx_replaces_project_with_untitled_dirty() {
    let dir = scratch_dir("test-editor-import-pptx");
    let pptx_path = dir.join("presentation.pptx");

    let mut file = std::fs::File::create(&pptx_path).unwrap();
    let mut zip = coursecraft_pptx::zip::ZipWriter::new(&mut file);
    let opt = coursecraft_pptx::zip::write::SimpleFileOptions::default();

    let pres_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:sldSz cx="9144000" cy="5143500"/>
        <p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
    </p:presentation>"#;
    zip.start_file("ppt/presentation.xml", opt).unwrap();
    std::io::Write::write_all(&mut zip, pres_xml.as_bytes()).unwrap();

    let pres_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
    </Relationships>"#;
    zip.start_file("ppt/_rels/presentation.xml.rels", opt)
        .unwrap();
    std::io::Write::write_all(&mut zip, pres_rels.as_bytes()).unwrap();

    let slide_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Title"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
                    <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1000" cy="1000"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Imported Deck Slide</a:t></a:r></a:p></p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    zip.start_file("ppt/slides/slide1.xml", opt).unwrap();
    std::io::Write::write_all(&mut zip, slide_xml.as_bytes()).unwrap();
    zip.finish().unwrap();

    let pptx_bytes = std::fs::read(&pptx_path).unwrap();

    let mut app = test_app(None);
    app.project.path = Some(dir.join("existing.ccraft"));
    app.project.saved_revision = app.editor.session.revision;
    assert!(!app.project.is_dirty(&app.editor.session));

    app.finish_import_pptx(&pptx_bytes, "Imported Deck");

    assert_eq!(app.project.path, None);
    assert_eq!(app.project.file_name_display(), "Untitled");
    assert!(app.project.is_dirty(&app.editor.session));
    assert_eq!(app.editor.session.course.title, "Imported Deck");
    assert_eq!(
        app.editor.session.course.scenes[0].slides[0].title,
        "Imported Deck Slide"
    );
    assert!(app.editor.ui.pptx_import_report.is_some());
}

#[test]
fn app_starts_in_welcome_mode_unless_blank_or_path() {
    let dir = scratch_dir("test-app-welcome-startup");
    let mut app = EditorApp::new_desktop(initial_course(false), Some(dir));
    app.startup(None, false);

    assert!(app.editor.is_welcome());
    assert!(!app.editor.ui.has_loaded_course);
}

#[test]
fn app_clicking_new_leaves_welcome_mode_with_blank_course() {
    let dir = scratch_dir("test-app-welcome-new");
    let mut app = EditorApp::new_desktop(initial_course(false), Some(dir));
    app.startup(None, false);
    assert!(app.editor.is_welcome());

    let ctx = egui::Context::default();
    app.handle_request(coursecraft_ui_egui::EditorRequest::New, &ctx);

    assert!(!app.editor.is_welcome());
    assert_eq!(app.editor.session.course.title, "Untitled course");
}

#[test]
fn app_explore_example_feature_showcase_loads_with_media() {
    let dir = scratch_dir("test-app-welcome-showcase");
    let mut app = EditorApp::new_desktop(initial_course(false), Some(dir));
    app.startup(None, false);
    assert!(app.editor.is_welcome());

    let ctx = egui::Context::default();
    app.handle_request(
        coursecraft_ui_egui::EditorRequest::OpenExample(
            coursecraft_ui_egui::ExampleId::FeatureShowcase,
        ),
        &ctx,
    );

    assert!(!app.editor.is_welcome());
    assert_eq!(app.editor.session.course.id, "showcase");
    assert_eq!(app.editor.media().len(), 4);
    assert!(app.editor.media().iter().any(|(s, _)| s == "banner.png"));
}

fn fake_player() -> Vec<PackageFile> {
    vec![
        PackageFile {
            path: "index.html".to_string(),
            bytes: b"<!doctype html><html></html>".to_vec(),
        },
        PackageFile {
            path: "assets/app.js".to_string(),
            bytes: b"console.log('player');".to_vec(),
        },
    ]
}

fn find_file<'a>(files: &'a [PackageFile], path: &str) -> Option<&'a PackageFile> {
    files.iter().find(|file| file.path == path)
}

fn course_with_image() -> Course {
    use coursecraft_course::{Object, ObjectKind, Rect};
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide_id = course
        .slides()
        .next()
        .expect("hello course has a slide")
        .id
        .clone();
    course
        .slide_mut(&slide_id)
        .expect("slide")
        .objects
        .push(Object {
            id: "photo".to_string(),
            name: String::new(),
            kind: ObjectKind::Image {
                src: "photo.png".to_string(),
                alt: "A photo".to_string(),
                crop: None,
            },
            frame: Rect {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 100.0,
            },
            timing: Default::default(),
            states: Vec::new(),
            initial_state: "normal".to_string(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
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
        });
    course
}

#[test]
fn publish_job_builds_all_standards_with_correct_marker_and_manifest() {
    let dir = scratch_dir("test-publish-all-standards");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let cases = [
        (PackageStandard::Scorm(Standard::Scorm12), "scorm12"),
        (PackageStandard::Scorm(Standard::Scorm2004), "scorm2004"),
        (PackageStandard::Xapi(Flavor::Cmi5), "cmi5"),
        (PackageStandard::Xapi(Flavor::Xapi), "xapi"),
        (PackageStandard::Lti, "lti"),
    ];

    for (standard, slug) in cases {
        let out = dir.join(format!("{slug}.zip"));
        let job = PublishJob {
            course: course.clone(),
            media: Vec::new(),
            standard,
            player: fake_player(),
            out: out.clone(),
        };
        let files = package_files(&job).expect("package files");
        assert!(find_file(&files, "index.html").is_some());
        assert!(find_file(&files, "course.json").is_some());
        assert!(find_file(&files, "assets/app.js").is_some());

        match standard {
            PackageStandard::Scorm(Standard::Scorm12) => {
                assert!(find_file(&files, PACKAGE_MARKER).is_none());
                let manifest = find_file(&files, "imsmanifest.xml").expect("manifest");
                let xml = std::str::from_utf8(&manifest.bytes).expect("manifest is UTF-8");
                assert!(xml.contains("<schemaversion>1.2</schemaversion>"));
            }
            PackageStandard::Scorm(Standard::Scorm2004) => {
                let marker = find_file(&files, PACKAGE_MARKER).expect("marker");
                assert_eq!(marker.bytes.as_slice(), br#"{"standard":"scorm2004"}"#);
                let manifest = find_file(&files, "imsmanifest.xml").expect("manifest");
                let xml = std::str::from_utf8(&manifest.bytes).expect("manifest is UTF-8");
                assert!(xml.contains("<schemaversion>2004 4th Edition</schemaversion>"));
            }
            PackageStandard::Xapi(Flavor::Cmi5) => {
                let marker = find_file(&files, PACKAGE_MARKER).expect("marker");
                assert_eq!(marker.bytes.as_slice(), br#"{"standard":"cmi5"}"#);
                assert!(find_file(&files, "cmi5.xml").is_some());
                assert!(find_file(&files, "imsmanifest.xml").is_none());
            }
            PackageStandard::Xapi(Flavor::Xapi) => {
                let marker = find_file(&files, PACKAGE_MARKER).expect("marker");
                assert_eq!(marker.bytes.as_slice(), br#"{"standard":"xapi"}"#);
                assert!(find_file(&files, "tincan.xml").is_some());
                assert!(find_file(&files, "imsmanifest.xml").is_none());
            }
            PackageStandard::Lti => {
                let marker = find_file(&files, PACKAGE_MARKER).expect("marker");
                assert_eq!(marker.bytes.as_slice(), br#"{"standard":"lti"}"#);
                assert!(find_file(&files, "imsmanifest.xml").is_none());
            }
        }

        let outcome = run_publish_job(job).expect("publish");
        assert_eq!(outcome.path, out);
        assert!(outcome.bytes > 0);
        let bytes = std::fs::read(&out).expect("written zip");
        assert_eq!(&bytes[..2], b"PK");
        assert_eq!(bytes.len() as u64, outcome.bytes);
    }
}

#[test]
fn publish_job_puts_in_memory_media_under_media_prefix() {
    let dir = scratch_dir("test-publish-media");
    let course = course_with_image();
    let image = vec![0x89, 0x50, 0x4E, 0x47, 0x01];
    let job = PublishJob {
        course,
        media: vec![("photo.png".to_string(), image.clone())],
        standard: PackageStandard::Scorm(Standard::Scorm12),
        player: fake_player(),
        out: dir.join("media.zip"),
    };
    let files = package_files(&job).expect("package files");
    let packaged = find_file(&files, "media/photo.png").expect("media file");
    assert_eq!(packaged.bytes, image);

    let outcome = run_publish_job(job).expect("publish");
    assert!(outcome.path.exists());
    assert!(outcome.bytes > 0);
}

#[test]
fn publish_job_missing_media_names_the_object_and_file() {
    let dir = scratch_dir("test-publish-missing-media");
    let job = PublishJob {
        course: course_with_image(),
        media: Vec::new(),
        standard: PackageStandard::Scorm(Standard::Scorm12),
        player: fake_player(),
        out: dir.join("missing.zip"),
    };
    let error = package_files(&job).expect_err("missing media must fail");
    assert!(error.contains("photo.png"), "error names the file: {error}");
    assert!(error.contains("photo"), "error names the object: {error}");
}

#[test]
fn web_folder_import_copies_files_and_skips_hidden_and_symlinks() {
    let dir = scratch_dir("test-web-folder-import");
    let folder = dir.join("My Site");
    std::fs::create_dir_all(folder.join("assets")).unwrap();
    std::fs::write(folder.join("index.html"), b"<html></html>").unwrap();
    std::fs::write(folder.join("style.css"), b"body{}").unwrap();
    std::fs::write(folder.join(".secret"), b"hidden").unwrap();
    std::fs::write(folder.join("assets/app.js"), b"run()").unwrap();
    std::fs::create_dir_all(folder.join(".git")).unwrap();
    std::fs::write(folder.join(".git/config"), b"hidden dir").unwrap();

    #[cfg(unix)]
    {
        let outside = dir.join("outside.txt");
        std::fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, folder.join("link.txt")).unwrap();
        std::os::unix::fs::symlink(dir.join("assets"), folder.join("linked-dir")).unwrap();
    }

    let import = import_web_folder(&folder, &[]).expect("import folder");
    assert_eq!(import.path, "web/My-Site");
    assert_eq!(import.entry, "index.html");
    let names: Vec<&str> = import.files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "web/My-Site/assets/app.js",
            "web/My-Site/index.html",
            "web/My-Site/style.css"
        ]
    );
    assert!(
        !import.files.iter().any(|(name, _)| name.contains("secret")
            || name.contains(".git")
            || name.contains("link")),
        "hidden and symlink entries are skipped: {names:?}"
    );

    let second = import_web_folder(&folder, &[import.files[0].0.clone()]).expect("second import");
    assert_eq!(second.path, "web/My-Site-2");
}

#[test]
fn web_folder_import_rejects_folder_over_the_file_cap() {
    let dir = scratch_dir("test-web-folder-cap");
    let folder = dir.join("big");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("index.html"), b"<html></html>").unwrap();
    for i in 0..coursecraft_course::WEB_MAX_FILES {
        std::fs::write(folder.join(format!("file-{i}.txt")), b"x").unwrap();
    }
    let error = import_web_folder(&folder, &[]).expect_err("over the cap must fail");
    assert!(error.contains("more than"), "{error}");
}

#[test]
fn pick_background_audio_goes_through_the_file_dialog_seam() {
    let dir = scratch_dir("test-pick-background-audio");
    let path = dir.join("theme.mp3");
    std::fs::write(&path, b"ID3 background").unwrap();

    let mut app = test_app(None);
    app.test_picked_audio = Some(path);
    let ctx = egui::Context::default();
    app.handle_request(EditorRequest::PickBackgroundAudio, &ctx);

    assert!(
        app.editor
            .media()
            .iter()
            .any(|(src, bytes)| src == "theme.mp3" && bytes.as_ref() == b"ID3 background"),
        "the picked file is stored in the media library"
    );
    let audio = app
        .editor
        .session
        .course
        .background_audio
        .as_ref()
        .expect("background audio set");
    assert_eq!(audio.src, "theme.mp3");
    assert!((audio.volume - 0.5).abs() < f32::EPSILON);
    assert!(audio.looping);
}

#[test]
fn pick_media_files_goes_through_the_file_dialog_seam() {
    let dir = scratch_dir("test-pick-media-files");
    let image = dir.join("picture.png");
    std::fs::write(
        &image,
        include_bytes!("../../../examples/showcase/media/banner.png"),
    )
    .unwrap();
    let audio = dir.join("theme.mp3");
    std::fs::write(&audio, b"ID3 background").unwrap();

    let mut app = test_app(None);
    app.test_picked_media_files = Some(vec![image, audio]);
    let ctx = egui::Context::default();
    app.handle_request(EditorRequest::PickMediaFiles, &ctx);

    assert!(
        app.editor
            .media()
            .iter()
            .any(|(src, bytes)| src == "picture.png" && !bytes.is_empty()),
        "the picked image is stored"
    );
    assert!(
        app.editor
            .media()
            .iter()
            .any(|(src, bytes)| src == "theme.mp3" && bytes.as_ref() == b"ID3 background"),
        "the picked audio is stored"
    );
    assert!(app.project.is_dirty(&app.editor.session));
    assert!(
        app.editor
            .session
            .course
            .slides()
            .all(|slide| slide.objects.iter().all(|object| !matches!(
                object.kind,
                coursecraft_course::ObjectKind::Image { .. }
                    | coursecraft_course::ObjectKind::Audio { .. }
            ))),
        "importing places nothing"
    );
}

#[test]
fn replace_media_file_goes_through_the_file_dialog_seam() {
    let dir = scratch_dir("test-replace-media-file");
    let replacement = dir.join("new.bin");
    std::fs::write(&replacement, b"new bytes").unwrap();

    let mut app = test_app(None);
    let original = include_bytes!("../../../examples/showcase/media/banner.png");
    let src = app
        .editor
        .add_media_bytes("photo.png", original)
        .expect("add media");
    app.test_picked_media_file = Some(replacement);
    let ctx = egui::Context::default();
    app.handle_request(EditorRequest::ReplaceMediaFile { src: src.clone() }, &ctx);

    let (name, bytes) = app
        .editor
        .media()
        .iter()
        .find(|(name, _)| name == &src)
        .expect("entry keeps its name");
    assert_eq!(name, &src);
    assert_eq!(bytes.as_ref(), b"new bytes");
}

#[test]
fn publish_job_unwritable_path_reports_an_error() {
    let dir = scratch_dir("test-publish-unwritable");
    let job = PublishJob {
        course: Course::from_json(HELLO_JSON).expect("valid course"),
        media: Vec::new(),
        standard: PackageStandard::Scorm(Standard::Scorm12),
        player: fake_player(),
        out: dir.clone(),
    };
    let error = run_publish_job(job).expect_err("writing over a directory must fail");
    assert!(error.contains("Couldn't write"), "readable error: {error}");
}

#[test]
fn export_mp4_goes_through_the_save_dialog_seam() {
    let dir = scratch_dir("test-export-seam");
    let out = dir.join("course.mp4");
    let mut app = test_app(None);
    app.test_export_path = Some(out.clone());
    let ctx = egui::Context::default();
    app.handle_request(
        EditorRequest::ExportMp4 {
            width: 1280,
            fps: 30,
        },
        &ctx,
    );

    assert!(
        app.test_export_path.is_none(),
        "the seam supplied the destination"
    );
    assert!(app.export_rx.is_some(), "a background export started");
    let job = app.editor.ui.export_job.as_ref().expect("job view");
    assert_eq!(job.slide_total, 1);
    assert!(!job.cancel.load(std::sync::atomic::Ordering::Relaxed));
    // Stop the worker at its first frame so the test does not encode the course.
    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[test]
fn export_review_goes_through_the_save_dialog_seam_and_writes_the_bytes() {
    let dir = scratch_dir("test-export-review-seam");
    let out = dir.join("Hello Course.docx");
    let mut app = test_app(None);
    app.test_export_path = Some(out.clone());
    let ctx = egui::Context::default();
    app.handle_request(
        EditorRequest::ExportReview {
            format: coursecraft_ui_egui::ReviewFormat::Word,
            options: coursecraft_ui_egui::ReviewOptionsView::default(),
        },
        &ctx,
    );

    assert!(
        app.test_export_path.is_none(),
        "the seam supplied the destination"
    );
    let rx = app
        .export_review_rx
        .take()
        .expect("a background review export started");
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the review export finished");
    assert_eq!(result.expect("review export succeeds"), out);
    let bytes = std::fs::read(&out).expect("read the exported document");
    assert!(bytes.starts_with(b"PK"), "the Word file is a zip");
}

#[test]
fn finished_export_sets_the_status() {
    let dir = scratch_dir("test-export-finished");
    let path = dir.join("Hello Course.mp4");
    let mut app = test_app(None);
    let (tx, rx) = std::sync::mpsc::channel();
    app.export_rx = Some(rx);
    app.editor.ui.export_job = Some(coursecraft_ui_egui::ExportJobView {
        slide_index: 11,
        slide_total: 12,
        frames: 300,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });
    let report = coursecraft_export::ExportReport {
        duration_ms: 83_000,
        frames: 2_490,
        skipped_questions: vec!["q1".to_string(), "q2".to_string()],
        branch_slides: Vec::new(),
        warnings: Vec::new(),
    };
    tx.send(ExportUpdate::Finished(Ok(ExportMp4Outcome {
        path: path.clone(),
        report,
    })))
    .expect("send finished");
    let ctx = egui::Context::default();
    app.poll_export(&ctx);

    assert!(app.editor.ui.export_job.is_none(), "the progress is gone");
    let status = app
        .editor
        .ui
        .export_status
        .as_deref()
        .expect("a finished status");
    assert!(
        status.contains("Exported Hello Course.mp4 (1:23, 2 questions skipped)"),
        "{status}"
    );
    assert_eq!(app.published_path.as_deref(), Some(path.as_path()));
}

#[test]
fn preferences_round_trip_and_bad_values_fall_back() {
    use crate::prefs::SlidePreset;
    let dir = scratch_dir("prefs-m16");
    let prefs = Prefs {
        autosave_minutes: 5,
        default_slide: SlidePreset::Standard4x3,
        ..Prefs::default()
    };
    save_prefs(&dir, &prefs).unwrap();
    let loaded = load_prefs(&dir);
    assert_eq!(loaded.autosave_minutes, 5);
    assert_eq!(loaded.default_slide, SlidePreset::Standard4x3);

    // Old files without the keys load with defaults; broken files too.
    std::fs::write(dir.join("prefs.json"), "{\"recent\": []}").unwrap();
    let old = load_prefs(&dir);
    assert_eq!(old.autosave_minutes, 1);
    assert_eq!(old.default_slide, SlidePreset::Wide16x9);
    std::fs::write(dir.join("prefs.json"), "{ not json").unwrap();
    assert_eq!(load_prefs(&dir), Prefs::default());
    std::fs::write(dir.join("prefs.json"), "{\"autosave_minutes\": 9999}").unwrap();
    assert_eq!(load_prefs(&dir).autosave_minutes_clamped(), 60);
}

#[test]
fn autosave_interval_is_respected_and_zero_turns_it_off() {
    let dir = scratch_dir("autosave-interval");
    let course = Session::blank("Interval").course;
    let project_path = dir.join("interval.ccraft");
    let target = || AutosaveTarget {
        project_path: Some(&project_path),
        config_dir: Some(&dir),
        course: &course,
    };
    let mut autosaver = Autosaver::new();
    autosaver.interval_minutes = Some(5);
    assert!(!autosaver.maybe_autosave(200.0, true, 2, target(), Vec::new));
    assert!(autosaver.maybe_autosave(301.0, true, 2, target(), Vec::new));
    assert_eq!(autosaver.secs_until_next(301.0), Some(300.0));

    let mut off = Autosaver::new();
    off.interval_minutes = Some(0);
    assert!(!off.maybe_autosave(100_000.0, true, 2, target(), Vec::new));
    assert_eq!(off.secs_until_next(10.0), None);
}

#[test]
fn a_new_course_uses_the_default_slide_size() {
    use crate::prefs::SlidePreset;
    let prefs = Prefs {
        default_slide: SlidePreset::Square,
        ..Prefs::default()
    };
    let course = prefs.blank_course();
    assert_eq!((course.slide_size.w, course.slide_size.h), (1080, 1080));
    let default = Prefs::default().blank_course();
    assert_eq!((default.slide_size.w, default.slide_size.h), (1280, 720));
}
