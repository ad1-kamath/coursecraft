//! SCORM packaging vocabulary for CourseCraft.
//!
//! Implemented from the public ADL SCORM 1.2 specification (Content Aggregation
//! Model + Run-Time Environment). Standalone L0 crate: depends on nothing else in
//! the workspace, so it can be published and reused by other tools.
//!
//! Scope today: a single-SCO SCORM 1.2 `imsmanifest.xml` and the `cmi.core`
//! status vocabulary the player reports through. SCORM 2004, xAPI and cmi5 are
//! roadmap items (`docs/roadmap.md`).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::fmt::Write as _;

/// Errors from manifest generation. Every invalid input is an error, never a panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScormError {
    #[error("manifest identifier must be non-empty and contain only [A-Za-z0-9_.-], got {0:?}")]
    BadIdentifier(String),
    #[error("course title must not be empty")]
    EmptyTitle,
    #[error("launch file must be a relative path inside the package, got {0:?}")]
    BadPath(String),
    #[error("mastery score must be 0..=100, got {0}")]
    BadMasteryScore(u8),
}

/// A SCORM standard a package can target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Standard {
    /// SCORM 1.2, the default for packages without a `coursecraft-package.json`.
    #[default]
    Scorm12,
    /// SCORM 2004 4th Edition.
    Scorm2004,
}

impl Standard {
    /// The stable identifier written into `coursecraft-package.json`.
    #[must_use]
    pub fn as_id(self) -> &'static str {
        match self {
            Self::Scorm12 => "scorm12",
            Self::Scorm2004 => "scorm2004",
        }
    }

    /// Parse the stable identifier from `coursecraft-package.json`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "scorm12" => Some(Self::Scorm12),
            "scorm2004" => Some(Self::Scorm2004),
            _ => None,
        }
    }
}

/// `cmi.core.lesson_status` values (SCORM 1.2 RTE §3.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonStatus {
    Passed,
    Completed,
    Failed,
    Incomplete,
    Browsed,
    NotAttempted,
}

impl LessonStatus {
    /// The exact vocabulary string the LMS expects.
    #[must_use]
    pub fn as_cmi(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Incomplete => "incomplete",
            Self::Browsed => "browsed",
            Self::NotAttempted => "not attempted",
        }
    }
}

/// What goes into a single-SCO SCORM 1.2 package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSpec {
    /// Manifest identifier, e.g. `coursecraft.hello-course`.
    pub identifier: String,
    pub title: String,
    /// Entry point relative to the package root, e.g. `index.html`.
    pub launch: String,
    /// Every file in the package (relative paths), launch file included.
    pub files: Vec<String>,
    /// `adlcp:masteryscore` (0..=100). `None` = completion-only course.
    pub mastery_score: Option<u8>,
}

fn valid_identifier(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn valid_relative_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains("://")
        && p.split('/').all(|seg| !seg.is_empty() && seg != "..")
}

/// Escape text for XML element content and attribute values.
#[must_use]
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 forbids most C0 controls; drop them rather than emit an invalid file.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn validate_spec(spec: &ManifestSpec) -> Result<(), ScormError> {
    if !valid_identifier(&spec.identifier) {
        return Err(ScormError::BadIdentifier(spec.identifier.clone()));
    }
    if spec.title.trim().is_empty() {
        return Err(ScormError::EmptyTitle);
    }
    for p in std::iter::once(&spec.launch).chain(spec.files.iter()) {
        if !valid_relative_path(p) {
            return Err(ScormError::BadPath(p.clone()));
        }
    }
    if let Some(m) = spec.mastery_score
        && m > 100
    {
        return Err(ScormError::BadMasteryScore(m));
    }
    Ok(())
}

fn sorted_files(spec: &ManifestSpec) -> Vec<&str> {
    let mut files: Vec<&str> = spec.files.iter().map(String::as_str).collect();
    if !files.contains(&spec.launch.as_str()) {
        files.push(&spec.launch);
    }
    files.sort_unstable();
    files.dedup();
    files
}

/// Build a SCORM 1.2 `imsmanifest.xml` for a single-SCO package.
///
/// # Errors
/// Returns a [`ScormError`] for an invalid identifier, empty title, unsafe path
/// or out-of-range mastery score.
pub fn manifest_scorm12(spec: &ManifestSpec) -> Result<String, ScormError> {
    validate_spec(spec)?;

    let id = xml_escape(&spec.identifier);
    let title = xml_escape(&spec.title);
    let launch = xml_escape(&spec.launch);

    let files = sorted_files(spec);

    // `write!` into a String cannot fail; the results are ignored deliberately.
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        x,
        "<manifest identifier=\"{id}\" version=\"1.0\"\n  xmlns=\"http://www.imsproject.org/xsd/imscp_rootv1p1p2\"\n  xmlns:adlcp=\"http://www.adlnet.org/xsd/adlcp_rootv1p2\"\n  xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n  xsi:schemaLocation=\"http://www.imsproject.org/xsd/imscp_rootv1p1p2 imscp_rootv1p1p2.xsd http://www.imsglobal.org/xsd/imsmd_rootv1p2p1 imsmd_rootv1p2p1.xsd http://www.adlnet.org/xsd/adlcp_rootv1p2 adlcp_rootv1p2.xsd\">"
    );
    x.push_str("  <metadata>\n    <schema>ADL SCORM</schema>\n    <schemaversion>1.2</schemaversion>\n  </metadata>\n");
    let _ = writeln!(x, "  <organizations default=\"{id}_org\">");
    let _ = writeln!(x, "    <organization identifier=\"{id}_org\">");
    let _ = writeln!(x, "      <title>{title}</title>");
    let _ = writeln!(
        x,
        "      <item identifier=\"{id}_item\" identifierref=\"{id}_res\" isvisible=\"true\">"
    );
    let _ = writeln!(x, "        <title>{title}</title>");
    if let Some(m) = spec.mastery_score {
        let _ = writeln!(x, "        <adlcp:masteryscore>{m}</adlcp:masteryscore>");
    }
    x.push_str("      </item>\n    </organization>\n  </organizations>\n  <resources>\n");
    let _ = writeln!(
        x,
        "    <resource identifier=\"{id}_res\" type=\"webcontent\" adlcp:scormtype=\"sco\" href=\"{launch}\">"
    );
    for f in files {
        let _ = writeln!(x, "      <file href=\"{}\"/>", xml_escape(f));
    }
    x.push_str("    </resource>\n  </resources>\n</manifest>\n");
    Ok(x)
}

/// Build a SCORM 2004 4th Edition `imsmanifest.xml` for a single-SCO package.
///
/// CourseCraft reports success itself, so the manifest carries no
/// `adlcp:masteryscore` and no `<imsss:sequencing>` rules.
///
/// # Errors
/// Returns a [`ScormError`] for an invalid identifier, empty title, unsafe path
/// or out-of-range mastery score.
pub fn manifest_scorm2004(spec: &ManifestSpec) -> Result<String, ScormError> {
    validate_spec(spec)?;

    let id = xml_escape(&spec.identifier);
    let title = xml_escape(&spec.title);
    let launch = xml_escape(&spec.launch);
    let files = sorted_files(spec);

    // `write!` into a String cannot fail; the results are ignored deliberately.
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        x,
        "<manifest identifier=\"{id}\" version=\"1.0\"\n  xmlns=\"http://www.imsglobal.org/xsd/imscp_v1p1\"\n  xmlns:adlcp=\"http://www.adlnet.org/xsd/adlcp_v1p3\"\n  xmlns:adlseq=\"http://www.adlnet.org/xsd/adlseq_v1p3\"\n  xmlns:adlnav=\"http://www.adlnet.org/xsd/adlnav_v1p3\"\n  xmlns:imsss=\"http://www.imsglobal.org/xsd/imsss\"\n  xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n  xsi:schemaLocation=\"http://www.imsglobal.org/xsd/imscp_v1p1 imscp_v1p1.xsd http://www.adlnet.org/xsd/adlcp_v1p3 adlcp_v1p3.xsd\">"
    );
    x.push_str("  <metadata>\n    <schema>ADL SCORM</schema>\n    <schemaversion>2004 4th Edition</schemaversion>\n  </metadata>\n");
    let _ = writeln!(x, "  <organizations default=\"{id}_org\">");
    let _ = writeln!(x, "    <organization identifier=\"{id}_org\">");
    let _ = writeln!(x, "      <title>{title}</title>");
    let _ = writeln!(
        x,
        "      <item identifier=\"{id}_item\" identifierref=\"{id}_res\" isvisible=\"true\">"
    );
    let _ = writeln!(x, "        <title>{title}</title>");
    x.push_str("      </item>\n    </organization>\n  </organizations>\n  <resources>\n");
    let _ = writeln!(
        x,
        "    <resource identifier=\"{id}_res\" type=\"webcontent\" adlcp:scormType=\"sco\" href=\"{launch}\">"
    );
    for f in files {
        let _ = writeln!(x, "      <file href=\"{}\"/>", xml_escape(f));
    }
    x.push_str("    </resource>\n  </resources>\n</manifest>\n");
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ManifestSpec {
        ManifestSpec {
            identifier: "coursecraft.hello".into(),
            title: "Safety & You <Basics>".into(),
            launch: "index.html".into(),
            files: vec!["player.wasm".into(), "course.json".into()],
            mastery_score: Some(80),
        }
    }

    #[test]
    fn manifest_contains_required_parts() {
        let xml = manifest_scorm12(&spec()).unwrap();
        assert!(xml.contains("<schemaversion>1.2</schemaversion>"));
        assert!(xml.contains("adlcp:scormtype=\"sco\" href=\"index.html\""));
        assert!(xml.contains("<adlcp:masteryscore>80</adlcp:masteryscore>"));
        assert!(xml.contains("Safety &amp; You &lt;Basics&gt;"));
        // launch file is listed even though the caller forgot it
        assert!(xml.contains("<file href=\"index.html\"/>"));
        assert_eq!(xml.matches("<file href=").count(), 3);
    }

    #[test]
    fn completion_only_course_has_no_mastery_score() {
        let mut s = spec();
        s.mastery_score = None;
        assert!(!manifest_scorm12(&s).unwrap().contains("masteryscore"));
    }

    #[test]
    fn rejects_hostile_input() {
        let mut s = spec();
        s.identifier = "bad id\"".into();
        assert!(matches!(
            manifest_scorm12(&s),
            Err(ScormError::BadIdentifier(_))
        ));

        for bad in [
            "../escape.html",
            "/abs.html",
            "a//b",
            "http://x/y",
            "a\\b",
            "",
        ] {
            let mut s = spec();
            s.launch = bad.into();
            assert!(
                matches!(manifest_scorm12(&s), Err(ScormError::BadPath(_))),
                "{bad}"
            );
        }

        let mut s = spec();
        s.mastery_score = Some(101);
        assert_eq!(manifest_scorm12(&s), Err(ScormError::BadMasteryScore(101)));

        let mut s = spec();
        s.title = "   ".into();
        assert_eq!(manifest_scorm12(&s), Err(ScormError::EmptyTitle));
    }

    #[test]
    fn escape_drops_control_chars() {
        assert_eq!(xml_escape("a\u{0}b\tc"), "ab\tc");
    }

    #[test]
    fn lesson_status_vocabulary() {
        assert_eq!(LessonStatus::NotAttempted.as_cmi(), "not attempted");
        assert_eq!(LessonStatus::Passed.as_cmi(), "passed");
    }

    #[test]
    fn standard_ids_round_trip() {
        assert_eq!(Standard::Scorm12.as_id(), "scorm12");
        assert_eq!(Standard::Scorm2004.as_id(), "scorm2004");
        assert_eq!(Standard::from_id("scorm12"), Some(Standard::Scorm12));
        assert_eq!(Standard::from_id("scorm2004"), Some(Standard::Scorm2004));
        assert_eq!(Standard::from_id("scorm1.2"), None);
        assert_eq!(Standard::default(), Standard::Scorm12);
    }

    #[test]
    fn scorm2004_manifest_contains_required_parts() {
        let xml = manifest_scorm2004(&spec()).unwrap();
        assert!(xml.contains("<schema>ADL SCORM</schema>"));
        assert!(xml.contains("<schemaversion>2004 4th Edition</schemaversion>"));
        assert!(xml.contains("http://www.imsglobal.org/xsd/imscp_v1p1"));
        assert!(xml.contains("http://www.adlnet.org/xsd/adlcp_v1p3"));
        assert!(xml.contains("http://www.adlnet.org/xsd/adlseq_v1p3"));
        assert!(xml.contains("http://www.adlnet.org/xsd/adlnav_v1p3"));
        assert!(xml.contains("http://www.imsglobal.org/xsd/imsss"));
        assert!(xml.contains("adlcp:scormType=\"sco\" href=\"index.html\""));
        assert!(xml.contains("Safety &amp; You &lt;Basics&gt;"));
        assert!(xml.contains("<file href=\"index.html\"/>"));
        assert_eq!(xml.matches("<file href=").count(), 3);
        assert!(!xml.contains("masteryscore"));
        assert!(!xml.contains("sequencing"));
        assert!(!xml.contains("scormtype"));
    }

    #[test]
    fn scorm2004_manifest_rejects_hostile_input() {
        let mut s = spec();
        s.identifier = "bad id\"".into();
        assert!(matches!(
            manifest_scorm2004(&s),
            Err(ScormError::BadIdentifier(_))
        ));

        let mut s = spec();
        s.launch = "../escape.html".into();
        assert!(matches!(
            manifest_scorm2004(&s),
            Err(ScormError::BadPath(_))
        ));

        let mut s = spec();
        s.mastery_score = Some(101);
        assert_eq!(
            manifest_scorm2004(&s),
            Err(ScormError::BadMasteryScore(101))
        );

        let mut s = spec();
        s.title = "   ".into();
        assert_eq!(manifest_scorm2004(&s), Err(ScormError::EmptyTitle));
    }
}
