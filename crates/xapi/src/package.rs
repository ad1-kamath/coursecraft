//! cmi5 and Tin Can package manifests.

use std::fmt::Write as _;

/// What goes into a cmi5 or xAPI package manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    /// The course id; reduced to safe identifier characters.
    pub course_id: String,
    /// Course title, used for titles, names and descriptions.
    pub title: String,
    /// Passing percentage from 0 to 100.
    pub mastery_score: Option<u8>,
}

/// The stable activity id a course reports when the launch gives none.
#[must_use]
pub fn activity_id_for(course_id: &str) -> String {
    format!("urn:coursecraft:activity:{}", sanitize_id(course_id))
}

/// Build a cmi5 `cmi5.xml` course structure.
#[must_use]
pub fn cmi5_xml(spec: &PackageSpec) -> String {
    let course_id = format!("urn:coursecraft:course:{}", sanitize_id(&spec.course_id));
    let au_id = format!("urn:coursecraft:au:{}", sanitize_id(&spec.course_id));
    let title = xml_escape(&spec.title);
    let move_on = if spec.mastery_score.is_some() {
        "Passed"
    } else {
        "Completed"
    };

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(
        "<courseStructure xmlns=\"https://w3id.org/xapi/profiles/cmi5/v1/CourseStructure.xsd\">\n",
    );
    let _ = writeln!(x, "  <course id=\"{}\">", xml_escape(&course_id));
    let _ = writeln!(
        x,
        "    <title><langstring lang=\"en-US\">{title}</langstring></title>"
    );
    let _ = writeln!(
        x,
        "    <description><langstring lang=\"en-US\">{title}</langstring></description>"
    );
    let _ = write!(
        x,
        "    <au id=\"{}\" moveOn=\"{move_on}\" launchMethod=\"AnyWindow\"",
        xml_escape(&au_id)
    );
    if let Some(score) = spec.mastery_score {
        let _ = write!(x, " masteryScore=\"{}\"", format_mastery(score));
    }
    x.push_str(">\n");
    let _ = writeln!(
        x,
        "      <title><langstring lang=\"en-US\">{title}</langstring></title>"
    );
    let _ = writeln!(
        x,
        "      <description><langstring lang=\"en-US\">{title}</langstring></description>"
    );
    x.push_str("      <url>index.html</url>\n");
    x.push_str("    </au>\n");
    x.push_str("  </course>\n");
    x.push_str("</courseStructure>\n");
    x
}

/// Build a Tin Can `tincan.xml` manifest.
#[must_use]
pub fn tincan_xml(spec: &PackageSpec) -> String {
    let activity_id = activity_id_for(&spec.course_id);
    let title = xml_escape(&spec.title);

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<tincan xmlns=\"http://projecttincan.com/tincan.xsd\">\n");
    x.push_str("  <activities>\n");
    let _ = writeln!(
        x,
        "    <activity id=\"{}\" type=\"http://adlnet.gov/expapi/activities/course\">",
        xml_escape(&activity_id)
    );
    let _ = writeln!(x, "      <name lang=\"en-US\">{title}</name>");
    let _ = writeln!(x, "      <description lang=\"en-US\">{title}</description>");
    x.push_str("      <launch lang=\"en-US\">index.html</launch>\n");
    x.push_str("    </activity>\n");
    x.push_str("  </activities>\n");
    x.push_str("</tincan>\n");
    x
}

/// Keep only `[A-Za-z0-9_:.-]`, replacing anything else with `_`.
fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Format a percentage as a plain decimal (`80` becomes `0.8`, `100` becomes `1`).
fn format_mastery(percent: u8) -> String {
    let percent = percent.min(100);
    if percent == 100 {
        return "1".to_string();
    }
    let fraction = percent % 100;
    if fraction == 0 {
        return "0".to_string();
    }
    let mut out = format!("0.{fraction:02}");
    while out.ends_with('0') {
        out.pop();
    }
    out
}

fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}
