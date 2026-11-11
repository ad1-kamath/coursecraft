//! Tests for `batch`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use coursecraft_course::Course;
use coursecraft_format::{open_project, save_project};

use crate::batch::{BatchArgs, run_batch};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-old-picture-bytes";

fn scratch(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/batch-tests")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn script(dir: &Path, json: &str) -> PathBuf {
    let path = dir.join("script.json");
    std::fs::write(&path, json).unwrap();
    path
}

fn make(dir: &Path, name: &str, media: &[(String, Vec<u8>)]) -> PathBuf {
    let course = Course::from_json(HELLO).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, save_project(&course, media).unwrap()).unwrap();
    path
}

fn args(script: PathBuf, courses: Vec<PathBuf>) -> BatchArgs {
    BatchArgs {
        script,
        courses,
        dir: None,
        in_place: false,
        out_dir: None,
        publish: None,
        publish_dir: None,
        player: None,
        replace_media: Vec::new(),
        dry_run: false,
    }
}

fn title_of(path: &Path) -> String {
    open_project(&std::fs::read(path).unwrap())
        .unwrap()
        .course
        .title
}

fn failed(code: std::process::ExitCode) -> bool {
    format!("{code:?}") != format!("{:?}", std::process::ExitCode::SUCCESS)
}

const RENAME: &str = r#"[{"command":"course.set_title","params":{"title":"Renamed"}}]"#;

#[test]
fn a_failing_course_is_not_written_and_the_rest_are() {
    let dir = scratch("fail");
    let a = make(&dir, "a.ccraft", &[]);
    let b = make(&dir, "b.ccraft", &[]);
    let bad = dir.join("bad.ccraft");
    std::fs::write(&bad, b"not a zip").unwrap();
    let out = dir.join("out");
    let mut job = args(script(&dir, RENAME), vec![a, bad, b]);
    job.out_dir = Some(out.clone());
    let code = run_batch(&job).unwrap();
    assert!(failed(code), "one course failed, so the exit is 1");
    assert_eq!(title_of(&out.join("a.ccraft")), "Renamed");
    assert_eq!(title_of(&out.join("b.ccraft")), "Renamed");
    assert!(!out.join("bad.ccraft").exists());
}

#[test]
fn in_place_saves_changes_and_a_second_run_changes_nothing() {
    let dir = scratch("inplace");
    let a = make(&dir, "a.ccraft", &[]);
    let mut job = args(script(&dir, RENAME), vec![a.clone()]);
    job.in_place = true;
    assert!(!failed(run_batch(&job).unwrap()));
    assert_eq!(title_of(&a), "Renamed");
    let before = std::fs::read(&a).unwrap();
    assert!(!failed(run_batch(&job).unwrap()));
    assert_eq!(std::fs::read(&a).unwrap(), before, "no change, no write");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("batch-tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn dry_run_writes_nothing() {
    let dir = scratch("dry");
    let a = make(&dir, "a.ccraft", &[]);
    let before = std::fs::read(&a).unwrap();
    let mut job = args(script(&dir, RENAME), vec![a.clone()]);
    job.dry_run = true;
    assert!(!failed(run_batch(&job).unwrap()));
    assert_eq!(std::fs::read(&a).unwrap(), before);
}

#[test]
fn an_output_choice_is_required_and_exclusive() {
    let dir = scratch("choice");
    let a = make(&dir, "a.ccraft", &[]);
    let s = script(&dir, RENAME);
    assert!(run_batch(&args(s.clone(), vec![a.clone()])).is_err());
    let mut both = args(s, vec![a]);
    both.in_place = true;
    both.out_dir = Some(dir.join("o"));
    assert!(run_batch(&both).is_err());
}

#[test]
fn dir_takes_every_course_directly_inside() {
    let dir = scratch("dir");
    let inbox = dir.join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    make(&inbox, "one.ccraft", &[]);
    make(&inbox, "two.ccraft", &[]);
    std::fs::write(inbox.join("notes.txt"), "x").unwrap();
    let mut job = args(script(&dir, RENAME), Vec::new());
    job.dir = Some(inbox.clone());
    job.in_place = true;
    assert!(!failed(run_batch(&job).unwrap()));
    assert_eq!(title_of(&inbox.join("one.ccraft")), "Renamed");
    assert_eq!(title_of(&inbox.join("two.ccraft")), "Renamed");
}

#[test]
fn replace_media_swaps_bytes_for_the_same_kind_only() {
    let dir = scratch("media");
    let media = vec![("logo.png".to_owned(), PNG.to_vec())];
    let a = make(&dir, "a.ccraft", &media);
    let new_png = dir.join("new.png");
    std::fs::write(&new_png, b"\x89PNG\r\n\x1a\n-new-picture-bytes").unwrap();
    let wav = dir.join("sound.wav");
    std::fs::write(&wav, b"RIFF\0\0\0\0WAVEfmt ").unwrap();
    let s = script(&dir, "[]");

    let mut wrong = args(s.clone(), vec![a.clone()]);
    wrong.in_place = true;
    wrong.replace_media = vec![format!("logo.png={}", wav.display())];
    assert!(failed(run_batch(&wrong).unwrap()), "audio for an image");
    assert_eq!(
        open_project(&std::fs::read(&a).unwrap()).unwrap().media[0].1,
        PNG
    );

    let mut right = args(s, vec![a.clone()]);
    right.in_place = true;
    right.replace_media = vec![format!("logo.png={}", new_png.display())];
    assert!(!failed(run_batch(&right).unwrap()));
    let project = open_project(&std::fs::read(&a).unwrap()).unwrap();
    assert_eq!(project.media[0].0, "logo.png");
    assert!(project.media[0].1.ends_with(b"new-picture-bytes"));
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    found.sort();
    found
}

/// Engine command ids a text uses: every `"command": "x"` value, and every
/// backticked `group.name` whose group is a real command group and that is
/// not a file name.
fn command_ids(text: &str, groups: &std::collections::BTreeSet<&str>) -> Vec<String> {
    let mut ids = Vec::new();
    let quoted: Vec<&str> = text.split('"').collect();
    for window in quoted.windows(3) {
        if window[0] == "command" && window[1].trim() == ":" {
            ids.push(window[2].to_owned());
        }
    }
    for (i, part) in text.split('`').enumerate() {
        let Some((group, name)) = part.split_once('.') else {
            continue;
        };
        let word = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
        };
        let file = matches!(
            name,
            "json" | "csv" | "md" | "png" | "ccraft" | "xlf" | "xliff" | "txt" | "pdf" | "mp4"
        );
        if i % 2 == 1 && word(group) && word(name) && !file && groups.contains(group) {
            ids.push(part.to_owned());
        }
    }
    ids
}

#[test]
fn recipes_and_batch_examples_only_use_real_commands() {
    let registry = coursecraft_engine::registry();
    let known: std::collections::BTreeSet<&str> = registry.iter().map(|c| c.id).collect();
    let groups: std::collections::BTreeSet<&str> = known
        .iter()
        .filter_map(|id| id.split_once('.').map(|(g, _)| g))
        .collect();
    let mut checked = 0;
    let mut files = collect(&root().join("docs/recipes"), "md");
    files.extend(collect(&root().join("examples/batch"), "json"));
    assert!(files.len() >= 7, "recipes and batch examples exist");
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for id in command_ids(&text, &groups) {
            assert!(
                known.contains(id.as_str()),
                "{}: unknown command {id}",
                file.display()
            );
            checked += 1;
        }
    }
    assert!(checked >= 8, "the scan found the commands ({checked})");
}

#[test]
fn the_batch_examples_run() {
    let dir = scratch("examples");
    let a = make(&dir, "a.ccraft", &[("logo.png".to_owned(), PNG.to_vec())]);
    for name in ["rebrand", "policy-date", "set-logo"] {
        let script = root().join(format!("examples/batch/{name}.json"));
        let mut job = args(script, vec![a.clone()]);
        job.dry_run = true;
        assert!(!failed(run_batch(&job).unwrap()), "{name}");
    }
}
