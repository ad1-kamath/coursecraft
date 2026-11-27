//! Bug-hunt proof tests for `coursecraft-course` (0.6.1 Neon review).
//!
//! These tests started as ignored proofs of bugs; each now passes because the bug is fixed.

use coursecraft_course::{Course, Severity};

/// `Course::reports_result()` does not ignore disabled `submit_results` /
/// `complete_course` triggers, unlike `Slide::has_submit_results_trigger()`.
#[test]
fn reports_result_ignores_disabled_complete_course_trigger() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "disabled-report",
          "title": "Disabled Report",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "triggers": [{
                "disabled": true,
                "when": { "event": "timeline_ends" },
                "action": { "action": "complete_course", "status": "completed" }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid json");

    assert!(
        !course.reports_result(),
        "a disabled complete_course trigger will never fire and must not count as reporting a result"
    );
}

/// `check_object_kind` omits `check_outline` for `ObjectKind::Text` and
/// `ObjectKind::Image`, letting invalid outline colours and widths pass validation.
#[test]
fn validate_rejects_invalid_outline_on_text_and_image() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "bad-outline",
          "title": "Bad Outline",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "t1",
                  "kind": {
                    "type": "text",
                    "text": "Hello",
                    "outline": { "color": "not-a-color", "width": -5.0 }
                  },
                  "frame": { "x": 10, "y": 10, "w": 200, "h": 50 }
                },
                {
                  "id": "img1",
                  "kind": {
                    "type": "image",
                    "src": "pic.png",
                    "outline": { "color": "also-bad", "width": 99999.0 }
                  },
                  "frame": { "x": 10, "y": 80, "w": 200, "h": 100 }
                }
              ]
            }]
          }]
        }"#,
    )
    .expect("valid json");

    let errors: Vec<_> = course
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(
        errors.len() >= 2,
        "expected outline validation errors on both text and image objects, got: {errors:?}"
    );
}
