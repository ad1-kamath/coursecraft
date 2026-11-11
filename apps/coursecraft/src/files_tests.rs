//! Tests for safe saving: crash safety, verification, locked files, files that
//! changed on disk, damaged files and leftover temp files.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::files::{
    Hooks, LOCKED_MESSAGE, Project, open_from, remove_stale_temps, save_to, write_atomic_with,
};
use coursecraft_course::Course;

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn course_titled(title: &str) -> Course {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.title = title.to_string();
    course
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

/// The child half of the kill test. Does nothing in a normal run; when the
/// parent sets `KILL_CHILD_DIR` it saves again and again until it is killed.
#[test]
fn kill_child_writer() {
    let Some(dir) = std::env::var_os("KILL_CHILD_DIR") else {
        return;
    };
    let target = PathBuf::from(dir).join("p.ccraft");
    let mut n: u32 = 1;
    loop {
        let media = vec![("a.png".to_string(), vec![n as u8; 3000])];
        let _ = save_to(&target, &course_titled(&format!("v{n}")), &media);
        n = n.wrapping_add(1);
    }
}

#[test]
fn a_killed_save_never_damages_the_project() {
    let dir = scratch_dir("test-kill-save");
    let target = dir.join("p.ccraft");
    save_to(
        &target,
        &course_titled("v0"),
        &[("a.png".to_string(), vec![0u8; 3000])],
    )
    .unwrap();

    let exe = std::env::current_exe().unwrap();
    let mut seed: u64 = 0x9E37_79B9;
    let mut newest: u32 = 0;
    for round in 0..20 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let delay = 40 + (seed >> 33) % 90;
        let mut child = std::process::Command::new(&exe)
            .args(["--exact", "files_tests::kill_child_writer", "--nocapture"])
            .env("KILL_CHILD_DIR", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(delay));
        let _ = child.kill();
        let _ = child.wait();

        let (course, media) =
            open_from(&target).unwrap_or_else(|e| panic!("round {round}: target damaged: {e}"));
        let n: u32 = course.title.strip_prefix('v').unwrap().parse().unwrap();
        newest = newest.max(n);
        assert_eq!(media.len(), 1, "round {round}");
        assert_eq!(
            media[0].1,
            vec![n as u8; 3000],
            "round {round}: media matches {n}"
        );
    }

    assert!(
        newest > 0,
        "the child process must have completed some saves"
    );

    // The next successful save clears whatever temp files the kills left.
    save_to(&target, &course_titled("final"), &[]).unwrap();
    assert_eq!(names_in(&dir), vec!["p.ccraft".to_string()]);
}

#[test]
fn a_broken_temp_file_never_replaces_the_old_file() {
    let dir = scratch_dir("test-verify-refuses");
    let target = dir.join("p.ccraft");
    save_to(
        &target,
        &course_titled("old"),
        &[("a.png".to_string(), vec![1u8; 500])],
    )
    .unwrap();
    let before = std::fs::read(&target).unwrap();

    // Save new bytes, but flip one byte inside the stored picture on disk
    // after the write, as a failing disk or a sync client might.
    let media = vec![("a.png".to_string(), vec![7u8; 5000])];
    let bytes = coursecraft_format::save_project(&course_titled("new"), &media).unwrap();
    let corrupt = |tmp: &Path| -> std::io::Result<()> {
        let mut data = std::fs::read(tmp)?;
        let run = vec![7u8; 100];
        let at = data
            .windows(run.len())
            .position(|w| w == run.as_slice())
            .expect("stored picture bytes are in the file");
        data[at + 50] ^= 0xFF;
        std::fs::write(tmp, data)
    };
    let hooks = Hooks {
        after_write: &corrupt,
        ..Hooks::default()
    };
    let err = write_atomic_with(
        &target,
        &bytes,
        Some(&crate::files::verify_ccraft_file),
        &hooks,
    )
    .expect_err("the check must refuse a damaged copy");
    assert!(err.to_string().contains("existing file was kept"), "{err}");
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert_eq!(names_in(&dir), vec!["p.ccraft".to_string()]);
}

#[test]
fn a_locked_file_is_retried_until_the_lock_clears() {
    let dir = scratch_dir("test-lock-clears");
    let target = dir.join("p.ccraft");
    std::fs::write(&target, b"old").unwrap();
    let calls = Cell::new(0u32);
    let rename = |from: &Path, to: &Path| -> std::io::Result<()> {
        calls.set(calls.get() + 1);
        if calls.get() <= 3 {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            std::fs::rename(from, to)
        }
    };
    let hooks = Hooks {
        rename: &rename,
        retry_delay: Duration::ZERO,
        ..Hooks::default()
    };
    write_atomic_with(&target, b"new", None, &hooks).unwrap();
    assert_eq!(calls.get(), 4);
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
}

#[test]
fn a_file_that_stays_locked_gives_a_plain_message_and_keeps_the_old_file() {
    let dir = scratch_dir("test-lock-stays");
    let target = dir.join("p.ccraft");
    std::fs::write(&target, b"old").unwrap();
    let calls = Cell::new(0u32);
    let rename = |_: &Path, _: &Path| -> std::io::Result<()> {
        calls.set(calls.get() + 1);
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    };
    let hooks = Hooks {
        rename: &rename,
        retry_delay: Duration::ZERO,
        ..Hooks::default()
    };
    let err = write_atomic_with(&target, b"new", None, &hooks).unwrap_err();
    assert_eq!(err.to_string(), LOCKED_MESSAGE);
    assert_eq!(calls.get(), 5, "five tries in all");
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert_eq!(names_in(&dir), vec!["p.ccraft".to_string()]);
}

#[test]
fn a_change_made_on_disk_is_noticed() {
    let dir = scratch_dir("test-changed-on-disk");
    let path = dir.join("p.ccraft");
    save_to(&path, &course_titled("mine"), &[]).unwrap();

    let mut project = Project::new(0);
    assert!(!project.changed_on_disk(), "untitled is never in conflict");
    project.path = Some(path.clone());
    assert!(!project.changed_on_disk(), "no stamp yet");
    project.record_disk_stamp();
    assert!(!project.changed_on_disk());

    // Another machine's edit arrives through the sync service.
    save_to(&path, &course_titled("theirs, with more text"), &[]).unwrap();
    assert!(project.changed_on_disk());

    // Our own save records the new state and clears the conflict.
    project.record_disk_stamp();
    assert!(!project.changed_on_disk());

    // A deleted file is not a conflict: saving just recreates it.
    std::fs::remove_file(&path).unwrap();
    assert!(!project.changed_on_disk());
}

#[test]
fn save_asks_before_overwriting_a_changed_file_and_overwrite_saves() {
    let dir = scratch_dir("test-conflict-flow");
    let path = dir.join("p.ccraft");
    save_to(&path, &course_titled("first"), &[]).unwrap();

    let mut app = crate::EditorApp::new_desktop(course_titled("first"), None);
    app.project.path = Some(path.clone());
    app.project.record_disk_stamp();
    app.editor.session.course.title = "edited here".into();

    save_to(&path, &course_titled("edited elsewhere"), &[]).unwrap();
    let theirs = std::fs::read(&path).unwrap();

    assert!(!app.perform_save(), "save is held back");
    assert!(app.conflict_prompt.is_some());
    assert_eq!(std::fs::read(&path).unwrap(), theirs, "file untouched");

    assert!(app.save_project_file(true), "Overwrite saves");
    assert_eq!(open_from(&path).unwrap().0.title, "edited here");
    assert!(!app.project.changed_on_disk());
}

#[test]
fn autosave_never_prompts_and_never_touches_the_project_file() {
    let dir = scratch_dir("test-autosave-safe");
    let path = dir.join("p.ccraft");
    save_to(&path, &course_titled("mine"), &[]).unwrap();
    let mut project = Project::new(0);
    project.path = Some(path.clone());
    project.record_disk_stamp();
    let before = std::fs::read(&path).unwrap();

    let auto = crate::autosave::autosave_path_for_project(Some(&path), Path::new(""));
    crate::autosave::write_autosave(&course_titled("unsaved"), &[], &auto).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!project.changed_on_disk());
    assert_eq!(open_from(&auto).unwrap().0.title, "unsaved");
}

#[test]
fn damaged_project_files_give_a_plain_error_and_stay_untouched() {
    let dir = scratch_dir("test-damaged");
    let good = dir.join("good.ccraft");
    save_to(
        &good,
        &course_titled("ok"),
        &[("a.png".to_string(), vec![5u8; 4000])],
    )
    .unwrap();
    let full = std::fs::read(&good).unwrap();

    let mut random = vec![0u8; 3000];
    let mut x: u64 = 0xDEAD_BEEF;
    for b in &mut random {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        *b = (x >> 40) as u8;
    }
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty.ccraft", Vec::new()),
        ("truncated.ccraft", full[..full.len() / 2].to_vec()),
        ("tiny.ccraft", full[..10].to_vec()),
        ("random.ccraft", random),
        ("bad.json", b"{ not json".to_vec()),
    ];
    for (name, bytes) in cases {
        let path = dir.join(name);
        std::fs::write(&path, &bytes).unwrap();
        let err = open_from(&path).expect_err(name);
        assert!(err.contains("can't be opened"), "{name}: {err}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name} untouched");
    }
}

#[test]
fn a_dead_process_temp_file_is_removed_at_the_next_save_and_others_stay() {
    let dir = scratch_dir("test-stale-temps");
    let path = dir.join("p.ccraft");
    save_to(&path, &course_titled("a"), &[]).unwrap();

    // A process id far above any real one: nobody is running it.
    std::fs::write(dir.join("p.ccraft.tmp-999999999"), b"half").unwrap();
    // Not a pid suffix, another project's temp, and one of our own: all stay.
    std::fs::write(dir.join("p.ccraft.tmp-notes"), b"keep").unwrap();
    std::fs::write(dir.join("other.ccraft.tmp-999999999"), b"keep").unwrap();
    let own = format!("p.ccraft.tmp-{}", std::process::id());
    std::fs::write(dir.join(&own), b"live").unwrap();

    // Opening ignores them.
    assert_eq!(open_from(&path).unwrap().0.title, "a");
    assert!(dir.join("p.ccraft.tmp-999999999").exists());

    remove_stale_temps(&path);
    assert!(!dir.join("p.ccraft.tmp-999999999").exists());
    assert!(dir.join("p.ccraft.tmp-notes").exists());
    assert!(dir.join("other.ccraft.tmp-999999999").exists());
    assert!(dir.join(&own).exists());
}

// ---------------------------------------------------------------- folders

fn media_items() -> Vec<(String, Vec<u8>)> {
    vec![
        ("logo.png".to_string(), vec![1u8; 40]),
        ("fonts/f.ttf".to_string(), vec![2u8; 30]),
    ]
}

#[test]
fn a_folder_project_saves_opens_and_notices_outside_changes_by_name() {
    use crate::files::{open_folder_from, save_folder_to};
    use coursecraft_format::FolderRecord;

    let dir = scratch_dir("test-folder-project");
    let path = dir.join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save_folder_to(&path, &course_titled("mine"), &media_items(), &mut record).unwrap();

    let ((course, media), opened_record) = open_folder_from(&path).unwrap();
    assert_eq!(course.title, "mine");
    assert_eq!(media.len(), 2);
    let mut project = Project::new(0);
    project.path = Some(path.clone());
    project.folder = Some(opened_record);
    assert!(project.is_folder());
    assert!(!project.changed_on_disk());

    // A teammate replaces media/logo.png through git.
    std::fs::write(path.join("media/logo.png"), b"their new logo, longer").unwrap();
    assert!(project.changed_on_disk());
    assert_eq!(project.changed_files(), vec!["media/logo.png".to_string()]);

    // open_from reads a folder too, for places that need no record.
    assert_eq!(open_from(&path).unwrap().0.title, "mine");
}

#[test]
fn saving_a_folder_project_asks_first_and_keeps_no_versions() {
    use crate::files::save_folder_to;
    use coursecraft_format::FolderRecord;

    let dir = scratch_dir("test-folder-conflict-flow");
    let path = dir.join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save_folder_to(&path, &course_titled("first"), &media_items(), &mut record).unwrap();

    let mut app = crate::EditorApp::new_desktop(course_titled("first"), None);
    let config = dir.join("config");
    std::fs::create_dir_all(&config).unwrap();
    app.config_dir = Some(config.clone());
    app.finish_open(&path, Some(path.clone()), false);
    assert!(app.project.is_folder());
    app.editor.session.course.title = "edited here".into();

    std::fs::write(path.join("media/logo.png"), b"teammate's logo, new size").unwrap();
    let theirs = std::fs::read(path.join("media/logo.png")).unwrap();

    assert!(!app.perform_save(), "save is held back");
    let prompt = app.conflict_prompt.clone().expect("prompt");
    assert_eq!(prompt.files, vec!["media/logo.png".to_string()]);
    let sentence = crate::changed_files_message("p.ccraft-folder", &prompt.files);
    assert!(sentence.contains("media/logo.png"), "{sentence}");
    assert_eq!(std::fs::read(path.join("media/logo.png")).unwrap(), theirs);
    assert!(!open_from(&path).unwrap().0.title.contains("edited"));

    assert!(app.save_project_file(true), "Overwrite saves");
    assert_eq!(open_from(&path).unwrap().0.title, "edited here");
    assert!(!app.project.changed_on_disk());
    assert!(!config.join("versions").exists(), "git is the history");
    assert!(app.project.is_folder());
}

#[test]
fn many_changed_files_are_listed_with_a_count() {
    let files: Vec<String> = (0..12).map(|n| format!("media/{n}.png")).collect();
    let text = crate::changed_files_message("p", &files);
    assert!(text.contains("media/0.png") && text.contains("media/7.png"));
    assert!(!text.contains("media/8.png"));
    assert!(text.contains("and 4 more"), "{text}");
}

#[test]
fn a_too_big_folder_gets_the_same_plain_sentence_as_a_zip() {
    let zip = crate::files::plain_error(&coursecraft_format::FormatError::TooLarge);
    let folder = crate::files::plain_error(&coursecraft_format::FormatError::EntryTooLarge(
        "media/x.mp4".into(),
    ));
    assert!(zip.starts_with("this project is too big to open again"));
    assert!(folder.starts_with("this project is too big to open again"));
    assert!(folder.contains("up to 4 GB"));
}
