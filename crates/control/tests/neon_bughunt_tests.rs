//! Bug-hunt proof tests for `coursecraft-control` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_control::{HeadlessHost, dispatch};
use coursecraft_course::Course;
use coursecraft_engine::Session;
use serde_json::json;

/// `course_pointer_value` omits `"tracking"`, so `course.get` with
/// `{"pointer": "/tracking"}` returns `NOT_FOUND` even when `course.tracking`
/// is set and present in `course.get {}`.
#[test]
fn course_get_pointer_tracking_returns_not_found() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "tracking-ptr",
          "title": "Tracking Ptr",
          "tracking": {
            "slides_viewed": { "mode": "all" }
          },
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

    let mut host = HeadlessHost::new(Session::new(course), None);
    let full = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "course.get",
            "params": { "token": "t" }
        }),
        "t",
    );
    assert!(
        full.get("result").and_then(|r| r.get("tracking")).is_some(),
        "full course.get includes tracking: {full}"
    );

    let ptr_res = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "course.get",
            "params": { "token": "t", "pointer": "/tracking" }
        }),
        "t",
    );
    assert!(
        ptr_res.get("error").is_none() && ptr_res.get("result").is_some(),
        "course.get with pointer /tracking failed even though tracking is set: {ptr_res}"
    );
}
