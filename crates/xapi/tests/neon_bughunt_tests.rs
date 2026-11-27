//! Bug-hunt proof tests for `coursecraft-xapi` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_xapi::{Flavor, LaunchParams};

/// `LaunchParams::parse` for `Flavor::Xapi` keeps empty `registration=` and
/// `activity_id=` query parameters as `Some("")` instead of `None`, preventing
/// fallback to `activity_id_for` and emitting invalid empty IDs to the LRS.
#[test]
fn xapi_launch_keeps_empty_registration_and_activity_id() {
    let query = "endpoint=https%3A%2F%2Flrs.example.invalid%2Fxapi&actor=%7B%22mbox%22%3A%22mailto%3Aa%40b.invalid%22%7D&auth=Basic%20dGVzdA%3D%3D&registration=&activity_id=";
    let params = LaunchParams::parse(query, Flavor::Xapi).expect("parse xapi launch");
    assert_eq!(
        params.activity_id, None,
        "empty activity_id query param should be treated as absent (None), got {:?}",
        params.activity_id
    );
    assert_eq!(
        params.registration, None,
        "empty registration query param should be treated as absent (None), got {:?}",
        params.registration
    );
}
