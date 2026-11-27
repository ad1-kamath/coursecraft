//! Bug-hunt proof tests for `coursecraft-cli` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use std::ffi::OsString;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use coursecraft_cli::main_with_args;
use coursecraft_course::Course;
use coursecraft_format::{open_project_from, save_project_to};

static SEQ: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> std::path::PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "coursecraft-neon-cli-{name}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run_cli(args: &[&str]) -> ExitCode {
    let argv: Vec<OsString> = std::iter::once("coursecraft-cli")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    main_with_args(argv)
}

/// 1x1 valid PNG bytes.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// `coursecraft-cli run` writes plain JSON even when `-o` ends in `.ccraft`,
/// destroying embedded media and leaving an unopenable `.ccraft` file.
#[test]
fn cli_run_corrupts_ccraft_archive_and_drops_media() {
    let dir = temp_dir("run-ccraft");
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "cli-run-test",
          "title": "Original Title",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "img1",
                "kind": { "type": "image", "src": "pic.png" },
                "frame": { "x": 10, "y": 10, "w": 100, "h": 100 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let in_path = dir.join("in.ccraft");
    let out_path = dir.join("out.ccraft");
    let script_path = dir.join("script.json");

    let mut buf = std::io::Cursor::new(Vec::new());
    save_project_to(&course, &[("pic.png".to_owned(), TINY_PNG)], &mut buf).expect("save ccraft");
    std::fs::write(&in_path, buf.into_inner()).expect("write in.ccraft");
    std::fs::write(
        &script_path,
        r#"[{"command": "course.set_title", "params": {"title": "Updated Title"}}]"#,
    )
    .expect("write script");

    let code = run_cli(&[
        "run",
        in_path.to_str().expect("utf8"),
        script_path.to_str().expect("utf8"),
        "-o",
        out_path.to_str().expect("utf8"),
    ]);
    assert_eq!(code, ExitCode::SUCCESS);

    let out_bytes = std::fs::read(&out_path).expect("read out.ccraft");
    let reopened = open_project_from(std::io::Cursor::new(out_bytes));
    let _ = std::fs::remove_dir_all(&dir);

    let project = reopened.expect("out.ccraft must remain a valid .ccraft zip archive");
    assert!(
        project.media.iter().any(|(name, _)| name == "pic.png"),
        "embedded pic.png must be preserved in out.ccraft"
    );
}

/// `coursecraft-cli publish` on a `.ccraft` archive discards embedded archive
/// media and fails with `missing media file`.
#[test]
fn cli_publish_ccraft_discards_embedded_media() {
    let dir = temp_dir("pub-ccraft");
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "cli-pub-test",
          "title": "Publish Test",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "img1",
                "kind": { "type": "image", "src": "pic.png" },
                "frame": { "x": 10, "y": 10, "w": 100, "h": 100 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let in_path = dir.join("course.ccraft");
    let out_zip = dir.join("pkg.zip");
    let player_dir = dir.join("player");
    std::fs::create_dir_all(&player_dir).expect("mkdir player");
    std::fs::write(player_dir.join("index.html"), b"<html></html>").expect("index");
    std::fs::write(player_dir.join("coursecraft-player.js"), b"// js").expect("js");
    std::fs::write(player_dir.join("coursecraft-player_bg.wasm"), b"\0asm").expect("wasm");

    let mut buf = std::io::Cursor::new(Vec::new());
    save_project_to(&course, &[("pic.png".to_owned(), TINY_PNG)], &mut buf).expect("save ccraft");
    std::fs::write(&in_path, buf.into_inner()).expect("write ccraft");

    let code = run_cli(&[
        "publish",
        in_path.to_str().expect("utf8"),
        "-o",
        out_zip.to_str().expect("utf8"),
        "--player",
        player_dir.to_str().expect("utf8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        code,
        ExitCode::SUCCESS,
        "publishing a valid self-contained .ccraft archive with embedded media must succeed"
    );
}

/// `coursecraft-cli import-xliff` promises `-o` is `(never the input)`, yet
/// silently overwrites the input course file in place when `-o` matches input.
#[test]
fn cli_import_xliff_overwrites_input_file_in_place() {
    let dir = temp_dir("xliff-inplace");
    let course_path = dir.join("course.json");
    let xlf_path = dir.join("fr.xlf");
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "xliff-inplace",
          "title": "English Title",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000
            }]
          }]
        }"#,
    )
    .expect("valid course");
    std::fs::write(&course_path, course.to_json().expect("json")).expect("write course");

    let xlf = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="xliff-inplace" source-language="en" target-language="fr" datatype="plaintext">
    <body>
      <trans-unit id="course.title">
        <source>English Title</source>
        <target>Titre Francais</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    std::fs::write(&xlf_path, xlf).expect("write xlf");

    let code = run_cli(&[
        "import-xliff",
        course_path.to_str().expect("utf8"),
        xlf_path.to_str().expect("utf8"),
        "-o",
        course_path.to_str().expect("utf8"),
    ]);
    let after = std::fs::read_to_string(&course_path).expect("read after");
    let _ = std::fs::remove_dir_all(&dir);

    assert_ne!(
        code,
        ExitCode::SUCCESS,
        "import-xliff must refuse to overwrite its input file in place (input became: {after})"
    );
}
