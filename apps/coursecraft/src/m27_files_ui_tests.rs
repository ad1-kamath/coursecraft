//! M27 Step D-9: End-to-end tests for Save / Open / Autosave / Version History
//! (**D13.1–D13.12**) driving the real `EditorApp` headlessly via `egui::Context::run_ui`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use coursecraft_course::{Course, FontFace, FontStyle};
use coursecraft_ui_egui::{Editor, EditorRequest, ExampleId, TemplateId};
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

use crate::autosave::autosave_path_for_project;
use crate::files::{
    Hooks, open_folder_from, open_from, remove_stale_temps, save_to, verify_ccraft_file,
    write_atomic_with,
};
use crate::prefs::{Prefs, save_prefs};
use crate::{EditorApp, PendingAction, initial_course, versions};

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

struct AppHarness {
    ctx: Context,
    app: EditorApp,
    screen_rect: Rect,
    time: f64,
}

impl AppHarness {
    fn new(course: Course, config_dir: Option<PathBuf>) -> Self {
        let ctx = Context::default();
        coursecraft_ui_egui::coursecraft_render::apply_style(
            &ctx,
            coursecraft_ui_egui::coursecraft_render::ThemeMode::Dark,
        );
        let mut style = (*ctx.global_style()).clone();
        style.animation_time = 0.0;
        ctx.set_global_style(style);
        let mut app = EditorApp::new_desktop(course, config_dir);
        app.editor.ui.show_welcome = false;
        app.editor.ui.has_loaded_course = true;
        Self {
            ctx,
            app,
            screen_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 900.0)),
            time: 1.0,
        }
    }

    fn next_raw_input(&mut self, events: Vec<Event>) -> RawInput {
        let ctx_time = self.ctx.input(|i| i.time);
        if ctx_time > self.time {
            self.time = ctx_time + 1.0 / 60.0;
        }
        let time = self.time;
        self.time += 1.0 / 60.0;
        RawInput {
            screen_rect: Some(self.screen_rect),
            time: Some(time),
            events,
            ..Default::default()
        }
    }

    fn advance_time(&mut self, secs: f64) {
        self.time += secs.max(0.0);
    }

    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        let raw_input = self.next_raw_input(events);
        self.ctx.run_ui(raw_input, |ui| {
            self.app.show_app(ui);
        })
    }

    fn idle_frame(&mut self) {
        let out = self.frame(Vec::new());
        out.drop_without_applying_deltas();
    }

    fn wait_for_autosave(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while self.app.autosave_rx.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            self.idle_frame();
        }
        assert!(
            self.app.autosave_rx.is_none(),
            "background autosave did not finish within timeout"
        );
    }

    fn find_texts(&mut self, label: &str) -> Vec<Pos2> {
        fn walk(shape: &egui::epaint::Shape, label: &str, out: &mut Vec<Pos2>) {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    let r = t.visual_bounding_rect();
                    let center = if r.is_positive()
                        && r.center().x.is_finite()
                        && r.center().y.is_finite()
                    {
                        r.center()
                    } else {
                        t.galley.rect.translate(t.pos.to_vec2()).center()
                    };
                    out.push(center);
                }
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        walk(s, label, out);
                    }
                }
                _ => {}
            }
        }
        let out = self.frame(Vec::new());
        let mut found = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, label, &mut found);
        }
        out.drop_without_applying_deltas();
        found
    }

    fn all_texts(&mut self) -> Vec<String> {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    let s = t.galley.text().trim().to_string();
                    if !s.is_empty() {
                        out.push(s);
                    }
                }
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        walk(s, out);
                    }
                }
                _ => {}
            }
        }
        let out = self.frame(Vec::new());
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut texts);
        }
        out.drop_without_applying_deltas();
        texts
    }

    fn click_at(&mut self, pos: Pos2) {
        let out1 = self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        out1.drop_without_applying_deltas();
        let out2 = self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        out2.drop_without_applying_deltas();
        self.idle_frame();
    }

    fn click_button(&mut self, label: &str) {
        self.idle_frame();
        let pos = self
            .find_texts(label)
            .into_iter()
            .last()
            .unwrap_or_else(|| panic!("button/text '{label}' not found in UI"));
        self.click_at(pos);
    }

    fn click_button_index(&mut self, label: &str, index: usize) {
        self.idle_frame();
        let matches = self.find_texts(label);
        let pos = *matches.get(index).unwrap_or_else(|| {
            panic!(
                "button/text '{label}' index {index} not found (count={})",
                matches.len()
            )
        });
        self.click_at(pos);
    }

    /// Verify no text shapes in the foreground modal/window layer are horizontally clipped (or vertically clipped outside ScrollAreas).
    fn assert_foreground_texts_unclipped(&mut self, check_vertical: bool) {
        fn collect(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<(String, Rect, Rect)>) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    let s = t.galley.text().trim().to_string();
                    if !s.is_empty() {
                        let r = t.galley.rect.translate(t.pos.to_vec2());
                        out.push((s, r, clip));
                    }
                }
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        collect(s, clip, out);
                    }
                }
                _ => {}
            }
        }
        let out = self.frame(Vec::new());
        let mut items = Vec::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, clipped.clip_rect, &mut items);
        }
        out.drop_without_applying_deltas();
        for (text, rect, clip) in &items {
            let horiz_ok = rect.min.x >= clip.min.x - 2.0 && rect.max.x <= clip.max.x + 2.0;
            let vert_ok = !check_vertical
                || (rect.min.y >= clip.min.y - 2.0 && rect.max.y <= clip.max.y + 2.0);
            assert!(
                horiz_ok && vert_ok,
                "text '{text}' clipped: rect={rect:?} clip={clip:?}"
            );
        }
    }
}

fn minimal_course_with_rich_media(title: &str) -> (Course, Vec<(String, Vec<u8>)>) {
    let mut course = Course::from_json(HELLO_JSON).unwrap();
    course.title = title.to_string();

    let img_src = "hero.png".to_string();
    let audio_src = "narration.wav".to_string();
    let captions_src = "narration.vtt".to_string();
    let web_folder = "web/widget".to_string();
    let font_src = "fonts/custom.ttf".to_string();

    let slide_id = course.scenes[0].slides[0].id.clone();
    let mut editor = Editor::new(course);
    assert!(
        editor
            .run(
                "object.add",
                serde_json::json!({
                    "slide": slide_id,
                    "id": "img_obj",
                    "kind": { "type": "image", "src": img_src, "alt": "Hero" },
                    "frame": { "x": 20.0, "y": 20.0, "w": 100.0, "h": 80.0 }
                }),
            )
            .is_some()
    );
    assert!(
        editor
            .run(
                "object.add",
                serde_json::json!({
                    "slide": slide_id,
                    "id": "audio_obj",
                    "kind": { "type": "audio", "src": audio_src, "captions": captions_src },
                    "frame": { "x": 140.0, "y": 20.0, "w": 60.0, "h": 40.0 }
                }),
            )
            .is_some()
    );
    assert!(editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": slide_id,
                "id": "web_obj",
                "kind": { "type": "web", "source": { "type": "folder", "path": web_folder, "entry": "index.html" } },
                "frame": { "x": 220.0, "y": 20.0, "w": 200.0, "h": 150.0 }
            }),
        )
        .is_some());
    assert!(
        editor
            .run(
                "object.add",
                serde_json::json!({
                    "slide": slide_id,
                    "id": "txt_obj",
                    "kind": { "type": "text", "text": "Hello" },
                    "frame": { "x": 20.0, "y": 200.0, "w": 300.0, "h": 40.0 }
                }),
            )
            .is_some()
    );

    let mut course = editor.session.course;
    course.fonts.push(FontFace {
        family: "CourseCustom".into(),
        style: FontStyle::Regular,
        src: font_src.clone(),
    });

    let media = vec![
        (img_src, vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4]),
        (audio_src, b"RIFF....WAVEfmt ".to_vec()),
        (
            captions_src,
            b"WEBVTT\n\n00:00.000 --> 00:02.000\nHi\n".to_vec(),
        ),
        (
            "web/widget/index.html".to_string(),
            b"<!doctype html><html><body>Hi</body></html>".to_vec(),
        ),
        (
            "web/widget/app.js".to_string(),
            b"console.log('ok');".to_vec(),
        ),
        (font_src, b"\x00\x01\x00\x00dummyfontbytes".to_vec()),
    ];
    (course, media)
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn d13_1_ccraft_save_edit_save_reopen_fidelity_and_no_tmp() {
    let dir = scratch_dir("d13-1-ccraft-fidelity");
    let cfg = dir.join("cfg");
    let proj_path = dir.join("proj.ccraft");
    let (course, media) = minimal_course_with_rich_media("D13.1 Initial");

    let mut h = AppHarness::new(course.clone(), Some(cfg.clone()));
    for (name, bytes) in &media {
        h.app.editor.media.push(name.clone(), bytes.clone().into());
    }
    h.app.test_save_as_path = Some(proj_path.clone());
    assert!(h.app.perform_save_as());
    assert_eq!(h.app.project.path.as_deref(), Some(proj_path.as_path()));
    assert_eq!(h.app.project.saved_revision, h.app.editor.session.revision);

    // Edit title and save again.
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "D13.1 Edited" }),
    );
    assert!(h.app.project.is_dirty(&h.app.editor.session));
    assert!(h.app.perform_save());
    assert!(!h.app.project.is_dirty(&h.app.editor.session));

    // Zero leftover .tmp files beside proj.ccraft.
    let leftover_tmp: Vec<String> = names_in(&dir)
        .into_iter()
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(
        leftover_tmp.is_empty(),
        "leftover tmp files: {leftover_tmp:?}"
    );

    // Reopen in fresh EditorApp.
    let mut h2 = AppHarness::new(initial_course(true), Some(cfg));
    h2.app
        .finish_open(&proj_path, Some(proj_path.clone()), false);
    assert_eq!(h2.app.editor.session.course.title, "D13.1 Edited");
    assert_eq!(h2.app.editor.session.course.fonts.len(), 1);
    assert_eq!(h2.app.editor.session.course.fonts[0].family, "CourseCustom");
    assert_eq!(
        h2.app.project.saved_revision,
        h2.app.editor.session.revision
    );
    for (name, bytes) in &media {
        let got = h2
            .app
            .editor
            .media
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.as_ref());
        assert_eq!(got, Some(bytes.as_slice()), "media mismatch for {name}");
    }
}

#[test]
fn d13_2_folder_project_incremental_save_and_reopen_parity() {
    let dir = scratch_dir("d13-2-folder-incremental");
    let cfg = dir.join("cfg");
    let folder = dir.join("proj-dir");
    let (course, media) = minimal_course_with_rich_media("D13.2 Folder");

    let mut h = AppHarness::new(course, Some(cfg.clone()));
    for (name, bytes) in &media {
        h.app.editor.media.push(name.clone(), bytes.clone().into());
    }
    h.app.test_save_as_folder_path = Some(folder.clone());
    assert!(h.app.perform_save_as_folder());
    assert!(h.app.project.is_folder());
    assert!(folder.join("course.json").is_file());

    let hero_path = folder.join("media/hero.png");
    let mtime_before = std::fs::metadata(&hero_path).unwrap().modified().unwrap();

    // Sleep briefly so any rewrite would visibly advance filesystem mtime.
    std::thread::sleep(Duration::from_millis(50));

    // Edit title only; media untouched.
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "D13.2 Updated" }),
    );
    assert!(h.app.perform_save());

    let mtime_after = std::fs::metadata(&hero_path).unwrap().modified().unwrap();
    assert_eq!(
        mtime_before, mtime_after,
        "unchanged media file in folder project must not be rewritten"
    );

    // Reopen from folder in fresh EditorApp.
    let mut h2 = AppHarness::new(initial_course(true), Some(cfg));
    h2.app.finish_open(&folder, Some(folder.clone()), false);
    assert!(h2.app.project.is_folder());
    assert_eq!(h2.app.editor.session.course.title, "D13.2 Updated");
    let (disk_course, disk_media) = open_folder_from(&folder).unwrap().0;
    assert_eq!(disk_course.title, "D13.2 Updated");
    assert_eq!(disk_media.len(), media.len());
}

#[test]
fn d13_3_save_as_failure_preserves_project_path_folder_and_stamp() {
    let dir = scratch_dir("d13-3-save-as-failure");
    let cfg = dir.join("cfg");
    let good_path = dir.join("good.ccraft");
    let (course, media) = minimal_course_with_rich_media("Good Project");
    save_to(&good_path, &course, &media).unwrap();

    let mut h = AppHarness::new(initial_course(true), Some(cfg));
    h.app
        .finish_open(&good_path, Some(good_path.clone()), false);
    let orig_stamp = h.app.project.disk_stamp;
    assert!(orig_stamp.is_some());
    assert!(!h.app.project.is_folder());

    // Create a regular file to act as an impossible parent directory.
    let blocker = dir.join("not-a-dir");
    std::fs::write(&blocker, b"blocker").unwrap();

    // Attempt Save As targeting a path under `blocker`.
    h.app.test_save_as_path = Some(blocker.join("bad.ccraft"));
    assert!(
        !h.app.perform_save_as(),
        "Save As to invalid parent must fail"
    );
    assert!(
        h.app.error_modal.is_some(),
        "Save As failure should set error_modal"
    );
    assert_eq!(h.app.project.path.as_deref(), Some(good_path.as_path()));
    assert!(!h.app.project.is_folder());
    assert_eq!(h.app.project.disk_stamp, orig_stamp);
    h.app.error_modal = None;

    // Attempt Save As Folder targeting a path under `blocker`.
    h.app.test_save_as_folder_path = Some(blocker.join("bad-dir"));
    assert!(
        !h.app.perform_save_as_folder(),
        "Save As Folder to invalid parent must fail"
    );
    assert!(h.app.error_modal.is_some());
    assert_eq!(h.app.project.path.as_deref(), Some(good_path.as_path()));
    assert!(!h.app.project.is_folder());
    assert_eq!(h.app.project.disk_stamp, orig_stamp);
}

#[test]
fn d13_4_open_paths_preserve_preferences_autosave_interval() {
    let dir = scratch_dir("d13-4-autosave-interval");
    let cfg = dir.join("cfg");
    let proj_path = dir.join("sample.ccraft");
    let course = Course::from_json(HELLO_JSON).unwrap();
    save_to(&proj_path, &course, &[]).unwrap();

    for minutes in [0u32, 5u32] {
        let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
        h.app.prefs.autosave_minutes = minutes;
        h.app.autosaver.interval_minutes = Some(minutes);

        // 1. finish_open
        h.app
            .finish_open(&proj_path, Some(proj_path.clone()), false);
        assert_eq!(
            h.app.autosaver.interval_minutes,
            Some(minutes),
            "finish_open must preserve autosave_minutes={minutes}"
        );

        // 2. PendingAction::OpenExample
        h.app
            .execute_pending_action(PendingAction::OpenExample(ExampleId::HealthyHabits), &h.ctx);
        assert_eq!(
            h.app.autosaver.interval_minutes,
            Some(minutes),
            "OpenExample must preserve autosave_minutes={minutes}"
        );

        // 3. PendingAction::OpenTemplate
        h.app
            .execute_pending_action(PendingAction::OpenTemplate(TemplateId::Blank), &h.ctx);
        assert_eq!(
            h.app.autosaver.interval_minutes,
            Some(minutes),
            "OpenTemplate must preserve autosave_minutes={minutes}"
        );
    }
}

#[test]
fn d13_5_untitled_autosave_write_restore_discard_and_cleanup_on_save() {
    let dir = scratch_dir("d13-5-untitled-autosave");
    let cfg = dir.join("cfg");
    let untitled_auto = autosave_path_for_project(None, &cfg);

    let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Recovered Untitled" }),
    );
    assert!(h.app.project.is_dirty(&h.app.editor.session));

    // Advance virtual clock past 60s and run frame to trigger update_autosave.
    h.advance_time(65.0);
    h.idle_frame();
    h.wait_for_autosave();
    assert!(
        untitled_auto.is_file(),
        "untitled autosave should be written"
    );

    // The window that wrote it ends (its process id is gone); a fresh window then
    // detects the autosave and shows restore_autosave_modal -> Discard.
    let untitled_auto = end_session(&untitled_auto, &cfg);
    {
        let mut h_discard = AppHarness::new(initial_course(true), Some(cfg.clone()));
        h_discard.app.startup(None, false);
        assert!(h_discard.app.restore_prompt.is_some());
        h_discard.click_button("Discard");
        assert!(
            !untitled_auto.exists(),
            "Discard should delete untitled autosave"
        );
        assert!(h_discard.app.restore_prompt.is_none());
    }

    // Re-create untitled autosave and test Restore + explicit Save cleanup.
    h.advance_time(65.0);
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Recovered Untitled 2" }),
    );
    h.idle_frame();
    h.wait_for_autosave();
    let own_auto = autosave_path_for_project(None, &cfg);
    assert!(own_auto.is_file());
    let untitled_auto = end_session(&own_auto, &cfg);

    let mut h_restore = AppHarness::new(initial_course(true), Some(cfg));
    h_restore.app.startup(None, false);
    assert!(h_restore.app.restore_prompt.is_some());
    h_restore.click_button("Restore");
    assert!(h_restore.app.restore_prompt.is_none());
    assert_eq!(
        h_restore.app.editor.session.course.title,
        "Recovered Untitled 2"
    );
    assert!(
        h_restore
            .app
            .project
            .is_dirty(&h_restore.app.editor.session)
    );

    // Explicit Save removes untitled autosave.
    let saved_path = dir.join("saved-untitled.ccraft");
    h_restore.app.test_save_as_path = Some(saved_path);
    assert!(h_restore.app.perform_save_as());
    assert!(!untitled_auto.exists());
    assert!(!own_auto.exists());
}

/// Rename an untitled autosave as if the window that wrote it had ended.
fn end_session(auto: &Path, cfg: &Path) -> PathBuf {
    let ended = crate::autosave::untitled_autosave_path_for(cfg, 2_000_000_000, 1);
    std::fs::rename(auto, &ended).unwrap();
    ended
}

#[test]
fn d13_6_named_and_folder_autosave_recovery() {
    let dir = scratch_dir("d13-6-named-folder-autosave");
    let cfg = dir.join("cfg");

    // Part A: Named .ccraft
    let named = dir.join("named.ccraft");
    let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Named Saved" }),
    );
    h.app.test_save_as_path = Some(named.clone());
    assert!(h.app.perform_save_as());

    std::thread::sleep(Duration::from_millis(50));
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Named Dirty" }),
    );
    h.advance_time(65.0);
    h.idle_frame();
    h.wait_for_autosave();
    let named_auto = autosave_path_for_project(Some(&named), &cfg);
    assert!(named_auto.is_file());

    let mut h2 = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h2.app.initiate_open(named.clone());
    assert!(h2.app.restore_prompt.is_some());
    h2.click_button("Restore");
    assert_eq!(h2.app.project.saved_revision, u64::MAX);
    assert_eq!(h2.app.editor.session.course.title, "Named Dirty");
    assert!(h2.app.perform_save());
    assert!(!named_auto.exists());
    assert_eq!(open_from(&named).unwrap().0.title, "Named Dirty");

    // Part B: Folder project
    let folder = dir.join("folder-proj");
    let mut hf = AppHarness::new(initial_course(true), Some(cfg.clone()));
    hf.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Folder Saved" }),
    );
    hf.app.test_save_as_folder_path = Some(folder.clone());
    assert!(hf.app.perform_save_as_folder());

    std::thread::sleep(Duration::from_millis(50));
    hf.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Folder Dirty" }),
    );
    hf.advance_time(65.0);
    hf.idle_frame();
    hf.wait_for_autosave();
    let folder_auto = autosave_path_for_project(Some(&folder), &cfg);
    assert!(folder_auto.is_file());

    let mut hf2 = AppHarness::new(initial_course(true), Some(cfg));
    hf2.app.initiate_open(folder.clone());
    assert!(hf2.app.restore_prompt.is_some());
    hf2.click_button("Restore");
    assert_eq!(hf2.app.project.saved_revision, u64::MAX);
    assert!(hf2.app.project.is_folder());
    assert_eq!(hf2.app.editor.session.course.title, "Folder Dirty");
    assert!(hf2.app.perform_save());
    assert!(!folder_auto.exists());
    assert_eq!(open_folder_from(&folder).unwrap().0.0.title, "Folder Dirty");
}

#[test]
fn d13_7_changed_on_disk_conflict_modal_for_ccraft_and_folder() {
    let dir = scratch_dir("d13-7-changed-on-disk");
    let cfg = dir.join("cfg");

    // Part A: .ccraft
    let conflict = dir.join("conflict.ccraft");
    let mut c_init = Course::from_json(HELLO_JSON).unwrap();
    c_init.title = "Original".into();
    save_to(&conflict, &c_init, &[]).unwrap();

    let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h.app.finish_open(&conflict, Some(conflict.clone()), false);

    // External modification on disk
    std::thread::sleep(Duration::from_millis(50));
    let mut c_ext = c_init.clone();
    c_ext.title = "External Edit".into();
    save_to(&conflict, &c_ext, &[]).unwrap();

    // In-app edit and Save -> triggers changed_on_disk_modal
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Local Edit" }),
    );
    assert!(!h.app.perform_save());
    assert!(h.app.conflict_prompt.is_some());

    // 1. Cancel keeps external file intact and dismisses modal
    h.click_button("Cancel");
    assert!(h.app.conflict_prompt.is_none());
    assert_eq!(open_from(&conflict).unwrap().0.title, "External Edit");

    // 2. Save a copy leaves external file intact
    assert!(!h.app.perform_save());
    assert!(h.app.conflict_prompt.is_some());
    let copy_path = dir.join("conflict-copy.ccraft");
    h.app.test_save_as_path = Some(copy_path.clone());
    h.click_button("Save a copy");
    assert!(h.app.conflict_prompt.is_none());
    assert_eq!(open_from(&conflict).unwrap().0.title, "External Edit");
    assert_eq!(open_from(&copy_path).unwrap().0.title, "Local Edit");

    // 3. Reopen original, modify externally, and Overwrite
    h.app.finish_open(&conflict, Some(conflict.clone()), false);
    std::thread::sleep(Duration::from_millis(50));
    save_to(&conflict, &c_ext, &[]).unwrap();
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Overwritten" }),
    );
    assert!(!h.app.perform_save());
    assert!(h.app.conflict_prompt.is_some());
    h.click_button("Overwrite");
    assert!(h.app.conflict_prompt.is_none());
    assert_eq!(open_from(&conflict).unwrap().0.title, "Overwritten");

    // Part B: Folder project (course.json modified externally)
    let folder = dir.join("conflict-folder");
    let mut hf = AppHarness::new(c_init, Some(cfg));
    hf.app.test_save_as_folder_path = Some(folder.clone());
    assert!(hf.app.perform_save_as_folder());

    std::thread::sleep(Duration::from_millis(50));
    let ext_json = format!("{}\n", c_ext.to_json().unwrap());
    std::fs::write(folder.join("course.json"), ext_json).unwrap();

    hf.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Folder Local" }),
    );
    assert!(!hf.app.perform_save());
    assert!(hf.app.conflict_prompt.is_some());
    let folder_copy = dir.join("conflict-folder-copy");
    hf.app.test_save_as_folder_path = Some(folder_copy.clone());
    hf.click_button("Save a copy");
    assert!(hf.app.conflict_prompt.is_none());
    assert_eq!(
        open_folder_from(&folder).unwrap().0.0.title,
        "External Edit"
    );
    assert_eq!(
        open_folder_from(&folder_copy).unwrap().0.0.title,
        "Folder Local"
    );
}

#[test]
fn d13_8_unsaved_changes_modal_save_dont_save_cancel() {
    let dir = scratch_dir("d13-8-unsaved-changes");
    let cfg = dir.join("cfg");
    let proj = dir.join("unsaved.ccraft");

    let mut h = AppHarness::new(initial_course(true), Some(cfg));
    h.app.test_save_as_path = Some(proj.clone());
    assert!(h.app.perform_save_as());

    // Make dirty -> trigger New -> assert unsaved_changes_modal renders cleanly -> Cancel
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Edited Before New" }),
    );
    h.app.handle_request(EditorRequest::New, &h.ctx);
    assert!(h.app.pending_action.is_some());
    h.idle_frame();
    h.assert_foreground_texts_unclipped(true);
    h.click_button("Cancel");
    assert!(h.app.pending_action.is_none());
    assert_eq!(h.app.editor.session.course.title, "Edited Before New");

    // Trigger New -> Don't Save -> discards edits and opens blank
    h.app.handle_request(EditorRequest::New, &h.ctx);
    assert!(h.app.pending_action.is_some());
    h.click_button("Don't Save");
    assert!(h.app.pending_action.is_none());
    assert!(h.app.project.path.is_none());
    assert_eq!(h.app.editor.session.course.title, "Untitled course");

    // Reopen proj, dirty it, trigger Open -> Save -> saves then executes Open
    let other = dir.join("other.ccraft");
    let mut other_c = Course::from_json(HELLO_JSON).unwrap();
    other_c.title = "Other Course".into();
    save_to(&other, &other_c, &[]).unwrap();

    h.app.finish_open(&proj, Some(proj.clone()), false);
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Saved Before Open" }),
    );
    h.app.test_open_path = Some(other);
    h.app.handle_request(EditorRequest::Open, &h.ctx);
    assert!(h.app.pending_action.is_some());
    h.click_button("Save");
    assert!(h.app.pending_action.is_none());
    assert_eq!(open_from(&proj).unwrap().0.title, "Saved Before Open");
    assert_eq!(h.app.editor.session.course.title, "Other Course");

    // Dirty and trigger Quit -> Save -> sets allow_close
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Saved Before Quit" }),
    );
    h.app.guard_unsaved(PendingAction::Quit, &h.ctx);
    assert!(h.app.pending_action.is_some());
    h.click_button("Save");
    assert!(h.app.allow_close);
}

#[test]
fn d13_9_version_history_lifecycle_dedup_prune_keep_zero_and_ui() {
    let dir = scratch_dir("d13-9-version-lifecycle");
    let cfg = dir.join("cfg");
    let proj = dir.join("versions.ccraft");

    let prefs = Prefs {
        versions_to_keep: 10,
        ..Prefs::default()
    };
    let _ = save_prefs(&cfg, &prefs);

    let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h.app.test_save_as_path = Some(proj.clone());

    // Save 1: no snapshot yet
    h.app
        .editor
        .run("course.set_title", serde_json::json!({ "title": "v0" }));
    assert!(h.app.perform_save_as());
    assert_eq!(versions::list_versions(&cfg, &proj).len(), 0);

    // Edit + Save 2: 1 snapshot ("v0")
    h.app
        .editor
        .run("course.set_title", serde_json::json!({ "title": "v1" }));
    assert!(h.app.perform_save());
    assert_eq!(versions::list_versions(&cfg, &proj).len(), 1);

    // Save 3 with identical bytes: snapshot_with on unchanged proj.ccraft -> 2nd call is deduped
    let snap2 = versions::snapshot_with(
        &cfg,
        &proj,
        10,
        "v1",
        versions::CAP_BYTES,
        SystemTime::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert!(snap2.is_some());
    let v_before = versions::list_versions(&cfg, &proj).len();
    let dedup = versions::snapshot_with(
        &cfg,
        &proj,
        10,
        "v1",
        versions::CAP_BYTES,
        SystemTime::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert!(dedup.is_none(), "identical bytes must be deduplicated");
    assert_eq!(versions::list_versions(&cfg, &proj).len(), v_before);

    // Edit + save 12 more times with keep = 10 -> pruned to 10
    for i in 2..14 {
        h.app.editor.run(
            "course.set_title",
            serde_json::json!({ "title": format!("v{i}") }),
        );
        assert!(h.app.perform_save());
    }
    let list = versions::list_versions(&cfg, &proj);
    assert_eq!(list.len(), 10, "versions should be pruned to keep=10");

    // Open Version History window and check UI rows render cleanly
    h.app
        .handle_request(EditorRequest::ShowVersionHistory, &h.ctx);
    assert!(h.app.editor.ui.versions.open);
    h.idle_frame();
    h.assert_foreground_texts_unclipped(false);
    let texts = h.all_texts();
    assert!(texts.iter().any(|t| t == "Open as copy"));
    assert!(texts.iter().any(|t| t == "Delete"));

    // keep == 0 creates no new snapshots
    let snap_zero = versions::snapshot_with(
        &cfg,
        &proj,
        0,
        "none",
        versions::CAP_BYTES,
        SystemTime::now() + Duration::from_secs(60),
    )
    .unwrap();
    assert!(snap_zero.is_none());
}

#[test]
fn d13_10_version_history_open_as_copy_and_delete_cache_invalidation() {
    let dir = scratch_dir("d13-10-open-copy-delete-cache");
    let cfg = dir.join("cfg");
    let proj = dir.join("proj.ccraft");

    let mut h = AppHarness::new(initial_course(true), Some(cfg.clone()));
    h.app.test_save_as_path = Some(proj.clone());
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "First Title" }),
    );
    assert!(h.app.perform_save_as());

    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Second Title" }),
    );
    assert!(h.app.perform_save());

    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Current Title" }),
    );
    assert!(h.app.perform_save());

    // Populate VERSION_CACHE by listing versions
    let listed = versions::list_versions(&cfg, &proj);
    assert_eq!(listed.len(), 2);
    let newest_path = listed[0].path.clone();
    assert!(
        versions::is_cached(&newest_path),
        "list_versions should populate VERSION_CACHE"
    );

    // Open Version History window
    h.app
        .handle_request(EditorRequest::ShowVersionHistory, &h.ctx);
    h.idle_frame();

    // Click Delete on the newest row (index 0) -> removes file AND invalidates VERSION_CACHE
    h.click_button_index("Delete", 0);
    assert!(
        !newest_path.exists(),
        "deleted version file must be removed"
    );
    assert!(
        !versions::is_cached(&newest_path),
        "BUG-D9: deleting a version must invalidate VERSION_CACHE"
    );
    assert_eq!(versions::list_versions(&cfg, &proj).len(), 1);

    // Click Open as copy on remaining version ("First Title") -> untitled dirty copy, original untouched
    h.click_button_index("Open as copy", 0);
    assert!(h.app.project.path.is_none());
    assert!(h.app.project.is_dirty(&h.app.editor.session));
    assert!(
        h.app
            .editor
            .session
            .course
            .title
            .starts_with("First Title (restored ")
    );
    assert_eq!(open_from(&proj).unwrap().0.title, "Current Title");
}

#[test]
fn d13_11_damaged_and_future_schema_open_and_latest_version_offer() {
    let dir = scratch_dir("d13-11-damaged-and-future");
    let cfg = dir.join("cfg");
    let corrupt = dir.join("corrupt.ccraft");

    let mut c1 = Course::from_json(HELLO_JSON).unwrap();
    c1.title = "Healthy Snapshot".into();
    save_to(&corrupt, &c1, &[]).unwrap();
    versions::snapshot_before_save(&cfg, &corrupt, 5, &c1.title).unwrap();

    // Truncate corrupt.ccraft on disk
    std::fs::write(&corrupt, b"PK\x03\x04truncated").unwrap();

    let mut h = AppHarness::new(initial_course(true), Some(cfg));
    h.app.finish_open(&corrupt, Some(corrupt.clone()), false);
    assert!(h.app.error_modal.is_some());
    assert!(h.app.open_latest_offer.is_some());

    h.idle_frame();
    let texts = h.all_texts();
    assert!(texts.iter().any(|t| t == "Open latest version"));
    h.click_button("Open latest version");
    assert!(h.app.error_modal.is_none());
    assert!(h.app.open_latest_offer.is_none());
    assert!(h.app.project.path.is_none());
    assert!(
        h.app
            .editor
            .session
            .course
            .title
            .starts_with("Healthy Snapshot (restored ")
    );

    // (b) Future schema_version: 99 -> plain "made with a newer CourseCraft" message
    let future_json = dir.join("future.json");
    std::fs::write(
        &future_json,
        r#"{"schema_version": 99, "id": "f", "title": "Future", "scenes": []}"#,
    )
    .unwrap();
    h.app
        .finish_open(&future_json.clone(), Some(future_json), false);
    let err = h.app.error_modal.as_deref().unwrap_or("");
    assert!(
        err.contains("made with a newer CourseCraft"),
        "expected plain newer-CourseCraft message, got: {err}"
    );
}

#[test]
fn d13_12_rapid_20x_save_burst_and_stale_temp_and_verify_guard() {
    let dir = scratch_dir("d13-12-rapid-burst");
    let cfg = dir.join("cfg");
    let proj = dir.join("burst.ccraft");

    let mut h = AppHarness::new(initial_course(true), Some(cfg));
    h.app.test_save_as_path = Some(proj.clone());
    assert!(h.app.perform_save_as());

    for i in 1..=20 {
        h.app.editor.run(
            "course.set_title",
            serde_json::json!({ "title": format!("Burst {i}") }),
        );
        assert!(h.app.perform_save(), "save {i} failed");
    }
    assert_eq!(open_from(&proj).unwrap().0.title, "Burst 20");

    // Plant a stale .tmp-<dead_pid> beside proj and verify remove_stale_temps cleans it
    let stale_tmp = dir.join("burst.ccraft.tmp-999999");
    std::fs::write(&stale_tmp, b"partial").unwrap();
    assert!(stale_tmp.exists());
    remove_stale_temps(&proj);
    assert!(
        !stale_tmp.exists(),
        "stale temp from dead pid should be removed"
    );
    assert_eq!(open_from(&proj).unwrap().0.title, "Burst 20");

    // verify_ccraft_file blocks replacing good save if newly written bytes fail validation
    let res = write_atomic_with(
        &proj,
        b"not a valid ccraft zip",
        Some(&verify_ccraft_file),
        &Hooks::default(),
    );
    assert!(res.is_err(), "verify_ccraft_file must reject invalid zip");
    assert_eq!(
        open_from(&proj).unwrap().0.title,
        "Burst 20",
        "last known-good file must remain intact"
    );
}

/// L-05: App shell modal buttons use sentence case while keeping platform-standard `"Don't Save"`.
#[test]
fn app_shell_modals_use_sentence_case_and_keep_dont_save() {
    let dir = scratch_dir("l05-app-shell-modals");
    let cfg = dir.join("cfg");
    let proj = dir.join("modal-copy.ccraft");

    let mut c_init = Course::from_json(HELLO_JSON).unwrap();
    c_init.title = "Initial".into();
    let mut h = AppHarness::new(c_init.clone(), Some(cfg.clone()));
    h.app.test_save_as_path = Some(proj.clone());
    assert!(h.app.perform_save_as());

    // 1. Unsaved-changes modal keeps platform-standard "Don't Save".
    h.app.editor.run(
        "course.set_title",
        serde_json::json!({ "title": "Unsaved Edit" }),
    );
    h.app.handle_request(EditorRequest::New, &h.ctx);
    h.idle_frame();
    let unsaved_texts = h.all_texts();
    assert!(
        unsaved_texts.iter().any(|t| t == "Don't Save"),
        "unsaved-changes modal must keep platform-standard \"Don't Save\": {unsaved_texts:?}"
    );
    h.click_button("Cancel");

    // 2. Conflict modal shows sentence-case "Save a copy" (never "Save a Copy").
    std::thread::sleep(Duration::from_millis(50));
    let mut c_ext = Course::from_json(HELLO_JSON).unwrap();
    c_ext.title = "External".into();
    save_to(&proj, &c_ext, &[]).unwrap();
    assert!(!h.app.perform_save());
    assert!(h.app.conflict_prompt.is_some());
    h.idle_frame();
    let conflict_texts = h.all_texts();
    assert!(
        conflict_texts.iter().any(|t| t == "Save a copy"),
        "conflict modal must show 'Save a copy': {conflict_texts:?}"
    );
    assert!(
        !conflict_texts.iter().any(|t| t == "Save a Copy"),
        "conflict modal must not show Title Case 'Save a Copy'"
    );
    h.click_button("Cancel");

    // 3. Error modal with a kept version shows sentence-case "Open latest version".
    let corrupt = dir.join("corrupt-l05.ccraft");
    save_to(&corrupt, &c_init, &[]).unwrap();
    versions::snapshot_before_save(&cfg, &corrupt, 5, &c_init.title).unwrap();
    std::fs::write(&corrupt, b"PK\x03\x04truncated").unwrap();
    h.app.finish_open(&corrupt, Some(corrupt.clone()), false);
    assert!(h.app.error_modal.is_some());
    assert!(h.app.open_latest_offer.is_some());
    h.idle_frame();
    let error_texts = h.all_texts();
    assert!(
        error_texts.iter().any(|t| t == "Open latest version"),
        "error modal must show 'Open latest version': {error_texts:?}"
    );
    assert!(
        !error_texts.iter().any(|t| t == "Open Latest Version"),
        "error modal must not show Title Case 'Open Latest Version'"
    );
}
