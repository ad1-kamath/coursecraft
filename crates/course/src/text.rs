//! Rich text run helpers: normalization, text edits, and range styling.

use crate::{Color, Link, Paragraph, TextRun};

/// Normalize a list of text runs:
/// - drops empty runs (`text.is_empty()`)
/// - merges adjacent runs that have identical overrides.
#[must_use]
pub fn normalize_runs(runs: &[TextRun]) -> Vec<TextRun> {
    let mut result: Vec<TextRun> = Vec::new();
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = result.last_mut()
            && last.bold == run.bold
            && last.italic == run.italic
            && last.underline == run.underline
            && last.strikethrough == run.strikethrough
            && last.baseline == run.baseline
            && last.letter_spacing == run.letter_spacing
            && last.size == run.size
            && last.color == run.color
            && last.link == run.link
            && last.font == run.font
        {
            last.text.push_str(&run.text);
            continue;
        }
        result.push(run.clone());
    }
    result
}

/// Patch of overrides to apply to a range of text runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextRunPatch {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub baseline: Option<crate::Baseline>,
    pub letter_spacing: Option<f32>,
    pub size: Option<f32>,
    pub color: Option<Color>,
    /// A font family name. `None` leaves the run's font alone.
    pub font: Option<String>,
    /// `None` leaves links alone. `Some(None)` removes a link.
    pub link: Option<Option<Link>>,
}

/// Maps a plain-text edit onto existing runs by common prefix/suffix (in chars,
/// never splitting a UTF-8 character).
///
/// Deleted characters are removed from their runs; inserted characters take the overrides
/// of the run at the insertion point (the run before the insertion point; at position 0
/// the first run). The result is normalized.
#[must_use]
pub fn apply_text_edit(runs: &[TextRun], old_plain: &str, new_plain: &str) -> Vec<TextRun> {
    if runs.is_empty() || new_plain.is_empty() {
        return Vec::new();
    }

    let old_chars: Vec<char> = old_plain.chars().collect();
    let new_chars: Vec<char> = new_plain.chars().collect();

    // Find common prefix length in characters
    let mut prefix_len = 0;
    while prefix_len < old_chars.len()
        && prefix_len < new_chars.len()
        && old_chars.get(prefix_len) == new_chars.get(prefix_len)
    {
        prefix_len = prefix_len.saturating_add(1);
    }

    // Find common suffix length in characters (non-overlapping with prefix)
    let max_suffix = old_chars
        .len()
        .saturating_sub(prefix_len)
        .min(new_chars.len().saturating_sub(prefix_len));
    let mut suffix_len = 0;
    while suffix_len < max_suffix {
        let old_idx = old_chars.len().saturating_sub(1).saturating_sub(suffix_len);
        let new_idx = new_chars.len().saturating_sub(1).saturating_sub(suffix_len);
        if old_chars.get(old_idx) == new_chars.get(new_idx) {
            suffix_len = suffix_len.saturating_add(1);
        } else {
            break;
        }
    }

    // Determine overrides for inserted chars:
    // "inserted chars take the overrides of the run at the insertion point
    // (the run before the insertion point; at position 0 the first run)"
    let insert_run_idx = if prefix_len == 0 {
        0
    } else {
        let target_char = prefix_len.saturating_sub(1);
        let mut char_count: usize = 0;
        let mut found = 0;
        for (i, run) in runs.iter().enumerate() {
            let next_count = char_count.saturating_add(run.text.chars().count());
            if target_char < next_count {
                found = i;
                break;
            }
            char_count = next_count;
            found = i;
        }
        found
    };

    let inserted_overrides = runs
        .get(insert_run_idx)
        .cloned()
        .unwrap_or_else(|| TextRun {
            text: String::new(),
            bold: None,
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        });

    let suffix_start_char = old_chars.len().saturating_sub(suffix_len);

    let mut prefix_runs = Vec::new();
    let mut cur: usize = 0;
    for run in runs {
        let count = run.text.chars().count();
        let next = cur.saturating_add(count);
        if cur < prefix_len {
            let take_count = (prefix_len.saturating_sub(cur)).min(count);
            let chars: String = run.text.chars().take(take_count).collect();
            if !chars.is_empty() {
                prefix_runs.push(TextRun {
                    text: chars,
                    bold: run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strikethrough: run.strikethrough,
                    baseline: run.baseline,
                    letter_spacing: run.letter_spacing,
                    size: run.size,
                    color: run.color.clone(),
                    link: run.link.clone(),
                    font: run.font.clone(),
                });
            }
        }
        cur = next;
    }

    let inserted_end = new_chars.len().saturating_sub(suffix_len);
    if prefix_len < inserted_end {
        let inserted_chars: String = new_chars
            .get(prefix_len..inserted_end)
            .unwrap_or(&[])
            .iter()
            .collect();
        if !inserted_chars.is_empty() {
            prefix_runs.push(TextRun {
                text: inserted_chars,
                bold: inserted_overrides.bold,
                italic: inserted_overrides.italic,
                underline: inserted_overrides.underline,
                strikethrough: inserted_overrides.strikethrough,
                baseline: inserted_overrides.baseline,
                letter_spacing: inserted_overrides.letter_spacing,
                size: inserted_overrides.size,
                color: inserted_overrides.color,
                link: inserted_overrides.link,
                font: inserted_overrides.font,
            });
        }
    }

    let mut suffix_runs = Vec::new();
    let mut cur_s: usize = 0;
    for run in runs {
        let count = run.text.chars().count();
        let next = cur_s.saturating_add(count);
        if next > suffix_start_char {
            let skip_count = suffix_start_char.saturating_sub(cur_s);
            let chars: String = run.text.chars().skip(skip_count).collect();
            if !chars.is_empty() {
                suffix_runs.push(TextRun {
                    text: chars,
                    bold: run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strikethrough: run.strikethrough,
                    baseline: run.baseline,
                    letter_spacing: run.letter_spacing,
                    size: run.size,
                    color: run.color.clone(),
                    link: run.link.clone(),
                    font: run.font.clone(),
                });
            }
        }
        cur_s = next;
    }

    prefix_runs.extend(suffix_runs);
    normalize_runs(&prefix_runs)
}

/// Splits runs at the range edges and sets the given overrides on the covered part.
///
/// An empty range applies to the whole text. If every run ends up with no overrides,
/// returns an empty vec (plain text).
#[must_use]
pub fn apply_style_to_range(
    runs_or_plain: &[TextRun],
    plain_text: &str,
    char_range: (usize, usize),
    patch: &TextRunPatch,
) -> Vec<TextRun> {
    if plain_text.is_empty() {
        return Vec::new();
    }
    let total_chars = plain_text.chars().count();
    let (mut start, mut end) = char_range;
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    let (start, end) = if start == end {
        (0, total_chars)
    } else {
        (start.min(total_chars), end.min(total_chars))
    };

    let base_runs: Vec<TextRun> = if runs_or_plain.is_empty() {
        vec![TextRun {
            text: plain_text.to_owned(),
            bold: None,
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        }]
    } else {
        runs_or_plain.to_vec()
    };

    let mut split_runs = Vec::new();
    let mut cur_pos: usize = 0;

    for run in base_runs {
        let run_chars: Vec<char> = run.text.chars().collect();
        let run_len = run_chars.len();
        let run_start = cur_pos;
        let run_end = cur_pos.saturating_add(run_len);

        let seg1_end = run_start.max(start.min(run_end));
        let seg2_start = seg1_end;
        let seg2_end = run_end.min(end).max(run_start);
        let seg3_start = run_start.max(end).min(run_end);

        if seg1_end > run_start {
            let s: String = run_chars
                .get(0..seg1_end.saturating_sub(run_start))
                .unwrap_or(&[])
                .iter()
                .collect();
            if !s.is_empty() {
                split_runs.push(TextRun {
                    text: s,
                    bold: run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strikethrough: run.strikethrough,
                    baseline: run.baseline,
                    letter_spacing: run.letter_spacing,
                    size: run.size,
                    color: run.color.clone(),
                    link: run.link.clone(),
                    font: run.font.clone(),
                });
            }
        }

        if seg2_end > seg2_start {
            let s: String = run_chars
                .get(seg2_start.saturating_sub(run_start)..seg2_end.saturating_sub(run_start))
                .unwrap_or(&[])
                .iter()
                .collect();
            if !s.is_empty() {
                let mut styled = run.clone();
                styled.text = s;
                if let Some(b) = patch.bold {
                    styled.bold = Some(b);
                }
                if let Some(it) = patch.italic {
                    styled.italic = Some(it);
                }
                if let Some(u) = patch.underline {
                    styled.underline = Some(u);
                }
                if let Some(v) = patch.strikethrough {
                    styled.strikethrough = Some(v);
                }
                if let Some(v) = patch.baseline {
                    styled.baseline = Some(v);
                }
                if let Some(v) = patch.letter_spacing {
                    styled.letter_spacing = Some(crate::clamp_letter_spacing(v));
                }
                if let Some(sz) = patch.size {
                    styled.size = Some(sz);
                }
                if let Some(ref c) = patch.color {
                    styled.color = Some(c.clone());
                }
                if let Some(ref link) = patch.link {
                    styled.link = link.clone();
                }
                if let Some(ref font) = patch.font {
                    styled.font = Some(font.clone());
                }
                split_runs.push(styled);
            }
        }

        if run_end > seg3_start {
            let s: String = run_chars
                .get(seg3_start.saturating_sub(run_start)..run_len)
                .unwrap_or(&[])
                .iter()
                .collect();
            if !s.is_empty() {
                split_runs.push(TextRun {
                    text: s,
                    bold: run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strikethrough: run.strikethrough,
                    baseline: run.baseline,
                    letter_spacing: run.letter_spacing,
                    size: run.size,
                    color: run.color.clone(),
                    link: run.link.clone(),
                    font: run.font.clone(),
                });
            }
        }

        cur_pos = run_end;
    }

    let normalized = normalize_runs(&split_runs);

    let all_no_overrides = normalized.iter().all(|r| {
        r.bold.is_none()
            && r.italic.is_none()
            && r.underline.is_none()
            && r.strikethrough.is_none()
            && r.baseline.is_none()
            && r.letter_spacing.is_none()
            && r.size.is_none()
            && r.color.is_none()
            && r.link.is_none()
            && r.font.is_none()
    });

    if all_no_overrides {
        Vec::new()
    } else {
        normalized
    }
}

/// The number of `\n`-separated paragraphs in `text`.
#[must_use]
pub fn paragraph_count(text: &str) -> usize {
    text.chars()
        .filter(|c| *c == '\n')
        .count()
        .saturating_add(1)
}

/// Paragraph styles padded to one entry per paragraph in `text`.
#[must_use]
pub fn paragraphs_for(text: &str, paragraphs: &[Paragraph]) -> Vec<Paragraph> {
    (0..paragraph_count(text))
        .map(|index| paragraphs.get(index).cloned().unwrap_or_default())
        .collect()
}

/// The half-open paragraph index range touched by the character range
/// `start..end`. An empty range selects the paragraph at `start`; the end of a
/// non-empty selection that sits exactly on a newline does not add the next
/// paragraph.
#[must_use]
pub fn paragraph_range_at(text: &str, start: usize, end: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let clamp = |value: usize| value.min(chars.len());
    let (start, end) = (clamp(start.min(end)), clamp(start.max(end)));
    let index_at = |value: usize| {
        chars
            .iter()
            .take(value)
            .filter(|c| **c == '\n')
            .count()
            .min(paragraph_count(text).saturating_sub(1))
    };
    let first = index_at(start);
    if start == end {
        return (first, first.saturating_add(1));
    }
    // A selection ending just after a newline includes the paragraph before it,
    // not the new one it starts.
    let mut last_char = end.saturating_sub(1);
    if chars.get(last_char) == Some(&'\n') && last_char > start {
        last_char = last_char.saturating_sub(1);
    }
    let last = index_at(last_char);
    (first.min(last), last.saturating_add(1))
}

/// Keep paragraph styles aligned when `old_plain` becomes `new_plain`.
///
/// Inserted paragraphs copy the paragraph they split from; joining paragraphs
/// keeps the first one's style. Unchanged paragraphs keep theirs. Trailing
/// default paragraphs are dropped so a file stays short.
#[must_use]
pub fn align_paragraphs(
    old_plain: &str,
    old_paragraphs: &[Paragraph],
    new_plain: &str,
) -> Vec<Paragraph> {
    if new_plain.is_empty() {
        return Vec::new();
    }
    let old_chars: Vec<char> = old_plain.chars().collect();
    let new_chars: Vec<char> = new_plain.chars().collect();
    let old_styles = paragraphs_for(old_plain, old_paragraphs);

    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars.get(prefix) == new_chars.get(prefix)
    {
        prefix = prefix.saturating_add(1);
    }
    let max_suffix = old_chars
        .len()
        .saturating_sub(prefix)
        .min(new_chars.len().saturating_sub(prefix));
    let mut suffix = 0usize;
    while suffix < max_suffix {
        let old_idx = old_chars.len().saturating_sub(1).saturating_sub(suffix);
        let new_idx = new_chars.len().saturating_sub(1).saturating_sub(suffix);
        if old_chars.get(old_idx) == new_chars.get(new_idx) {
            suffix = suffix.saturating_add(1);
        } else {
            break;
        }
    }

    let old_len = old_chars.len();
    let new_len = new_chars.len();
    let suffix_start = new_len.saturating_sub(suffix);
    let map_char = |value: usize| -> usize {
        if value <= prefix {
            value.min(old_len)
        } else if suffix > 0 && value >= suffix_start {
            value
                .saturating_add(old_len)
                .saturating_sub(new_len)
                .min(old_len)
        } else {
            prefix.min(old_len)
        }
    };

    let mut result = Vec::new();
    let mut new_start = 0usize;
    for line in new_plain.split('\n') {
        let mapped = map_char(new_start);
        let old_para = old_chars
            .iter()
            .take(mapped)
            .filter(|c| **c == '\n')
            .count();
        result.push(old_styles.get(old_para).cloned().unwrap_or_default());
        new_start = new_start
            .saturating_add(line.chars().count())
            .saturating_add(1);
    }
    while result.last() == Some(&Paragraph::default()) {
        result.pop();
    }
    result
}

/// Apply `link` (or clear it when `None`) to the characters in `char_range`.
#[must_use]
pub fn apply_link_to_range(
    runs_or_plain: &[TextRun],
    plain_text: &str,
    char_range: (usize, usize),
    link: Option<Link>,
) -> Vec<TextRun> {
    let patch = TextRunPatch {
        link: Some(link),
        ..TextRunPatch::default()
    };
    apply_style_to_range(runs_or_plain, plain_text, char_range, &patch)
}
