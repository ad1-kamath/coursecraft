//! The project's web links in one place, and the pre-filled GitHub issue URL.
//!
//! Nothing here touches the network. A link is only built; the person opens it
//! (Help menu items) and reads it in their browser before submitting anything.

/// The project's home on GitHub.
pub const REPO_URL: &str = "https://github.com/ad1-kamath/coursecraft";

/// Issue template file for a problem report.
pub const BUG_TEMPLATE: &str = "bug_report.md";
/// Issue template file for a feature idea.
pub const FEATURE_TEMPLATE: &str = "feature_request.md";

/// Longest URL, in bytes, a pre-filled issue link may have. Browsers and
/// GitHub refuse much longer ones.
pub const MAX_URL_BYTES: usize = 7_000;

/// Appended to a body that had to be cut.
pub const CUT_NOTE: &str = "\n\n… (cut; use Copy for the full text)";

/// Percent-encode text for a URL query value, byte by byte over UTF-8.
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        push_encoded(&mut out, ch);
    }
    out
}

fn push_encoded(out: &mut String, ch: char) {
    if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~') {
        out.push(ch);
        return;
    }
    let mut buf = [0_u8; 4];
    for byte in ch.encode_utf8(&mut buf).bytes() {
        out.push_str(&format!("%{byte:02X}"));
    }
}

/// The encoded length of one char.
fn encoded_len(ch: char) -> usize {
    if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~') {
        1
    } else {
        ch.len_utf8() * 3
    }
}

/// Cut `body` on a char boundary so its encoded form, plus the cut note, fits
/// in `budget` bytes. Returns the text unchanged when it already fits.
fn fit_body(body: &str, budget: usize) -> String {
    let total: usize = body.chars().map(encoded_len).sum();
    if total <= budget {
        return body.to_string();
    }
    let note: usize = CUT_NOTE.chars().map(encoded_len).sum();
    let room = budget.saturating_sub(note);
    let mut used = 0;
    let mut end = 0;
    for (index, ch) in body.char_indices() {
        used += encoded_len(ch);
        if used > room {
            break;
        }
        end = index + ch.len_utf8();
    }
    format!("{}{CUT_NOTE}", body.get(..end).unwrap_or_default())
}

/// A link that opens GitHub's new-issue page with the template chosen and the
/// title and body filled in. The body is cut so the whole URL stays under
/// [`MAX_URL_BYTES`].
#[must_use]
pub fn new_issue_url(template: &str, title: &str, body: &str) -> String {
    let head = format!(
        "{REPO_URL}/issues/new?template={}&title={}&body=",
        percent_encode(template),
        percent_encode(title)
    );
    let budget = (MAX_URL_BYTES - 1).saturating_sub(head.len());
    format!("{head}{}", percent_encode(&fit_body(body, budget)))
}

/// The project's discussions page, for questions and ideas.
#[must_use]
pub fn discussions_url() -> String {
    format!("{REPO_URL}/discussions")
}

/// The releases page, where new versions are published.
#[must_use]
pub fn releases_url() -> String {
    format!("{REPO_URL}/releases")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_reserved_and_non_ascii() {
        assert_eq!(percent_encode("a b&c#d"), "a%20b%26c%23d");
        assert_eq!(percent_encode("l1\nl2"), "l1%0Al2");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode("日"), "%E6%97%A5");
        assert_eq!(percent_encode("safe-_.~09Az"), "safe-_.~09Az");
    }

    #[test]
    fn issue_url_has_template_title_and_body() {
        let url = new_issue_url("bug_report.md", "It broke & burned", "line 1\nline 2 #3");
        assert!(url.starts_with(REPO_URL));
        assert!(url.contains("/issues/new?template=bug_report.md&title=It%20broke%20%26%20burned"));
        assert!(url.ends_with("&body=line%201%0Aline%202%20%233"));
        assert!(!url.contains(' ') && !url.contains('\n'));
    }

    #[test]
    fn long_body_is_cut_on_a_char_boundary_under_the_limit() {
        for filler in ["x", "é", "日", "😀", "&"] {
            let body = filler.repeat(8_000);
            let url = new_issue_url("bug_report.md", "T", &body);
            assert!(url.len() < MAX_URL_BYTES, "{filler}: {}", url.len());
            assert!(url.len() > MAX_URL_BYTES - 40, "{filler} cut too early");
            assert!(url.ends_with(&percent_encode(CUT_NOTE)));
        }
    }

    #[test]
    fn short_body_is_not_cut() {
        let url = new_issue_url("t", "T", "short");
        assert!(url.ends_with("&body=short"));
        assert!(!url.contains("cut"));
    }

    #[test]
    fn other_links_hang_off_the_repo() {
        assert_eq!(discussions_url(), format!("{REPO_URL}/discussions"));
        assert_eq!(releases_url(), format!("{REPO_URL}/releases"));
    }
}
