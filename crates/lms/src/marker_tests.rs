//! Tests for reading `coursecraft-package.json`.

use coursecraft_scorm::Standard;
use coursecraft_xapi::Flavor;

use crate::marker::{ReportingStandard, reporting_from_marker};

#[test]
fn missing_marker_is_scorm12_without_warning() {
    let (standard, warning) = reporting_from_marker(None);
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(warning.is_none());
}

#[test]
fn scorm12_marker_is_read() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":"scorm12"}"#));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(warning.is_none());
}

#[test]
fn scorm2004_marker_is_read() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":"scorm2004"}"#));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm2004));
    assert!(warning.is_none());
}

#[test]
fn cmi5_marker_is_read() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":"cmi5"}"#));
    assert_eq!(standard, ReportingStandard::Xapi(Flavor::Cmi5));
    assert!(warning.is_none());
}

#[test]
fn xapi_marker_is_read() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":"xapi"}"#));
    assert_eq!(standard, ReportingStandard::Xapi(Flavor::Xapi));
    assert!(warning.is_none());
}

#[test]
fn bad_json_falls_back_with_warning() {
    let (standard, warning) = reporting_from_marker(Some("{not json"));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(
        warning
            .as_deref()
            .is_some_and(|text| text.contains("not valid JSON"))
    );
}

#[test]
fn missing_standard_falls_back_with_warning() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"other":true}"#));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(
        warning
            .as_deref()
            .is_some_and(|text| text.contains("no string standard"))
    );
}

#[test]
fn non_string_standard_falls_back_with_warning() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":2004}"#));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(
        warning
            .as_deref()
            .is_some_and(|text| text.contains("no string standard"))
    );
}

#[test]
fn unknown_standard_falls_back_with_warning() {
    let (standard, warning) = reporting_from_marker(Some(r#"{"standard":"scorm1.2"}"#));
    assert_eq!(standard, ReportingStandard::Scorm(Standard::Scorm12));
    assert!(
        warning
            .as_deref()
            .is_some_and(|text| text.contains("unknown"))
    );
}
