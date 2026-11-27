//! Regression tests for the file, import, export and publish bugs found in the
//! 0.6.1 Neon review (N41 to N47, N57, N61, N121).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use coursecraft_cli::main_with_args;
use coursecraft_course::Course;
use coursecraft_format::{open_project_from, save_project_to};

static SEQ: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("coursecraft-io-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn run_cli(args: &[&str]) -> ExitCode {
    let argv: Vec<OsString> = std::iter::once("coursecraft-cli")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    main_with_args(argv)
}

fn s(path: &Path) -> &str {
    path.to_str().expect("utf8 path")
}

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn image_course(src: &str) -> Course {
    Course::from_json(&format!(
        r#"{{
          "schema_version": 1, "id": "io-fixes", "title": "Original",
          "scenes": [{{ "id": "sc1", "title": "Scene 1", "slides": [{{
            "id": "s1", "title": "Slide 1", "duration_ms": 5000,
            "objects": [{{ "id": "img1", "kind": {{ "type": "image", "src": "{src}" }},
              "frame": {{ "x": 10, "y": 10, "w": 100, "h": 100 }} }}]
          }}] }}]
        }}"#
    ))
    .expect("course")
}

fn write_ccraft(path: &Path, course: &Course, media: &[(String, &[u8])]) {
    let mut buf = std::io::Cursor::new(Vec::new());
    save_project_to(course, media, &mut buf).expect("save ccraft");
    std::fs::write(path, buf.into_inner()).expect("write ccraft");
}

fn open_ccraft(path: &Path) -> coursecraft_format::Project {
    open_project_from(std::io::Cursor::new(std::fs::read(path).expect("read"))).expect("opens")
}

fn make_player(dir: &Path) -> PathBuf {
    let player = dir.join("player");
    std::fs::create_dir_all(&player).expect("player");
    std::fs::write(player.join("index.html"), b"<html></html>").expect("index");
    std::fs::write(player.join("coursecraft-player.js"), b"// js").expect("js");
    std::fs::write(player.join("coursecraft-player_bg.wasm"), b"\0asm").expect("wasm");
    player
}

fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        false
    }
}

#[test]
fn new_writes_a_real_archive_for_a_ccraft_path_in_any_case() {
    let dir = temp_dir("new");
    let out = dir.join("blank.CCRAFT");
    assert_eq!(
        run_cli(&["new", s(&out), "--title", "Blank"]),
        ExitCode::SUCCESS
    );
    assert_eq!(open_ccraft(&out).course.title, "Blank");
}

#[test]
fn run_keeps_media_when_it_writes_a_ccraft_or_a_json() {
    let dir = temp_dir("run");
    let input = dir.join("in.ccraft");
    write_ccraft(
        &input,
        &image_course("pic.png"),
        &[("pic.png".to_owned(), PNG)],
    );
    let script = dir.join("script.json");
    std::fs::write(
        &script,
        r#"[{"command": "course.set_title", "params": {"title": "Updated"}}]"#,
    )
    .expect("script");

    let out_json = dir.join("json-out").join("course.json");
    assert_eq!(
        run_cli(&["run", s(&input), s(&script), "-o", s(&out_json)]),
        ExitCode::SUCCESS
    );
    assert_eq!(
        std::fs::read(dir.join("json-out/media/pic.png")).expect("media beside the json"),
        PNG
    );
    assert_eq!(run_cli(&["validate", s(&out_json)]), ExitCode::SUCCESS);
}

#[test]
fn publish_of_a_ccraft_does_not_look_for_media_beside_it() {
    let dir = temp_dir("publish");
    let input = dir.join("course.ccraft");
    write_ccraft(
        &input,
        &image_course("pic.png"),
        &[("pic.png".to_owned(), PNG)],
    );
    // A decoy next to the archive must not be used in place of the archive's own file.
    std::fs::create_dir_all(dir.join("media")).expect("decoy dir");
    std::fs::write(dir.join("media/pic.png"), b"decoy").expect("decoy");
    let player = make_player(&dir);
    let out = dir.join("pkg.zip");
    assert_eq!(
        run_cli(&["publish", s(&input), "-o", s(&out), "--player", s(&player)]),
        ExitCode::SUCCESS
    );
    let zip = std::fs::read(&out).expect("zip");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("zip opens");
    let mut found = Vec::new();
    {
        let mut file = archive.by_name("media/pic.png").expect("media in package");
        std::io::Read::read_to_end(&mut file, &mut found).expect("read");
    }
    assert_eq!(found, PNG);
}

#[test]
fn import_xliff_refuses_to_overwrite_its_input_and_keeps_media() {
    let dir = temp_dir("xliff");
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("media/images")).expect("media");
    std::fs::write(project.join("media/images/hero.png"), PNG).expect("hero");
    let course = image_course("images/hero.png");
    std::fs::write(project.join("course.json"), course.to_json().expect("json")).expect("course");
    let xlf = dir.join("fr.xlf");
    std::fs::write(
        &xlf,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" source-language="en" target-language="fr" datatype="plaintext">
    <body><trans-unit id="course/title"><source>Original</source><target>Titre</target></trans-unit></body>
  </file>
</xliff>"#,
    )
    .expect("xlf");

    // The input course.json, and its folder, are both refused.
    let input_json = project.join("course.json");
    let before = std::fs::read(&input_json).expect("before");
    assert_eq!(
        run_cli(&[
            "import-xliff",
            s(&input_json),
            s(&xlf),
            "-o",
            s(&input_json)
        ]),
        ExitCode::from(2)
    );
    assert_eq!(
        run_cli(&["import-xliff", s(&project), s(&xlf), "-o", s(&project)]),
        ExitCode::from(2)
    );
    assert_eq!(std::fs::read(&input_json).expect("after"), before);

    // A bare course.json with nested media works, and the nested key uses `/`.
    let out = dir.join("out.ccraft");
    let code = run_cli(&["import-xliff", s(&input_json), s(&xlf), "-o", s(&out)]);
    assert_eq!(code, ExitCode::SUCCESS);
    let reopened = open_ccraft(&out);
    assert_eq!(reopened.course.title, "Titre");
    assert!(
        reopened
            .media
            .iter()
            .any(|(name, _)| name == "images/hero.png"),
        "{:?}",
        reopened.media.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    // A `.json` output keeps its media beside it.
    let out_json = dir.join("copy").join("course.json");
    assert_eq!(
        run_cli(&["import-xliff", s(&input_json), s(&xlf), "-o", s(&out_json)]),
        ExitCode::SUCCESS
    );
    assert!(dir.join("copy/media/images/hero.png").is_file());
}

#[test]
fn media_folder_with_a_link_loop_does_not_hang() {
    let dir = temp_dir("loop");
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("media/images")).expect("media");
    std::fs::write(project.join("media/images/hero.png"), PNG).expect("hero");
    if !try_symlink_dir(&project.join("media"), &project.join("media/loop")) {
        eprintln!("skipped: this OS refused to create a directory symlink");
        return;
    }
    let course = image_course("images/hero.png");
    let json = project.join("course.json");
    std::fs::write(&json, course.to_json().expect("json")).expect("course");
    let out = dir.join("out.pdf");
    assert_eq!(
        run_cli(&["export-pdf", s(&json), "-o", s(&out)]),
        ExitCode::SUCCESS
    );
    assert!(out.is_file());
}

#[test]
fn publish_ignores_a_web_folder_link_that_leaves_media() {
    let dir = temp_dir("weblink");
    let project = dir.join("project");
    let outside = dir.join("outside");
    std::fs::create_dir_all(project.join("media/web")).expect("media");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(outside.join("index.html"), b"<html>secret</html>").expect("secret");
    if !try_symlink_dir(&outside, &project.join("media/web/site")) {
        eprintln!("skipped: this OS refused to create a directory symlink");
        return;
    }
    let course = Course::from_json(
        r#"{ "schema_version": 1, "id": "w", "title": "W",
          "scenes": [{ "id": "sc1", "title": "S", "slides": [{ "id": "s1", "title": "S",
            "duration_ms": 1000, "objects": [{ "id": "site",
              "kind": { "type": "web", "source": { "type": "folder", "path": "web/site" } },
              "frame": { "x": 0, "y": 0, "w": 100, "h": 100 } }] }] }] }"#,
    )
    .expect("course");
    let json = project.join("course.json");
    std::fs::write(&json, course.to_json().expect("json")).expect("course");
    let player = make_player(&dir);
    let out = dir.join("pkg.zip");
    let code = run_cli(&["publish", s(&json), "-o", s(&out), "--player", s(&player)]);
    assert_ne!(
        code,
        ExitCode::SUCCESS,
        "a web folder that leaves media/ must not be packed"
    );
}

#[test]
fn publish_out_dir_writes_the_unpacked_folder_for_lti() {
    let dir = temp_dir("lti");
    let json = dir.join("course.json");
    std::fs::write(&json, image_course("pic.png").to_json().expect("json")).expect("course");
    std::fs::create_dir_all(dir.join("media")).expect("media");
    std::fs::write(dir.join("media/pic.png"), PNG).expect("pic");
    let player = make_player(&dir);
    let published = dir.join("published");
    assert_eq!(
        run_cli(&[
            "publish",
            s(&json),
            "--out-dir",
            s(&published),
            "--standard",
            "lti",
            "--player",
            s(&player)
        ]),
        ExitCode::SUCCESS
    );
    assert!(published.join("index.html").is_file() || published.join("course.json").is_file());
}

#[test]
fn pack_creates_its_parent_folder() {
    let dir = temp_dir("pack");
    let project = dir.join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(
        project.join("course.json"),
        image_course("pic.png").to_json().expect("json"),
    )
    .expect("course");
    std::fs::create_dir_all(project.join("media")).expect("media");
    std::fs::write(project.join("media/pic.png"), PNG).expect("pic");
    let out = dir.join("deep/er/out.ccraft");
    assert_eq!(
        run_cli(&["pack", s(&project), "-o", s(&out)]),
        ExitCode::SUCCESS
    );
    assert_eq!(open_ccraft(&out).media.len(), 1);
}
