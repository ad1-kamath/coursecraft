//! Bug-hunt proof tests for `coursecraft-scorm` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_scorm::{ManifestSpec, manifest_scorm12, xml_escape};

/// `valid_relative_path` only rejects `"://"`, permitting `javascript:` URLs
/// and Windows drive-relative paths like `C:index.html` in `imsmanifest.xml`.
#[test]
fn scorm_manifest_accepts_javascript_launch_path() {
    let spec = ManifestSpec {
        identifier: "coursecraft.test".to_owned(),
        title: "Test".to_owned(),
        launch: "javascript:alert(1)".to_owned(),
        files: vec!["javascript:alert(1)".to_owned()],
        mastery_score: None,
    };
    let res = manifest_scorm12(&spec);
    assert!(
        res.is_err(),
        "manifest must reject javascript: URI as launch path, got Ok"
    );
}

/// `xml_escape` drops C0 control characters but lets XML 1.0 illegal
/// non-characters U+FFFE and U+FFFF pass through unmodified.
#[test]
fn scorm_xml_escape_strips_xml_illegal_fffe_ffff() {
    let escaped = xml_escape("Hello\u{FFFE}\u{FFFF}World");
    assert!(
        !escaped.contains('\u{FFFE}') && !escaped.contains('\u{FFFF}'),
        "XML 1.0 forbids U+FFFE and U+FFFF, but xml_escape returned {escaped:?}"
    );
}
