//! A small, hostile-input-safe WebVTT parser (L2).
//!
//! CourseCraft only needs cues and timing, so styling, positioning and regions
//! are skipped. Every limit and error path is defined here; nothing indexes
//! bytes derived from the input without a checked conversion.

#[cfg(test)]
mod tests;

/// Largest caption file accepted (2 MiB).
pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

/// Maximum number of cues kept. Parsing stops at the limit.
pub const MAX_CUES: usize = 20_000;

/// Maximum characters kept from one cue's text (cut on a character boundary).
pub const MAX_CUE_CHARS: usize = 4_000;

/// A parsed WebVTT cue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Cue start in milliseconds.
    pub start_ms: u64,
    /// Cue end in milliseconds.
    pub end_ms: u64,
    /// Cleaned text, with speaker prefix and line breaks preserved.
    pub text: String,
}

/// Why a caption file as a whole could not be parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CaptionError {
    /// The input exceeds [`MAX_INPUT_BYTES`].
    #[error("caption file is larger than 2 MiB ({size} bytes)")]
    TooLarge { size: usize },
    /// The required `WEBVTT` header is missing.
    #[error("missing WEBVTT header")]
    MissingHeader,
}

/// Parse WebVTT bytes into cues sorted by start time.
///
/// Invalid UTF-8 is decoded lossily. A malformed cue or one whose end is not
/// after its start is skipped; a too-large or header-less file is an error.
///
/// # Errors
/// Returns [`CaptionError::TooLarge`] for input above [`MAX_INPUT_BYTES`] and
/// [`CaptionError::MissingHeader`] when no `WEBVTT` header precedes the cues.
pub fn parse_webvtt(bytes: &[u8]) -> Result<Vec<Cue>, CaptionError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(CaptionError::TooLarge { size: bytes.len() });
    }
    let decoded = String::from_utf8_lossy(bytes);
    let text = decoded.strip_prefix('\u{feff}').unwrap_or(&decoded);
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized.split('\n');
    let mut found_header = false;
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            continue;
        }
        if is_header(line) {
            found_header = true;
            break;
        }
        return Err(CaptionError::MissingHeader);
    }
    if !found_header {
        return Err(CaptionError::MissingHeader);
    }

    let mut cues = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            parse_block(&block, &mut cues);
            block.clear();
            if cues.len() >= MAX_CUES {
                break;
            }
        } else {
            block.push(line);
        }
    }
    parse_block(&block, &mut cues);
    cues.sort_by_key(|cue| cue.start_ms);
    Ok(cues)
}

/// Text of every cue active at `position_ms`, joined with newlines.
///
/// A cue is active when `start <= position < end`.
#[must_use]
pub fn cue_text_at(cues: &[Cue], position_ms: u64) -> Option<String> {
    let active: Vec<&str> = cues
        .iter()
        .filter(|cue| cue.start_ms <= position_ms && position_ms < cue.end_ms)
        .map(|cue| cue.text.as_str())
        .collect();
    if active.is_empty() {
        None
    } else {
        Some(active.join("\n"))
    }
}

fn is_header(line: &str) -> bool {
    let line = line.trim_end();
    line == "WEBVTT" || line.starts_with("WEBVTT ") || line.starts_with("WEBVTT\t")
}

fn is_ignored_block(first: &str) -> bool {
    let first = first.trim_start();
    first == "NOTE"
        || first.starts_with("NOTE ")
        || first.starts_with("NOTE\t")
        || first == "STYLE"
        || first == "REGION"
}

fn parse_block(block: &[&str], cues: &mut Vec<Cue>) {
    if cues.len() >= MAX_CUES {
        return;
    }
    let Some(first) = block.first() else {
        return;
    };
    if is_ignored_block(first) {
        return;
    }
    let (timing_line, text_start) = if first.contains("-->") {
        (first, 1)
    } else {
        match block.get(1) {
            Some(timing) => (timing, 2),
            None => return,
        }
    };
    let Some((start_ms, end_ms)) = parse_timing(timing_line) else {
        return;
    };
    if end_ms <= start_ms {
        return;
    }
    let Some(lines) = block.get(text_start.min(block.len())..) else {
        return;
    };
    let Some(text) = clean_cue_text(lines) else {
        return;
    };
    cues.push(Cue {
        start_ms,
        end_ms,
        text,
    });
}

fn parse_timing(line: &str) -> Option<(u64, u64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((parse_timestamp(start.trim())?, parse_timestamp(end)?))
}

fn parse_timestamp(raw: &str) -> Option<u64> {
    let (clock, millis) = raw.split_once('.')?;
    if millis.len() != 3 || !millis.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let millis: u64 = millis.parse().ok()?;
    let fields: Vec<&str> = clock.split(':').collect();
    let (hours, minutes, seconds) = match fields.as_slice() {
        [minutes, seconds] => (0, parse_field(minutes)?, parse_field(seconds)?),
        [hours, minutes, seconds] => (
            parse_field(hours)?,
            parse_field(minutes)?,
            parse_field(seconds)?,
        ),
        _ => return None,
    };
    if minutes > 59 || seconds > 59 {
        return None;
    }
    hours
        .checked_mul(3_600_000)?
        .checked_add(minutes.checked_mul(60_000)?)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(millis)
}

fn parse_field(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

fn clean_cue_text(lines: &[&str]) -> Option<String> {
    let mut cleaned = Vec::with_capacity(lines.len());
    let mut speaker: Option<String> = None;
    for line in lines {
        let (text, tag_speaker) = strip_tags(line);
        if speaker.is_none() {
            speaker = tag_speaker;
        }
        cleaned.push(text);
    }
    let mut text = cleaned.join("\n");
    if let Some(speaker) = speaker {
        text = if text.is_empty() {
            speaker
        } else {
            format!("{speaker}: {text}")
        };
    }
    let text = decode_entities(&text);
    Some(text.chars().take(MAX_CUE_CHARS).collect())
}

/// Remove markup tags and return the speaker of a `<v ...>` tag, if any.
fn strip_tags(line: &str) -> (String, Option<String>) {
    let mut out = String::with_capacity(line.len());
    let mut speaker = None;
    let mut chars = line.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch != '<' {
            out.push(ch);
            continue;
        }
        let Some(relative_end) = line.get(index + 1..).and_then(|rest| rest.find('>')) else {
            out.push(ch);
            continue;
        };
        let tag_end = index + 1 + relative_end;
        if let Some(tag) = line.get(index + 1..tag_end)
            && speaker.is_none()
        {
            speaker = speaker_from_tag(tag);
        }
        while let Some(&(next, _)) = chars.peek() {
            if next <= tag_end {
                chars.next();
            } else {
                break;
            }
        }
    }
    (out, speaker)
}

fn speaker_from_tag(tag: &str) -> Option<String> {
    let rest = tag.strip_prefix(['v', 'V'])?.trim_start();
    let rest = if rest.starts_with('.') {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        rest.get(end..).unwrap_or_default().trim_start()
    } else {
        rest
    };
    let name = rest.split('.').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(index) = rest.find('&') {
        out.push_str(rest.get(..index).unwrap_or_default());
        let after = rest.get(index..).unwrap_or_default();
        let (replacement, consumed) = entity_at(after);
        out.push_str(replacement);
        rest = after.get(consumed..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

fn entity_at(input: &str) -> (&str, usize) {
    for (pattern, replacement) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&nbsp;", "\u{a0}"),
        ("&lrm;", "\u{200e}"),
        ("&rlm;", "\u{200f}"),
    ] {
        if input.starts_with(pattern) {
            return (replacement, pattern.len());
        }
    }
    ("&", 1)
}
