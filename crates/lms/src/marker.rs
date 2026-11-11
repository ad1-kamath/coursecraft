//! Reading the package marker that names a course's reporting standard.
//!
//! A missing `coursecraft-package.json` means SCORM 1.2. Anything that cannot
//! be understood also falls back to SCORM 1.2, with one warning for the host to
//! log. The marker never contains learner data, so warnings may quote the id.

use coursecraft_scorm::Standard;
use coursecraft_xapi::Flavor;

/// The reporting standard a package declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportingStandard {
    /// SCORM 1.2 or SCORM 2004.
    Scorm(Standard),
    /// cmi5 or plain xAPI.
    Xapi(Flavor),
}

/// Read the reporting standard from `coursecraft-package.json`.
///
/// A missing file means SCORM 1.2 without a warning. Bad JSON, a missing
/// string `standard` field or an unknown id means SCORM 1.2 with a warning
/// describing what was wrong.
#[must_use]
pub fn reporting_from_marker(text: Option<&str>) -> (ReportingStandard, Option<String>) {
    let Some(text) = text else {
        return (ReportingStandard::Scorm(Standard::Scorm12), None);
    };
    let parsed = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => value,
        Err(err) => {
            return (
                ReportingStandard::Scorm(Standard::Scorm12),
                Some(format!("coursecraft-package.json is not valid JSON: {err}")),
            );
        }
    };
    let Some(id) = parsed.get("standard").and_then(serde_json::Value::as_str) else {
        return (
            ReportingStandard::Scorm(Standard::Scorm12),
            Some("coursecraft-package.json has no string standard; assuming scorm12".into()),
        );
    };
    if let Some(standard) = Standard::from_id(id) {
        return (ReportingStandard::Scorm(standard), None);
    }
    if let Some(flavor) = Flavor::from_id(id) {
        return (ReportingStandard::Xapi(flavor), None);
    }
    (
        ReportingStandard::Scorm(Standard::Scorm12),
        Some(format!(
            "unknown coursecraft-package.json standard {id:?}; assuming scorm12"
        )),
    )
}
