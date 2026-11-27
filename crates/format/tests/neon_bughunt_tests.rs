//! Bug-hunt proof tests for `coursecraft-format` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use std::io::Write as _;

use coursecraft_course::{Course, ObjectKind};
use coursecraft_format::xliff::{apply_xliff, export_xliff, parse_xliff};
use coursecraft_format::{Limits, verify_project_with_limits};

/// `drop_gutter` strips every whitespace-only text segment outside `<g>` tags
/// whenever any `<g>` tag is present, destroying intentional spaces between
/// formatted runs (`<g id="r0">Click</g> <g id="r1">here</g>` -> `"Clickhere"`).
#[test]
fn xliff_drop_gutter_destroys_space_between_g_tags() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "xliff-space",
          "title": "XLIFF Space",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "t1",
                "kind": {
                  "type": "text",
                  "text": "Click here",
                  "runs": [
                    { "text": "Click", "bold": true },
                    { "text": " " },
                    { "text": "here", "italic": true }
                  ]
                },
                "frame": { "x": 10, "y": 10, "w": 200, "h": 40 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let exported = export_xliff(&course, "fr").expect("export");
    // Locate the trans-unit for t1 and provide a CAT-tool target with a space
    // between two <g> tags.
    let unit_id = exported
        .lines()
        .find(|l| l.contains("t1") && l.contains("trans-unit"))
        .and_then(|l| l.split("id=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("trans-unit id for t1");

    let translated_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="xliff-space" source-language="en" target-language="fr" datatype="plaintext">
    <body>
      <trans-unit id="{unit_id}">
        <source><g id="r0">Click</g><g id="r1"> </g><g id="r2">here</g></source>
        <target><g id="r0">Cliquez</g> <g id="r2">ici</g></target>
      </trans-unit>
    </body>
  </file>
</xliff>"#
    );

    let doc = parse_xliff(translated_xml.as_bytes()).expect("parse xliff");
    let (updated, report) = apply_xliff(&course, &doc);
    assert_eq!(report.applied, 1, "unit should apply: {report:?}");
    let ObjectKind::Text { text, .. } = &updated.scenes[0].slides[0].objects[0].kind else {
        panic!("expected text object");
    };
    assert_eq!(
        text, "Cliquez ici",
        "space between <g> tags was silently deleted, producing {text:?}"
    );
}

/// `reject_declarations` unconditionally rejects any XLIFF containing `]]>`,
/// breaking all valid `<![CDATA[...]]>` sections even though `parse_xliff`
/// has an `Event::CData` handler.
#[test]
fn xliff_rejects_valid_cdata_section() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="c1" source-language="en" target-language="fr" datatype="plaintext">
    <body>
      <trans-unit id="course.title">
        <source><![CDATA[Hello & Welcome]]></source>
        <target><![CDATA[Bonjour & Bienvenue]]></target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    let res = parse_xliff(xml.as_bytes());
    assert!(
        res.is_ok(),
        "valid XLIFF with CDATA section should parse cleanly, got: {res:?}"
    );
}

/// `verify_project_with_limits` skips path-safety checks (`is_safe_relative_path`)
/// and duplicate entry checks that `open_project_from_with_limits` enforces, so
/// archives containing unsafe `../` paths pass verification.
#[test]
fn verify_project_accepts_unsafe_traversal_path() {
    let course_json = r#"{
      "schema_version": 1,
      "id": "verify-path",
      "title": "Verify Path",
      "scenes": [{
        "id": "sc1",
        "title": "Scene 1",
        "slides": [{
          "id": "s1",
          "title": "Slide 1",
          "duration_ms": 5000
        }]
      }]
    }"#;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("course.json", opts).expect("zip");
        zip.write_all(course_json.as_bytes()).expect("write");
        zip.start_file("media/../../escape.txt", opts).expect("zip");
        zip.write_all(b"evil").expect("write");
        zip.finish().expect("finish");
    }
    let res = verify_project_with_limits(std::io::Cursor::new(buf.into_inner()), Limits::default());
    assert!(
        res.is_err(),
        "verify_project_with_limits must reject unsafe '../' entry paths, got Ok"
    );
}

/// N78 (b): a deflated media entry far larger than the entry limit is refused
/// with `EntryTooLarge` and is never fully inflated.
#[test]
fn verify_project_refuses_deflate_bomb_entry() {
    let course_json = r#"{"schema_version":1,"id":"v","title":"V","scenes":[{"id":"sc1","title":"S","slides":[{"id":"s1","title":"T","duration_ms":5000}]}]}"#;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("course.json", opts).expect("zip");
        zip.write_all(course_json.as_bytes()).expect("write");
        zip.start_file("media/bomb.bin", opts).expect("zip");
        let zeros = vec![0u8; 1 << 20];
        for _ in 0..16 {
            zip.write_all(&zeros).expect("write");
        }
        zip.finish().expect("finish");
    }
    let limits = Limits {
        max_entry_bytes: 1 << 20,
        ..Limits::default()
    };
    let res = verify_project_with_limits(std::io::Cursor::new(buf.into_inner()), limits);
    assert!(
        matches!(res, Err(coursecraft_format::FormatError::EntryTooLarge(_))),
        "got {res:?}"
    );
}
